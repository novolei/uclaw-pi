//! memory_recall_eval — M0 baseline for memory recall quality.
//!
//! Runs a golden set of natural-language queries (Chinese + English) against a
//! read-only snapshot of a uClaw data dir and reports how often each recall
//! path surfaces the expected fact:
//!
//! * `graph_fts`   — `MemoryGraphStore::fts_search` (raw trigram FTS, top-K)
//! * `graph_plan`  — `MemoryRecallEngine::build_recall_plan` query-dependent
//!                   layers (triggered → relevant → expanded), in that order
//! * `bucket_seal` — `BucketSealAdapter::recall` (namespace "global") with the
//!                   inert embedder, i.e. FTS5-only (what runs when memU is down)
//! * `pi_context`  — the pi-path prompt block: `load_context` (graph + adapter
//!                   supplement per the user's `promptRecallBackend`) composed
//!                   with user_model + reflections via `PiPromptContext::compose`
//!                   (facts/genes/gbrain need a running app and are skipped).
//!
//! Nothing touches the live DBs: both `uclaw.db` and `bucket_seal/chunks.db`
//! are copied with `VACUUM INTO` into a temp dir first.
//!
//! Usage:
//!   cargo run --example memory_recall_eval -- \
//!       [--data-dir ~/.uclaw-pi] [--golden FILE.jsonl] [--out report.json] [--verbose]
//!
//! Golden file: one JSON object per line —
//!   {"id":"son-name","lang":"zh","category":"profile","query":"我儿子叫什么",
//!    "expect_any":["刘书维"],"expect_word":["RL"]}
//! A result "hits" when its text contains any `expect_any` substring
//! (case-insensitive) or any `expect_word` as a whole ASCII word (case-sensitive).
//! The default golden path is `<data-dir>/eval/memory_recall_golden.jsonl` so
//! real personal facts stay out of the repo; see `golden.sample.jsonl` here.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use anyhow::{Context, Result};
use regex::Regex;
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};

use uclaw_core::agent::memory_context::{
    load_context, AdapterRecall, MemoryContextInputs, PiPromptContext,
};
use uclaw_core::memory::MemoryStore;
use uclaw_core::memory_adapter::{MemoryAdapter, RecallOpts};
use uclaw_core::memory_bucket_seal::{
    BucketSealAdapter, BucketSealStore, Embedder, InertEmbedder, InertSummariser, Summariser,
};
use uclaw_core::memory_graph::recall::{
    MemoryRecallConfig, MemoryRecallConfigDto, MemoryRecallEngine,
};
use uclaw_core::memory_graph::store::MemoryGraphStore;

const K: usize = 10;
const KS: [usize; 4] = [1, 3, 5, 10];
const SPACE_ID: &str = "default";
const BS_NAMESPACE: &str = "global";
const PI_CONTEXT_BUDGET: usize = 12_000;
const RANKED_BACKENDS: [&str; 3] = ["graph_fts", "graph_plan", "bucket_seal"];

#[derive(Debug, Deserialize)]
struct GoldenItem {
    id: String,
    lang: String,
    #[serde(default)]
    category: String,
    query: String,
    #[serde(default)]
    expect_any: Vec<String>,
    #[serde(default)]
    expect_word: Vec<String>,
}

struct Matcher {
    subs: Vec<String>,
    words: Vec<Regex>,
}

impl Matcher {
    fn new(item: &GoldenItem) -> Result<Self> {
        let words = item
            .expect_word
            .iter()
            .map(|w| Regex::new(&format!(r"(?-u:\b){}(?-u:\b)", regex::escape(w))))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(Self {
            subs: item.expect_any.iter().map(|s| s.to_lowercase()).collect(),
            words,
        })
    }

    fn hits(&self, text: &str) -> bool {
        let lower = text.to_lowercase();
        self.subs.iter().any(|s| lower.contains(s)) || self.words.iter().any(|r| r.is_match(text))
    }
}

