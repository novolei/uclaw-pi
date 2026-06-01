# MemUAdapter (piece 3a) — Implementation Plan

> REQUIRED SUB-SKILL: executing-plans / subagent-driven-development. Steps use `- [ ]`.

**Goal:** `MemUAdapter` over `MemUClient`, registered under `"memu"` when memU is available. Mapping per `docs/superpowers/specs/2026-06-01-memu-adapter-design.md`.

**Key paths:** `crate::memu::client::MemUClient`, `crate::memory::EnrichedMemoryItem`, `super::{traits::MemoryAdapter, types::{MemoryCategory, MemoryEntry, NamespaceSummary, RecallOpts}}`. Reference impl: `memory_adapter/legacy_kv.rs`.

---

### Task 1: `memu_adapter.rs` — pure mapping fns + trait impl + unit tests

**Files:** Create `src-tauri/src/memory_adapter/memu_adapter.rs`; modify `src-tauri/src/memory_adapter/mod.rs` (`mod memu_adapter; pub use memu_adapter::MemUAdapter;`).

- [ ] **Step 1: Pure free functions** (these are the unit-tested core, no client needed):
  - `fn category_to_memory_type(cat: &MemoryCategory) -> String` — Core→`"knowledge"`, Conversation→`"event"`, Daily→`"event"`, Custom(s)→`s.clone()`.
  - `fn memory_type_to_category(mt: &str) -> MemoryCategory` — `"knowledge"|"profile"`→Core, `"event"`→Conversation, else Custom(mt).
  - `fn user_scope(ns: Option<&str>) -> Option<serde_json::Value>` — `ns.map(|n| json!({"user_id": n}))`.
  - `fn enriched_to_entry(item: EnrichedMemoryItem, ns: Option<&str>, sid: Option<&str>) -> MemoryEntry`:
    - `id`: `item.metadata.get("id").and_then(|v| v.as_str()).unwrap_or_default().to_string()`
    - `key: String::new()`, `content: item.content`, `namespace: ns.map(String::from)`,
    - `category: memory_type_to_category(&item.memory_type)`, `timestamp: item.created_at.unwrap_or_default()`,
    - `session_id: sid.map(String::from)`, `score: Some(item.relevance_score)`.
  - `fn value_to_entry(v: &serde_json::Value, ns: Option<&str>, sid: Option<&str>) -> MemoryEntry` — defensive: `content` from `"content"|"memory_content"`; `id` from `"id"`; `memory_type` from `"memory_type"` (→category); `created_at`→timestamp; `score: None`; `key: ""`.
- [ ] **Step 2: Struct + trait impl**:
```rust
pub struct MemUAdapter { client: Arc<crate::memu::client::MemUClient> }
impl MemUAdapter { pub fn new(client: Arc<crate::memu::client::MemUClient>) -> Self { Self { client } } }

#[async_trait]
impl MemoryAdapter for MemUAdapter {
    fn name(&self) -> &str { "memu" }

    async fn recall(&self, query: &str, limit: usize, opts: RecallOpts<'_>) -> anyhow::Result<Vec<MemoryEntry>> {
        let items = self.client
            .retrieve_with_context(query, None, limit, false)
            .await
            .map_err(|e| anyhow::anyhow!("memu recall: {}", e))?;
        Ok(items.into_iter().map(|it| enriched_to_entry(it, opts.namespace, opts.session_id)).collect())
    }

    async fn store(&self, namespace: &str, _key: &str, content: &str, category: MemoryCategory, _session_id: Option<&str>) -> anyhow::Result<()> {
        let mt = category_to_memory_type(&category);
        self.client
            .create_item(&mt, content, vec![category.to_string()], user_scope(Some(namespace)))
            .await
            .map(|_| ())
            .map_err(|e| anyhow::anyhow!("memu store: {}", e))
    }

    async fn list(&self, namespace: Option<&str>, category: Option<&MemoryCategory>, session_id: Option<&str>) -> anyhow::Result<Vec<MemoryEntry>> {
        let mt = category.map(category_to_memory_type);
        let res = self.client
            .list_items(None, mt.as_deref(), Some(50), Some(0), user_scope(namespace))
            .await
            .map_err(|e| anyhow::anyhow!("memu list: {}", e))?;
        Ok(res.items.iter().map(|v| value_to_entry(v, namespace, session_id)).collect())
    }

    async fn get(&self, _namespace: &str, _key: &str) -> anyhow::Result<Option<MemoryEntry>> {
        tracing::debug!("memu: get-by-key unsupported (semantic store, no stable key)");
        Ok(None)
    }
    async fn delete(&self, _namespace: &str, _key: &str) -> anyhow::Result<bool> {
        tracing::debug!("memu: delete-by-key unsupported (memU deletes by item id)");
        Ok(false)
    }
    async fn clear_namespace(&self, _namespace: &str) -> anyhow::Result<u64> {
        tracing::debug!("memu: clear_namespace unsupported (no bulk delete by scope)");
        Ok(0)
    }
    async fn namespace_summaries(&self) -> anyhow::Result<Vec<NamespaceSummary>> {
        Ok(Vec::new())
    }
}
```
- [ ] **Step 3: Unit tests** (pure fns only — no live memU):
  - `category_round_trips`: Core→"knowledge"→Core; Conversation→"event"→Conversation; Custom("x")→"x"→Custom("x").
  - `enriched_maps_fields`: build `EnrichedMemoryItem { content:"hi", memory_type:"knowledge", relevance_score:0.7, categories:vec![], metadata: json!({"id":"m1"}), created_at: Some("2026-01-01T00:00:00Z".into()) }`; assert entry `{id:"m1", content:"hi", category:Core, score:Some(0.7), timestamp:"2026-..."}`, namespace/session from args.
  - `value_to_entry_defensive`: `json!({"content":"c","id":"x","memory_type":"event"})` → entry `{content:"c", id:"x", category:Conversation}`; and an empty `json!({})` → no panic, content "".
