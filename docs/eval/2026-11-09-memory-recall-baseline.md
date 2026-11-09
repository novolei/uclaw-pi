# Memory recall baseline (M0) — 2026-11-09

Harness: `src-tauri/examples/memory_recall_eval/` —
`cargo run --example memory_recall_eval -- --verbose --out report.json`.
It snapshots `~/.uclaw-pi/uclaw.db` + `bucket_seal/chunks.db` with `VACUUM INTO`
(live data untouched) and runs a JSONL golden set. The real golden set holds the
user's personal facts, so it lives outside the repo at
`~/.uclaw-pi/eval/memory_recall_golden.jsonl`; `golden.sample.jsonl` shows the format.

Golden set: 37 natural-language questions taken from the user's real "do you
remember…" turns — 27 Chinese, 10 English; 28 profile facts, 3 past episodes,
6 learned-skill lookups. 35/37 answers are verifiably stored somewhere
(memory_graph and/or bucket_seal chunks).

Data at run time: 675 graph nodes (space `default`), 600 bucket_seal chunks,
`promptRecallBackend = bucket_seal` (limit 5), bucket_seal embedder = inert
(FTS5-only, i.e. what runs whenever memU is down).

## Ranked recall (expected fact in top-k)

| backend | hit@1 | hit@3 | hit@5 | hit@10 | MRR@10 | p50 latency |
|---|---|---|---|---|---|---|
| graph_fts (`MemoryGraphStore::fts_search`) | 0.0% | 2.7% | 2.7% | 2.7% | 0.014 | 0.1 ms |
| graph_plan (triggered+relevant+expanded) | 2.7% | 5.4% | 13.5% | 16.2% | 0.057 | 40 ms |
| bucket_seal (`BucketSealAdapter::recall`) | 0.0% | 0.0% | 0.0% | 0.0% | 0.000 | 1.6 ms |

Chinese vs English is similarly bad (graph_plan hit@10: zh 14.8%, en 20.0%).

## What reaches the pi-engine prompt

| | value |
|---|---|
| expected fact stored anywhere | 94.6% |
| fact present in composed pi context | 32.4% (zh 22.2%, en 60.0%) |
| …of which from the query-dependent recall block | 10.8% before compose, **0% after** |
| queries whose Relevant-memories section survived | 0 / 37 |
| queries whose bucket_seal `<recalled_memories>` survived | 0 / 37 |
| median recall block size | 8,669 chars |

Every pi-context hit comes from the query-independent `user_model` / reflections
blocks, not from recall.

## Root causes (verified)

1. **Keyword-only retrieval of whole questions.** Both FTS paths match the raw
   question text. Trigram FTS (graph) needs the exact substring; bucket_seal's
   `porter unicode61` treats a run of Chinese characters as one token, so a
   Chinese sentence never matches. Probe: the query `刘书维` hits at rank 1 in all
   three backends; `我儿子叫什么名字` hits nowhere. No embeddings or reranker in the
   loop (memU retired from L3; bucket_seal vector recall runs over
   InertSummariser placeholder summaries).
2. **Prompt budget starves recall.** `format_recall_for_prompt` puts Learned
   Skills (the same ~5–10 SOPs every turn, e.g. finance-data/yt-dlp) first, then
   Boot, and drops Relevant first when over budget. Then
   `PiPromptContext::compose` truncates the recall block at `CAP_RECALL = 8_000`
   **bytes** (`b.len()`), ≈2,700 CJK chars, which keeps only the skills section.
   The bucket_seal supplement is appended last and is always cut.
3. **Cross-lingual gap.** Profile facts are stored as English sentences
   ("The user enjoys playing table tennis") while users ask in Chinese.

## Use

Re-run after each memory change; the numbers above are the bar to beat. Target
for the unified memory upgrade: pi-context hit / stored ≥ 80% on this set, with
the recall block (not user_model) carrying the hit.

