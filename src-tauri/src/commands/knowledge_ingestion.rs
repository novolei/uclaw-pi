//! Knowledge-ingestion-domain Tauri commands — thin wrappers.
//!
//! Per the code-organization ADR (2026-05-31), this domain has **no service**:
//! every command delegates to the [`crate::ingestion::IngestionManager`] held in
//! [`crate::app::AppState`] (`state.ingestion`) — it owns the job queue and all
//! ingestion logic behind `submit` / `status` / `list`, so there is **no inline
//! `state.db` SQL to lift** and the JUDGMENT RULE resolves to a thin move.
//!
//! Relocated verbatim from the legacy `tauri_commands.rs` god file (the
//! `// ─── Knowledge Ingestion Commands` section): the 4 `#[tauri::command]`s.
//! The `#[cfg(test)]` modules that followed the section in the god file
//! (`list_chat_sessions_for_spec_tests`, `learning_set_state_sql_tests`,
//! `setup_script_tests`) cover *other* domains and were left behind.

use tauri::State;

use crate::app::AppState;

#[tauri::command]
pub async fn ingest_files(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    paths: Vec<String>,
) -> Result<Vec<String>, String> {
    let mut ids = Vec::new();
    for p in paths {
        let id = state
            .ingestion
            .submit(crate::ingestion::IngestionSource::File(p), app.clone())
            .await;
        ids.push(id);
    }
    Ok(ids)
}

#[tauri::command]
pub async fn ingest_url(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    url: String,
) -> Result<String, String> {
    Ok(state
        .ingestion
        .submit(crate::ingestion::IngestionSource::Url(url), app)
        .await)
}

#[tauri::command]
pub async fn ingest_job_status(
    state: State<'_, AppState>,
    id: String,
) -> Result<Option<crate::ingestion::IngestionJob>, String> {
    Ok(state.ingestion.status(&id).await)
}

#[tauri::command]
pub async fn ingest_list_jobs(
    state: State<'_, AppState>,
) -> Result<Vec<crate::ingestion::IngestionJob>, String> {
    Ok(state.ingestion.list().await)
}
