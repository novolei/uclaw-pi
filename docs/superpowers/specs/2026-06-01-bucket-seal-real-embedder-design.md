# bucket_seal Real Embedder + Vector Recall (piece 1b) — Design

**Date:** 2026-06-01
**Status:** Approved (scope: MemUEmbedder + vector recall); implementation pending
**Scope:** make bucket_seal embeddings real (memU FastEmbed) AND make `recall` use them

## Goal

Replace bucket_seal's `InertEmbedder` (zero vectors) with a real embedder reusing **memU's FastEmbed** (`MemUClient::embed_text`, bge-small-en-v1.5, 384-dim), reconcile the dimension, and add a **vector-recall path** so `BucketSealAdapter::recall` ranks the sealed memory by semantic similarity — not just FTS5 keywords. This is what makes a real embedder actually matter (the embedder alone doesn't change recall; recall must *use* the embeddings).

## Findings that shape it (surfaced + approved)

1. **Dimension mismatch.** `EMBEDDING_DIM` is hard-coded `1024` (for Ollama bge-m3); memU FastEmbed is `384`. `pack_checked`/`unpack_embedding` validate against the const. → Change `EMBEDDING_DIM` to `384`. Safe now: bucket_seal is new/unused and only ever wrote `InertEmbedder` zero-vectors, so there are no meaningful 1024-dim blobs to migrate (any stale inert blobs are zeros — wipe/re-seal, per the existing dimension-change note in `embed/mod.rs`).
2. **recall doesn't use embeddings.** `BucketSealAdapter::recall` is FTS5-only; embeddings live on sealed `mem_tree_summaries` and nothing reads them. → Add a vector path.
3. **Corpus = sealed summaries.** Only `mem_tree_summaries` have embeddings (chunks don't). So vector recall ranks the **sealed/consolidated** memory (matches "sealed chunk" intent), reusing existing embeddings — no schema change. **Caveat:** summaries only exist after an L0 seal fires (≥ `INPUT_TOKEN_BUDGET` of stored content), so on small stores the vector path is empty and recall falls back to FTS5. Documented, inherent.

## Design (4 parts)

### 1. `MemUEmbedder` — `memory_bucket_seal/score/embed/memu.rs`
```rust
pub struct MemUEmbedder { client: Arc<crate::memu::client::MemUClient> }
#[async_trait] impl Embedder for MemUEmbedder {
    fn name(&self) -> &'static str { "memu_fastembed" }
    async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        let mut vs = self.client.embed_text(&[text]).await.map_err(|e| anyhow!("memu embed: {e}"))?;
        let v = vs.pop().ok_or_else(|| anyhow!("memu embed: empty result"))?;
        if v.len() != EMBEDDING_DIM { anyhow::bail!("memu embed: {} dims, expected {}", v.len(), EMBEDDING_DIM); }
        Ok(v)
    }
}
```
Integration-only (needs live memU); the dim guard is the contract.

### 2. `EMBEDDING_DIM` 1024 → 384
`memory_bucket_seal/score/embed/mod.rs`: change the const + its doc (provider note → memU FastEmbed bge-small, 384; existing inert 1024 blobs invalid, wipe/re-seal). `InertEmbedder` (`vec![0.0; EMBEDDING_DIM]`) + pack/unpack/checked auto-adjust; update the one test comment mentioning "1024".

### 3. Vector recall in `BucketSealAdapter::recall`
- Pure helper `rank_by_cosine(query: &[f32], cands: &[(String /*id*/, String /*content*/, Vec<f32>)], limit) -> Vec<(String, String, f32)>` — cosine desc, take `limit`. **Unit-tested.**
- In `recall`: embed the query via `self.embedder.embed(query)`. On `Err` (memU offline) → skip the vector path (FTS5-only fallback — never fails the turn). On `Ok(qv)`:
  - resolve the namespace's tree (`get_or_create_source_tree`), `SELECT id, content, embedding FROM mem_tree_summaries WHERE tree_id=?1 AND embedding IS NOT NULL AND deleted=0`, decode each blob (`decode_optional_blob`/`unpack_embedding`), `rank_by_cosine` → top-K summary entries (`MemoryEntry{ id, key:"", content, category:Core, score:Some(cos as f64), namespace, … }`).
- **Merge:** vector-summary hits first (semantic), then the existing FTS5-chunk hits, dedup by `id`, cap at `limit`. Category filter (`opts.category`) still applies post-merge.

### 4. AppState wiring
`app.rs` (the bucket_seal adapter construction, ~1000): build the embedder as `MemUEmbedder` when `memu_client.is_some()`, else `InertEmbedder` (fallback). The adapter already takes `Arc<dyn Embedder>`; only the construction changes.

## File structure

| File | Change |
|---|---|
| `memory_bucket_seal/score/embed/memu.rs` (new) | `MemUEmbedder` |
| `memory_bucket_seal/score/embed/mod.rs` | `EMBEDDING_DIM`→384 + `pub mod memu; pub use memu::MemUEmbedder;` + doc/test comment |
| `memory_bucket_seal/adapter.rs` | `rank_by_cosine` helper + vector path in `recall` + merge + tests |
| `app.rs` | bucket_seal embedder = MemUEmbedder when memU present |

## Non-goals

- No chunk-level embeddings (no `mem_tree_chunks` schema change) — vector recall is over sealed summaries only.
- No change to the seal pipeline (`append_leaf` already embeds summaries via the injected embedder — now real).
- No default-backend flip. No new deps (memU already present).
- bge-m3/Ollama path dropped in favor of reusing memU (the approved tradeoff).

## Testing

- `rank_by_cosine`: hand-crafted 384-ish vectors → assert ordering + limit + a zero-query returns stable (cosine 0) order.
- `EMBEDDING_DIM` change: existing `embed/mod.rs` tests pass (they use the const); fix the literal-1024 comment.
- Vector recall in `recall`: a deterministic test `Embedder` (distinct vectors per text, NOT inert-zero) + store enough to force a seal, then `recall(query)` asserts the semantically-closest summary ranks above an unrelated one. (If forcing a seal in a unit test is too heavy, rely on `rank_by_cosine` unit tests + the FTS5 fallback test + an `#[ignore]` integration test.)
- `MemUEmbedder` is integration-only (live memU).
- Full `cargo test --lib` no new failures vs the environmental baseline; clippy clean; no new deps.

## Risks

- **Inert/offline embedder → no vector recall:** zero-query cosine is 0 for all → vector path returns nothing useful; recall degrades to FTS5. Acceptable (real embedder is the point; fallback is safe).
- **memU latency in recall:** embedding the query adds one memU call per recall when enabled. recall_adapter_block (piece 2) already bounds adapter recall off the critical path; and the embed error → FTS5 fallback.
- **Dim-change stale blobs:** any pre-existing 1024-dim inert summary blob fails `unpack` after the change → `decode_optional_blob` errors on that row. Mitigation: bucket_seal is unused; if a stale blob exists it's a zero-vector — the vector query skips/handles decode errors per-row (log + skip, don't fail recall).
