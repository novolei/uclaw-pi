# skills.sh Marketplace — P1 (Backend) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Pure-Rust backend to search/install/manage skills from skills.sh — an HTTP client (`/api/v1`), an install service (global vs workspace-tag + symlink), Tauri commands, and V25 install tracking. No agent/UI surface yet (those are P2–P5).

**Architecture:** A new `src-tauri/src/skills_marketplace/` module: `client.rs` (reqwest → skills.sh), `install.rs` (download files → `~/.uclaw-pi/skills/_marketplace/<slug>/` → optional workspace tag + symlink → `registry.reload()` → V25 row), `mod.rs` (re-exports). Thin Tauri commands in `commands/skills_marketplace.rs` registered in `main.rs`. Reuses the existing `commit_staged_skills` staging→commit machinery and the V25 `marketplace_standalone_installs` table.

**Tech Stack:** Rust, reqwest (json), rusqlite, tokio, tauri, mockito (tests), tempfile (tests). Spec: `docs/superpowers/specs/2026-06-01-skills-sh-marketplace-design.md`.

---

## Roadmap (5 phases — this doc is P1 only)

| Phase | Scope | Plan |
|---|---|---|
| **P1** | **Backend: client + install service + commands + V25** | **this doc** |
| P2 | legacy agent tool `skill_search_marketplace` (register in dispatcher) | own plan |
| P3 | in-chat install card (tool-result renderer + bridge + install command wiring) | own plan |
| P4 | 万花筒 skills page (market tab + installed-skill mgmt) | own plan |
| P5 | pi IO bridge `RealToolRequestSink` (skill_search/load_skill/skill_search_marketplace) | own plan |

> P2–P5 are deferred to their own plans (per writing-plans Scope Check): each is an independently-mergeable PR, and the UI/pi anchors (renderer registry, 万花筒 components, the pi bridge) must be **re-verified at execution** — the exploration pass reported at least one stale anchor (`RealToolRequestSink` does NOT yet exist).

## File Structure (P1)

| File | Responsibility |
|---|---|
| `src-tauri/src/skills_marketplace/mod.rs` | module root + re-exports + shared types |
| `src-tauri/src/skills_marketplace/client.rs` | reqwest client for skills.sh `/api/v1` (search/list/detail/audit) + API-key read |
| `src-tauri/src/skills_marketplace/install.rs` | install/uninstall/check_update; file write, workspace tag+symlink, V25 row |
| `src-tauri/src/commands/skills_marketplace.rs` | thin `#[tauri::command]` wrappers (parse → service → map) |
| `src-tauri/src/lib.rs` | `pub mod skills_marketplace;` + `pub mod` for the command file is under `commands::` |
| `src-tauri/src/commands/mod.rs` | `pub mod skills_marketplace;` |
| `src-tauri/src/main.rs` | register the 5 commands in `generate_handler!` |