- [ ] **Step 4:** `cargo test --manifest-path src-tauri/Cargo.toml --lib memory_adapter::memu_adapter 2>&1 | tail -10` → pass. (Note: `recall/store/list` are integration-only — not unit-tested.)
- [ ] **Step 5:** Commit `git add src-tauri/src/memory_adapter/memu_adapter.rs src-tauri/src/memory_adapter/mod.rs && git commit -m "feat(memory_adapter): MemUAdapter — recall via memU retrieve (minimal KV-only ops)"`

---

### Task 2: Register under `"memu"` in AppState (conditional)

**Files:** `src-tauri/src/app.rs` (between the bucket_seal insert ≈1014 and `let memory_adapters = ...` ≈1015).

- [ ] **Step 1:** Insert:
```rust
        if let Some(ref memu) = memu_client {
            let memu_adapter = std::sync::Arc::new(crate::memory_adapter::MemUAdapter::new(memu.clone()))
                as std::sync::Arc<dyn crate::memory_adapter::MemoryAdapter>;
            memory_adapters_map.insert(memu_adapter.name().to_string(), memu_adapter);
        }
```
(verify `memu_client` is the in-scope local at that point — it is, used in the struct literal below).
- [ ] **Step 2:** `cargo build --manifest-path src-tauri/Cargo.toml --lib 2>&1 | grep -E "^error" | head` → zero errors.
- [ ] **Step 3:** Commit `git add src-tauri/src/app.rs && git commit -m "feat(app): register MemUAdapter under \"memu\" when memU is available"`

---

### Task 3: Verification

- [ ] `cargo test --manifest-path src-tauri/Cargo.toml --lib memory_adapter` → all pass (incl. the new memu_adapter unit tests + existing router/legacy tests).
- [ ] Full `cargo test --manifest-path src-tauri/Cargo.toml --lib 2>&1 | tail -8` → no NEW failures vs the 5 environmental baseline.
- [ ] `cargo clippy --manifest-path src-tauri/Cargo.toml --lib 2>&1 | grep memu_adapter` → no hits.
- [ ] `git diff main -- src-tauri/Cargo.toml` empty.
- [ ] Note in the PR: `recall`/`store`/`list` are integration-only (need a live memU); set `prompt_recall_backend="memu"` to use it as the prompt's semantic recall source.

---

## Self-Review
- Spec coverage: all 8 methods + the pure mapping fns + conditional registration covered.
- No placeholders: concrete code; the integration-only methods are explicitly out of unit-test scope (documented, not faked).
- Off/additive: registers only when memU present; default backend unchanged; no other call site touched.
