//! Conversation-domain Tauri commands — thin wrappers.
//!
//! Two flavors, both kept thin per the code-organization ADR (2026-05-31):
//!   - `create`/`list`/`delete` delegate to the in-memory
//!     [`crate::agent::session::SessionManager`] held in `state.session_manager`
//!     (the manager IS the logic holder — no SQL to lift, and it's an async
//!     `RwLock` rather than a `&Connection`), so they have no service.
//!   - `list_recent_threads`/`get_messages`/`toggle_star_conversation` have
//!     inline SQL → lifted into [`crate::services::conversation_service`]; these
//!     just lock `state.db`, call the service, and map errors.

use tauri::State;

use crate::app::AppState;
use crate::error::Error;
use crate::ipc::{
    ConversationResponse, CreateConversationInput, GetMessagesInput, MessageResponse, RecentThread,
    ToggleStarInput, ToggleStarResponse,
};
use crate::services::conversation_service::{ConversationService, DbConversation};

/// Create a new chat conversation via the session manager.
#[tauri::command]
pub async fn create_conversation(
    state: State<'_, AppState>,
    input: CreateConversationInput,
) -> Result<ConversationResponse, Error> {
    let space_id = input.space_id.unwrap_or_else(|| "default".into());
    let title = input.title.unwrap_or_else(|| "New Chat".into());

    let summary = {
        let mut session_mgr = state.session_manager.write().await;
        session_mgr.create(&title, &space_id)
    };

    Ok(ConversationResponse {
        id: summary.id,
        space_id: summary.space_id,
        title: summary.title,
        message_count: summary.message_count,
        created_at: summary.created_at,
        updated_at: summary.updated_at,
    })
}

/// List all chat conversations known to the session manager.
#[tauri::command]
pub async fn list_conversations(
    state: State<'_, AppState>,
) -> Result<Vec<ConversationResponse>, Error> {
    let session_mgr = state.session_manager.read().await;
    Ok(session_mgr
        .list()
        .into_iter()
        .map(|s| ConversationResponse {
            id: s.id,
            space_id: s.space_id,
            title: s.title,
            message_count: s.message_count,
            created_at: s.created_at,
            updated_at: s.updated_at,
        })
        .collect())
}

/// Merge recent chat conversations + agent sessions into one updated_at-DESC
/// list (cap 20) for the search palette's browse mode.
#[tauri::command]
pub async fn list_recent_threads(state: State<'_, AppState>) -> Result<Vec<RecentThread>, Error> {
    let conn = state
        .db
        .lock()
        .map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;
    DbConversation.list_recent_threads(&conn)
}

/// Read all messages for a conversation (reprojecting persisted ContentBlocks).
#[tauri::command]
pub async fn get_messages(
    state: State<'_, AppState>,
    input: GetMessagesInput,
) -> Result<Vec<MessageResponse>, Error> {
    let conn = state
        .db
        .lock()
        .map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;
    DbConversation.get_messages(&conn, &input.conversation_id)
}

/// Delete a conversation via the session manager. Returns whether it existed.
#[tauri::command]
pub async fn delete_conversation(state: State<'_, AppState>, id: String) -> Result<bool, Error> {
    let mut session_mgr = state.session_manager.write().await;
    Ok(session_mgr.delete(&id))
}

/// Flip a conversation's `starred` flag.
#[tauri::command]
pub async fn toggle_star_conversation(
    state: State<'_, AppState>,
    input: ToggleStarInput,
) -> Result<ToggleStarResponse, Error> {
    let starred = {
        let conn = state
            .db
            .lock()
            .map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;
        DbConversation.toggle_star(&conn, &input.conversation_id)?
    };
    Ok(ToggleStarResponse {
        conversation_id: input.conversation_id,
        starred,
    })
}
