//! Persona-domain Tauri commands — thin wrappers.
//!
//! Per the code-organization ADR (2026-05-31), every command locks `state.db`
//! and delegates to [`crate::services::persona_service::DbPersona`], which owns
//! the SQL (via [`crate::agent::persona::store::PersonaStore`]) plus the
//! mutate-then-reload-timeline / render-prompt compositions that previously
//! lived inline here. Bodies do only: lock → call service → return.
//!
//! Input/output types come from [`crate::agent::persona`]; the
//! [`PersonaConfigResponse`] wire type lives with the service that builds it.

use tauri::State;

use crate::agent::persona::{
    BondProfile, CreatePersonaJournalEntryInput, PersonaRelationshipTimeline,
    PromotePersonaJournalEntryInput, ProposePersonaKeepsakeInput, RecordPersonaEventInput,
    UpdatePersonaBadgeVisibilityInput, UpdatePersonaKeepsakeStatusInput,
    UpdatePersonaRelationshipSettingsInput, VoiceProfile,
};
use crate::app::AppState;
use crate::error::Error;
use crate::services::persona_service::{DbPersona, PersonaConfigResponse, PersonaService};

/// Lock the shared DB connection, mapping a poisoned lock to an internal error.
fn lock_db<'a>(
    state: &'a State<'_, AppState>,
) -> Result<std::sync::MutexGuard<'a, rusqlite::Connection>, Error> {
    state
        .db
        .lock()
        .map_err(|e| Error::Internal(format!("DB lock: {e}")))
}

#[tauri::command]
pub async fn get_persona_config(
    state: State<'_, AppState>,
) -> Result<PersonaConfigResponse, Error> {
    let conn = lock_db(&state)?;
    DbPersona.voice_config(&conn)
}

#[tauri::command]
pub async fn update_persona_voice_profile(
    state: State<'_, AppState>,
    input: VoiceProfile,
) -> Result<PersonaConfigResponse, Error> {
    let conn = lock_db(&state)?;
    DbPersona.set_voice(&conn, input)
}

#[tauri::command]
pub async fn get_persona_relationship_timeline(
    state: State<'_, AppState>,
) -> Result<PersonaRelationshipTimeline, Error> {
    let conn = lock_db(&state)?;
    DbPersona.timeline(&conn)
}

#[tauri::command]
pub async fn record_persona_event(
    state: State<'_, AppState>,
    input: RecordPersonaEventInput,
) -> Result<PersonaRelationshipTimeline, Error> {
    let conn = lock_db(&state)?;
    DbPersona.record_event(&conn, &input)
}

#[tauri::command]
pub async fn propose_persona_keepsake(
    state: State<'_, AppState>,
    input: ProposePersonaKeepsakeInput,
) -> Result<PersonaRelationshipTimeline, Error> {
    let conn = lock_db(&state)?;
    DbPersona.propose_keepsake(&conn, &input)
}

#[tauri::command]
pub async fn update_persona_keepsake_status(
    state: State<'_, AppState>,
    input: UpdatePersonaKeepsakeStatusInput,
) -> Result<PersonaRelationshipTimeline, Error> {
    let conn = lock_db(&state)?;
    DbPersona.update_keepsake_status(&conn, &input)
}

#[tauri::command]
pub async fn create_persona_journal_entry(
    state: State<'_, AppState>,
    input: CreatePersonaJournalEntryInput,
) -> Result<PersonaRelationshipTimeline, Error> {
    let conn = lock_db(&state)?;
    DbPersona.create_journal_entry(&conn, &input)
}

#[tauri::command]
pub async fn delete_persona_journal_entry(
    state: State<'_, AppState>,
    id: String,
) -> Result<PersonaRelationshipTimeline, Error> {
    let conn = lock_db(&state)?;
    DbPersona.delete_journal_entry(&conn, &id)
}

#[tauri::command]
pub async fn promote_persona_journal_entry(
    state: State<'_, AppState>,
    input: PromotePersonaJournalEntryInput,
) -> Result<PersonaRelationshipTimeline, Error> {
    let conn = lock_db(&state)?;
    DbPersona.promote_journal_entry(&conn, &input)
}

#[tauri::command]
pub async fn update_persona_bond_profile(
    state: State<'_, AppState>,
    input: BondProfile,
) -> Result<PersonaRelationshipTimeline, Error> {
    let conn = lock_db(&state)?;
    DbPersona.update_bond_profile(&conn, &input)
}

#[tauri::command]
pub async fn update_persona_relationship_settings(
    state: State<'_, AppState>,
    input: UpdatePersonaRelationshipSettingsInput,
) -> Result<PersonaRelationshipTimeline, Error> {
    let conn = lock_db(&state)?;
    DbPersona.update_relationship_settings(&conn, &input)
}

#[tauri::command]
pub async fn update_persona_badge_visibility(
    state: State<'_, AppState>,
    input: UpdatePersonaBadgeVisibilityInput,
) -> Result<PersonaRelationshipTimeline, Error> {
    let conn = lock_db(&state)?;
    DbPersona.update_badge_visibility(&conn, &input)
}