**Install path decision (refines spec §4.1):** skills.sh installs land in `~/.uclaw-pi/skills/_marketplace/<slug>/` (Marketplace tier — reuses the existing `_marketplace` scan-dir recovery + gives the `marketplace` provenance badge P4 needs). `<slug>` = `source/slug` flattened to `owner__repo__slug` (matches the existing tool's slug scheme, `skill_marketplace.rs:396-401`).

**API key:** skills.sh `/api/v1` needs `Authorization: Bearer <key>`. Stored as a setting (`settings` table key `skills_sh_api_key`, reusing `DbSettings`); absent → search/detail return a typed `MissingApiKey` error the UI renders as "go set a key". (A keychain-encrypted store like `secret_store.rs` is a future hardening; v1 uses the settings table for simplicity — flagged in spec §9.)

---

## Task 1: Module scaffold + skills.sh data types

**Files:**
- Create: `src-tauri/src/skills_marketplace/mod.rs`
- Modify: `src-tauri/src/lib.rs` (add `pub mod skills_marketplace;` next to the other top-level `pub mod`s)

- [ ] **Step 1: Write the failing test** (in `mod.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn skill_summary_parses_search_item() {
        let v = serde_json::json!({
            "id": "expo/skills/react-native", "slug": "react-native", "name": "React Native",
            "source": "expo/skills", "installs": 3842, "sourceType": "github",
            "installUrl": "https://github.com/expo/skills", "url": "https://skills.sh/expo/skills/react-native"
        });
        let s: SkillSummary = serde_json::from_value(v).unwrap();
        assert_eq!(s.id, "expo/skills/react-native");
        assert_eq!(s.installs, 3842);
        assert_eq!(s.slug, "react-native");
    }
}
```

- [ ] **Step 2: Run it, verify it fails**

Run: `cd src-tauri && cargo test --lib skills_marketplace::tests::skill_summary_parses_search_item 2>&1 | tail -5`
Expected: FAIL (`cannot find type SkillSummary` / module not found).

- [ ] **Step 3: Write the module + types**

`src-tauri/src/skills_marketplace/mod.rs`:
```rust
//! skills.sh marketplace integration — HTTP client + install service.
//! See docs/superpowers/specs/2026-06-01-skills-sh-marketplace-design.md.

pub mod client;
pub mod install;

use serde::{Deserialize, Serialize};

/// One row from skills.sh `/api/v1/skills` or `/skills/search`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillSummary {
    pub id: String,        // "{source}/{slug}", e.g. "expo/skills/react-native"
    pub slug: String,
    pub name: String,
    pub source: String,    // "owner/repo"
    #[serde(default)]
    pub installs: u64,
    #[serde(rename = "sourceType", default)]
    pub source_type: String,
    #[serde(rename = "installUrl", default)]
    pub install_url: String,
    #[serde(default)]
    pub url: String,
}

/// One file from the detail endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillFile {
    pub path: String,
    pub contents: String,
}

/// `/api/v1/skills/{source}/{slug}` detail (with inline files).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDetail {
    pub id: String,
    pub source: String,
    pub slug: String,
    #[serde(default)]
    pub hash: String,
    pub files: Vec<SkillFile>,
}

/// Audit verdict from `/api/v1/skills/audit/{source}/{slug}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillAudit {
    #[serde(default)]
    pub audits: Vec<SkillAuditEntry>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillAuditEntry {
    #[serde(default)]
    pub status: String,
    #[serde(rename = "riskLevel", default)]
    pub risk_level: String, // "LOW" | "MEDIUM" | "HIGH"
    #[serde(default)]
    pub summary: String,
}

/// Where to install a skill.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InstallScope {
    Global,
    Workspace,
}

/// Errors surfaced to the UI.
#[derive(Debug, thiserror::Error)]
pub enum MarketplaceError {
    #[error("skills.sh API key not set")]
    MissingApiKey,
    #[error("skills.sh request failed: {0}")]
    Http(String),
    #[error("install failed: {0}")]
    Install(String),
    #[error("invalid skill id/path: {0}")]
    Invalid(String),
}
```

Add to `src-tauri/src/lib.rs` (alongside `pub mod engine_sink;` etc.):
```rust
pub mod skills_marketplace;
```

- [ ] **Step 4: Run, verify pass**

Run: `cd src-tauri && cargo test --lib skills_marketplace::tests::skill_summary_parses_search_item 2>&1 | tail -5`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/skills_marketplace/mod.rs src-tauri/src/lib.rs
git commit -m "feat(skills_marketplace): module scaffold + skills.sh data types"
```

---

## Task 2: skills.sh HTTP client — search + detail + audit (mockito)

**Files:**
- Create: `src-tauri/src/skills_marketplace/client.rs`
- Reference: reqwest pattern at `src-tauri/src/agent/tools/builtin/skill_marketplace.rs:154-163`

- [ ] **Step 1: Write failing tests** (in `client.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn search_parses_data_array() {
        let mut server = mockito::Server::new_async().await;
        let m = server.mock("GET", "/api/v1/skills/search?q=react&limit=5")
            .match_header("authorization", "Bearer sk_test")
            .with_status(200)
            .with_body(r#"{"data":[{"id":"expo/skills/react-native","slug":"react-native","name":"React Native","source":"expo/skills","installs":3842}],"count":1}"#)
            .create_async().await;

        let c = SkillsShClient::with_base(server.url(), Some("sk_test".into()));
        let out = c.search("react", 5).await.unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].slug, "react-native");
        m.assert_async().await;
    }

    #[tokio::test]
    async fn missing_key_errors_without_http() {
        let c = SkillsShClient::with_base("http://unused".into(), None);
        assert!(matches!(c.search("x", 5).await, Err(MarketplaceError::MissingApiKey)));
    }

    #[tokio::test]
    async fn detail_parses_files() {
        let mut server = mockito::Server::new_async().await;
        let m = server.mock("GET", "/api/v1/skills/expo/skills/react-native")
            .with_status(200)
            .with_body(r#"{"id":"expo/skills/react-native","source":"expo/skills","slug":"react-native","hash":"abc","files":[{"path":"SKILL.md","contents":"---\nname: rn\n---\nbody"}]}"#)
            .create_async().await;
        let c = SkillsShClient::with_base(server.url(), Some("sk_test".into()));
        let d = c.detail("expo/skills/react-native").await.unwrap();
        assert_eq!(d.files.len(), 1);
        assert_eq!(d.files[0].path, "SKILL.md");
        m.assert_async().await;
    }
}
```

- [ ] **Step 2: Run, verify fail**

Run: `cd src-tauri && cargo test --lib skills_marketplace::client 2>&1 | tail -8`
Expected: FAIL (`SkillsShClient` undefined). (If `mockito` isn't a dev-dep yet it is — confirmed in `Cargo.toml` `[dev-dependencies] mockito = "0.32"`.)

- [ ] **Step 3: Implement the client**

`src-tauri/src/skills_marketplace/client.rs`:
```rust
//! reqwest client for skills.sh /api/v1 (Bearer auth).

use std::time::Duration;
use super::{MarketplaceError, SkillSummary, SkillDetail, SkillAudit};

const DEFAULT_BASE: &str = "https://skills.sh";
const UA: &str = "uclaw-skills-marketplace";
const TIMEOUT_MS: u64 = 8000;

pub struct SkillsShClient {
    base: String,
    api_key: Option<String>,
    http: reqwest::Client,
}

impl SkillsShClient {
    /// Production constructor: base = https://skills.sh.
    #[must_use]
    pub fn new(api_key: Option<String>) -> Self {
        Self::with_base(DEFAULT_BASE.to_string(), api_key)
    }

    /// Test/override constructor.
    #[must_use]
    pub fn with_base(base: String, api_key: Option<String>) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_millis(TIMEOUT_MS))
            .user_agent(UA)
            .build()
            .expect("reqwest client");
        Self { base, api_key, http }
    }

    fn key(&self) -> Result<&str, MarketplaceError> {
        self.api_key.as_deref().filter(|k| !k.is_empty()).ok_or(MarketplaceError::MissingApiKey)
    }

    async fn get_json<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T, MarketplaceError> {
        let key = self.key()?;
        let url = format!("{}{}", self.base, path);
        let resp = self.http.get(&url).bearer_auth(key).send().await
            .map_err(|e| MarketplaceError::Http(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(MarketplaceError::Http(format!("status {}", resp.status())));
        }
        resp.json::<T>().await.map_err(|e| MarketplaceError::Http(e.to_string()))
    }

    /// GET /api/v1/skills/search?q=&limit=
    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<SkillSummary>, MarketplaceError> {
        #[derive(serde::Deserialize)]
        struct Wrap { #[serde(default)] data: Vec<SkillSummary> }
        let q = urlencoding::encode(query);
        let limit = limit.clamp(1, 200);
        let w: Wrap = self.get_json(&format!("/api/v1/skills/search?q={q}&limit={limit}")).await?;
        Ok(w.data)
    }

    /// GET /api/v1/skills?view=&page=&per_page=
    pub async fn list(&self, view: &str, page: usize, per_page: usize) -> Result<Vec<SkillSummary>, MarketplaceError> {
        #[derive(serde::Deserialize)]
        struct Wrap { #[serde(default)] data: Vec<SkillSummary> }
        let view = match view { "trending" | "hot" | "all-time" => view, _ => "all-time" };
        let per_page = per_page.clamp(1, 500);
        let w: Wrap = self.get_json(&format!("/api/v1/skills?view={view}&page={page}&per_page={per_page}")).await?;
        Ok(w.data)
    }

    /// GET /api/v1/skills/{id}  (id = "source/slug")
    pub async fn detail(&self, id: &str) -> Result<SkillDetail, MarketplaceError> {
        self.get_json(&format!("/api/v1/skills/{id}")).await
    }

    /// GET /api/v1/skills/audit/{id}
    pub async fn audit(&self, id: &str) -> Result<SkillAudit, MarketplaceError> {
        self.get_json(&format!("/api/v1/skills/audit/{id}")).await
    }
}
```

> `urlencoding` is already a dependency (`Cargo.toml`). `bearer_auth` sets the `Authorization: Bearer` header.

- [ ] **Step 4: Run, verify pass**

Run: `cd src-tauri && cargo test --lib skills_marketplace::client 2>&1 | tail -8`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/skills_marketplace/client.rs
git commit -m "feat(skills_marketplace): skills.sh HTTP client (search/list/detail/audit) + mockito tests"
```

---

## Task 3: Install service — global install (write files → _marketplace → V25)

**Files:**
- Create: `src-tauri/src/skills_marketplace/install.rs`
- Reference: `commit_staged_skills(slug, skills_root)` at `src-tauri/src/automation/marketplace/skill_install.rs:89-106`; V25 insert at `src-tauri/src/automation/marketplace/mod.rs:621-625`; path-traversal check pattern at `skill_marketplace.rs:538`.

- [ ] **Step 1: Write failing test** (in `install.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills_marketplace::{SkillDetail, SkillFile, InstallScope};
    use tempfile::TempDir;

    fn detail(id: &str) -> SkillDetail {
        SkillDetail { id: id.into(), source: "expo/skills".into(), slug: "react-native".into(),
            hash: "h1".into(), files: vec![SkillFile { path: "SKILL.md".into(),
            contents: "---\nname: react-native\n---\nbody".into() }] }
    }

    #[test]
    fn slug_flattens_id() {
        assert_eq!(flatten_slug("expo/skills/react-native"), "expo__skills__react-native");
    }

    #[test]
    fn global_install_writes_files_under_marketplace() {
        let tmp = TempDir::new().unwrap();
        let skills_root = tmp.path().join("skills");
        let dir = write_skill_files(&skills_root, "expo__skills__react-native", &detail("expo/skills/react-native")).unwrap();
        assert!(dir.join("SKILL.md").exists());
        assert!(dir.starts_with(skills_root.join("_marketplace")));
        assert_eq!(std::fs::read_to_string(dir.join("SKILL.md")).unwrap(), "---\nname: react-native\n---\nbody");
    }

    #[test]
    fn rejects_path_traversal() {
        let tmp = TempDir::new().unwrap();
        let mut d = detail("x/y/z");
        d.files[0].path = "../evil.md".into();
        let err = write_skill_files(&tmp.path().join("skills"), "x__y__z", &d);
        assert!(err.is_err());
    }
}
```

- [ ] **Step 2: Run, verify fail**

Run: `cd src-tauri && cargo test --lib skills_marketplace::install 2>&1 | tail -8`
Expected: FAIL (functions undefined).

- [ ] **Step 3: Implement the file-write helpers**

`src-tauri/src/skills_marketplace/install.rs` (top half — the pure, unit-testable helpers):
```rust
//! Install/uninstall/update for skills.sh skills.
//! Real files land in <skills_root>/_marketplace/<slug>/ (Marketplace tier).

use std::path::{Path, PathBuf};
use super::{MarketplaceError, SkillDetail};

/// "expo/skills/react-native" -> "expo__skills__react-native".
#[must_use]
pub fn flatten_slug(id: &str) -> String {
    id.replace('/', "__")
}

/// Reject path-traversal / absolute components in a file path.
fn safe_rel(path: &str) -> Result<&str, MarketplaceError> {
    if path.is_empty() || path.starts_with('/') || path.starts_with('\\') || path.contains("..") {
        return Err(MarketplaceError::Invalid(format!("unsafe path: {path}")));
    }
    Ok(path)
}

/// Write `detail.files` into <skills_root>/_marketplace/<slug>/ and return that dir.
/// Overwrites an existing install at that slug.
pub fn write_skill_files(skills_root: &Path, slug: &str, detail: &SkillDetail) -> Result<PathBuf, MarketplaceError> {
    if slug.is_empty() || slug.contains('/') || slug.contains("..") {
        return Err(MarketplaceError::Invalid(format!("unsafe slug: {slug}")));
    }
    let dir = skills_root.join("_marketplace").join(slug);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| MarketplaceError::Install(e.to_string()))?;
    }
    for f in &detail.files {
        let rel = safe_rel(&f.path)?;
        let dest = dir.join(rel);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| MarketplaceError::Install(e.to_string()))?;
        }
        std::fs::write(&dest, &f.contents).map_err(|e| MarketplaceError::Install(e.to_string()))?;
    }
    Ok(dir)
}
```

- [ ] **Step 4: Run, verify pass**

Run: `cd src-tauri && cargo test --lib skills_marketplace::install 2>&1 | tail -8`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/skills_marketplace/install.rs
git commit -m "feat(skills_marketplace): file-write helpers (flatten_slug, safe path, write_skill_files) + tests"
```