## After P0 (prompt-budget fix)

P0: `PiPromptContext::compose` counts characters (was bytes); the recall block
renders query-dependent sections first (Triggered → Relevant → Expanded → Boot →
Learned Skills → Recent); boot-only skills become a one-line index; the
bucket_seal supplement goes before the graph block.

| | baseline | after P0 |
|---|---|---|
| fact present in composed pi context | 32.4% | 37.8% |
| …zh / en | 22.2% / 60.0% | 29.6% / 60.0% |
| hits carried by the query-dependent recall block | 4 | 6 |
| bucket_seal block survives | 0 / 37 | 2 / 37 |
| median recall block | 8,669 chars | 7,290 chars |

Ranked retrieval is unchanged (P0 does not touch search). `--dump-context` on
the remaining misses shows the cause has moved to retrieval: e.g. for
`我儿子叫什么名字` no profile fact is retrieved at all — the "rank 5" plan hit is a
learned-skill SOP that mentions the name. Next gain needs P1 (segmentation +
multilingual embeddings + rerank).

## After P1 (hybrid `memory_core`)

P1 adds `uclaw_core::memory_core`: jieba segmentation + BM25 (FTS5) and bge-m3
int8 dense vectors, fused with RRF and reranked by bge-reranker-v2-m3 int8;
`load_context` injects its `<relevant_memories>` block first. The harness gained
a `memory_core` ranked backend (`--models-dir`, `--rerank-top-n`,
`--min-rerank-score`, `--core-limit`) and passes `core` into `load_context`, so
the pi-context table reflects P1. It runs `db::migrations::run` on the **snapshot**
only, then indexes graph + chunk docs once per run (942 docs). Debug build, Apple
Silicon, real models from `~/.uclaw-pi/models/memory`.

| | baseline | after P0 | after P1 (final, `rerank_top_n=6`) |
|---|---|---|---|
| best ranked hit@5 (all) | 13.5% (graph_plan) | 13.5% | **70.3%** (`memory_core`) |
| fact present in composed pi context | 32.4% | 37.8% | **86.5%** (zh 85.2%, en 90.0%) |
| pi-context hit / stored | ~34% | ~40% | **91.4%** |
| hit carried by the query-dependent recall block | 4 | 6 | 29 / 37 (78.4%) |

### memory_core ranked recall — final defaults (`rerank_top_n=6`, `min_rerank_score=0.05`, `limit=6`)

| group | n | hit@1 | hit@3 | hit@5 | hit@10 | MRR | p50 ms |
|---|---|---|---|---|---|---|---|
| all | 37 | 45.9% | 64.9% | 70.3% | 70.3% | 0.557 | 697 |
| zh | 27 | 48.1% | 66.7% | 70.4% | 70.4% | 0.577 | 642 |
| en | 10 | 40.0% | 60.0% | 70.0% | 70.0% | 0.503 | 1235 |
| cat=profile | 28 | 39.3% | 60.7% | 67.9% | 67.9% | 0.504 | 598 |
| cat=episode | 3 | 66.7% | 66.7% | 66.7% | 66.7% | 0.667 | 777 |
| cat=skill | 6 | 66.7% | 83.3% | 83.3% | 83.3% | 0.750 | 1405 |

Other backends are unchanged (graph_fts 2.7%, graph_plan 13.5%, bucket_seal 0% at hit@5).

### Runs

| # | config | hit@5 all / zh / en | hit@10 all | MRR | p50 ms | pi hit/stored | pi hit (all) |
|---|---|---|---|---|---|---|---|
| 1 | default as shipped (`rerank_top_n=20`) | 83.8% / 88.9% / 70.0% | 83.8% | 0.668 | 2850 | 91.4% | 86.5% |
| 2 | BM25-only (empty `--models-dir`) | 67.6% / 74.1% / 50.0% | 83.8% | 0.591 | 0.7 | 82.9% | 78.4% |
| 3 | `--rerank-top-n 10` | 78.4% / 81.5% / 70.0% | 81.1% | 0.620 | 1242 | 91.4% | 86.5% |
| 4 | `--rerank-top-n 6` (**new default**) | 70.3% / 70.4% / 70.0% | 70.3% | 0.557 | 697 | 91.4% | 86.5% |
| 5 | `--rerank-top-n 8` | 70.3% / 74.1% / 60.0% | 75.7% | 0.572 | 1000 | 91.4% | 86.5% |

