//! Read-only IPC for the agent's learned self-model (P3–P5 memory). Thin wrappers
//! over the `reflection_service` store helpers — generation + consolidation live
//! in the ReflectionService; these only READ, for the MemoryModule "成长" tab.

use crate::memory_graph::reflection_service::{
    DaydreamRow, ReflectionRow, UserModelHistoryRow,
};

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReflectionDto {
    pub insight: String,
    pub confidence: f64,
    pub created_at: String,
}
impl From<ReflectionRow> for ReflectionDto {
    fn from(r: ReflectionRow) -> Self {
        Self { insight: r.insight, confidence: r.confidence, created_at: r.created_at }
    }
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserModelDto {
    pub summary: String,
    pub updated_at: String,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DaydreamDto {
    pub content: String,
    pub created_at: String,
}
impl From<DaydreamRow> for DaydreamDto {
    fn from(r: DaydreamRow) -> Self {
        Self { content: r.content, created_at: r.created_at }
    }
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserModelHistoryDto {
    pub summary: String,
    pub replaced_at: String,
}
impl From<UserModelHistoryRow> for UserModelHistoryDto {
    fn from(r: UserModelHistoryRow) -> Self {
        Self { summary: r.summary, replaced_at: r.replaced_at }
    }
}

#[tauri::command]
pub async fn list_reflections(
    state: tauri::State<'_, crate::app::AppState>,
    limit: usize,
) -> Result<Vec<ReflectionDto>, String> {
    let rows = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        crate::memory_graph::reflection_service::recent_reflections(&conn, limit)
            .map_err(|e| e.to_string())?
    };
    Ok(rows.into_iter().map(Into::into).collect())
}

#[tauri::command]
pub async fn get_agent_user_model(
    state: tauri::State<'_, crate::app::AppState>,
) -> Result<Option<UserModelDto>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    match conn.query_row(
        "SELECT summary, updated_at FROM user_model WHERE id = 'default'",
        [],
        |r| Ok(UserModelDto { summary: r.get(0)?, updated_at: r.get(1)? }),
    ) {
        Ok(dto) => Ok(Some(dto)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
pub async fn list_daydreams(
    state: tauri::State<'_, crate::app::AppState>,
    limit: usize,
) -> Result<Vec<DaydreamDto>, String> {
    let rows = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        crate::memory_graph::reflection_service::recent_daydreams(&conn, limit)
            .map_err(|e| e.to_string())?
    };
    Ok(rows.into_iter().map(Into::into).collect())
}

#[tauri::command]
pub async fn list_user_model_history(
    state: tauri::State<'_, crate::app::AppState>,
    limit: usize,
) -> Result<Vec<UserModelHistoryDto>, String> {
    let rows = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        crate::memory_graph::reflection_service::recent_user_model_history(&conn, limit)
            .map_err(|e| e.to_string())?
    };
    Ok(rows.into_iter().map(Into::into).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reflection_dto_maps_from_row() {
        let row = ReflectionRow { insight: "x".into(), confidence: 0.9, created_at: "t".into() };
        let dto: ReflectionDto = row.into();
        assert_eq!(dto.insight, "x");
        assert!((dto.confidence - 0.9).abs() < 1e-9);
        assert_eq!(dto.created_at, "t");
    }

    #[test]
    fn daydream_and_history_dto_map_from_row() {
        let d: DaydreamDto = DaydreamRow { content: "c".into(), created_at: "t".into() }.into();
        assert_eq!(d.content, "c");
        let h: UserModelHistoryDto =
            UserModelHistoryRow { summary: "s".into(), replaced_at: "t".into() }.into();
        assert_eq!(h.summary, "s");
        assert_eq!(h.replaced_at, "t");
    }
}