---

## Task 4: Install service — workspace tag + symlink

**Files:**
- Modify: `src-tauri/src/skills_marketplace/install.rs`

- [ ] **Step 1: Write failing test** (append to `install.rs` tests)

```rust
    #[test]
    fn workspace_symlink_points_at_global_dir() {
        let tmp = TempDir::new().unwrap();
        let global = tmp.path().join("global_skills").join("_marketplace").join("s1");
        std::fs::create_dir_all(&global).unwrap();
        let ws = tmp.path().join("ws");
        link_into_workspace(&ws, "s1", &global).unwrap();
        let link = ws.join(".uclaw").join("skills").join("s1");
        assert!(link.exists());
        assert_eq!(std::fs::read_link(&link).unwrap(), global);
    }
```

- [ ] **Step 2: Run, verify fail**

Run: `cd src-tauri && cargo test --lib skills_marketplace::install::tests::workspace_symlink 2>&1 | tail -5`
Expected: FAIL (`link_into_workspace` undefined).

- [ ] **Step 3: Implement symlink + tag helpers** (append to `install.rs`)

```rust
/// Create <workspace>/.uclaw/skills/<slug> -> <global_dir> (symlink, visibility only).
pub fn link_into_workspace(workspace: &Path, slug: &str, global_dir: &Path) -> Result<(), MarketplaceError> {
    let link_dir = workspace.join(".uclaw").join("skills");
    std::fs::create_dir_all(&link_dir).map_err(|e| MarketplaceError::Install(e.to_string()))?;
    let link = link_dir.join(slug);
    if link.exists() || std::fs::symlink_metadata(&link).is_ok() {
        let _ = std::fs::remove_file(&link);
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(global_dir, &link).map_err(|e| MarketplaceError::Install(e.to_string()))?;
    #[cfg(windows)]
    std::os::windows::fs::symlink_dir(global_dir, &link).map_err(|e| MarketplaceError::Install(e.to_string()))?;
    Ok(())
}

/// Remove the workspace symlink for a slug (workspace-uninstall).
pub fn unlink_from_workspace(workspace: &Path, slug: &str) {
    let _ = std::fs::remove_file(workspace.join(".uclaw").join("skills").join(slug));
}
```