Index build (incl. ~4 s model load): 150–165 s for 942 docs, all embedded, debug
build; BM25-only index: 0.9 s. Per-query p50 in run 1 is dominated by the cross-encoder
(20 candidates x up to 512 tokens); p50 scales roughly linearly with `rerank_top_n`
(20: 2850, 10: 1242, 8: 1000, 6: 697 ms). `min_rerank_score` and
`--core-limit` were not tuned: latency, not precision, was the failing criterion
and the gate on hit@5 is already at its limit with only 6 reranked candidates.

### Gate

| criterion | result | verdict |
|---|---|---|
| `memory_core` hit@5 >= 70% overall | 70.3% | PASS (thin: 26/37) |
| `memory_core` hit@5 >= 65% zh | 70.4% | PASS |
| pi-context hit / stored >= 60% | 91.4% | PASS |
| `memory_core` p50 < 800 ms | 697 ms | PASS |

The default (`rerank_top_n=20`) passed quality by a wide margin (83.8% hit@5) but
failed latency by 3.5x, so the default was lowered to 6 (`perf(memory_core): tune
defaults from P1 eval`). Latency is measured in a debug build; a release build
(Rust tokenization/RRF) should be somewhat faster, but ORT inference dominates, so
treat 697 ms as the right order of magnitude, not a guarantee on slower hardware.

### Fallback and deployment findings

* **BM25-only** (no models downloaded) already gives hit@5 67.6% (zh 74.1%) and
  pi hit/stored 82.9% in <1 ms: segmentation alone fixes most of the Chinese
  keyword gap. Vectors matter most for English questions over Chinese-or-English
  facts (hit@5 50% -> 70%) and for ordering (hit@1 51.4% BM25-only vs 56.8% with 20 reranked candidates).
* **Bug found and fixed while running this eval:** the first full runs were
  SIGKILLed after ~10 min with RSS above 40 GB. On macOS ONNX Runtime auto-registers
  the CoreML execution provider (`AutoDevicePolicy::MaxEfficiency`), which
  recompiles per dynamic batch shape. `embed.rs` now forces
  `AutoDevicePolicy::PreferCPU` for both embedder and reranker; RSS stays ~3 GB and
  embedding of 942 docs finishes in ~2.5 min. The running app's `memory_core`
  service would have hit the same blow-up.

### What still misses

At the final setting 11 of 37 questions miss the top 5; 9 of those are stored in
the index, i.e. the fact exists but never reaches the 6 reranked candidates or is
out-ranked. The patterns: (1) short profile questions whose answer sentence shares
little vocabulary with the question (a name, a place of origin) where several
near-duplicate "user likes/is..." facts compete; (2) preference questions (food,
sport) where many sibling facts score alike and the reranker, seeing only 6
candidates, drops the right one — run 1 with 20 candidates recovered most of
these, so the miss is a candidate-depth/latency trade-off rather than an
embedding failure; (3) one episode question answered only from a long raw chunk (not
investigated further). 2 of 37 questions have no stored answer at
all, so the ceiling is 94.6%. Many misses are still saved in the prompt by the
`user_model` / reflections blocks, which is why pi hit (86.5%) exceeds ranked hit@5
(70.3%); the recall block alone carries 78.4%. Next lever: a cheaper reranker input
(truncate candidates to ~256 tokens) so `rerank_top_n` can return to ~12–20 within
the latency budget.
