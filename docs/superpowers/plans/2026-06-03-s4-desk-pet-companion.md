# Desk-Pet Chat Companion (S4) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development or superpowers:executing-plans. Steps use `- [ ]`.
> **Status:** task-level roadmap. Implement AFTER S1 (#60) merges (S2/S3 optional but recommended). Two spikes gate the risky externals. Detail-fill code at execution against the merged `local_llm` module, the real `PetWidget`/`pet-atoms`, and the app's Tauri window setup (inspect first).

**Goal:** A desktop chat companion — an always-on-top transparent pet window with a chat bubble talking to local MiniCPM (streaming), runtime LoRA persona switch/import, and ported MiniCPM-Desk-Pet characters in the existing roster.

**Architecture:** Add streaming + persona/adapter to the S1 engine; a dedicated frameless transparent Tauri window rendering the extended `PetWidget` + a `ChatBubble`; Settings → MiniCPM persona dropdown + adapter import.

**Spec:** `docs/superpowers/specs/2026-06-03-s4-desk-pet-companion-design.md`

**Branch:** `pi/s4-desk-pet-companion` — stacked on S1 (and S3 if landed), rebase onto main when foundation merges.

---

## File structure

| File | Responsibility |
|---|---|
| `src-tauri/src/local_llm/engine.rs` | `stream` + `EngineGenerateOpts{thinking,max_tokens}` + `<think>` strip helper |
| `src-tauri/src/local_llm/provider.rs` | implement `LlmProvider::stream` (token→`StreamDelta`) |
| `src-tauri/src/local_llm/adapters.rs` | persona/LoRA registry; preload + `set_active_adapter`; import |
| `src-tauri/src/commands/pet.rs` | `pet_chat(msg)` (streams `pet:reply-delta`), `set_pet_persona`, `import_pet_adapter`, window show/hide |
| `src-tauri/src/main.rs` / window setup | create the frameless transparent always-on-top pet window |
| `ui/src/features/agent/components/PetWidget.tsx` | extend: chat-capable; render in desktop window |
| `ui/src/components/deskpet/DeskPetApp.tsx` + `ChatBubble.tsx` | desktop-window root + chat balloon (streamed) |
| `ui/src/atoms/pet-atoms.ts` | add persona/active-adapter atoms |
| `ui/src/features/settings/components/...MiniCPM...` | persona dropdown + 导入适配器 |
| `public/pet/<char>-<state>.webp` | ported MiniCPM-Desk-Pet character assets |
| `NOTICE` | derivation note if assets/adapters are redistributed |

## Tasks (ordered; two spikes first)

### Task 1 (SPIKE, gate): mistralrs streaming + LoRA runtime activation API
- In a `#[ignore]` test (real model), confirm mistralrs 0.8.1: (a) streaming chat API shape + token/usage callback; (b) `preload_adapters` + runtime `set_active_adapter` (or equivalent) actually switches output. Record the exact API in comments. If LoRA runtime-switch isn't workable, fall back to "persona = system prompt only" (note in spec) — do NOT block streaming on it.
- Commit spike.

### Task 2 (SPIKE, gate): Tauri transparent always-on-top click-through window
- Prototype a frameless, transparent, always-on-top window on the app's Tauri version; verify drag (`startDragging`) + mouse-transparency off the pet on macOS. Record the working window config. If unworkable, the degrade path is the in-app overlay (spec) — note it.
- Commit spike.

### Task 3: engine streaming + thinking opts
- Add `EngineGenerateOpts{thinking,max_tokens}`; refactor `generate` to take it (memory path passes `thinking:false`, S1 behavior preserved). Add `stream` on the engine using the spike's API; add a `<think>…</think>` stripper (pure fn, unit-tested).
- Tests: stripper fixtures; opts plumb through; memory path still `thinking:false`. Commit.

### Task 4: `LocalMistralRsProvider::stream`
- Implement `stream` mapping tokens → `StreamDelta::TextDelta` + final `Done{usage}` (replaces S1's unsupported stub). Keep `complete` as-is.
- Test: stub-backed stream yields deltas then Done. Commit.

### Task 5: adapters/persona backend (`adapters.rs`)
- `PetPersona{id,display_name,character,system_prompt,adapter:Option<...>}` registry (built-ins for Astro/Clawby + ported chars). Adapter dir `~/.uclaw-pi/models/adapters/`; `import_adapter(path)` copies+registers; `set_active_adapter(id)` activates (spike API). `pet_chat` runs engine.stream with persona prompt + active adapter.
- Tests: registry mapping; import copies+registers (temp dir); active-adapter state. Commit.

### Task 6: pet Tauri commands + window
- `pet_chat`, `set_pet_persona`, `import_pet_adapter`, `show/hide_desk_pet`. Register in `generate_handler!`. Create the pet window (spike config) toggled by a setting; persist position.
- Commit.

### Task 7: `DeskPetApp` + `ChatBubble` (frontend)
- Desktop-window root rendering the extended `PetWidget` + `ChatBubble` (compact input + streamed balloon via `pet:reply-delta`). Drag moves the OS window; off-pet area mouse-transparent. Pet state from the stream (reuse `usePetStateSync` patterns).
- vitest: bubble streams a mocked reply; send calls `pet_chat`. Commit.

### Task 8: extend `PetWidget` + persona atoms
- Make `PetWidget` chat-capable / render in the desktop window; add persona + active-adapter atoms; hover/click opens the bubble.
- vitest for the new states. Commit.

### Task 9: Settings → MiniCPM persona + adapter import UI
- Persona dropdown (switches character+prompt+adapter); 导入适配器 file picker → `import_pet_adapter`. Follow settings patterns.
- `npx tsc --noEmit` clean. Commit.

### Task 10: port MiniCPM-Desk-Pet characters
- **License check first** (clear redistribution; update `NOTICE` per the repo's derivation procedure). Add assets `/pet/<char>-<state>.webp`, persona entries (voice in system_prompt), optional bundled adapter. Register in `petCharacter` options.
- Manual visual check. Commit.

## Verification
- `cargo test --lib local_llm` (stream, stripper, adapters) + `cargo build` clean.
- `ui`: `npx tsc --noEmit`, vitest for bubble/persona.
- Manual: pet window appears/drags/click-through; chat streams from local MiniCPM; persona switch changes voice; import adapter → select → behavior changes; ported characters selectable.

## Notes / risks
- Two external unknowns are spike-gated (Tasks 1-2) with defined degrade paths (system-prompt-only personas; in-app overlay).
- Stacked on S1; rebase onto main when foundation merges.
- Asset/adapter redistribution licensing MUST be cleared (Task 10) before bundling MiniCPM-Desk-Pet content.
