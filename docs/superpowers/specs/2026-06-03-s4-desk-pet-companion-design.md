# S4 — Desk-pet chat companion (floating bubble + MiniCPM)

**Date:** 2026-06-03
**Status:** Design (decisions locked in brainstorming; pending review)
**Sub-project of:** Local MiniCPM initiative (S0 ✓ #58 · S1 ✓ #60 · S2 · S3 · **S4**)
**Depends on:** S1 (in-process engine; S4 adds streaming to `LocalMistralRsProvider`). References: `github.com/OpenBMB/MiniCPM-Desk-Pet`.

---

## Problem & goal

A desktop chat companion that always lives on the desktop: a floating bubble you click to chat with MiniCPM, reusing/extending uClaw's existing pet. Plus runtime persona (LoRA) adapter switch/import, and porting MiniCPM-Desk-Pet characters into the existing pet roster.

## Decisions (brainstorming)

| Topic | Choice |
|---|---|
| Floating form | **Independent always-on-top, transparent, frameless Tauri window** (the pet lives on the desktop, outside the main app). |
| Existing pet | **Extend the existing `PetWidget` (Astro/Clawby) to be chat-capable** — one pet system; the desktop window renders the same widget + a chat bubble. |
| Persona adapters | **Settings → MiniCPM: dropdown switch + file import** of LoRA adapters (mistralrs preload + runtime activation). |
| MiniCPM-Desk-Pet chars | **Port as new roster members** = animation assets + persona prompt + (optional) LoRA adapter, selectable alongside Astro/Clawby. |

## Architecture

### 1. Streaming in the local provider (prerequisite within S4)
S1 left `LocalMistralRsProvider::stream` unsupported. S4 implements it via mistralrs's streaming chat API (`Model::stream_chat_request` / token callback) → maps tokens to `StreamDelta::TextDelta` and a final `Done{usage}`. For the pet, `enable_thinking` may be **on** with headroom + a `<think>…</think>` stripper (the pet can "think"), unlike the short memory tasks. Add an `EngineGenerateOpts { thinking: bool, max_tokens }` so memory (thinking off) and pet (thinking on, stripped) share the engine.

### 2. Desktop pet window (frontend + Tauri)
- A dedicated frameless, transparent, always-on-top, click-through-except-on-pet window. Created via Tauri `WebviewWindowBuilder` (Rust) or a `tauri.conf.json` window def — **inspect the app's existing window setup at implementation**. Route `?view=pet` (or a dedicated entry) rendering a `DeskPetApp`.
- `DeskPetApp` renders the existing `PetWidget` (driven by `pet-atoms`) + a `ChatBubble` overlay. Dragging moves the OS window (`appWindow.startDragging()`); the rest of the window is mouse-transparent so it doesn't block the desktop.
- Toggle "桌面伙伴" in Settings → 桌面宠物 / MiniCPM: show/hide the window, remember position.

### 3. Chat bubble ↔ MiniCPM
- `ChatBubble`: a compact input + streamed reply balloon anchored to the pet. On send → a `pet_chat(message)` Tauri command that runs a short conversation against the **local** model (engine.complete/stream with the active persona's system prompt + active LoRA adapter), streaming tokens back via an event (`pet:reply-delta`).
- Pet visual state (`pet-atoms`: idle/thinking/typing) is driven by the stream (reuse `usePetStateSync` patterns): thinking on request, typing on deltas, idle on done.
- Conversation is lightweight/ephemeral (a small ring buffer), not the main agent loop — the pet is a companion, not the agent.

### 4. Persona + LoRA adapters
- `PetPersona { id, display_name, character (asset set), system_prompt, adapter: Option<AdapterRef> }`. Built-in personas map to Astro/Clawby + ported MiniCPM-Desk-Pet chars.
- `local_llm` adapter layer: at engine load, `preload_adapters([...])`; `set_active_adapter(id)` activates one at runtime (mistralrs LoRA activation). Adapter files live under `~/.uclaw-pi/models/adapters/`.
- Settings → MiniCPM: persona dropdown (switches character + system_prompt + adapter together) + "导入适配器" file picker (copies a `.gguf`/`.safetensors` LoRA into the adapters dir, registers it, makes it selectable).

### 5. Porting MiniCPM-Desk-Pet characters
- For each ported character: add its animation assets under `/pet/<char>-<state>.webp` (convert from the source repo's format if needed), a persona entry (display name + system prompt capturing its voice), and optionally bundle/point to a LoRA adapter. Register in the `petCharacter` options so it appears next to Astro/Clawby.
- **License check**: confirm MiniCPM-Desk-Pet asset licensing permits redistribution; record in NOTICE if derived (per repo's Apache derivation procedure).

## Data flow (pet chat)
```
DeskPet window → ChatBubble send
  → pet_chat(msg)                                  [S4 cmd]
  → engine.stream(persona.system_prompt + msg, opts{thinking:on}, active_adapter)  [S1 engine + S4 stream]
  → tokens → pet:reply-delta events → ChatBubble balloon (streamed)
  → pet-atoms: thinking→typing→idle
```

## Error handling
- Model missing/unloaded → bubble shows "本地模型未就绪,去设置下载" (links to S3/Settings). No crash.
- Adapter load/activate failure → fall back to base model + persona prompt only; log; toast in Settings.
- Window: if the always-on-top/transparent window can't be created on a platform, degrade to the in-app PetWidget overlay (no desktop window).

## Testing
- Engine streaming: `EngineGenerateOpts` thinking on/off; `<think>` stripper (unit, fixture strings); stream maps to `StreamDelta` (unit with a stub).
- Persona/adapter: persona registry mapping (unit); adapter import copies+registers (unit with temp dir); active-adapter switch state (unit).
- Frontend: `ChatBubble` streaming render (vitest, mocked `pet:reply-delta`); persona switch updates character+prompt.
- Manual: the desktop window appears, drags, is click-through off-pet; chat streams from the local model; switching persona changes voice; importing an adapter then selecting it changes behavior.

## Scope guardrails
- **In S4:** desktop pet window, chat bubble ↔ local MiniCPM (streaming), persona/LoRA switch+import, porting MiniCPM-Desk-Pet characters, `stream` impl on the local provider.
- **Not S4:** making the pet drive the full agent loop/tools; multi-pet simultaneously; voice/TTS (separate effort); training adapters (import only).

## Risks
- **Tauri transparent always-on-top + click-through** behavior is platform-specific (macOS vs Windows); needs a spike (Task 1) on the real Tauri version. Degrade path defined above.
- **mistralrs LoRA activation API** for runtime switch must be verified (research said preload+activate; confirm 0.8.1 API in a spike).
- **mistralrs streaming API** shape (`stream_chat_request` / callback) — confirm in the same spike.
- **Asset licensing** for ported characters — must clear before redistribution.
