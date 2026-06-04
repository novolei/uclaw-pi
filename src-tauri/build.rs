//! Tauri build script.
//!
//! In addition to the standard `tauri_build::build()`, this shim makes
//! **debug** builds survive in checkouts where the bundled resource artifacts
//! (`pyembed/python`, `bunembed/bun`, `gbrain-source`, the frontend `static/`,
//! …) haven't been produced yet — most commonly **git worktrees** and fresh
//! clones. `tauri_build::build()` hard-fails with `resource path "X" doesn't
//! exist` for *any* declared `bundle.resources` path that's missing, which
//! blocks `cargo build` / `cargo check` / `cargo test` even though a debug
//! build never actually bundles those resources.
//!
//! Fix: before delegating to `tauri_build`, in the **debug profile only**,
//! create an empty placeholder for each missing declared resource (and the
//! `frontendDist`) and emit a `cargo:warning` for each. Release builds are left
//! untouched — they keep the hard failure so a real bundle can never ship with
//! empty placeholders standing in for the embedded runtimes.

use std::path::{Path, PathBuf};

fn main() {
    // Debug only. In release we want the original hard failure to protect bundle
    // integrity (an empty `pyembed/python` would silently ship a broken app).
    if std::env::var("PROFILE").as_deref() == Ok("debug") {
        if let Err(e) = ensure_dev_resource_placeholders() {
            // Never fail the build on the shim itself — fall through to tauri,
            // which will surface the real missing-resource error if relevant.
            println!("cargo:warning=resource-placeholder shim skipped: {e}");
        }
    }

    tauri_build::build();
}

/// Read `tauri.conf.json`, and for each declared `bundle.resources` source path
/// plus `build.frontendDist` that doesn't exist, create an empty placeholder so
/// `tauri_build`'s existence check passes. Best-effort and noisy (one
/// `cargo:warning` per placeholder). Paths are resolved relative to this build
/// script's CWD (the `src-tauri/` crate dir).
fn ensure_dev_resource_placeholders() -> Result<(), String> {
    let conf_raw = std::fs::read_to_string("tauri.conf.json")
        .map_err(|e| format!("read tauri.conf.json: {e}"))?;
    let conf: serde_json::Value =
        serde_json::from_str(&conf_raw).map_err(|e| format!("parse tauri.conf.json: {e}"))?;

    let mut sources: Vec<String> = Vec::new();

    // bundle.resources is either an object {src: target} or an array [src, …].
    match conf.pointer("/bundle/resources") {
        Some(serde_json::Value::Object(map)) => {
            sources.extend(map.keys().cloned());
        }
        Some(serde_json::Value::Array(arr)) => {
            sources.extend(arr.iter().filter_map(|v| v.as_str().map(str::to_owned)));
        }
        _ => {}
    }

    // The frontend dist dir — also existence-checked by tauri.
    if let Some(dist) = conf
        .pointer("/build/frontendDist")
        .and_then(serde_json::Value::as_str)
    {
        sources.push(dist.to_owned());
    }

    for src in sources {
        // Globs can't be placeholdered meaningfully; skip them (a glob with no
        // matches doesn't trip the existence check the way a literal path does).
        if src.contains('*') || src.contains('?') || src.contains('[') {
            continue;
        }
        let path = PathBuf::from(&src);
        if path.exists() {
            continue;
        }
        if let Err(e) = create_placeholder(&path) {
            println!("cargo:warning=could not create dev placeholder for `{src}`: {e}");
            continue;
        }
        println!(
            "cargo:warning=created empty dev placeholder for missing bundle resource `{src}` \
             (debug build; populate it via the real artifact scripts before a release bundle)"
        );
    }

    Ok(())
}

/// Create an empty placeholder at `path`: an empty file if the final component
/// looks like a file (has an extension), otherwise an empty directory. Parent
/// directories are created as needed.
fn create_placeholder(path: &Path) -> std::io::Result<()> {
    let looks_like_file = path
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.contains('.'));
    if looks_like_file {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, b"")?;
    } else {
        std::fs::create_dir_all(path)?;
    }
    Ok(())
}
