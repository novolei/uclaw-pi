//! Read-only IPC for the agent's learned self-model (P3–P5 memory). Thin wrappers
//! over the `reflection_service` store helpers — generation + consolidation live
//! in the ReflectionService; these only READ, for the MemoryModule "成长" tab.

use crate::memory_graph::reflection_service::{
    DaydreamRow, UserModelHistoryRow,
};

/// SQLite `datetime('now')` returns `"YYYY-MM-DD HH:MM:SS"` in UTC but with NO zone
/// marker, so the frontend's `new Date(...)` parses it as LOCAL time (off by the local
/// offset — e.g. "8 小时前" for a row created seconds ago in UTC+8). Normalize to
/// RFC3339 UTC (`...T...Z`) so the frontend parses it as UTC and relative times are right.
fn to_iso_utc(s: String) -> String {
    if s.len() == 19 && s.as_bytes().get(10) == Some(&b' ') {
        format!("{}Z", s.replace(' ', "T"))
    } else {
        s
    }
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReflectionDto {
    pub id: String,
    pub insight: String,
    pub confidence: f64,
    pub created_at: String,
    pub archived_at: Option<String>,
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
        Self { content: r.content, created_at: to_iso_utc(r.created_at) }
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
        Self { summary: r.summary, replaced_at: to_iso_utc(r.replaced_at) }
    }
}

#[tauri::command]
pub async fn list_reflections(
    state: tauri::State<'_, crate::app::AppState>,
    limit: usize,
    include_archived: Option<bool>,
) -> Result<Vec<ReflectionDto>, String> {
    let include = include_archived.unwrap_or(false);
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let sql = if include {
        "SELECT id, insight, confidence, created_at, archived_at FROM reflections \
         ORDER BY created_at DESC LIMIT ?1"
    } else {
        "SELECT id, insight, confidence, created_at, archived_at FROM reflections \
         WHERE archived_at IS NULL ORDER BY created_at DESC LIMIT ?1"
    };
    let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params![limit as i64], |r| {
            Ok(ReflectionDto {
                id: r.get(0)?,
                insight: r.get(1)?,
                confidence: r.get(2)?,
                created_at: to_iso_utc(r.get::<_, String>(3)?),
                archived_at: r.get::<_, Option<String>>(4)?.map(to_iso_utc),
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| e.to_string())?;
    Ok(rows)
}

#[tauri::command]
pub async fn get_agent_user_model(
    state: tauri::State<'_, crate::app::AppState>,
) -> Result<Option<UserModelDto>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    match conn.query_row(
        "SELECT summary, updated_at FROM user_model WHERE id = 'default'",
        [],
        |r| Ok(UserModelDto { summary: r.get(0)?, updated_at: to_iso_utc(r.get(1)?) }),
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

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileFactDto {
    pub id: String,
    pub title: String,
    pub created_at: String,
}

#[tauri::command]
pub async fn list_profile_facts(
    state: tauri::State<'_, crate::app::AppState>,
) -> Result<Vec<ProfileFactDto>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare(
        "SELECT id, title, created_at FROM memory_nodes \
         WHERE kind = 'user_profile' AND title IS NOT NULL AND title != '' \
         ORDER BY created_at DESC LIMIT 100",
    ).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| Ok(ProfileFactDto {
            id: r.get(0)?, title: r.get(1)?, created_at: to_iso_utc(r.get::<_, String>(2)?),
        }))
        .map_err(|e| e.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| e.to_string())?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reflections_query_round_trips_id_and_archived() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::memory_graph::reflection_service::apply_reflections_schema(&conn);
        use crate::memory_graph::reflection_service::insert_reflection;
        insert_reflection(&conn, "r1", "live one", 0.9, 0).unwrap();
        insert_reflection(&conn, "r2", "archived one", 0.8, 0).unwrap();
        conn.execute("UPDATE reflections SET archived_at = datetime('now') WHERE id='r2'", []).unwrap();
        let live: Vec<(String, Option<String>)> = conn
            .prepare("SELECT id, archived_at FROM reflections WHERE archived_at IS NULL").unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().flatten().collect();
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].0, "r1");
        assert!(live[0].1.is_none());
        let all: i64 = conn.query_row("SELECT COUNT(*) FROM reflections", [], |r| r.get(0)).unwrap();
        assert_eq!(all, 2);
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