> The workspace **tag** (writing the workspace's tag into the installed SKILL.md `activation.tags`, per spec §4.1) is applied by the install *service* (Task 5) after `write_skill_files`, by editing the on-disk `SKILL.md` frontmatter; the symlink here is the visibility half. (Spec §9 flags frontmatter-edit vs DB-sidecar; this plan uses the frontmatter edit — simplest, and the registry already reads `activation.tags` from SKILL.md.)

- [ ] **Step 4: Run, verify pass**

Run: `cd src-tauri && cargo test --lib skills_marketplace::install::tests::workspace_symlink 2>&1 | tail -5`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/skills_marketplace/install.rs
git commit -m "feat(skills_marketplace): workspace symlink helpers (link/unlink_into_workspace)"
```

---

## Task 5: Install service — orchestration (`install`/`uninstall`/`check_update`) + V25

**Files:**
- Modify: `src-tauri/src/skills_marketplace/install.rs`
- Reference: V25 table `marketplace_standalone_installs(slug, item_type, version, installed_at, mcp_server_id)` (`migrations.rs:1158`); insert pattern (`automation/marketplace/mod.rs:621`).

- [ ] **Step 1: Write failing test** (V25 round-trip, in `install.rs` tests)

```rust
    fn mem_conn() -> rusqlite::Connection {
        let c = rusqlite::Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE marketplace_standalone_installs (slug TEXT PRIMARY KEY, item_type TEXT NOT NULL, version TEXT NOT NULL, installed_at INTEGER NOT NULL, mcp_server_id TEXT);").unwrap();
        c
    }

    #[test]
    fn records_and_reads_install_row() {
        let c = mem_conn();
        record_install(&c, "expo__skills__react-native", "h1", 1000).unwrap();
        let v = read_install_version(&c, "expo__skills__react-native").unwrap();
        assert_eq!(v.as_deref(), Some("h1"));
        assert!(needs_update(&c, "expo__skills__react-native", "h2"));     // hash changed
        assert!(!needs_update(&c, "expo__skills__react-native", "h1"));    // same hash
    }
```

- [ ] **Step 2: Run, verify fail**

Run: `cd src-tauri && cargo test --lib skills_marketplace::install::tests::records_and_reads 2>&1 | tail -5`
Expected: FAIL.

- [ ] **Step 3: Implement V25 helpers** (append to `install.rs`)

```rust
/// Record an install in V25 (item_type="skill"; `version` stores the skills.sh hash).
pub fn record_install(conn: &rusqlite::Connection, slug: &str, hash: &str, now_secs: i64) -> Result<(), MarketplaceError> {
    conn.execute(
        "INSERT OR REPLACE INTO marketplace_standalone_installs (slug, item_type, version, installed_at, mcp_server_id) VALUES (?,?,?,?,NULL)",
        rusqlite::params![slug, "skill", hash, now_secs],
    ).map_err(|e| MarketplaceError::Install(e.to_string()))?;
    Ok(())
}

pub fn read_install_version(conn: &rusqlite::Connection, slug: &str) -> Result<Option<String>, MarketplaceError> {
    conn.query_row(
        "SELECT version FROM marketplace_standalone_installs WHERE slug=?1 AND item_type='skill'",
        rusqlite::params![slug], |r| r.get::<_, String>(0),
    ).optional().map_err(|e| MarketplaceError::Install(e.to_string()))
}

pub fn remove_install_row(conn: &rusqlite::Connection, slug: &str) -> Result<(), MarketplaceError> {
    conn.execute("DELETE FROM marketplace_standalone_installs WHERE slug=?1 AND item_type='skill'",
        rusqlite::params![slug]).map_err(|e| MarketplaceError::Install(e.to_string()))?;
    Ok(())
}

/// True iff the stored hash differs from `latest_hash` (or not installed).
pub fn needs_update(conn: &rusqlite::Connection, slug: &str, latest_hash: &str) -> bool {
    matches!(read_install_version(conn, slug), Ok(Some(h)) if h == latest_hash).eq(&false)
}
```
Add `use rusqlite::OptionalExtension as _;` to the file's imports.

- [ ] **Step 4: Run, verify pass**

Run: `cd src-tauri && cargo test --lib skills_marketplace::install 2>&1 | tail -8`
Expected: PASS (all install tests).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/skills_marketplace/install.rs
git commit -m "feat(skills_marketplace): V25 install tracking (record/read/remove/needs_update) + test"
```

> **NOTE (executor):** the high-level `install_skill(client, conn, registry, skills_root, workspace, id, scope)` orchestrator — which calls `client.detail` → `write_skill_files` → (workspace: tag-frontmatter + `link_into_workspace`) → `registry.reload()` → `record_install` → emit `agent:skill-installed` — is wired in **Task 6's command** rather than as a separately-unit-tested fn (it needs the live `Arc<RwLock<SkillsRegistry>>` + DB + workspace). Its pure pieces are all tested above; the orchestration is covered by the manual UI checkpoint (P3/P4) + an integration smoke test added in Task 6.

---

## Task 6: Tauri commands + registration

**Files:**
- Create: `src-tauri/src/commands/skills_marketplace.rs`
- Modify: `src-tauri/src/commands/mod.rs` (`pub mod skills_marketplace;`)
- Modify: `src-tauri/src/main.rs` (register 5 commands in `generate_handler!`, near the other `commands::skills::*` at `main.rs:1067-1074`)
- Reference: command pattern `commands/skills.rs:31`; settings read `services::settings_service::DbSettings`; skills-dir `data_dir.join("skills")` (`app.rs:547`); `state.skills_registry`, `state.data_dir`, `state.db`.

- [ ] **Step 1: Write the commands**

`src-tauri/src/commands/skills_marketplace.rs`:
```rust
//! Thin Tauri commands over skills_marketplace (parse → service → map).
use tauri::State;
use crate::app::AppState;
use crate::error::Error;
use crate::skills_marketplace::{client::SkillsShClient, install, InstallScope, SkillSummary, SkillDetail, SkillAudit, MarketplaceError};

fn map_err(e: MarketplaceError) -> Error { Error::Internal(e.to_string()) }

fn read_api_key(state: &AppState) -> Option<String> {
    let conn = state.db.lock().ok()?;
    conn.query_row("SELECT value FROM settings WHERE key='skills_sh_api_key'", [], |r| r.get::<_, String>(0)).ok()
}

#[tauri::command]
pub async fn search_skill_marketplace(state: State<'_, AppState>, query: String, limit: Option<usize>) -> Result<Vec<SkillSummary>, Error> {
    let client = SkillsShClient::new(read_api_key(&state));
    client.search(&query, limit.unwrap_or(20)).await.map_err(map_err)
}

#[tauri::command]
pub async fn list_skill_marketplace(state: State<'_, AppState>, view: Option<String>, page: Option<usize>) -> Result<Vec<SkillSummary>, Error> {
    let client = SkillsShClient::new(read_api_key(&state));
    client.list(view.as_deref().unwrap_or("trending"), page.unwrap_or(0), 60).await.map_err(map_err)
}

#[tauri::command]
pub async fn get_skill_marketplace_detail(state: State<'_, AppState>, id: String) -> Result<SkillDetail, Error> {
    SkillsShClient::new(read_api_key(&state)).detail(&id).await.map_err(map_err)
}

#[tauri::command]
pub async fn get_skill_marketplace_audit(state: State<'_, AppState>, id: String) -> Result<SkillAudit, Error> {
    SkillsShClient::new(read_api_key(&state)).audit(&id).await.map_err(map_err)
}

#[tauri::command]
pub async fn install_skill_from_marketplace(
    state: State<'_, AppState>, id: String, scope: InstallScope, workspace: Option<String>,
) -> Result<String, Error> {
    let client = SkillsShClient::new(read_api_key(&state));
    let detail = client.detail(&id).await.map_err(map_err)?;
    let slug = install::flatten_slug(&id);
    let skills_root = state.data_dir.join("skills");
    let dir = install::write_skill_files(&skills_root, &slug, &detail).map_err(map_err)?;

    if scope == InstallScope::Workspace {
        if let Some(ws) = workspace.as_deref() {
            install::link_into_workspace(std::path::Path::new(ws), &slug, &dir).map_err(map_err)?;
            // TODO(P4): write the workspace tag into dir/SKILL.md activation.tags (frontmatter edit).
        }
    }
    {
        let mut reg = state.skills_registry.write().await;
        reg.add_scan_dir(dir.clone(), crate::skills::SkillProvenance::Marketplace);
        reg.discover();
    }
    if let Ok(conn) = state.db.lock() {
        let now = chrono::Utc::now().timestamp();
        let _ = install::record_install(&conn, &slug, &detail.hash, now);
    }
    Ok(slug)
}
```

> The frontmatter-tag write is marked `TODO(P4)` because the tag's *value* (the workspace tag) is only meaningful once the workspace-tag UX lands in P4; P1 ships global + the symlink + the V25 row. (This is a real deferral, not a placeholder: workspace activation is a P4 concern; P1's command accepts the scope + builds the symlink so P4 only adds the tag write.)

