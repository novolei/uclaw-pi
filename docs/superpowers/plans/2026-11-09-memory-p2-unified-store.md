# Memory P2 — Unified Store, gbrain Retirement & Memory Center Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** One `memory_items` store with a single `observe()` write path, migrated from the 7 scattered user-profile stores, memU and gbrain (then gbrain retired), searched by MemoryCore, exposed to the agent as `memory_search`/`memory_write` tools, and to the user as a redesigned Memory Center.

**Architecture:** `memory_core::items` owns `memory_items` (bi-temporal: `valid_at`/`invalid_at`; provenance; confidence; usage counters) and `observe()` (normalize → exact dedupe → near-duplicate merge via the embedder when loaded → insert or reinforce). The MemoryCore index reads items instead of graph versions. One-time importers read the old stores (read-only) and gbrain (export from a scratch COPY of its PGlite dir). Every live writer is redirected to `observe()`; gbrain code, boot, bundling and UI are removed. The Memory Center replaces the 10-tab memory module after a design-approval gate.

**Tech Stack:** Rust, rusqlite (SQLite FTS5), fastembed (existing), Bun only for the one-time gbrain export, React/TS + Tailwind tokens + shadcn/Radix + `motion/react`, vitest, Playwright, browser mock.

**Spec:** `docs/superpowers/specs/2026-11-09-unified-memory-design.md` (§2.2, §4, §5 P2 row, §6 decision 1). Recon: side-session notes `2026-11-09-p2-recon-backend.md`, `2026-11-09-p2-recon-frontend.md`. Depends on: `docs/superpowers/plans/2026-11-09-memory-p1-ui-backfill.md` (design tokens/primitives, engine status) landing first.

## Global Constraints

- All Global Constraints of the P1 UI backfill plan apply (Tauri command registration, tokens-only colors, settings primitives, Chinese copy, motion, all states, mock parity, verification commands, commit rules).
- Migrations: next free V-number after V61 is **V62**; register in `CONTEXT.md` › Active migration registry.
- **Never write to `~/.uclaw/` (the original uClaw app).** gbrain's live DB is `~/.uclaw/gbrain/.gbrain/brain.pglite` (via `database_path` in `~/.uclaw-pi/gbrain/.gbrain/config.json`). The importer copies that directory to a scratch dir and runs export against the copy; it never opens, locks, modifies or deletes the original.
- Importers are read-only on every source store, idempotent (marker rows), and produce a dry-run report before writing.
- Learned skills stay skills (Procedure nodes + SKILL.md); `memory_items` holds only `procedure_ref` pointers.
- memU stays running in P2 (its LLM extraction feeds MemorizationService); its outputs are redirected into `observe()`. memU retirement is P3 (with the new extractor).
- Deleting/forgetting an item purges it from `memory_items`, the FTS rows and vectors (no soft-delete leak into recall).
- Gate: M0 eval (`cargo run --example memory_recall_eval`) must not regress vs. the last recorded numbers (pi hit 83.8%, hit/stored 88.6%), and duplicate rate for user facts drops ≥ 80%.

## Review Focus

1. Two writers observe the same fact concurrently (per-turn extractor + MemorizationService) → one item, reinforced, not two rows. Test in Task 3.
2. A contradicting newer fact ("我现在在上海工作" vs older "在北京") → older item gets `invalid_at`, both visible in history, only the valid one recalled. Test in Task 3.
3. gbrain import while the original uClaw app is running → importer refuses or copies a consistent snapshot; never touches `~/.uclaw`. Test in Task 6 (lock detection on the copy) + a guard test that the source path is opened read-only.
4. Upgrade on a machine without Bun / without gbrain data → importer is a no-op with a clear status, app still starts. Test in Task 6.
5. Memory Center with 2,000+ items, very long content, and mixed zh/en → virtualized list, no jank, truncation with full text in the detail sheet. Checked in Task 11.

---

### Task 1: ADR — unified store is primary; gbrain retired; graph legacy

**Files:** Create `docs/adr/2026-11-09-unified-memory-items-primary.md` (supersedes `2026-05-20-gbrain-primary-freeze-l2-cognitive.md` if present; otherwise note it was removed in a11d57dc). Modify `scripts/git-hooks/checks/check-memory-graph-freeze.sh` message to point at the new ADR (memory_graph stays write-frozen for new code; new writes go to `memory_items`). Update `CLAUDE.md`/`CONTEXT.md` memory section lines that call gbrain primary.
- [ ] Write ADR (context: recon numbers; decision; consequences); update docs + hook message; commit `docs(adr): memory_items is the primary memory store; gbrain retired`.

