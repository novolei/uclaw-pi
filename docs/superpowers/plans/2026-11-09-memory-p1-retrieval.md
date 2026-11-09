# Memory P1 — Chinese/English Hybrid Retrieval Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** One in-process `MemoryCore::search` that finds stored facts for natural-language Chinese/English questions (jieba-segmented BM25 + bge-m3 vectors + RRF + bge-reranker-v2-m3), injected first into the agent prompt.

**Architecture:** New module `src-tauri/src/memory_core/` owns a derived search index in `uclaw.db`
(docs + segmented FTS5 + vector BLOBs) built from the existing stores (memory_graph active versions
and bucket_seal chunks) — the source stores are untouched until P2. Models are int8 ONNX files
downloaded on first run into `<data_dir>/models/memory/` and run via fastembed-rs
(`UserDefined*` constructors, so we control the download source). Without models, search degrades
to segmented BM25 only. `load_context` prepends a `<relevant_memories>` block from `MemoryCore`.

**Tech Stack:** Rust, rusqlite FTS5 (`unicode61`), `jieba-rs 0.11`, `fastembed 7.6.1` (ort `=2.0.0-rc.15`), reqwest (existing).

**Spec:** `docs/superpowers/specs/2026-11-09-unified-memory-design.md` (§3, §4, §5 row P1). Baseline + harness: `docs/eval/2026-11-09-memory-recall-baseline.md`, `src-tauri/examples/memory_recall_eval/`.

## Global Constraints

- No Python/Node on the recall path. memU stays only for the `memu_memory` tool and skills embeddings (retired in P2).
- Model download budget ≈ 1.2 GB total (user-approved 2026-11-09): **int8** files only, never fastembed's fp32 defaults (~2.2 GB each).
- Download sources, tried in order: `https://huggingface.co`, `https://hf-mirror.com`, `https://modelscope.cn` (URL forms in Task 4).
- Migrations: next free number is **V61** (main ends at V60). Register it in `CONTEXT.md` › *Active migration registry*.
- Data paths derive from the app data dir (`.uclaw-pi`), never `dirs::home_dir().join(".uclaw")` (pre-commit hook).
- Do not touch `crates/pi` (a parallel session is syncing upstream there).
- One logical change per commit; explicit `git add <files>`; no push/PR unless the user asks.
- Verification: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`; `cargo test --lib <filter>`; eval `cargo run --example memory_recall_eval -- --verbose`.

## Review Focus

1. **Models not downloaded yet / download fails or is interrupted** → recall still works (BM25-only), no panic, partial files never loaded (`.part` + size check), retried next launch. Test in Task 4 + Task 7.
2. **Query that segments to nothing** (`"你好"`, `"？？"`, emoji, empty) → `search` returns empty quickly, no FTS syntax error. Test in Task 2 + Task 7.
3. **Recall latency on the pi path (1500 ms deadline)** with reranker on CPU → rerank capped (`rerank_top_n = 20`, `max_length = 512`), search runs in `spawn_blocking`, and a deadline miss falls back as today. Measured in Task 10 (p50 must be < 800 ms, else lower `rerank_top_n`).
4. **Source rows edited or deleted after indexing** (memory version superseded, chunk removed) → stale docs removed on next sync, changed content re-embedded (content hash). Test in Task 6.
5. **Mixed-script / English-only content and queries** ("Where am I from?", `open-pandora`, `put_page`) → English tokens kept lowercased, identifiers with `-`/`_` survive segmentation. Test in Task 2.

---

### Task 1: Dependencies — bump `ort`, add `fastembed` and `jieba-rs`

**Files:**
- Modify: `src-tauri/Cargo.toml` (line 65 `ort`, add deps), `Cargo.lock`
- Modify (only if the bump breaks it): `src-tauri/src/stt/openflow/onnx_inference.rs`

**Interfaces:** Produces crates `fastembed`, `jieba_rs` usable from `uclaw_core`.

- [ ] **Step 1:** In `src-tauri/Cargo.toml` set `ort = { version = "=2.0.0-rc.15", features = ["ndarray", "half"] }` and add
  `fastembed = { version = "=7.6.1", default-features = false, features = ["ort-download-binaries-rustls-tls"] }` and `jieba-rs = "0.11"`.
  (No fastembed release uses ort rc.12; `ort-sys` links the native lib once, so both must be rc.15. `hf-hub` is deliberately off — Task 4 downloads.)
- [ ] **Step 2:** Run `cd src-tauri && cargo build 2>&1 | grep -E "^error" -A6 | head -40`. Expected: either clean, or errors only in `stt/openflow/onnx_inference.rs`.
- [ ] **Step 3:** Fix any rc.12→rc.15 API breaks in `onnx_inference.rs` (behavior unchanged).
- [ ] **Step 4:** Run `cargo build` (clean) and `cargo test --lib stt` (same pass count as before the bump).
- [ ] **Step 5: Commit** `build(deps): ort rc.15, add fastembed 7.6.1 + jieba-rs for memory retrieval` — body notes the STT adaptation if any.

### Task 2: Segmentation

**Files:**
- Create: `src-tauri/src/memory_core/mod.rs` (module skeleton, `pub mod segment;`), `src-tauri/src/memory_core/segment.rs`
- Modify: `src-tauri/src/lib.rs` (add `pub mod memory_core;`)

**Interfaces:**
- Produces: `pub fn segment(text: &str) -> Vec<String>` (index side: jieba `cut_for_search`, lowercased, punctuation/whitespace dropped, ASCII identifiers containing `-`/`_`/`.` kept whole, stopwords kept);
  `pub fn segment_query(text: &str) -> Vec<String>` (same, plus stopword removal, dedup, order kept);
  `pub fn fts_match_query(text: &str) -> Option<String>` (`segment_query` tokens, each double-quoted with `"` escaped, joined by ` OR `; `None` when empty);
  `pub fn segmented_text(text: &str) -> String` (`segment(text).join(" ")`, what goes into FTS).