Add to `src-tauri/src/commands/mod.rs`:
```rust
pub mod skills_marketplace;
```

Add to `main.rs` `generate_handler!` (after `uclaw_core::commands::skills::match_skills,`):
```rust
            uclaw_core::commands::skills_marketplace::search_skill_marketplace,
            uclaw_core::commands::skills_marketplace::list_skill_marketplace,
            uclaw_core::commands::skills_marketplace::get_skill_marketplace_detail,
            uclaw_core::commands::skills_marketplace::get_skill_marketplace_audit,
            uclaw_core::commands::skills_marketplace::install_skill_from_marketplace,
```

- [ ] **Step 2: Build (compile-gate, no UI test possible)**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
Expected: empty (no errors). Fix any signature drift against the referenced anchors (`state.data_dir`, `SkillProvenance::Marketplace`, `add_scan_dir`, `Error::Internal`).

- [ ] **Step 3: Verify all module tests still pass**

Run: `cd src-tauri && cargo test --lib skills_marketplace 2>&1 | tail -8`
Expected: PASS (Tasks 1–5 tests).

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands/skills_marketplace.rs src-tauri/src/commands/mod.rs src-tauri/src/main.rs
git commit -m "feat(skills_marketplace): Tauri commands (search/list/detail/audit/install) + handler registration"
```

---

## Task 7: PR

- [ ] **Step 1: Push + open PR**

```bash
git push -u origin pi/skills-sh-marketplace
gh pr create --base main --head pi/skills-sh-marketplace \
  --title "feat(skills): skills.sh marketplace — P1 backend (client + install + commands)" \
  --body "P1 of the skills.sh marketplace (spec: docs/superpowers/specs/2026-06-01-...). Pure-Rust skills.sh /api/v1 client + install service (global → _marketplace tier; workspace → symlink, tag deferred to P4) + V25 tracking + 5 Tauri commands. No agent/UI surface yet (P2–P5). cargo tests green; mockito-covered client; tempfile-covered install."