### Task 2: V62 schema + item model

**Files:** Modify `src-tauri/src/db/migrations.rs` (`SQL_V62`), `CONTEXT.md`; Create `src-tauri/src/memory_core/items.rs`.

**Interfaces:**
```sql
CREATE TABLE IF NOT EXISTS memory_items (
  id TEXT PRIMARY KEY, kind TEXT NOT NULL,          -- fact|preference|episode|lesson|entity_page|procedure_ref
  subject TEXT NOT NULL,                            -- 'user', entity slug, tool name, skill name …
  content TEXT NOT NULL, norm_hash TEXT NOT NULL,   -- sha256 of normalized (kind, subject, content)
  valid_at TEXT NOT NULL, invalid_at TEXT,          -- bi-temporal validity; NULL = currently valid
  superseded_by TEXT,                               -- id of the item that invalidated this one
  source TEXT NOT NULL, source_id TEXT,             -- 'graph'|'facet'|'reflection'|'user_model'|'memu'|'gbrain'|'chat'|'tool'|'user'
  confidence REAL NOT NULL DEFAULT 0.6,
  reinforce_count INTEGER NOT NULL DEFAULT 1, helpful INTEGER NOT NULL DEFAULT 0, harmful INTEGER NOT NULL DEFAULT 0,
  metadata_json TEXT, created_at TEXT NOT NULL, updated_at TEXT NOT NULL, last_used_at TEXT);
CREATE UNIQUE INDEX IF NOT EXISTS idx_memory_items_norm_valid ON memory_items(norm_hash) WHERE invalid_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_memory_items_subject ON memory_items(kind, subject);
CREATE TABLE IF NOT EXISTS memory_item_events (id INTEGER PRIMARY KEY AUTOINCREMENT, item_id TEXT NOT NULL,
  event TEXT NOT NULL /* created|reinforced|superseded|edited|forgotten|imported */, detail_json TEXT, at TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS memory_migrations (name TEXT PRIMARY KEY, completed_at TEXT NOT NULL, report_json TEXT);
```
Rust: `MemoryKind` enum, `MemoryItem` struct (serde camelCase), `normalize(text) -> String` (NFKC, lowercase ASCII, strip punctuation/whitespace runs, trailing particles), `norm_hash(kind, subject, content)`.
- [ ] Tests: V62 tables exist; normalize("The user is 30 years old.") == normalize("the user is 30 years old"); unique partial index rejects a second valid row with same hash. RED → GREEN. Commit `feat(db): V62 memory_items store`.

### Task 3: `observe()` + item CRUD

**Files:** `src-tauri/src/memory_core/items.rs`, `memory_core/mod.rs`.

**Interfaces:**
```rust
pub struct Observation { pub kind: MemoryKind, pub subject: String, pub content: String, pub source: String,
  pub source_id: Option<String>, pub confidence: f32, pub valid_at: Option<String>, pub supersedes_subject_key: Option<String> }
pub enum ObserveOutcome { Created(String), Reinforced(String), Merged { into: String, similarity: f32 }, Superseded { new: String, old: Vec<String> } }
impl MemoryCore {
  pub fn observe(&self, obs: Observation) -> anyhow::Result<ObserveOutcome>;          // exact hash → reinforce; else near-dup (cosine ≥ 0.92 with same kind+subject, embedder when loaded) → merge; supersedes_subject_key (e.g. "user.city") invalidates prior valid items with that key in metadata
  pub fn list_items(&self, q: &ItemQuery) -> anyhow::Result<Page<MemoryItem>>;          // filters: kind, subject, source, valid_only, text (uses search), sort, cursor
  pub fn get_item(&self, id: &str) -> anyhow::Result<Option<ItemDetail>>;               // item + events + superseded chain
  pub fn edit_item(&self, id: &str, content: &str) -> anyhow::Result<MemoryItem>;       // supersede-with-edit, event 'edited'
  pub fn forget_item(&self, id: &str) -> anyhow::Result<()>;                           // hard delete + purge index rows + event
  pub fn item_stats(&self) -> anyhow::Result<ItemStats>;                               // counts by kind/source, valid vs invalid
}
```
- [ ] Tests (BM25-only, no models): duplicate observe → Reinforced, reinforce_count 2; contradiction via supersedes_subject_key → old invalid_at set + superseded_by; concurrent observe of same fact from 2 threads → 1 valid row; forget purges FTS + vectors; edit creates new valid item and invalidates old. RED → GREEN. Commit `feat(memory_core): observe() write path with dedupe and supersession`.

