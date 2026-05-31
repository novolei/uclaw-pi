# ADR: Code-organization discipline for the pi-migration (no god files)

- **Status**: Accepted (2026-05-31)
- **Supersedes nothing**; **constrains** all new code under the pi-migration
  (`crates/uclaw-pi-engine/`, the `agent`/`chat`/`workspace` command + service
  layers, and the `ui` agent/chat features).
- **Relates to**: `docs/adr/2026-05-28-uclaw-pi-lightweight-product-philosophy.md`
  (pure kernel + optional layers), `docs/superpowers/specs/2026-05-27-pi-convergence-gap-audit.md`.

## Context

uClaw accreted **god files** — `src-tauri/src/tauri_commands.rs` is ~13k lines
mixing dozens of unrelated domains, with business logic inlined in command
bodies; the frontend has cross-imported internals and components that `invoke`
directly. The pi-migration is re-touching these seams (agent loop, chat stream,
workspace/session, cost, settings). Without a discipline, the migration would
re-grow the same god files (it already started: HTTP-API + cost commands were
appended to `tauri_commands.rs`; cost calculation landed inside the
`EventSink` bridge). This ADR fixes the target structure so new pi-migration
code is **structured, protocolized, and decoupled**.

## Decision

### Backend discipline (acceptance criteria)

1. **One domain per command file.** Commands live in `src-tauri/src/commands/<domain>.rs`
   (e.g. `agent.rs`, `chat.rs`, `workspace.rs`, `settings.rs`, `cost.rs`).
   Soft cap **~400 lines**; split when exceeded.
2. **Command bodies do four things only**: parse input → call a service →
   map result/error → emit event. **No inlined business logic.**
3. **Business logic lives in `services/<domain>.rs` as a `trait` + impl**
   (e.g. `trait WorkspaceService { fn list(&self) …; fn create(&self) … }`),
   **Tauri-independent and unit-testable**. Pricing, cost, persistence routing,
   cwd resolution, etc. are services — not command/bridge bodies.
4. **Registration is centralized** in `commands/mod.rs` (an aggregator that
   exposes the `generate_handler!` set). `main.rs` carries **no long
   `generate_handler!`** and no inlined domain logic.
5. **The service layer is the ACL.** pi-internal types are translated to the
   frontend contracts here (`agent:*` / `chat:stream-*` / `WorkspaceSession` /
   `ChatMessage`). `agent_service` drives pi through the **Engine Actor**, never
   by reaching into pi internals from a command.

### Frontend discipline (acceptance criteria)

1. **Feature self-containment.** Each `ui/src/features/<domain>/` owns its
   `components/ hooks/ atoms/ lib/`, exposing a **minimal public surface via
   `index.ts`**. **No cross-feature deep imports** — only the other feature's
   `index.ts`.
2. **Size caps**: component ≤ ~300 lines, hook/atom module ≤ ~200 lines,
   bridge-per-domain ≤ ~200 lines. Over cap ⇒ split.
3. **Separation of concerns.** Presentation components **do not `invoke`**.
   Data access goes through `lib/bridge/*` + hooks (TanStack Query / Jotai);
   side effects are centralized in hooks.
4. **Shared sinks down only.** Cross-feature reuse goes in `shared/`; features
   never depend horizontally on one another's internals.
5. **Single bridge entry.** All IPC goes through `lib/bridge/`; components and
   atoms never touch `@tauri-apps/api` directly. Command names + payload types
   are **generated** (`tauri-specta` / `ts-rs`), not hand-copied.

## Consequences

- **New code MUST conform.** No new domain command may be appended to
  `tauri_commands.rs`; no new business logic may land in `engine_sink.rs` /
  components.
- **Existing god files are decomposed incrementally**, pi-migration touchpoints
  first (agent, chat, workspace, cost, settings). Each extraction is its own
  bisectable PR: move commands → `commands/<domain>.rs`, logic → `services/<domain>.rs`,
  register via `commands/mod.rs`.
- **Known debt to repay** (introduced before this ADR): `get/set_http_api_enabled`
  + cwd lookup live in `tauri_commands.rs`; cost recompute + `cost_records`
  recording live in `engine_sink.rs`; pricing lives in `agent/types.rs`. These
  are the **first restructure targets** (→ `services/settings.rs`,
  `services/cost.rs` with a `PricingService`/`CostService` trait, thin
  `commands/{settings,cost}.rs`).
- `tauri-specta`/`ts-rs` adoption removes hand-maintained TS payload types and
  enforces the bridge contract.