- Stopword list (exact, Chinese function words that wreck BM25): `我 你 他 她 它 我们 你们 的 了 吗 呢 吧 啊 是 在 有 和 与 什么 怎么 哪 哪里 哪个 哪些 那个 这个 一下 还 都 就 也 请 帮 帮我 知道 记得 还记得 之前 现在` plus English `the a an is are am do does did i me my you your what which where who how of to in on for`.
- Jieba instance: one `static JIEBA: OnceLock<jieba_rs::Jieba>`.

- [ ] **Step 1: Write failing tests** in `segment.rs`:
```rust
#[test] fn query_drops_function_words() {
    let t = segment_query("我儿子叫什么名字");
    assert!(t.contains(&"儿子".to_string()) && t.contains(&"名字".to_string()));
    assert!(!t.iter().any(|w| w == "我" || w == "什么"));
}
#[test] fn english_lowercased_and_identifiers_kept() {
    let t = segment_query("What app am I building, open-pandora?");
    for w in ["app", "building", "open-pandora"] { assert!(t.contains(&w.to_string()), "{w}"); }
    assert!(!t.iter().any(|w| w == "what" || w == "i"));
    assert!(segment_query("put_page failed").contains(&"put_page".to_string()));
}
```
  Plus: `fts_match_query("你好？？")` is `Some(...)` containing `"你好"`; `fts_match_query("？？ 😀")` and `fts_match_query("")` are `None`; `fts_match_query("a \"b\"")` escapes quotes (result contains `"""b"""` or omits the token, but never an unbalanced `"`).
- [ ] **Step 2:** `cargo test --lib memory_core::segment` → FAIL (not defined).
- [ ] **Step 3:** Implement the four functions.
- [ ] **Step 4:** `cargo test --lib memory_core::segment` → PASS.
- [ ] **Step 5: Commit** `feat(memory_core): jieba segmentation + FTS query builder`.

### Task 3: V61 search-index schema