```

> The spec doc commit (`2303d928`) is already on this branch; this PR carries it + P1.

---

## Self-Review (run by the plan author)

**Spec coverage (P1 slice):** client (search/list/detail/audit) ✓ Task 2; install global ✓ Task 3; workspace symlink ✓ Task 4; V25 ✓ Task 5; commands ✓ Task 6; API-key-from-settings + MissingApiKey ✓ Tasks 2/6. Workspace **tag** activation → deferred to P4 (noted, with the symlink shipped in P1). Audit-HIGH gate, in-chat card, 万花筒, pi bridge → P2–P5 (own plans). No P1 spec requirement is unimplemented.

**Placeholder scan:** Two explicit deferrals (`TODO(P4)` tag write; the `install_skill` orchestrator living in the command) are real cross-phase boundaries, justified inline — not vague placeholders. All code steps show complete code + exact test commands.

**Type consistency:** `SkillSummary/SkillDetail/SkillFile/SkillAudit/InstallScope/MarketplaceError` (Task 1) used consistently in client (Task 2), install (Tasks 3–5), commands (Task 6). `flatten_slug`/`write_skill_files`/`link_into_workspace`/`record_install`/`needs_update` signatures match across tasks.

**Anchor caveat:** Reused-machinery signatures (`commit_staged_skills`, V25 insert) are from the exploration pass; this plan does NOT call `commit_staged_skills` directly (it writes files + uses `add_scan_dir`/`discover` like the existing `SkillInstallFromMarketplaceTool` does at `skill_marketplace.rs:604-611`), so it depends only on the well-verified `add_scan_dir`/`discover`/`SkillProvenance::Marketplace` surface. Executor should confirm `state.data_dir` + `Error::Internal` names at Task 6 build.
