# Local MiniCPM + Desk-Pet — E2E Acceptance Guide

Covers verifying the S0→S5 initiative (per-scenario routing, local MiniCPM runtime,
smart download, first-launch onboarding, desk-pet companion, the S5 wiring).

There are **two tracks**. The browser track is automated and fast but mock-backed; the
full-app track is the only way to verify real download / Metal inference / the transparent
desk-pet window / scenario routing / $0 cost.

| | Track 1 — Browser (mock) | Track 2 — Full Tauri app |
|---|---|---|
| Command | `cd ui && npm run e2e` (or `npm run dev:mock-tauri` + open `:9527`) | `cargo tauri dev` (Tauri v2 CLI) |
| Backend | mocked (`ui/src/lib/dev-tauri-mock.ts`) | real Rust + mistralrs + Metal |
| Verifies | UI rendering, flows, state, onboarding gate, deskpet route, streaming UI (mock events) | real download, inference, transparent window, routing, cost |
| Automated? | yes (Playwright) | no (manual checklist below) |
| Data dir | n/a | `~/.uclaw-pi` (override with `UCLAW_HOME`) |

---

## Track 1 — Browser smoke (automated)

```bash
cd ui
npm install                       # first time (pulls @playwright/test)
npx playwright install chromium   # first time (browser binary)
npm run e2e                       # runs ui/e2e/smoke.spec.ts against dev:mock-tauri
```

