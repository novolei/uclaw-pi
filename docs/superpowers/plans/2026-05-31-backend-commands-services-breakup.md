# Backend `tauri_commands.rs` god-file breakup

> Decompose the 18k-line / 365-command `src-tauri/src/tauri_commands.rs` into
> `commands/<domain>.rs` (thin) + `services/<domain>_service.rs` (logic, Tauri-independent,
> unit-tested), per the code-organization ADR (`docs/adr/2026-05-31-pi-code-organization-discipline.md`).
> Multi-PR effort, one domain-slice per PR. `settings` is the landed reference slice.

**Goal:** every command body does only parse → call service → map/emit; all SQL/business
logic lives in a `services/` trait impl that takes `&Connection` (+ other Tauri-independent
params) so it unit-tests without Tauri.

## The recipe (per domain)

1. **`services/<domain>_service.rs`** — `trait <Domain>Service` + a `Db<Domain>` impl carrying
   the lifted logic, operating on `&rusqlite::Connection` (pass `workspace_root: &Path` etc.
   explicitly — never `AppState`/`State`). Add `#[cfg(test)] mod tests` with an in-memory DB.
   Register `pub mod <domain>_service;` in `services/mod.rs`.
2. **`commands/<domain>.rs`** — thin `#[tauri::command] pub async fn`s: lock `state.db`, call
   the service, map errors. NO SQL/logic. Register `pub mod <domain>;` in `commands/mod.rs`.
   Template: `commands/settings.rs` + `services/settings_service.rs`.
3. **Delete** the moved fns (+ now-orphaned private helpers) from `tauri_commands.rs`.
4. **Re-point `main.rs`** `invoke_handler!`: `uclaw_core::tauri_commands::<cmd>` →
   `uclaw_core::commands::<domain>::<cmd>` (each command listed explicitly — no glob).
   ⚠️ Forgetting a macro line compiles fine but fails at runtime — verify every command moves.
5. **Gates:** `cd src-tauri && cargo build 2>&1 | grep -E "^error"` empty · `cargo test --lib
   <domain>_service` green · the moved commands still appear in the `generate_handler!` macro.

## Worked example — `Space` (the first extraction of fat inline logic)

`list_spaces` today inlines: row read → NULL-`path` backfill (`compute_workspace_dir` + `mkdir`
+ persist) → JSON parse. Lift all of it into `SpaceService::list(&conn, workspace_root)`;
`create`/`delete` likewise. `commands/space.rs` becomes 3 three-line wrappers. The service is
unit-testable with a temp dir + `Connection::open_in_memory()`.

## Slice order (small/clean first → build the pattern, then the big messy domains)

| Slice | Domains | ~lines | notes |
|------|---------|-------|-------|
| **1** | Space, LLM Config, Notification, Background Task | ~290 | small CRUD — locks the pattern |
| 2 | Provider, Conversation | ~780 | clean medium domains |
| 3 | Search, Safety, Tool Approval | ~660 | Search keeps the flat UNION-of-branches shape (CLAUDE.md) |
| 4 | Persona, System Prompt, Memory, MCP | ~900 | |
| 5 | Skills, Learned Skills, Channel/IM | ~1400 | |
| 6 | Chat, Artifact (+ tree), gbrain | ~1400 | Chat is the biggest single domain |
| 7 | Memory Graph, Memory OS (EntityPage/Wiki/Health/Lint/Drift/synth/Export/Sync/learning), Fragment | ~2500 | decompose Memory OS into sub-slices |
| 8 | Bootstrap, Embedding config, Setup-script, Persona, Slash, MEMUBOT, remainder | — | sweep the tail |

No new DB migrations expected (pure reorg of existing commands). If a service needs a new
table, take the next free V-number per the *Active migration registry* in `CONTEXT.md`.

## Progress — as of 2026-06-01 (clean-domain phase COMPLETE, then paused)

**12 slices merged (PR #4–#14), 30 domains extracted, `tauri_commands.rs` 18,001 → 10,004 lines (−44%).**
`commands/` 31 files · `services/` 16 files. The actual slices were re-grouped from the original
table by *clean-vs-messy + real section boundaries* (the banner headers don't cleanly bound
domains — watch for interleaved interlopers). Extracted (✅): Space, LLM Config, Notification,
Background Task, Conversation, Cost, Search, Safety, Tool Approval, Memory, MCP (+mcp_audit),
Provider, Persona, Skills, System Prompt, Artifact, Channel/IM, Learned Skills, Browser,
Automation, Trajectory, Workspace, MEMUBOT, GEP, Knowledge Ingestion, Humane Automation,
Bootstrap, Dev/Testing, System Tray. Services landed: space, conversation, search, mcp_audit,
persona, system_prompt, channel, automation, workspace (extended) — the rest are thin
manager-delegating moves (≈16 domains had no `&Connection` SQL → correctly NO service).

**RESUME POINT — the risky/messy half (~half the remaining 10k lines), paused per user:**
- 🔴 **Agent Session** (~2.4k) and **Chat** (~1.3k) and **Agent Teams** (~1.5k) — live agent/chat
  send paths; behavior-preserving moves, but require **human UI verification** in the native
  window after extraction.
- 🟠 **Memory Recall Config** (~1.3k grab-bag: recall-config + embedding-endpoint + setup-script +
  gbrain) and **Memory OS cluster** (EntityPage/Wiki/Health/Fragment ~1.3k) — need decomposition
  into proper sub-domains/sub-slices.
- 🟠 **Memory Graph grab-bag** (memory_graph_* + proactive/metrics/stream/skill settings mixed),
  **Marketplace**, and scattered interlopers (`respond_plan_mode_suggest`, `get_app_health`,
  `get_memu_status`, `memu_embed_text`, `memory_graph_delete_node`, `stop_agent_session`).

**Flags for follow-up (not blockers):**
- 2 **pre-existing** test failures (NOT from the breakup — byte-identical moves): `shell::
  test_daemon_mode_approval_unchanged` (safety-relevant: daemon-mode approval `Never` vs `Always`)
  and `skill_marketplace::truncate_for_error_long` (UTF-8 off-by-2). Each a small standalone fix.
- `commands/workspace.rs` (973) + `commands/browser_cmds.rs` (801) exceed the ~400-line soft cap
  (big cohesive single domains) — optional sub-module split.
- 10 stale `git stash` entries are pre-existing repo cruft (May 18–25, old branches) — left alone.

## Out of scope (separate concerns)
- `main.rs` Stage-3 service registration (unchanged).
- The `tauri-specta`/`ts-rs` bridge-type generation (ADR rule 5 end-state).
