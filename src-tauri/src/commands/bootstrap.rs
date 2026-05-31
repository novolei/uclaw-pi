//! Bootstrap-domain Tauri commands — thin wrappers.
//!
//! Per the code-organization ADR (2026-05-31), this domain has **no service**:
//! both commands read/write the in-memory user-settings handle held in
//! [`crate::app::AppState`] (`state.settings`, an `RwLock<UserSettings>` persisted
//! to the on-disk config file via `UserSettings::save`) plus a couple of paths off
//! `AppState` — there is **no inline `state.db` SQL to lift**, so the JUDGMENT RULE
//! resolves to a thin move.
//!
//! Relocated verbatim from the legacy `tauri_commands.rs` god file (the
//! `// ─── Bootstrap Commands` section): the 2 `#[tauri::command]`s. The HTTP-API
//! toggle that historically shared this section already moved to
//! `commands::settings` + `services::settings_service` in an earlier slice.

use tauri::State;

use crate::app::AppState;
use crate::error::Error;
use crate::ipc::{GetSettingsResponse, PatchSettingsInput};

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<GetSettingsResponse, Error> {
    let settings = state.settings.read().await;
    Ok(GetSettingsResponse {
        language: settings.language.clone(),
        theme: settings.theme.clone(),
        config_path: state.config_path.to_string_lossy().into(),
        data_path: state.data_dir.to_string_lossy().into(),
        monthly_budget_usd: settings.monthly_budget_usd,
    })
}

#[tauri::command]
pub async fn patch_settings(state: State<'_, AppState>, input: PatchSettingsInput) -> Result<GetSettingsResponse, Error> {
    let mut settings = state.settings.write().await;
    if let Some(lang) = input.language {
        settings.language = lang;
    }
    if let Some(theme) = input.theme {
        settings.theme = theme;
    }
    // Outer Some = field was present in the JSON; inner is the new value (or None to clear).
    if let Some(budget) = input.monthly_budget_usd {
        // Clamp negatives/zero to None — belt-and-suspenders for IPC robustness.
        settings.monthly_budget_usd = budget.filter(|&b| b > 0.0);
    }
    settings.save(&state.config_path)?;
    drop(settings);
    get_settings(state).await
}