`playwright.config.ts` auto-starts `npm run dev:mock-tauri` (http://127.0.0.1:9527) via its
`webServer`. The smoke (`ui/e2e/smoke.spec.ts`) asserts:

1. App loads with no uncaught console errors.
2. First-run onboarding gate shows (welcome step) when the onboarding flag is cleared; skip
   reaches the app.
3. Settings → 智能 → LocalModelSettings renders; quant selector shows Q4_K_M / Q8_0 / F16;
   clicking download drives the mocked progress UI (`local-model:download-progress` events).
4. `/?view=deskpet` renders `DeskPetApp` + `ChatBubble`; sending a message shows the
   mock-streamed reply (`pet:reply-delta`/`pet:reply-done`).
5. The persona dropdown lists Clawd among the 5 personas.

To explore manually in a browser: `cd ui && npm run dev:mock-tauri` then open
`http://127.0.0.1:9527` (and `…/?view=deskpet`). The mocked commands live in
`ui/src/lib/dev-tauri-mock.ts` — extend the fixtures there if a new flow needs UI coverage.

**Limitation:** the mock returns canned data. Real download/inference/window behavior is
NOT exercised — that's Track 2.

Component-level checks (also automated, separate from Playwright):
```bash
cd ui && npm test          # vitest (per-test bridge mocks): App gate, LocalModelSettings, PetWidget, ChatBubble, …
cd src-tauri && cargo test --lib local_llm providers::service memory_os_llm
```

---

## Track 2 — Full app manual acceptance (real backend)

Run the native app on the macOS desktop session:
```bash
cargo tauri dev            # if the CLI is missing: cargo install tauri-cli
# Engine routing log on stdout:
RUST_LOG=uclaw_core=info cargo tauri dev
```

Work through the checklist. "Reset to fresh state" is in the appendix.

### A. First-launch onboarding (S3 + S5-C)
- [ ] With **no model configured** (fresh data dir, see appendix), launch → the onboarding
      wizard appears (welcome → API key → **本地模型** → theme → done).
- [ ] The 本地模型 step shows the env checklist (disk / RAM / Metal / network) with ✅/⚠️/❌.
- [ ] "跳过" advances; finishing/ skipping sets `localStorage['uclaw.onboarding.complete']='1'`.
- [ ] Relaunch → onboarding does **not** reappear.
- [ ] A profile that already has an active model **never** shows onboarding (even without the flag).

### B. Per-scenario routing — the original question (S0 + S1)
- [ ] Settings → 智能 → 模型分配: assign **摘要模型 (summarizer)** to `MiniCPM (本地) / minicpm5-1b`
      (download the model first if needed — see C).
- [ ] Trigger a memory consolidation/summary pass (use the app long enough, or the memory
      inspector's 立即整合).
- [ ] With `RUST_LOG=uclaw_core=info`, confirm the log line:
      `memory_os_llm: routed completion to role  role=summarizer  resolved_model=minicpm5-1b`.
- [ ] Confirm the Memory-OS pass produced non-empty text (the `enable_thinking(false)` fix) and
      the cost record for it is **$0** (local inference is not billed).
- [ ] (Sanity) The main chat turn still uses your configured chat model, not the local one.

### C. Smart download + quant (S2 + S5-B)
- [ ] Settings → 本地模型: the quant selector offers Q4_K_M / Q8_0 / F16.
- [ ] Click download → progress bar shows a **source label** (ModelScope/HF) + phase
      (probing → downloading → verifying) + cancel.
- [ ] After completion, the GGUF exists at `~/.uclaw-pi/models/minicpm5-1b/MiniCPM5-1B-<QUANT>.gguf`
      and its SHA256 matches (the downloader verifies; a tampered file is rejected).
- [ ] Kill the app mid-download, relaunch, re-download → it **resumes** (Range) from the `.part`.
- [ ] Select a **different quant** (e.g. Q8_0) → after downloading it, the next local inference
      loads the Q8_0 file (the engine force-unloads on quant change). Confirm via the model file
      used / log.
- [ ] (Network) Throttle or block one source → the probe picks the other (or fails over).

### D. Engine lifecycle (S1)
- [ ] First local-model use loads + warms the model (a few-second first-call latency).
- [ ] Leave it idle > 10 min → log `local_llm: unloaded idle MiniCPM model to free RAM`; RAM drops.
- [ ] Next use reloads transparently.

### E. Desk-pet companion (S4 + S5-A)
- [ ] Settings → 桌面宠物: toggle **桌面伙伴** on → a frameless, **transparent, always-on-top**
      pet window appears on the desktop.
- [ ] Drag the pet → the window moves. Click on **empty** area around the pet → the click passes
      **through** to whatever is behind (click-through); clicking the pet/bubble does not.
- [ ] Click the pet → a chat bubble opens; type a message → the reply **streams** token-by-token
      from the local MiniCPM model; pet state animates thinking → typing → idle.
- [ ] Switch persona to **Clawd** → the sprite changes to the clawd animations
      (`/pet/clawd-*.webp`) and the voice/tone changes; also try astro/clawby/sprout/pixel.
- [ ] If the local model isn't present, the bubble shows "本地模型未就绪…" (no crash).

### F. Regression sanity
- [ ] Cloud-only usage (no local model assigned) behaves exactly as before — main chat, memory
      passes fall back to the chat/active model; no errors.

---

## Appendix — reset to a fresh state

**Fresh install (for onboarding / first-run):**
```bash
mv ~/.uclaw-pi ~/.uclaw-pi.bak      # hide the real profile (providers.json, models, db)
# in the app's webview devtools console, also clear the onboarding flag if testing the gate:
#   localStorage.removeItem('uclaw.onboarding.complete')
# restore afterwards:
# rm -rf ~/.uclaw-pi && mv ~/.uclaw-pi.bak ~/.uclaw-pi
```
Or point the app at a throwaway home: `UCLAW_HOME=/tmp/uclaw-test cargo tauri dev`.

**Re-trigger onboarding without wiping the profile:** in devtools console
`localStorage.removeItem('uclaw.onboarding.complete')` AND ensure no active model
(or temporarily clear it in Settings), then reload.

**Model location:** `~/.uclaw-pi/models/minicpm5-1b/` · **config:** `~/.uclaw-pi/providers.json`
(holds `role_models` + `active_local_quant`).

**Useful logs:** run with `RUST_LOG=uclaw_core=info` (or `=debug` for more) and watch for
`memory_os_llm: routed completion to role`, `local_llm: unloaded idle MiniCPM model`, and the
download/verify lines.
