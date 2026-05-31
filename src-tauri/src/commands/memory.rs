//! Memory-domain Tauri commands — thin wrappers.
//!
//! Per the code-organization ADR (2026-05-31), these have **no service**: every
//! command delegates to the in-memory [`crate::memory::MemoryStore`] held in
//! `state.memory_store` (an `Arc<MemoryStore>`). The store *is* the logic holder
//! — it owns its own SQL behind `set_full` / `get_full` / `search_full` / … — so
//! there is nothing to lift, and the JUDGMENT RULE resolves to a thin move. The
//! commands just parse input, call the store, and map [`crate::memory::MemoryEntry`]
//! into the wire [`MemoryEntryResponse`].
//!
//! `entry_to_response` was Memory-only and moves here as a module-private helper.

use tauri::State;

use crate::app::AppState;
use crate::error::Error;
use crate::ipc::{
    MemoryBulkImportInput, MemoryBulkImportResponse, MemoryClearInput, MemoryClearResponse,
    MemoryEntryResponse, MemoryGetInput, MemoryListInput, MemorySearchInput, MemorySetInput,
};

/// Map an in-memory [`crate::memory::MemoryEntry`] into the wire response. Shared
/// by every command that returns entries (`memory_set` / `memory_get` /
/// `memory_search` / `memory_list` / `memory_export`).
fn entry_to_response(e: crate::memory::MemoryEntry) -> MemoryEntryResponse {
    MemoryEntryResponse {
        id: e.id,
        key: e.key,
        value: e.value,
        kind: e.kind,
        namespace: e.namespace,
        space_id: e.space_id,
        tags: e.tags,
        metadata: e.metadata,
        created_at: e.created_at,
        updated_at: e.updated_at,
        expires_at: e.expires_at,
    }
}

#[tauri::command]
pub async fn memory_set(
    state: State<'_, AppState>,
    input: MemorySetInput,
) -> Result<MemoryEntryResponse, Error> {
    use crate::memory::{MemoryKind, SetMemoryOpts};
    let kind = MemoryKind::from_str(input.kind.as_deref().unwrap_or("note"));
    let entry = state.memory_store.set_full(SetMemoryOpts {
        space_id: input.space_id.unwrap_or_else(|| "global".into()),
        namespace: input.namespace.unwrap_or_else(|| "default".into()),
        key: input.key,
        value: input.value,
        kind,
        tags: input.tags.unwrap_or_default(),
        metadata: input.metadata,
        ttl_seconds: input.ttl_seconds,
    })?;
    Ok(entry_to_response(entry))
}

#[tauri::command]
pub async fn memory_get(
    state: State<'_, AppState>,
    input: MemoryGetInput,
) -> Result<Option<MemoryEntryResponse>, Error> {
    let namespace = input.namespace.unwrap_or_else(|| "default".into());
    let space_id = input.space_id.unwrap_or_else(|| "global".into());
    Ok(state
        .memory_store
        .get_full(&input.key, &namespace, &space_id)
        .map(entry_to_response))
}

#[tauri::command]
pub async fn memory_delete(
    state: State<'_, AppState>,
    input: MemoryGetInput,
) -> Result<bool, Error> {
    let namespace = input.namespace.unwrap_or_else(|| "default".into());
    let space_id = input.space_id.unwrap_or_else(|| "global".into());
    Ok(state
        .memory_store
        .delete_full(&input.key, &namespace, &space_id))
}

#[tauri::command]
pub async fn memory_search(
    state: State<'_, AppState>,
    input: MemorySearchInput,
) -> Result<Vec<MemoryEntryResponse>, Error> {
    let limit = input.limit.unwrap_or(20);
    let results = state.memory_store.search_full(
        &input.query,
        input.namespace.as_deref(),
        input.space_id.as_deref(),
        input.kind.as_deref(),
        limit,
    );
    Ok(results.into_iter().map(entry_to_response).collect())
}

#[tauri::command]
pub async fn memory_list(
    state: State<'_, AppState>,
    input: MemoryListInput,
) -> Result<Vec<MemoryEntryResponse>, Error> {
    use crate::memory::ListFilter;
    let filter = ListFilter {
        space_id: input.space_id,
        namespace: input.namespace,
        kind: input.kind,
        tag: input.tag,
        limit: input.limit,
        offset: input.offset,
    };
    let results = state.memory_store.list_filtered(&filter);
    Ok(results.into_iter().map(entry_to_response).collect())
}

#[tauri::command]
pub async fn memory_clear_namespace(
    state: State<'_, AppState>,
    input: MemoryClearInput,
) -> Result<MemoryClearResponse, Error> {
    let deleted = state
        .memory_store
        .clear_namespace(&input.namespace, input.space_id.as_deref());
    Ok(MemoryClearResponse { deleted })
}

#[tauri::command]
pub async fn memory_prune_expired(
    state: State<'_, AppState>,
) -> Result<MemoryClearResponse, Error> {
    let deleted = state.memory_store.prune_expired();
    Ok(MemoryClearResponse { deleted })
}

#[tauri::command]
pub async fn memory_bulk_import(
    state: State<'_, AppState>,
    input: MemoryBulkImportInput,
) -> Result<MemoryBulkImportResponse, Error> {
    use crate::memory::{MemoryKind, SetMemoryOpts};
    let entries: Vec<SetMemoryOpts> = input
        .entries
        .into_iter()
        .map(|e| SetMemoryOpts {
            space_id: e.space_id.unwrap_or_else(|| "global".into()),
            namespace: e.namespace.unwrap_or_else(|| "default".into()),
            key: e.key,
            value: e.value,
            kind: MemoryKind::from_str(e.kind.as_deref().unwrap_or("note")),
            tags: e.tags.unwrap_or_default(),
            metadata: e.metadata,
            ttl_seconds: e.ttl_seconds,
        })
        .collect();
    let result = state.memory_store.bulk_import(entries);
    Ok(MemoryBulkImportResponse {
        imported: result.imported,
        skipped: result.skipped,
        errors: result.errors,
    })
}

#[tauri::command]
pub async fn memory_export(
    state: State<'_, AppState>,
    input: MemoryListInput,
) -> Result<Vec<MemoryEntryResponse>, Error> {
    use crate::memory::ListFilter;
    let filter = ListFilter {
        space_id: input.space_id,
        namespace: input.namespace,
        kind: input.kind,
        tag: input.tag,
        limit: input.limit,
        offset: input.offset,
    };
    let results = state.memory_store.export(&filter);
    Ok(results.into_iter().map(entry_to_response).collect())
}

#[tauri::command]
pub async fn memory_list_namespaces(
    state: State<'_, AppState>,
    space_id: Option<String>,
) -> Result<Vec<String>, Error> {
    Ok(state.memory_store.list_namespaces(space_id.as_deref()))
}