**Files:**
- Modify: `src-tauri/src/db/migrations.rs` (add `SQL_V61` + runner entry, mirroring V60's shape), `CONTEXT.md` (registry row `| V61 | memory_search_docs + memory_search_fts + memory_search_vectors (P1 retrieval index) | open |`)

**Interfaces:** Produces tables (exact):
```sql
CREATE TABLE IF NOT EXISTS memory_search_docs (
  doc_id TEXT PRIMARY KEY,          -- "<source>:<source_id>"
  source TEXT NOT NULL,             -- 'graph' | 'chunk'
  source_id TEXT NOT NULL,
  title TEXT NOT NULL DEFAULT '',
  content TEXT NOT NULL,
  content_hash TEXT NOT NULL,       -- sha256 hex of title+"\n"+content
  source_updated_at TEXT,
  indexed_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE VIRTUAL TABLE IF NOT EXISTS memory_search_fts USING fts5(
  doc_id UNINDEXED, segmented, tokenize = 'unicode61 remove_diacritics 2'
);
CREATE TABLE IF NOT EXISTS memory_search_vectors (
  doc_id TEXT PRIMARY KEY, model_id TEXT NOT NULL, dim INTEGER NOT NULL, embedding BLOB NOT NULL
);
```
- [ ] **Step 1: Failing test** next to the V60 test: after `run_migrations` on an in-memory DB, all three tables exist and inserting a row into `memory_search_fts` then `MATCH '"儿子"'` on segmented text `"用户 的 儿子 叫 刘书维"` returns it.
- [ ] **Step 2:** `cargo test --lib db::migrations` → FAIL.
- [ ] **Step 3:** Add `SQL_V61` and the runner step.
- [ ] **Step 4:** `cargo test --lib db::migrations` → PASS.
- [ ] **Step 5: Commit** `feat(db): V61 memory search index tables` (body: registry updated).

### Task 4: Model manifest + downloader

**Files:**
- Create: `src-tauri/src/memory_core/models.rs`

**Interfaces:**
- Produces:
```rust
pub struct ModelFile { pub path: &'static str, pub min_bytes: u64 }
pub struct ModelSpec { pub id: &'static str, pub hf_repo: &'static str, pub ms_repo: &'static str, pub files: &'static [ModelFile] }
pub const BGE_M3_INT8: ModelSpec;           // id "bge-m3-int8", hf+ms repo "Xenova/bge-m3"
pub const BGE_RERANKER_V2_M3_INT8: ModelSpec; // id "bge-reranker-v2-m3-int8", hf+ms repo "onnx-community/bge-reranker-v2-m3-ONNX"
pub fn model_dir(models_root: &Path, spec: &ModelSpec) -> PathBuf;   // models_root/<id>
pub fn is_ready(models_root: &Path, spec: &ModelSpec) -> bool;        // every file exists with len >= min_bytes
pub async fn ensure_model(client: &reqwest::Client, models_root: &Path, spec: &ModelSpec) -> anyhow::Result<PathBuf>;
pub fn source_urls(spec: &ModelSpec, file: &str) -> [String; 3];
```
- Files for both specs: `onnx/model_int8.onnx` (min_bytes 500_000_000), `tokenizer.json` (1_000_000), `config.json`, `special_tokens_map.json`, `tokenizer_config.json` (min_bytes 1). Sizes verified 2026-11-09: 568 MB / 571 MB.
- URL forms (exact): `https://huggingface.co/{hf_repo}/resolve/main/{file}`, `https://hf-mirror.com/{hf_repo}/resolve/main/{file}`, `https://modelscope.cn/models/{ms_repo}/resolve/master/{file}`.
- Download each missing file streaming to `<file>.part`, try sources in order, rename on success only if `len >= min_bytes`; on all-fail return the last error (caller retries next launch). Skip files already ready.

- [ ] **Step 1: Failing tests:** `source_urls(&BGE_M3_INT8, "tokenizer.json")` equals the three exact URLs; `is_ready` false on empty temp dir, true after writing files of the required sizes (write sparse files via `File::set_len`); a leftover `onnx/model_int8.onnx.part` does not make `is_ready` true.
- [ ] **Step 2:** `cargo test --lib memory_core::models` → FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** PASS. Also an `#[ignore]` test `downloads_tokenizer_json_live` that fetches only `config.json` + `tokenizer.json` for the reranker into a tempdir (run manually: `cargo test --lib downloads_tokenizer_json_live -- --ignored`).
- [ ] **Step 5: Commit** `feat(memory_core): int8 model manifest + mirrored downloader`.

### Task 5: Embedder and reranker wrappers

**Files:**
- Create: `src-tauri/src/memory_core/embed.rs`

**Interfaces:**
- Consumes: `ModelSpec`, `model_dir` (Task 4).
- Produces:
```rust
pub struct Embedder { /* Mutex<fastembed::TextEmbedding> */ }
impl Embedder {
    pub fn load(models_root: &Path) -> anyhow::Result<Self>;          // BGE_M3_INT8, Pooling::Cls, max_length 512
    pub fn model_id(&self) -> &'static str;                           // "bge-m3-int8"
    pub fn dim(&self) -> usize;                                       // 1024
    pub fn embed(&self, texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>>; // L2-normalized
}
pub struct Reranker { /* Mutex<fastembed::TextRerank> */ }
impl Reranker {
    pub fn load(models_root: &Path) -> anyhow::Result<Self>;          // BGE_RERANKER_V2_M3_INT8, max_length 512
    pub fn rerank(&self, query: &str, docs: &[String]) -> anyhow::Result<Vec<(usize, f32)>>; // (input index, score) best first
}
pub fn cosine(a: &[f32], b: &[f32]) -> f32;
pub fn pack(v: &[f32]) -> Vec<u8>;  pub fn unpack(b: &[u8]) -> Vec<f32>;   // little-endian f32
```
- Load via `TextEmbedding::try_new_from_user_defined(UserDefinedEmbeddingModel::new(onnx_bytes, TokenizerFiles{..}).with_pooling(Pooling::Cls), InitOptionsUserDefined::default().with_max_length(512))` and the reranker equivalent (use the builder methods that exist in 7.6.1; field names above are from its source).

- [ ] **Step 1: Failing tests:** `pack`/`unpack` round-trip; `cosine` of identical normalized vectors ≈ 1.0 and orthogonal ≈ 0.0. `#[ignore]` model test `bge_m3_cross_lingual` (needs downloaded models under `$UCLAW_TEST_MODELS`): cosine("我喜欢打乒乓球", "The user enjoys playing table tennis") > cosine("我喜欢打乒乓球", "The user ran ls -a in a terminal"); `#[ignore]` `reranker_orders_relevant_first`: for query "我儿子叫什么名字", doc "用户的儿子叫刘书维" ranks above "用户喜欢吃鱼".
- [ ] **Step 2:** FAIL. **Step 3:** Implement. **Step 4:** PASS (non-ignored); run ignored ones once models exist (Task 8 downloads them).
- [ ] **Step 5: Commit** `feat(memory_core): fastembed bge-m3 embedder + bge-reranker wrappers`.

### Task 6: Index sync

**Files:**
- Create: `src-tauri/src/memory_core/index.rs`

**Interfaces:**
- Consumes: `segmented_text` (Task 2), V61 tables (Task 3), `Embedder` (Task 5).
- Produces:
```rust
pub struct SourceDoc { pub doc_id: String, pub source: &'static str, pub source_id: String, pub title: String, pub content: String, pub updated_at: Option<String> }
pub fn graph_docs(conn: &Connection) -> rusqlite::Result<Vec<SourceDoc>>;   // memory_nodes ⋈ active memory_versions, space_id='default'; doc_id "graph:<node_id>"; content truncated to 2000 chars
pub fn chunk_docs(chunks: &Connection) -> rusqlite::Result<Vec<SourceDoc>>; // mem_tree_chunks; doc_id "chunk:<id>"
pub struct SyncStats { pub upserted: usize, pub removed: usize, pub embedded: usize }
pub fn sync(conn: &Connection, docs: &[SourceDoc], embedder: Option<&Embedder>) -> anyhow::Result<SyncStats>;
```
- `sync` semantics: upsert docs whose `content_hash` changed (rewrite FTS row, drop old vector); delete docs/FTS/vectors whose doc_id is absent from `docs` **for the same source set** passed in; when `embedder` is Some, embed every doc lacking a vector or whose vector `model_id` differs, batches of 16, in one transaction per batch.

- [ ] **Step 1: Failing tests** (in-memory DB migrated to V61, `embedder: None`): first sync of 2 docs → `upserted 2`; second identical sync → `upserted 0`; changing one doc's content → `upserted 1`; dropping one doc → `removed 1` and its FTS row gone; FTS `MATCH '"儿子"'` finds the doc with content "用户的儿子叫刘书维".
- [ ] **Step 2:** FAIL. **Step 3:** Implement. **Step 4:** PASS.
- [ ] **Step 5: Commit** `feat(memory_core): incremental index sync from graph + bucket_seal`.

### Task 7: Hybrid search

**Files:**
- Create: `src-tauri/src/memory_core/search.rs`; Modify: `src-tauri/src/memory_core/mod.rs`

**Interfaces:**
- Produces in `search.rs`: `pub fn rrf_fuse(lists: &[Vec<String>], k: f32) -> Vec<(String, f32)>` (score = Σ 1/(k + rank), rank 1-based, sorted desc, ties by first appearance).
- Produces in `mod.rs`:
```rust
pub struct MemoryCoreConfig { pub bm25_top: usize /*50*/, pub vector_top: usize /*50*/, pub rrf_k: f32 /*60.0*/, pub rerank_top_n: usize /*20*/, pub min_rerank_score: f32 /*0.05*/, pub limit: usize /*6*/ }
pub struct MemoryHit { pub doc_id: String, pub source: String, pub title: String, pub content: String, pub score: f32 }
pub struct MemoryCore { /* db: Arc<Mutex<Connection>>, models_root: PathBuf, embedder/reranker: RwLock<Option<Arc<..>>>, config */ }
impl MemoryCore {
    pub fn new(db: Arc<std::sync::Mutex<Connection>>, models_root: PathBuf, config: MemoryCoreConfig) -> Self;
    pub fn try_load_models(&self) -> bool;               // loads whichever models are ready; idempotent
    pub fn embedder(&self) -> Option<Arc<Embedder>>;
    pub fn search(&self, query: &str, limit: usize) -> anyhow::Result<Vec<MemoryHit>>; // sync; callers use spawn_blocking
    pub fn render_block(hits: &[MemoryHit]) -> Option<String>; // "<relevant_memories>\n- [title] content(≤300 chars)\n…</relevant_memories>\n", None if empty
}
```
- Pipeline: BM25 (`fts_match_query`, `ORDER BY bm25(memory_search_fts)`, `bm25_top`) + vector (embed query, brute-force cosine over `memory_search_vectors` with matching `model_id`, `vector_top`) → `rrf_fuse` → take `rerank_top_n` → rerank if loaded and drop scores `< min_rerank_score` → `limit`. Without models: BM25 order only.

- [ ] **Step 1: Failing tests:** `rrf_fuse([[a,b],[b,c]], 60.0)` → `b` first; `search` with no models on an index holding "用户的儿子叫刘书维" and "用户喜欢吃鱼" returns the son doc first for "我儿子叫什么名字"; `search("？？", 5)` → empty `Ok`; `render_block(&[])` → `None`.
- [ ] **Step 2:** FAIL. **Step 3:** Implement. **Step 4:** PASS.
- [ ] **Step 5: Commit** `feat(memory_core): hybrid BM25 + vector + RRF + rerank search`.

### Task 8: Background service + app wiring; retire memU embedder from bucket_seal

**Files:**
- Create: `src-tauri/src/memory_core/service.rs`
- Modify: `src-tauri/src/app.rs` (AppState field `pub memory_core: Arc<crate::memory_core::MemoryCore>` near `memory_adapters` ~216; construct with `db.clone()` and `data_dir.join("models").join("memory")`; bucket_seal embedder at ~1021 becomes `InertEmbedder` unconditionally), `src-tauri/src/main.rs` (`[Stage 3]` block, ~321: spawn the service)

**Interfaces:**
- Consumes: `ensure_model` (Task 4), `MemoryCore` (Task 7), `graph_docs`/`chunk_docs`/`sync` (Task 6), `BucketSealStore` (existing, for the chunks connection).
- Produces: `pub fn spawn(core: Arc<MemoryCore>, chunks_db_path: PathBuf)` — tokio task: (1) immediate BM25-only sync; (2) `ensure_model` for both specs (logs progress; on failure retry after 30 min); (3) `try_load_models` then full sync with embeddings; (4) re-sync every 60 s (cheap: hash comparison). All DB/model work in `spawn_blocking`.

- [ ] **Step 1:** Implement `service.rs`; wire AppState + Stage 3; switch bucket_seal embedder to `InertEmbedder` (its vector recall ran over InertSummariser placeholders, so this loses nothing; memU no longer sits on the recall path).
- [ ] **Step 2:** `cargo build` clean; `cargo test --lib memory_core` PASS.
- [ ] **Step 3:** Run the app (`cargo tauri dev` as in this session), confirm in the log: initial sync stats, model download progress, "memory_core: models loaded", embedded count ≈ number of docs. Run the Task 5 `#[ignore]` model tests against `~/.uclaw-pi/models/memory`.
- [ ] **Step 4: Commit** `feat(memory_core): background index + model service; drop memU from bucket_seal embedding` (body: new background service registered in Stage 3; AppState field).

### Task 9: Inject into the prompt

**Files:**
- Modify: `src-tauri/src/agent/memory_context.rs` (`MemoryContextInputs` gets `pub core: Option<Arc<crate::memory_core::MemoryCore>>`; `load_context` runs `core.search(query, core.config().limit)` inside `tokio::task::spawn_blocking` on the cloned `Arc`, and a search error logs a warning and yields no block; `compose_memory_context` gains a leading `core_block: Option<&str>` parameter and its order becomes core → adapter → graph → session → browser). Add `pub fn config(&self) -> &MemoryCoreConfig` to `MemoryCore`.
- Modify: every `MemoryContextInputs { .. }` construction in `src-tauri/src/tauri_commands.rs` (pi agent bg task ~5085, legacy agent, chat paths) to pass `state.memory_core`.

**Interfaces:** Consumes `MemoryCore::search`, `MemoryCore::render_block`.

- [ ] **Step 1: Failing test** in `memory_context.rs`: `compose_memory_context` with a core block puts it first: `"<c>\n<a>\n<g>\n<s>\n<b>\n"`.
- [ ] **Step 2:** FAIL. **Step 3:** Implement + update call sites (`grep -n "MemoryContextInputs {" src-tauri/src` must show every site passing `core`).
- [ ] **Step 4:** `cargo build` clean; `cargo test --lib memory_context` PASS.
- [ ] **Step 5: Commit** `feat(memory): inject MemoryCore hits first in the memory block` (body lists every call site).

### Task 10: Eval gate + tuning

**Files:**
- Modify: `src-tauri/examples/memory_recall_eval/main.rs` (new ranked backend `memory_core`: build `MemoryCore` on the snapshot DB with models from `<data-dir>/models/memory`, run `sync` over graph+chunk docs first, print whether models were loaded; pass `core` into `load_context`); `docs/eval/2026-11-09-memory-recall-baseline.md` (append "After P1")

- [ ] **Step 1:** Implement the backend plus a `--models-dir <path>` flag (default `<data-dir>/models/memory`); run `cargo run --example memory_recall_eval -- --verbose`.
- [ ] **Step 2: Gate:** `memory_core` hit@5 ≥ 70% overall and ≥ 65% zh; pi-context hit / stored ≥ 60%; `memory_core` p50 latency < 800 ms. If latency fails, lower `rerank_top_n` (20 → 10); if precision fails, tune `min_rerank_score` and `limit` — record each tried setting and its numbers in the doc.
- [ ] **Step 3:** Run BM25-only (models dir pointed at an empty dir via `--models-dir`) and record it too, so the no-model fallback is measured.
- [ ] **Step 4: Commit** `feat(eval): memory_core backend; record P1 results`.
