// SPDX-License-Identifier: AGPL-3.0-or-later
//! Local LLM-domain Tauri commands (S1 + S2) — thin wrappers over
//! [`crate::local_llm`]: presence check, network-aware download (quant +
//! source-probe + resume), a source latency probe, and download cancellation.
//!
//! Per the code-organization ADR (2026-05-31), command bodies are thin: parse
//! input → call the local-llm helpers → map result/error → emit event. The
//! engine handle + idle-unload reaper are wired in `main.rs`/`app.rs`, not here.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::Duration;

use tauri::{AppHandle, Emitter, State};

use crate::app::AppState;
use crate::error::Error;
use crate::local_llm::download::{Quant, Source};
use crate::local_llm::preflight::{self, EnvReport};

/// Model ref (`provider_id/model_id`) the local MiniCPM is assigned under. Must
/// match the `local-minicpm` registry id (`providers/registry.rs`) + the
/// `minicpm5-1b` model id (`list_local_minicpm_models`); `set_role_model`
/// stores it verbatim and the resolver `splitn(2, '/')`s it back apart.
const LOCAL_MODEL_REF: &str = "local-minicpm/minicpm5-1b";

/// Roles auto-assigned to the local model after warmup (cheap background work,
/// editable later in Settings).
const LOCAL_MODEL_ROLES: [&str; 2] = ["summarizer", "utility"];

/// Probe budget for `probe_download_sources` (shorter than the download-time
/// probe so the UI's "测速中" spinner stays snappy).
const UI_PROBE_TIMEOUT: Duration = Duration::from_secs(6);

/// Shared cancel flag. Set by `cancel_download`, polled by the in-flight
/// download loop, and reset at the start of each `download_local_model`. A
/// module static (not `AppState`) keeps the single-flight contract obvious:
/// there is at most one download in flight at a time.
fn cancel_flag() -> &'static AtomicBool {
    static FLAG: OnceLock<AtomicBool> = OnceLock::new();
    FLAG.get_or_init(|| AtomicBool::new(false))
}

/// Parse an optional quant string (e.g. `"q8_0"`, `"Q4_K_M"`) into a [`Quant`],
/// defaulting to [`Quant::default`] when absent. Accepts the same string reprs
/// the UI persists (snake_case + uppercase aliases via serde).
fn parse_quant(quant: Option<String>) -> Result<Quant, Error> {
    match quant {
        None => Ok(Quant::default()),
        Some(s) => serde_json::from_value::<Quant>(serde_json::Value::String(s.clone()))
            .map_err(|_| Error::Internal(format!("unknown quant: {s}"))),
    }
}

/// Parse an optional source string into a [`Source`]; `None` means "probe the
/// fastest". Accepts the stable [`Source::label`] values (`"modelscope"` /
/// `"huggingface"`) the UI sends + emits on progress events.
fn parse_source(source: Option<String>) -> Result<Option<Source>, Error> {
    match source.as_deref() {
        None => Ok(None),
        Some(s) => Source::ALL
            .into_iter()
            .find(|src| src.label() == s)
            .map(Some)
            .ok_or_else(|| Error::Internal(format!("unknown source: {s}"))),
    }
}

/// True iff the local MiniCPM GGUF for `quant` is downloaded (default quant when
/// the UI doesn't pass one).
#[tauri::command]
pub async fn is_local_model_present(
    state: State<'_, AppState>,
    quant: Option<String>,
) -> Result<bool, Error> {
    let quant = parse_quant(quant)?;
    Ok(crate::local_llm::paths::is_model_present(&state.data_dir, quant))
}

