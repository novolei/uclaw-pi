//! Notification-domain Tauri commands.
//!
//! No DB and no new service: these delegate to the in-memory
//! [`crate::notifications::NotificationManager`] held in `state.notifications`
//! (the manager IS the service). Relocated verbatim from `tauri_commands.rs`
//! per the code-organization ADR (2026-05-31).

use tauri::State;

use crate::app::AppState;
use crate::error::Error;
use crate::ipc::NotificationItem;

/// The notification history (newest-first), mapped to the IPC shape.
#[tauri::command]
pub async fn get_notifications(state: State<'_, AppState>) -> Result<Vec<NotificationItem>, Error> {
    let mgr = state.notifications.lock().await;
    Ok(mgr
        .history()
        .into_iter()
        .map(|n| NotificationItem {
            id: n.id,
            title: n.title,
            message: n.message,
            level: match n.level {
                crate::notifications::NotificationLevel::Info => "info".into(),
                crate::notifications::NotificationLevel::Success => "success".into(),
                crate::notifications::NotificationLevel::Warning => "warning".into(),
                crate::notifications::NotificationLevel::Error => "error".into(),
            },
            source: n.source,
            timestamp: n.timestamp,
        })
        .collect())
}

/// Clear the in-memory notification history.
#[tauri::command]
pub async fn clear_notifications(state: State<'_, AppState>) -> Result<bool, Error> {
    let mut mgr = state.notifications.lock().await;
    mgr.clear();
    Ok(true)
}