#[derive(Debug, Serialize)]
struct BackendResult {
    /// 1-based rank of the first hit within top-K, if any.
    first_hit_rank: Option<usize>,
    returned: usize,
    latency_ms: f64,
    error: Option<String>,
    top: Vec<String>,
}

#[derive(Debug, Serialize)]
struct QueryReport {
    id: String,
    lang: String,
    category: String,
    query: String,
    stored_in_graph: bool,
    stored_in_chunks: bool,
    backends: HashMap<String, BackendResult>,
    /// Expected fact present anywhere in the composed pi prompt block.
    pi_context_hit: bool,
    /// ...and specifically inside the recall part (graph + adapter supplement).
    pi_recall_block_hit: bool,
    pi_context_chars: usize,
    pi_context_latency_ms: f64,
    /// Section headers present in the recall block (shows what the budget kept).
    recall_block_sections: Vec<String>,
    recall_block_chars: usize,
}

struct Args {
    data_dir: PathBuf,
    golden: Option<PathBuf>,
    out: Option<PathBuf>,
    verbose: bool,
    /// Print each query's composed pi context (for debugging what the model sees).
    dump_context: bool,
}

fn parse_args() -> Result<Args> {
    let home = std::env::var("HOME").context("HOME not set")?;
    let mut args = Args {
        data_dir: PathBuf::from(home).join(".uclaw-pi"),
        golden: None,
        out: None,
        verbose: false,
        dump_context: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--data-dir" => args.data_dir = PathBuf::from(it.next().context("--data-dir value")?),
            "--golden" => args.golden = Some(PathBuf::from(it.next().context("--golden value")?)),
            "--out" => args.out = Some(PathBuf::from(it.next().context("--out value")?)),
            "--verbose" | "-v" => args.verbose = true,
            "--dump-context" => args.dump_context = true,
            other => anyhow::bail!("unknown argument: {other}"),
        }
    }
    Ok(args)
}

/// Consistent read-only copy of a (possibly WAL-mode, in-use) SQLite DB.
fn snapshot(src: &Path, dst: &Path) -> Result<()> {
    let conn = Connection::open_with_flags(src, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("open {}", src.display()))?;
    conn.execute("VACUUM INTO ?1", [dst.to_string_lossy().as_ref()])
        .with_context(|| format!("snapshot {}", src.display()))?;
    Ok(())
}

fn load_golden(path: &Path) -> Result<Vec<GoldenItem>> {
    let raw = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    raw.lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty() && !l.trim_start().starts_with("//"))
        .map(|(i, l)| serde_json::from_str(l).with_context(|| format!("golden line {}", i + 1)))
        .collect()
}

fn load_recall_config(data_dir: &Path) -> MemoryRecallConfig {
    std::fs::read_to_string(data_dir.join("config.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v.get("memoryRecallConfig").cloned())
        .and_then(|v| serde_json::from_value::<MemoryRecallConfigDto>(v).ok())
        .map(MemoryRecallConfig::from)
        .unwrap_or_default()
}

/// node_id → title + all active version content, for the graph space.
fn graph_corpus(conn: &Connection) -> Result<HashMap<String, String>> {
    let mut stmt = conn.prepare(
        "SELECT n.id, n.title, COALESCE(v.content, '')
           FROM memory_nodes n
           LEFT JOIN memory_versions v ON v.node_id = n.id AND v.status = 'active'
          WHERE n.space_id = ?1",
    )?;
    let mut out: HashMap<String, String> = HashMap::new();
    let rows = stmt.query_map([SPACE_ID], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))
    })?;
    for row in rows {
        let (id, title, content) = row?;
        let e = out.entry(id).or_insert_with(|| title);
        e.push('\n');
        e.push_str(&content);
    }
    Ok(out)
}

fn chunk_corpus(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT content FROM mem_tree_chunks")?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn snippet(s: &str) -> String {
    let flat: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    flat.chars().take(90).collect()
}