/// Concurrently probe ModelScope + HuggingFace latency for `quant` and report
/// the fastest reachable source plus per-source latencies (ms; `null` if
/// unreachable). Used by the UI to label the progress bar before downloading.
#[tauri::command]
pub async fn probe_download_sources(quant: Option<String>) -> Result<serde_json::Value, Error> {
    let quant = parse_quant(quant)?;
    let ms = Source::ModelScope.reachable_latency(quant, UI_PROBE_TIMEOUT);
    let hf = Source::HuggingFace.reachable_latency(quant, UI_PROBE_TIMEOUT);
    let (ms, hf) = tokio::join!(ms, hf);
    let fastest = crate::local_llm::download::source::pick_fastest(ms, hf);
    Ok(serde_json::json!({
        "fastest": fastest.map(|s| s.label()),
        "latencies": {
            Source::ModelScope.label(): ms.map(|d| d.as_millis() as u64),
            Source::HuggingFace.label(): hf.map(|d| d.as_millis() as u64),
        }
    }))
}

/// Request cancellation of the in-flight download. Best-effort: the stream loop
/// aborts before its next chunk and preserves the `.part` so a later download
/// resumes from where it stopped.
#[tauri::command]
pub async fn cancel_download() -> Result<(), Error> {
    cancel_flag().store(true, Ordering::SeqCst);
    Ok(())
}

/// Download the MiniCPM GGUF for `quant` (default Q4_K_M) from `source`
/// (default: probe the fastest mirror), emitting progress on
/// `local-model:download-progress` (`{ downloaded, total, source, phase }`,
/// where phase ∈ `probing`|`downloading`|`verifying`). Returns the absolute
/// path to the downloaded GGUF on success.
#[tauri::command]
pub async fn download_local_model(
    app: AppHandle,
    state: State<'_, AppState>,
    quant: Option<String>,
    source: Option<String>,
) -> Result<String, Error> {
    let quant = parse_quant(quant)?;
    let prefer = parse_source(source)?;
    let data_dir = state.data_dir.clone();

    // Fresh download: clear any stale cancel request from a prior run.
    cancel_flag().store(false, Ordering::SeqCst);

    // Resolved source label for progress events; updated once probing settles.
    let source_label = std::sync::Arc::new(std::sync::Mutex::new(
        prefer.map(|s| s.label().to_string()),
    ));

    // "probing" phase: emit a probing event up front. If no explicit source was
    // requested, resolve+label the fastest mirror so the bar shows it ASAP.
    let emit = |app: &AppHandle, downloaded: u64, total: u64, src: Option<String>, phase: &str| {
        let _ = app.emit(
            "local-model:download-progress",
            serde_json::json!({
                "downloaded": downloaded,
                "total": total,
                "source": src,
                "phase": phase,
            }),
        );
    };
    emit(&app, 0, 0, source_label.lock().unwrap().clone(), "probing");
    // If no explicit source was requested, probe once here to label the bar AND
    // reuse that result as `prefer` so `download_model` doesn't probe a 2nd time.
    let mut effective_prefer = prefer;
    if effective_prefer.is_none() {
        if let Ok(s) =
            crate::local_llm::download::source::probe_fastest(quant, UI_PROBE_TIMEOUT).await
        {
            effective_prefer = Some(s);
            *source_label.lock().unwrap() = Some(s.label().to_string());
            emit(&app, 0, 0, source_label.lock().unwrap().clone(), "probing");
        }
    }

    let app_progress = app.clone();
    let label_progress = source_label.clone();
    let path = crate::local_llm::download::download_model_cancellable(
        &data_dir,
        quant,
        effective_prefer,
        move |downloaded, total| {
            let src = label_progress.lock().unwrap().clone();
            let _ = app_progress.emit(
                "local-model:download-progress",
                serde_json::json!({
                    "downloaded": downloaded,
                    "total": total,
                    "source": src,
                    "phase": "downloading",
                }),
            );
        },
        || cancel_flag().load(Ordering::SeqCst),
    )
    .await
    .map_err(|e| match e {
        crate::local_llm::download::DownloadError::Cancelled => {
            Error::Internal("download cancelled".to_string())
        }
        other => Error::Internal(format!("download failed: {other}")),
    })?;

    // "verifying" already ran inside download_model (size+sha256); emit a final
    // verifying tick so the UI can show the closing phase before completion.
    emit(&app, 0, 0, source_label.lock().unwrap().clone(), "verifying");
    Ok(path.to_string_lossy().to_string())
}

