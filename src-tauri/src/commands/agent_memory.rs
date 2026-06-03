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
