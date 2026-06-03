// SPDX-License-Identifier: AGPL-3.0-or-later
//! Desk-pet (S4) Tauri commands — streamed local-MiniCPM chat, persona
//! selection (system-prompt-only), and the frameless transparent always-on-top
//! pet window.
//!
//! Streaming contract (frontend listens on these events):
//!   - `pet:reply-delta` { text }   per answer-text delta
//!   - `pet:reply-done`  {}         once the stream finishes
//!   - `pet:reply-error` { message } if the model isn't ready / stream fails
//!
//! Personas are system-prompt-only (mistralrs 0.8.1 has no runtime LoRA switch;
//! see `local_llm/s4_spike_test.rs`). The active persona id lives in a module
//! static — minimal, single desk pet at a time.

use std::sync::{OnceLock, RwLock};

use tauri::{AppHandle, Emitter, Manager};

use crate::error::Error;
use crate::local_llm::engine::{EngineGenerateOpts, LocalStreamChunk};
use crate::local_llm::persona::{self, PetPersona};

/// Window label for the desk-pet webview (matches the runtime builder below).
const DESKPET_LABEL: &str = "deskpet";

/// Pet generation knobs: snappy + short. thinking:false keeps replies fast
/// (flip later if a "thinking" pet state is desired).
const PET_MAX_TOKENS: u32 = 256;
const PET_TEMPERATURE: f32 = 0.7;

/// Active persona id (defaults to the first built-in). A module static keeps
/// the single-pet contract obvious — no AppState field needed.
fn active_persona_id() -> &'static RwLock<String> {
    static ID: OnceLock<RwLock<String>> = OnceLock::new();
    ID.get_or_init(|| RwLock::new(persona::default_persona().id))
}

/// Resolve the persona for this chat: explicit `persona_id` if it names a
/// built-in, else the persisted active persona, else the default.
fn resolve_persona(persona_id: Option<String>) -> PetPersona {
    if let Some(id) = persona_id {
        if let Some(p) = persona::persona_by_id(&id) {
            return p;
        }
    }
    let active = active_persona_id().read().unwrap().clone();
    persona::persona_by_id(&active).unwrap_or_else(persona::default_persona)
}

/// Stream a pet reply for `message` under `persona_id` (or the active persona).
/// Emits `pet:reply-delta` per delta then `pet:reply-done`; on failure emits
/// `pet:reply-error` and returns `Err` ("本地模型未就绪" when the model is
/// missing/unloadable).
#[tauri::command]
pub async fn pet_chat(
    app: AppHandle,
    message: String,
    persona_id: Option<String>,
) -> Result<(), Error> {
    let persona = resolve_persona(persona_id);

    let engine = crate::local_llm::local_engine().ok_or_else(|| {
        let _ = app.emit("pet:reply-error", serde_json::json!({ "message": "本地模型未就绪" }));
        Error::Internal("local LLM engine not initialized".into())
    })?;

    let opts = EngineGenerateOpts {
        thinking: false,
        max_tokens: PET_MAX_TOKENS,
        temperature: PET_TEMPERATURE,
    };

    let (_model_id, mut rx) = match engine
        .stream(persona.system_prompt.clone(), message, opts)
        .await
    {
        Ok(v) => v,
        Err(e) => {
            // Model missing / load failure → user-facing "本地模型未就绪".
            let _ = app.emit(
                "pet:reply-error",
                serde_json::json!({ "message": "本地模型未就绪" }),
            );
            return Err(Error::Internal(format!("pet_chat stream failed: {e}")));
        }
    };

    // Pump deltas to the frontend. If the pump errors mid-stream the channel
    // closes without a Done → emit reply-error so the bubble doesn't hang.
    let mut saw_done = false;
    while let Some(chunk) = rx.recv().await {
        match chunk {
            LocalStreamChunk::Text(text) => {
                let _ = app.emit("pet:reply-delta", serde_json::json!({ "text": text }));
            }
            LocalStreamChunk::Done { .. } => {
                saw_done = true;
                let _ = app.emit("pet:reply-done", serde_json::json!({}));
            }
        }
    }

    if !saw_done {
        let _ = app.emit(
            "pet:reply-error",
            serde_json::json!({ "message": "生成中断，请重试" }),
        );
        return Err(Error::Internal("pet_chat stream ended without Done".into()));
    }

    Ok(())
}

/// List the built-in personas for the Settings dropdown.
#[tauri::command]
pub async fn list_pet_personas() -> Result<Vec<PetPersona>, Error> {
    Ok(persona::builtin_personas())
}

/// Set the active persona by id. Errors if the id is unknown.
#[tauri::command]
pub async fn set_pet_persona(id: String) -> Result<(), Error> {
    if persona::persona_by_id(&id).is_none() {
        return Err(Error::Internal(format!("unknown persona id: {id}")));
    }
    *active_persona_id().write().unwrap() = id;
    Ok(())
}

/// Show the desk-pet window, creating it on first call (frameless, transparent,
/// always-on-top, click-through-by-default). Idempotent: a second call just
/// shows the existing window.
#[tauri::command]
pub async fn show_desk_pet(app: AppHandle) -> Result<(), Error> {
    if let Some(win) = app.get_webview_window(DESKPET_LABEL) {
        win.show().map_err(|e| Error::Internal(e.to_string()))?;
        let _ = win.set_focus();
        return Ok(());
    }

    let win = tauri::WebviewWindowBuilder::new(
        &app,
        DESKPET_LABEL,
        tauri::WebviewUrl::App("index.html?view=deskpet".into()),
    )
    .title("uClaw Desk Pet")
    .inner_size(220.0, 260.0)
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .resizable(false)
    .skip_taskbar(true)
    .shadow(false)
    .build()
    .map_err(|e| Error::Internal(format!("desk-pet window creation failed: {e}")))?;

    // Start click-through; the JS layer flips it off while the cursor is over
    // the opaque pet sprite (pointer hit-testing).
    if let Err(e) = win.set_ignore_cursor_events(true) {
        tracing::warn!(error = %e, "desk-pet: set_ignore_cursor_events failed");
    }
    Ok(())
}

/// Hide the desk-pet window if it exists (keeps it created for a fast re-show).
#[tauri::command]
pub async fn hide_desk_pet(app: AppHandle) -> Result<(), Error> {
    if let Some(win) = app.get_webview_window(DESKPET_LABEL) {
        win.hide().map_err(|e| Error::Internal(e.to_string()))?;
    }
    Ok(())
}

/// Toggle mouse-transparency (click-through) on the desk-pet window. The JS
/// layer calls this with `true` over empty areas and `false` over the sprite.
#[tauri::command]
pub async fn set_desk_pet_click_through(app: AppHandle, ignore: bool) -> Result<(), Error> {
    if let Some(win) = app.get_webview_window(DESKPET_LABEL) {
        win.set_ignore_cursor_events(ignore)
            .map_err(|e| Error::Internal(e.to_string()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_persona_explicit_wins() {
        let p = resolve_persona(Some("clawby".to_string()));
        assert_eq!(p.id, "clawby");
    }

    #[test]
    fn resolve_persona_unknown_falls_back_to_active_default() {
        let p = resolve_persona(Some("nope".to_string()));
        assert_eq!(p.id, persona::default_persona().id);
    }

    #[test]
    fn resolve_persona_none_uses_active() {
        let p = resolve_persona(None);
        assert_eq!(p.id, persona::default_persona().id);
    }
}