/// Run the first-launch environment preflight for `quant` (default Q4_K_M):
/// disk free vs the quant's headroom requirement, available RAM, Metal/GPU
/// availability, and per-source network reachability. Pure report — performs
/// no download. Serialized camelCase for the onboarding checklist.
#[tauri::command]
pub async fn check_local_model_environment(
    state: State<'_, AppState>,
    quant: Option<String>,
) -> Result<EnvReport, Error> {
    let quant = parse_quant(quant)?;
    Ok(preflight::check_environment(&state.data_dir, quant).await)
}

/// Warm up the local model: load the GGUF + JIT the runtime by running a
/// trivial 1-token completion. The generated text is discarded; only the
/// load/JIT side effect matters (subsequent role calls are then warm). Returns
/// an error if the engine is uninitialized or the model is missing/corrupt.
#[tauri::command]
pub async fn warmup_local_model() -> Result<(), Error> {
    let engine = crate::local_llm::local_engine()
        .ok_or_else(|| Error::Internal("local engine not initialized".to_string()))?;
    engine
        .complete("", "ok", 1, 0.0)
        .await
        .map(|_| ())
        .map_err(|e| Error::Internal(format!("local model warmup failed: {e}")))
}

/// Auto-assign the local MiniCPM to the cheap background roles (`summarizer`,
/// `utility`) after a successful download + warmup. Best-effort per the S3
/// design: a failure on either role surfaces as an error the UI can log, but is
/// non-fatal to onboarding (the user can assign manually in Settings).
#[tauri::command]
pub async fn assign_local_model_to_roles(state: State<'_, AppState>) -> Result<(), Error> {
    for role in LOCAL_MODEL_ROLES {
        state
            .provider_service
            .set_role_model(role, Some(LOCAL_MODEL_REF.to_string()))
            .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_model_ref_matches_registry_split() {
        // The resolver splits on the FIRST '/': provider id then model id.
        let parts: Vec<&str> = LOCAL_MODEL_REF.splitn(2, '/').collect();
        assert_eq!(parts, vec!["local-minicpm", "minicpm5-1b"]);
    }

    #[test]
    fn local_model_roles_are_summarizer_and_utility() {
        assert_eq!(LOCAL_MODEL_ROLES, ["summarizer", "utility"]);
    }

    #[test]
    fn parse_quant_defaults_to_q4km() {
        assert_eq!(parse_quant(None).unwrap(), Quant::Q4KM);
    }

    #[test]
    fn parse_quant_accepts_known_reprs() {
        assert_eq!(parse_quant(Some("q4_k_m".into())).unwrap(), Quant::Q4KM);
        assert_eq!(parse_quant(Some("Q4_K_M".into())).unwrap(), Quant::Q4KM);
        assert_eq!(parse_quant(Some("q8_0".into())).unwrap(), Quant::Q8_0);
        assert_eq!(parse_quant(Some("f16".into())).unwrap(), Quant::F16);
        assert_eq!(parse_quant(Some("F16".into())).unwrap(), Quant::F16);
    }

    #[test]
    fn parse_quant_rejects_garbage() {
        assert!(parse_quant(Some("q2".into())).is_err());
    }

    #[test]
    fn parse_source_none_is_probe() {
        assert_eq!(parse_source(None).unwrap(), None);
    }

    #[test]
    fn parse_source_accepts_known_labels() {
        assert_eq!(parse_source(Some("modelscope".into())).unwrap(), Some(Source::ModelScope));
        assert_eq!(parse_source(Some("huggingface".into())).unwrap(), Some(Source::HuggingFace));
    }

    #[test]
    fn parse_source_rejects_garbage() {
        assert!(parse_source(Some("s3".into())).is_err());
    }
}
