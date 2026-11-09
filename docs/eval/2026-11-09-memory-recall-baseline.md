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