### Task 4: Index reads memory_items

**Files:** `memory_core/index.rs` (`item_docs(conn)` replacing `graph_docs` in the service/eval; doc_id `item:<id>`; only valid items), `service.rs`, `examples/memory_recall_eval/main.rs` (use item docs; keep graph_docs available behind a flag for comparison).
- [ ] Tests: sync over items indexes only valid ones; forgetting removes the doc. Commit `feat(memory_core): index memory_items instead of graph versions`.

### Task 5: One-time importer from in-app stores + memU

**Files:** Create `src-tauri/src/memory_core/import/{mod.rs,graph.rs,profile.rs,memu.rs}`.
Mapping (from recon; binding): graph user_profile active versions → fact/preference (skip titles `用户偏好:*`); manual/voice/clipboard episodes → episode; failure_memory → lesson (render metadata to text); personality/boot/environment → entity_page; Procedure → procedure_ref (skill name + description); task_memory + tool_memory → skip; facets (state active only, skip ones failing a junk filter: value length < 2 or ends with a question particle) → preference/fact; reflections (all, archived → invalid_at) → lesson; user_model + history → entity_page `user` (history → invalidated versions); daydreams → skip; memU `profile` → fact/preference via observe (dedupe!), `event` → episode (happened_at → valid_at); KV browser_task → skip.
`pub fn run_import(core: &MemoryCore, sources: &ImportSources, dry_run: bool) -> anyhow::Result<ImportReport>` with per-source counts (read, skipped-junk, created, reinforced, merged); marker `memory_migrations('inapp-v1')`; memu.db opened `SQLITE_OPEN_READ_ONLY`.
- [ ] Tests on fixture DBs: dedupe collapses 5 variants of the same fact into 1 item with reinforce_count 5; junk filters; idempotent second run = 0 created. Then a dry-run on a COPY of the user's real DBs (report recorded in the PR body, aggregate counts only). Commit `feat(memory_core): one-time import from graph, facets, reflections, user_model, memU`.

### Task 6: gbrain importer (copy → export → items)

**Files:** Create `memory_core/import/gbrain.rs`.
Steps: read `database_path` from `~/.uclaw-pi/gbrain/.gbrain/config.json` (fallback `<GBRAIN_HOME>/.gbrain/brain.pglite`); if missing or Bun/gbrain entry not found → `Skipped{reason}`; copy the directory to `<data>/import/gbrain-snapshot-<ts>/` (refuse if the source contains a live PGlite lock file — report "原版 uClaw 正在运行，请关闭后重试"); write a temp config pointing at the copy; run `bun cli.ts export --dir <tmp>/export` with `GBRAIN_HOME=<tmp>` (timeout 10 min); parse each `<slug>.md` (frontmatter + body) → `entity_page` item (subject = slug, content = title + compiled truth, metadata: type, tags, links found in body, source_ref slug); marker `memory_migrations('gbrain-v1')`; delete the scratch copy after success.
- [ ] Tests: parser on fixture markdown; path guard asserts the original dir is never opened for write (open only via copy; unit test the function that picks paths); missing Bun → Skipped. Manual run on the user's data (record counts). Commit `feat(memory_core): import gbrain pages from a snapshot copy`.

### Task 7: Redirect live writers to `observe()`

**Files (from recon, file:line in notes):** `proactive/preference_extractor.rs` (observe preference; drop raw-utterance captures), `proactive/failure_memory.rs` (lesson), `memorization/service.rs` persist fan-out (facets/memU/gbrain_drafts → observe; drafts watcher ingests into items), `gbrain/chat_extractor.rs` + `agent/dispatcher/turn_runner.rs` (put_page → observe entity_page), `browser/memory_adapter.rs` (gbrain branch → observe episode), `ingestion/merge.rs` (gbrain get/put → items), `memory_policy` (`GbrainWrite` → `ItemWrite`, delete `targets/gbrain.rs`), facets `LearningScheduler` (observe preference/fact instead of writing facets table), reflection_service (insert → also observe lesson; user_model → entity_page `user`).
- [ ] Per writer: a test that one call yields exactly one observe; `cargo build`; commit per area (`refactor(memory): route <area> writes through observe()`).

