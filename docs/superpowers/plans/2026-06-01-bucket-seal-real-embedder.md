# bucket_seal Real Embedder + Vector Recall — Plan

> Per `docs/superpowers/specs/2026-06-01-bucket-seal-real-embedder-design.md`. Steps `- [ ]`.

**Paths:** `crate::memu::client::MemUClient`, `memory_bucket_seal::score::embed::{Embedder, EMBEDDING_DIM, cosine_similarity, unpack_embedding}`, `mem_tree_summaries` table.

---

### Task 1: `MemUEmbedder` + `EMBEDDING_DIM` 1024→384

**Files:** create `memory_bucket_seal/score/embed/memu.rs`; modify `embed/mod.rs`.

- [ ] **Step 1:** `embed/mod.rs` — change `pub const EMBEDDING_DIM: usize = 1024;` → `384`; update the doc block (provider = memU FastEmbed `bge-small-en-v1.5`, 384-dim; prior 1024 bge-m3 blobs invalid → wipe/re-seal). Fix the test comment "expected EMBEDDING_DIM (1024)" → 384. Add `pub mod memu; pub use memu::MemUEmbedder;`.
- [ ] **Step 2:** `embed/memu.rs`:
```rust
use std::sync::Arc;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use super::{Embedder, EMBEDDING_DIM};
use crate::memu::client::MemUClient;

/// Real embedder backed by memU's FastEmbed (`bge-small-en-v1.5`, 384-dim).
pub struct MemUEmbedder { client: Arc<MemUClient> }
impl MemUEmbedder { pub fn new(client: Arc<MemUClient>) -> Self { Self { client } } }

#[async_trait]
impl Embedder for MemUEmbedder {
    fn name(&self) -> &'static str { "memu_fastembed" }
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let mut vs = self.client.embed_text(&[text]).await
            .map_err(|e| anyhow!("memu embed: {}", e))?;
        let v = vs.pop().ok_or_else(|| anyhow!("memu embed: empty result"))?;
        if v.len() != EMBEDDING_DIM {
            anyhow::bail!("memu embed: {} dims, expected {}", v.len(), EMBEDDING_DIM);
        }
        Ok(v)
    }
}
```
- [ ] **Step 3:** `cargo test --lib memory_bucket_seal::score::embed` → pass (the dim-change auto-adjusts existing tests). Build green.
- [ ] **Step 4:** Commit `feat(memory_bucket_seal): MemUEmbedder (FastEmbed) + EMBEDDING_DIM 384`.

---

### Task 2: vector recall in `BucketSealAdapter::recall`

**Files:** `memory_bucket_seal/adapter.rs`.

- [ ] **Step 1: Pure helper** (module-level, near the other fns):
```rust
/// Rank `(id, content, embedding)` candidates by cosine to `query`, desc; take `limit`.
fn rank_by_cosine(
    query: &[f32],
    cands: Vec<(String, String, Vec<f32>)>,
    limit: usize,
) -> Vec<(String, String, f32)> {
    let mut scored: Vec<(String, String, f32)> = cands
        .into_iter()
        .map(|(id, content, emb)| {
            let s = crate::memory_bucket_seal::score::embed::cosine_similarity(query, &emb);
            (id, content, s)
        })
        .collect();
    scored.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(limit);
    scored
}
```
- [ ] **Step 2: Vector path** — a private async method `recall_vector(&self, query, namespace, limit) -> Vec<MemoryEntry>` that: embeds the query (`self.embedder.embed(query).await`; on `Err` → return `vec![]`), resolves the tree (`get_or_create_source_tree(&self.store, namespace)`), then `SELECT id, content, embedding FROM mem_tree_summaries WHERE tree_id=?1 AND embedding IS NOT NULL AND deleted=0`, decodes each blob via `crate::memory_bucket_seal::score::embed::unpack_embedding` (skip+`tracing::debug!` on per-row decode error), `rank_by_cosine`, maps each to `MemoryEntry { id, key:String::new(), content, namespace:Some(namespace.into()), category:MemoryCategory::Core, timestamp:String::new(), session_id:None, score:Some(cos as f64) }`.
- [ ] **Step 3: Merge into `recall`** — after computing the existing FTS5 `out`, prepend the vector hits: build `let vec_hits = self.recall_vector(&match_query_or_raw, ns, limit).await;` — actually embed the RAW `query` (not the FTS-sanitised one). Then merge: vector hits first, then FTS5 `out`, dedup by `id`, `truncate(limit)`; keep the existing `opts.category` post-filter applying to both. (Vector path only runs when `opts.namespace` is `Some` — needs a tree scope; if `None`, skip vector, FTS5-only.)
- [ ] **Step 4: Tests** — `rank_by_cosine_orders_desc` (3 hand vectors: query close to A, far from B → A first; `limit` truncates). A `recall` integration test with a deterministic non-inert test `Embedder` is optional/`#[ignore]` if forcing a seal is heavy; the helper test + existing FTS tests are the floor.
- [ ] **Step 5:** `cargo test --lib memory_bucket_seal::adapter` → pass. Commit `feat(memory_bucket_seal): vector recall over sealed summaries (merged with FTS5)`.

---

### Task 3: AppState wires MemUEmbedder when memU present

**Files:** `src-tauri/src/app.rs` (bucket_seal embedder construction).

- [ ] **Step 1:** Replace the `InertEmbedder` build with:
```rust
        let bucket_seal_embedder: std::sync::Arc<dyn crate::memory_bucket_seal::Embedder> =
            match &memu_client {
                Some(memu) => std::sync::Arc::new(
                    crate::memory_bucket_seal::score::embed::MemUEmbedder::new(memu.clone()),
                ),
                None => std::sync::Arc::new(crate::memory_bucket_seal::InertEmbedder::new()),
            };
```
(verify the existing local name for the embedder + that `MemUEmbedder` is reachable — re-export at `memory_bucket_seal::score::embed::MemUEmbedder`, or add a top-level `pub use` if cleaner).
- [ ] **Step 2:** Build green. Commit `feat(app): bucket_seal uses MemUEmbedder (FastEmbed) when memU is available`.

---

### Task 4: Verification
- [ ] `cargo test --lib memory_bucket_seal` → pass. Full `cargo test --lib` → no NEW failures vs baseline.
- [ ] `cargo clippy --lib 2>&1 | grep -E "embed/memu|adapter\.rs"` → clean. `git diff main -- Cargo.toml` empty.
- [ ] PR notes: MemUEmbedder integration-only; vector recall is over sealed summaries (empty until a seal fires → FTS5 fallback); EMBEDDING_DIM 1024→384 invalidates stale inert blobs (none real).