fn ranked(texts: Vec<String>, m: &Matcher, ms: f64, error: Option<String>) -> BackendResult {
    let texts: Vec<String> = texts.into_iter().take(K).collect();
    BackendResult {
        first_hit_rank: texts.iter().position(|t| m.hits(t)).map(|p| p + 1),
        returned: texts.len(),
        latency_ms: ms,
        error,
        top: texts.iter().take(3).map(|t| snippet(t)).collect(),
    }
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let args = parse_args()?;
    let golden_path = args
        .golden
        .clone()
        .unwrap_or_else(|| args.data_dir.join("eval").join("memory_recall_golden.jsonl"));
    let golden = load_golden(&golden_path)?;

    // ── Snapshot both stores into a temp dir ──────────────────────────────
    let tmp = tempfile::tempdir()?;
    let db_path = tmp.path().join("uclaw.db");
    snapshot(&args.data_dir.join("uclaw.db"), &db_path)?;
    let bs_dir = tmp.path().join("bucket_seal");
    std::fs::create_dir_all(bs_dir.join("content"))?;
    let bs_db = bs_dir.join("chunks.db");
    snapshot(&args.data_dir.join("bucket_seal").join("chunks.db"), &bs_db)?;

    // ── Wire the same components the app builds (app.rs) ──────────────────
    let conn = Arc::new(Mutex::new(Connection::open(&db_path)?));
    let graph_store = Arc::new(MemoryGraphStore::new(conn.clone()));
    let recall_cfg = load_recall_config(&args.data_dir);
    let prompt_backend = recall_cfg.prompt_recall_backend.clone().filter(|b| !b.is_empty());
    let prompt_limit = recall_cfg.prompt_recall_limit;
    let engine = MemoryRecallEngine::new(graph_store.clone(), recall_cfg);
    let memory_store = MemoryStore::new(conn.clone());

    let bs_store = Arc::new(BucketSealStore::open(&bs_db)?);
    bs_store.ensure_schema()?;
    let embedder: Arc<dyn Embedder> = Arc::new(InertEmbedder::new());
    let summariser: Arc<dyn Summariser> = Arc::new(InertSummariser::new());
    let bucket_seal = Arc::new(BucketSealAdapter::new(
        bs_store,
        bs_dir.join("content"),
        embedder,
        summariser,
    ));
    let mut adapters: HashMap<String, Arc<dyn MemoryAdapter>> = HashMap::new();
    adapters.insert("bucket_seal".into(), bucket_seal.clone() as Arc<dyn MemoryAdapter>);

    // Query-independent pi prompt dims we can reproduce from the DB.
    let (graph_texts, user_model, reflections) = {
        let c = conn.lock().unwrap();
        let corpus = graph_corpus(&c)?;
        let um = uclaw_core::memory_graph::reflection_service::get_user_model(&c)
            .ok()
            .flatten()
            .map(|m| format!("## User Model\n{m}"));
        let refl = uclaw_core::memory_graph::reflection_service::recent_reflections(&c, 3)
            .unwrap_or_default();
        let refl = (!refl.is_empty()).then(|| {
            let mut s = String::from("## Recent Reflections\n");
            for r in &refl {
                s.push_str(&format!("- ({:.2}) {}\n", r.confidence, r.insight));
            }
            s
        });
        (corpus, um, refl)
    };
    let chunk_texts = chunk_corpus(&Connection::open_with_flags(
        &bs_db,
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?)?;

    eprintln!(
        "golden={} queries | graph nodes={} | chunks={} | prompt_recall_backend={:?} (limit {})",
        golden.len(),
        graph_texts.len(),
        chunk_texts.len(),
        prompt_backend,
        prompt_limit
    );

    let mut reports = Vec::new();
    for item in &golden {
        let m = Matcher::new(item)?;
        let mut backends = HashMap::new();

        // graph_fts
        let t = Instant::now();
        let (texts, err) = match graph_store.fts_search(SPACE_ID, &item.query, K) {
            Ok(rs) => (
                rs.into_iter()
                    .map(|r| graph_texts.get(&r.node_id).cloned().unwrap_or(r.title))
                    .collect(),
                None,
            ),
            Err(e) => (Vec::new(), Some(e.to_string())),
        };
        backends.insert("graph_fts".to_string(), ranked(texts, &m, ms(t), err));

        // graph_plan (query-dependent layers only; boot is query-independent)
        let t = Instant::now();
        let (texts, err) = match engine.build_recall_plan(SPACE_ID, &item.query, false).await {
            Ok(p) => (
                p.triggered
                    .iter()
                    .chain(p.relevant.iter())
                    .chain(p.expanded.iter())
                    .map(|c| format!("{}\n{}", c.title, c.content))
                    .collect(),
                None,
            ),
            Err(e) => (Vec::new(), Some(e.to_string())),
        };
        backends.insert("graph_plan".to_string(), ranked(texts, &m, ms(t), err));

        // bucket_seal
        let t = Instant::now();
        let opts = RecallOpts { namespace: Some(BS_NAMESPACE), ..Default::default() };
        let (texts, err) = match bucket_seal.recall(&item.query, K, opts).await {
            Ok(es) => (es.into_iter().map(|e| format!("{}\n{}", e.key, e.content)).collect(), None),
            Err(e) => (Vec::new(), Some(e.to_string())),
        };
        backends.insert("bucket_seal".to_string(), ranked(texts, &m, ms(t), err));

        // pi_context
        let t = Instant::now();
        let adapter_recall = prompt_backend.as_deref().map(|backend| AdapterRecall {
            adapters: &adapters,
            default_backend: "bucket_seal",
            backend,
            limit: prompt_limit,
        });
        let loaded = load_context(MemoryContextInputs {
            recall_engine: &engine,
            memory_store: &memory_store,
            space_id: SPACE_ID,
            conversation_id: "memory-recall-eval",
            query: &item.query,
            browser_ctx: None,
            adapter_recall,
        })
        .await;
        let recall_block = loaded.context.clone().unwrap_or_default();
        let composed = PiPromptContext {
            user_model: user_model.clone(),
            reflections: reflections.clone(),
            recall: loaded.context,
            ..Default::default()
        }
        .compose(PI_CONTEXT_BUDGET)
        .unwrap_or_default();
        let pi_ms = ms(t);
        if args.dump_context {
            println!("===== [{}] {}\n{}\n", item.id, item.query, composed);
        }

        let report = QueryReport {
            id: item.id.clone(),
            lang: item.lang.clone(),
            category: item.category.clone(),
            query: item.query.clone(),
            stored_in_graph: graph_texts.values().any(|t| m.hits(t)),
            stored_in_chunks: chunk_texts.iter().any(|t| m.hits(t)),
            backends,
            pi_context_hit: m.hits(&composed),
            pi_recall_block_hit: m.hits(&recall_block),
            pi_context_chars: composed.chars().count(),
            pi_context_latency_ms: pi_ms,
            recall_block_sections: recall_block
                .lines()
                .filter(|l| l.starts_with('#') || (l.starts_with('<') && !l.starts_with("</")))
                .map(|l| l.chars().take(60).collect())
                .collect(),
            recall_block_chars: recall_block.chars().count(),
        };
        if args.verbose {
            print_query(&report);
        }
        reports.push(report);
    }

    print_summary(&reports);
    if let Some(out) = &args.out {
        std::fs::write(out, serde_json::to_string_pretty(&reports)?)?;
        eprintln!("per-query report written to {}", out.display());
    }
    Ok(())
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

fn print_query(r: &QueryReport) {
    let rank = |b: &str| {
        r.backends[b]
            .first_hit_rank
            .map(|x| x.to_string())
            .unwrap_or_else(|| "-".into())
    };
    println!(
        "[{}|{}] {} | stored g={} c={} | fts={} plan={} bs={} | pi={} ",
        r.lang,
        r.id,
        r.query,
        yn(r.stored_in_graph),
        yn(r.stored_in_chunks),
        rank("graph_fts"),
        rank("graph_plan"),
        rank("bucket_seal"),
        yn(r.pi_context_hit),
    );
}

fn yn(b: bool) -> &'static str {
    if b { "y" } else { "n" }
}

fn pct(n: usize, d: usize) -> String {
    if d == 0 { "  n/a".into() } else { format!("{:5.1}%", 100.0 * n as f64 / d as f64) }
}

fn print_summary(reports: &[QueryReport]) {
    let mut groups: Vec<(String, Vec<&QueryReport>)> =
        vec![("all".into(), reports.iter().collect())];
    let mut langs: Vec<String> = reports.iter().map(|r| r.lang.clone()).collect();
    langs.sort();
    langs.dedup();
    for l in langs {
        groups.push((format!("lang={l}"), reports.iter().filter(|r| r.lang == l).collect()));
    }
    let mut cats: Vec<String> = reports.iter().map(|r| r.category.clone()).collect();
    cats.sort();
    cats.dedup();
    for c in cats.into_iter().filter(|c| !c.is_empty()) {
        groups.push((format!("cat={c}"), reports.iter().filter(|r| r.category == c).collect()));
    }

    println!("\n## Ranked recall (hit@k = expected fact within top-k; MRR@{K})\n");
    println!("| group | n | backend | hit@1 | hit@3 | hit@5 | hit@10 | MRR | errors | p50 ms |");
    println!("|---|---|---|---|---|---|---|---|---|---|");
    for (name, rs) in &groups {
        for b in RANKED_BACKENDS {
            let ranks: Vec<Option<usize>> =
                rs.iter().map(|r| r.backends[b].first_hit_rank).collect();
            let hit = |k: usize| ranks.iter().filter(|x| x.is_some_and(|r| r <= k)).count();
            let mrr: f64 = ranks.iter().map(|x| x.map_or(0.0, |r| 1.0 / r as f64)).sum::<f64>()
                / rs.len().max(1) as f64;
            let errors = rs.iter().filter(|r| r.backends[b].error.is_some()).count();
            let mut lat: Vec<f64> = rs.iter().map(|r| r.backends[b].latency_ms).collect();
            lat.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let p50 = lat.get(lat.len() / 2).copied().unwrap_or(0.0);
            println!(
                "| {name} | {} | {b} | {} | {} | {} | {} | {:.3} | {errors} | {p50:.1} |",
                rs.len(),
                pct(hit(KS[0]), rs.len()),
                pct(hit(KS[1]), rs.len()),
                pct(hit(KS[2]), rs.len()),
                pct(hit(KS[3]), rs.len()),
                mrr,
            );
        }
    }

    println!("\n## Pi prompt context (expected fact present in the injected block)\n");
    println!("| group | n | stored anywhere | pi_context hit | recall-block hit | hit / stored | avg chars |");
    println!("|---|---|---|---|---|---|---|");
    for (name, rs) in &groups {
        let n = rs.len();
        let stored: Vec<&&QueryReport> =
            rs.iter().filter(|r| r.stored_in_graph || r.stored_in_chunks).collect();
        let ctx = rs.iter().filter(|r| r.pi_context_hit).count();
        let rec = rs.iter().filter(|r| r.pi_recall_block_hit).count();
        let ctx_stored = stored.iter().filter(|r| r.pi_context_hit).count();
        let chars = rs.iter().map(|r| r.pi_context_chars).sum::<usize>() / n.max(1);
        println!(
            "| {name} | {n} | {} | {} | {} | {} | {chars} |",
            pct(stored.len(), n),
            pct(ctx, n),
            pct(rec, n),
            pct(ctx_stored, stored.len()),
        );
    }
}
