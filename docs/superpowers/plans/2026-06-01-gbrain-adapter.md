# GbrainAdapter (piece 3b) — Implementation Plan

> REQUIRED SUB-SKILL: executing-plans. Steps use `- [ ]`. Mapping per `docs/superpowers/specs/2026-06-01-gbrain-adapter-design.md`.

**Paths:** `crate::gbrain::browse::{search, put_page, list_pages, get_page, SearchHit, PageSummary, PageDetail, GbrainError}`, `crate::mcp::SharedMcpManager`, `super::{traits::MemoryAdapter, types::{MemoryCategory, MemoryEntry, NamespaceSummary, RecallOpts}}`. Reference: `memory_adapter/memu_adapter.rs`.

**Signatures (verified):**
- `search(&SharedMcpManager, query:&str, limit:u32, offset:u32) -> Result<Vec<SearchHit>, GbrainError>`
- `put_page(&SharedMcpManager, slug:&str, content:&str) -> Result<(), GbrainError>`
- `list_pages(&SharedMcpManager, limit:u32, sort:Option<String>, page_type:Option<String>, tag:Option<String>, updated_after:Option<String>) -> Result<Vec<PageSummary>, GbrainError>`
- `get_page(&SharedMcpManager, slug:&str) -> Result<PageDetail, GbrainError>` (fuzzy)
- `SearchHit{slug,title,snippet,similarity:f64}` · `PageSummary{slug,title,page_type,updated_at:Option<String>}` · `PageDetail{slug,title,page_type,compiled_truth,…,updated_at:Option<String>,…}`

---

### Task 1: `gbrain_adapter.rs` — pure fns + trait impl + unit tests

**Files:** create `src-tauri/src/memory_adapter/gbrain_adapter.rs`; modify `mod.rs` (`mod gbrain_adapter; pub use gbrain_adapter::GbrainAdapter;`).

- [ ] **Step 1: Pure fns** (the unit-tested core):
  - `fn slugify(ns: &str, key: &str) -> String` — `format!("{ns}-{key}")`, lowercase, map each char to itself if `ascii_alphanumeric` else `'-'`, collapse consecutive `-`, trim leading/trailing `-`.
  - `fn hit_to_entry(h: SearchHit, ns: Option<&str>, sid: Option<&str>) -> MemoryEntry` — id/key=`h.slug`(clone), content=`h.snippet`, namespace=ns, category=Core, timestamp=`""`, session_id=sid, score=`Some(h.similarity)`.
  - `fn summary_to_entry(p: PageSummary, ns: Option<&str>, sid: Option<&str>) -> MemoryEntry` — id/key=`p.slug`, content=`p.title`, timestamp=`p.updated_at.unwrap_or_default()`, category=Core, score=None.
  - `fn detail_to_entry(d: PageDetail, ns: Option<&str>, sid: Option<&str>) -> MemoryEntry` — id/key=`d.slug`, content=`d.compiled_truth`, timestamp=`d.updated_at.unwrap_or_default()`, category=Core, score=None.
- [ ] **Step 2: Struct + impl:**
```rust
pub struct GbrainAdapter { mcp: crate::mcp::SharedMcpManager }
impl GbrainAdapter { pub fn new(mcp: crate::mcp::SharedMcpManager) -> Self { Self { mcp } } }

#[async_trait]
impl MemoryAdapter for GbrainAdapter {
    fn name(&self) -> &str { "gbrain" }

    async fn recall(&self, query: &str, limit: usize, opts: RecallOpts<'_>) -> anyhow::Result<Vec<MemoryEntry>> {
        let hits = crate::gbrain::browse::search(&self.mcp, query, limit as u32, 0).await
            .map_err(|e| anyhow::anyhow!("gbrain recall: {}", e.to_command_string()))?;
        Ok(hits.into_iter().map(|h| hit_to_entry(h, opts.namespace, opts.session_id)).collect())
    }
    async fn store(&self, namespace: &str, key: &str, content: &str, _c: MemoryCategory, _s: Option<&str>) -> anyhow::Result<()> {
        crate::gbrain::browse::put_page(&self.mcp, &slugify(namespace, key), content).await
            .map_err(|e| anyhow::anyhow!("gbrain store: {}", e.to_command_string()))
    }
    async fn list(&self, namespace: Option<&str>, _cat: Option<&MemoryCategory>, session_id: Option<&str>) -> anyhow::Result<Vec<MemoryEntry>> {
        let pages = crate::gbrain::browse::list_pages(&self.mcp, 50, None, None, None, None).await
            .map_err(|e| anyhow::anyhow!("gbrain list: {}", e.to_command_string()))?;
        Ok(pages.into_iter().map(|p| summary_to_entry(p, namespace, session_id)).collect())
    }
    async fn get(&self, namespace: &str, key: &str) -> anyhow::Result<Option<MemoryEntry>> {
        match crate::gbrain::browse::get_page(&self.mcp, &slugify(namespace, key)).await {
            Ok(d) => Ok(Some(detail_to_entry(d, Some(namespace), None))),
            Err(e) => { tracing::debug!(error = %e.to_command_string(), "gbrain get: not retrievable"); Ok(None) }
        }
    }
    async fn delete(&self, _n: &str, _k: &str) -> anyhow::Result<bool> { tracing::debug!("gbrain: delete unsupported (no delete tool)"); Ok(false) }
    async fn clear_namespace(&self, _n: &str) -> anyhow::Result<u64> { tracing::debug!("gbrain: clear_namespace unsupported"); Ok(0) }
    async fn namespace_summaries(&self) -> anyhow::Result<Vec<NamespaceSummary>> { Ok(Vec::new()) }
}
```
- [ ] **Step 3: Unit tests** (pure fns): `slugify_kebabs` (`slugify("ns x", "Key/1")` → `"ns-x-key-1"`, no leading/trailing/double `-`); `hit_maps_fields` (slug→id/key, snippet→content, similarity→score, Core); `summary_and_detail_map` (content source + timestamp from updated_at).
- [ ] **Step 4:** `cargo test --manifest-path src-tauri/Cargo.toml --lib memory_adapter::gbrain_adapter` → pass.
- [ ] **Step 5:** Commit `feat(memory_adapter): GbrainAdapter — recall/store/list/get via gbrain::browse`.

---

### Task 2: Register `"gbrain"` (unconditional)

**Files:** `src-tauri/src/app.rs` (beside the other inserts, before `let memory_adapters = ...`).

- [ ] **Step 1:** Insert (mcp_manager is always present; gbrain availability is checked per-call):
```rust
        let gbrain_adapter = std::sync::Arc::new(crate::memory_adapter::GbrainAdapter::new(mcp_manager.clone()))
            as std::sync::Arc<dyn crate::memory_adapter::MemoryAdapter>;
        memory_adapters_map.insert(gbrain_adapter.name().to_string(), gbrain_adapter);
```
(verify the local building memory_adapters is named `mcp_manager` at that scope — it's the field source; use the same handle the AppState struct gets.)
- [ ] **Step 2:** Build → zero errors. Commit `feat(app): register GbrainAdapter under "gbrain" (availability checked per-call)`.

---

### Task 3: Verification
- [ ] `cargo test --lib memory_adapter` → pass. Full `cargo test --lib` → no NEW failures vs baseline.
- [ ] `cargo clippy --lib 2>&1 | grep gbrain_adapter` → clean. `git diff main -- src-tauri/Cargo.toml` empty.
- [ ] PR note: recall/store/list/get integration-only (live gbrain); set `prompt_recall_backend="gbrain"` to use.
