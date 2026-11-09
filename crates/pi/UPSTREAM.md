# crates/pi — upstream pin

`crates/pi` is a vendored copy of **pi_agent_rust** (crate `pi_agent_rust`,
lib name `pi`).

| | |
|---|---|
| Upstream repo | https://github.com/Dicklesworthstone/pi_agent_rust |
| Pinned commit | `6e19cb5003842eceb9b2c1413baca66ae7d3a267` (main, 2026-11-06, "fix(lint): add semicolons for semicolon_if_nothing_returned") |
| Nearest release | v0.8.0 (2026-10-13) + ~3 weeks of main |
| Why not main HEAD | The 39 newer commits (to `b7e56949`, 2026-11-08) are all `[skip ci]` and HEAD does not compile (half-done FTUI message-renderer refactor in `src/extensions.rs`; missing `crate::Result` in `src/web_remote/companion.rs`). `6e19cb50` is the last commit that went through upstream CI. |
| Synced on | 2026-11-09 |
| Previous base | `895269a3f` (2026-05-28, between v0.1.16 and v0.1.17) — vendored in uClaw commit `f0b7ae82` |

## What is vendored

Upstream's crates.io package set (the `include` list in `Cargo.toml`):
`src/**`, `build.rs`, `Cargo.toml`, `LICENSE`, `README.md`, `CHANGELOG.md`,
the `include_str!`'d docs/fixtures, `legacy_pi_mono_code/.../models.generated.ts`,
plus:

- `docs/lsp-configuration.md` — `include_str!`'d by `src/lsp/registry.rs`.
- `vendor/rquickjs-sys` — upstream's `[patch.crates-io]` (quickjs-ng
  use-after-free fix). A member crate's `[patch]` is ignored by cargo, so the
  **workspace root `Cargo.toml` mirrors it**.

Not vendored: `tests/`, `benches/`, `fuzz/`, `examples/`, `.beads`, `scripts/`,
the rest of `legacy_pi_mono_code/`. Therefore `cargo test -p pi_agent_rust`
only covers in-file `#[cfg(test)]` units; verify pi changes with `cargo build`
of the app plus `uclaw-pi-engine` tests.

## Toolchain

Upstream pins `nightly-2026-08-31` (rustc 1.100) for its own CI; the crate
declares MSRV **1.95** and uses no `#![feature]`. uClaw builds it on stable
1.95, which needed one type-inference workaround (P7) and a few lockfile pins
to upstream's versions (`vergen*` 10.0.1 and `kstring` 2.0.2 — newer ones need
rustc 1.96; `fsqlite-*` 0.4.10/0.4.11 to match the `=0.4.10` ast/parser pins).

## How uClaw consumes it

`crates/uclaw-pi-engine` depends on it with
`default-features = false, features = ["bpe-tokens", "sqlite-sessions"]` (no
TUI/FTUI stack). `sqlite-sessions` is required — upstream's lib does not compile
without it — and is harmless now that it is pure-Rust fsqlite. pi runs with
`no_session = true`; uClaw owns persistence. The engine passes
`enabled_tools = PI_BUILTIN_TOOLS` (read/bash/edit/write/grep/find/ls/
hashline_edit) so pi's larger implicit default set (ask, web_search, jobs, hub,
lsp, …) never leaks in; `workspace_trusted` stays upstream's default `false`
(project `.uclaw-pi/settings.json`, extensions and project MCP are not loaded).
pi still adds its `xdev` dispatcher to the tool schema (essential tier).

Runtime requirements the engine must keep (both found by the smoke test):

- **16 MiB stack** for the `pi-engine` thread and runtime worker — pi 0.8's
  futures overflow the 2 MiB default in debug builds (upstream's CLI uses the
  same 16 MiB).
- Run prompts with `RuntimeHandle::spawn` holding an `OwnedMutexGuard`, **not**
  `spawn_local`: cross-thread wakes of local tasks can leave the
  current_thread driver parked in kqueue (~60 s stalls, ~4% of runs).