### Task 8: Prompt blocks + agent tools

**Files:** `tauri_commands.rs` pi paths (facts/user_model blocks read `entity_page user` + top-N valid preferences instead of FacetCache/user_model), `agent/memory_context.rs`, new `agent/tools/memory_tools.rs` (`memory_search {query, limit}` → MemoryCore::search over items+chunks; `memory_write {kind, subject, content}` → observe with source 'tool', confidence 0.7), registered in `agent/dispatcher.rs` + pi bridge (`engine_sink.rs` tool exposure) + SafetyManager (memory_write non-destructive; forget not exposed). Replace `GbrainKnowledgeSection` prompt text with a short memory-tools instruction.
- [ ] Tests: tool schemas; memory_write → item exists; prompt facts block contains the user entity page and no junk facet. Commit `feat(agent): memory_search/memory_write tools; prompt reads memory_items`.

### Task 9: Retire gbrain (backend + bundling)

Remove per the retirement checklist in `2026-11-09-p2-recon-backend.md`: GbrainAdapter + registration; gbrain prompt section; MCP transport/seed/init + tests; Stage-3 gbrain boot + app.rs discovery/launcher; gbrain Tauri commands (+ `generate_handler!` entries) and diagnostics; automation `gbrain_room_*` stubs (+ douyin spec references → memory tools or removed with the stub); bundling (tauri.conf.json resources, build.rs placeholders, setup scripts; keep Bun only if another feature needs it — verify with grep); config keys kept tolerant via serde defaults; on upgrade remove the `gbrain` entry from `~/.uclaw-pi/mcp_servers.json` (only that file); archive gbrain-specific learned skills into `skills/_archived/`. Never touch `~/.uclaw`.
- [ ] `cargo build`, full `cargo test --lib`, `grep -rn gbrain src-tauri/src` shows only migration/importer + comments. Commits per area.

### Task 10: Memory Center — design gate (no production UI code)

Load `ui-ux-pro-max`; build on `docs/design/memory-ui.md` from the P1 UI plan.
- [ ] Produce high-fidelity mockups (HTML via the browser pane or an Artifact) for: 概览 (what I know about you, recent learning, items needing review), 浏览 (unified list: search, kind/source/validity filters, dense-but-calm rows), detail Sheet (content, provenance timeline, superseded history, edit, forget with confirm, "why recalled" link), 成长 (user model + reflections, merged with facets), 图谱 (optional single nebula), and chat-chip → center deep link; light/dark + one named theme.
- [ ] **STOP for user approval** of the mockups; record decisions in `docs/design/memory-ui.md`.

### Task 11: Memory Center — build

**Files:** `ui/src/views/Kaleidoscope/modules/Memory/` (restructure to 4 sections; remove Wiki/双星云 tabs, `lib/gbrain-browse.ts`, `WikiView`, `DualNebulaView`, dual-nebula scene, gbrain tool renderer → generic memory tool renderer), absorb `LearnedProfileTab` into 成长, delete dead `MemoryPanel.tsx`/`MemorySettings.tsx`, settings 系统 gbrain rows/scripts/diagnostics; new IPC in `src-tauri/src/commands/memory_items.rs`: `memory_items_list`, `memory_items_get`, `memory_items_edit`, `memory_items_forget`, `memory_items_stats`, `memory_import_status`, `memory_import_run` (+ registration), mock stubs; recall chip deep link; virtualized list.
- [ ] Vitest per section + Playwright flow (search → open → edit → forget); tsc; visual QA screenshots (states × themes × widths) under `docs/design/screenshots/p2-memory-center/`; fix and re-shoot. Commits per section.

### Task 12: Gate

- [ ] Run importers on the user's data (dry-run report → real run), M0 eval (no regression; record "After P2" in `docs/eval/2026-11-09-memory-recall-baseline.md` incl. duplicate-rate before/after), full `cargo test --lib`, `npm test -- --run`, `npm run e2e`; restart the app and verify Memory Center on real data.