Smoke test recipe (not committed): a mock OpenAI-compatible SSE server that
returns one `ls` tool call then text, plus a tiny example that spawns
`PiEngine`, sends `Configure` (provider `deepseek`, `base_url` = the mock,
`api` = `openai-completions`) + `Prompt`, and waits for `chat:stream-complete`.
`asupersync` in the engine must stay on the same minor as pi (currently 0.5).

## Local patches (re-apply on every sync)

Every hunk is marked `uclaw-patch` in the source. Keep this list in sync.

| # | Files | What / why | Upstreamable? |
|---|---|---|---|
| P1 | `src/auth.rs`, `src/secrets.rs` | Split the 4 **public** installed-app Google OAuth client id/secret literals (auth.rs) and the fake Slack bot-token test fixture (secrets.rs) with `concat!()` so GitHub push-protection stops false-positiving. Runtime values identical. Applied directly in the import commit, since push protection scans every commit. | No (repo-hosting workaround) |
| P2 | `Cargo.toml` | Remove all `[[bin]]`/`[[example]]`/`[[bench]]`/`[[test]]` targets and set `autoexamples/autotests/autobenches = false` (those dirs are not vendored). | No |
| P3 | `src/config.rs`, `src/migrations.rs` | Project dir `.pi` → `.uclaw-pi` (`Config::project_dir()`), plus `migrate_project_dir()` that renames a legacy `<cwd>/.pi` on first use (never touches `~/.pi`); called by `uclaw-pi-engine`. | No (branding) |
| P4 | `src/mcp/config.rs`, `src/workspace_trust.rs` | Follow-up to P3: upstream hardcodes `.pi/mcp.json` (MCP load + trust scan) and `.pi/...` trust-surface labels; point them at `.uclaw-pi`. New in this sync. | Partly — upstream could use `Config::project_dir()` |
| P5 | `src/sdk.rs` | `SessionOptions.base_url` / `.api`: override the resolved model's endpoint + API shape with the user's provider config (any OpenAI-compatible provider). | Yes |
| P6 | `src/agent.rs` | `backfill_orphan_tool_results()` before each provider request: synthesize an error tool result for any assistant `tool_call` without one (abort mid-tool → OpenAI 400). uClaw #93. Upstream only records a result when the abort lands during tool execution; request-level normalization is still missing. | Yes |
| P7 | `src/tools/structured_json.rs` | Build `jaq_core::Native::<QueryData>::new(run)` directly instead of `.map(native::run)`: stable 1.95 can't unify the normalized `RunPtr<QueryData>` under `for<'a>` (nightly 1.100 can). Same behavior. New in this sync. | Yes (MSRV fix) |

Dropped in this sync (no longer needed):

- `SessionOptions.tool_approval` — **upstreamed** (identical field, now wired
  by upstream into `AgentConfig::tool_approval`).
- `sqlite-sessions` gating + `src/session_sqlite_stub.rs` + no-op
  `SessionIndex` — upstream replaced `sqlmodel-sqlite`/`libsqlite3-sys` with the
  pure-Rust `fsqlite`, so there is no native `sqlite3` `links` conflict with
  uClaw's `rusqlite` any more. `sqlite-sessions` is simply enabled.
- `default = []` feature override — replaced by `default-features = false` on
  the consumer side (upstream added the `tui` feature for SDK consumers).

## How to sync next time

```bash
git clone https://github.com/Dicklesworthstone/pi_agent_rust /tmp/pi-up
cd /tmp/pi-up && git log --oneline <pinned>..origin/main | wc -l
# 1. pristine import commit: replace crates/pi with `git archive <new>` of the
#    file set above (keep this UPSTREAM.md)
#    and pick a commit that upstream CI actually ran (not [skip ci])
# 2. re-apply P1..P7 (grep -rn uclaw-patch in the previous tree)
#    + mirror crates/pi's [patch.crates-io] in the workspace root Cargo.toml
# 3. fix crates/uclaw-pi-engine + src-tauri/src/engine_*.rs for API changes
# 4. update this file's pin + patch table
```
