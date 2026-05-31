//! Persona-domain logic as a Tauri-independent service — the business logic
//! behind the `commands::persona` thin commands.
//!
//! Operates on a borrowed `&Connection` (never `AppState`/`State`) so it
//! unit-tests without Tauri. Extracted from the legacy `tauri_commands.rs` god
//! file per the code-organization ADR (2026-05-31).
//!
//! The raw SQL already lives in [`crate::agent::persona::store::PersonaStore`]
//! (a per-`&Connection` store). What previously lived inline in the command
//! bodies — and is lifted here — is the **composition** around it: every
//! mutating persona command performs its write and then re-reads the full
//! relationship timeline to return (the old `persona_relationship_timeline_response`
//! helper), and the voice-profile commands additionally render the persona
//! prompt block into a [`PersonaConfigResponse`] (the old `persona_config_response`
//! helper). Owning those compositions here makes the commands one-liners and the
//! mutate-then-reload behavior unit-testable.

use rusqlite::Connection;

use crate::agent::persona::store::PersonaStore;
use crate::agent::persona::{
    BondProfile, CreatePersonaJournalEntryInput, PersonaRelationshipTimeline,
    PromotePersonaJournalEntryInput, ProposePersonaKeepsakeInput, RecordPersonaEventInput,
    UpdatePersonaBadgeVisibilityInput, UpdatePersonaKeepsakeStatusInput,
    UpdatePersonaRelationshipSettingsInput, VoiceProfile,
};
use crate::error::Error;

/// The persona config wire response: the built-in presets, the saved (or
/// default) global voice profile, and the rendered persona prompt block for a
/// live preview. Built by [`PersonaService::voice_config`] /
/// [`PersonaService::set_voice`].
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonaConfigResponse {
    pub presets: Vec<crate::agent::persona::PersonaPreset>,
    pub voice: VoiceProfile,
    pub rendered_prompt: String,
}

/// Render the config response for a given voice profile: attach the built-in
/// presets and the rendered prompt block (with default bond / gamification off,
/// matching the legacy preview).
fn build_config_response(voice: VoiceProfile) -> PersonaConfigResponse {
    let rendered_prompt = crate::agent::persona::render_persona_prompt_block(
        &crate::agent::persona::PersonaPromptContext {
            voice: voice.clone(),
            bond: BondProfile::default(),
            relationship_gamification_enabled: false,
        },
    );
    PersonaConfigResponse {
        presets: crate::agent::persona::built_in_presets(),
        voice,
        rendered_prompt,
    }
}

/// Persona configuration + relationship-timeline operations (the persona tables,
/// via [`PersonaStore`]).
pub trait PersonaService {
    /// Read the saved global voice profile (or the default) and render it into a
    /// [`PersonaConfigResponse`].
    fn voice_config(&self, conn: &Connection) -> Result<PersonaConfigResponse, Error>;
    /// Clamp + persist the global voice profile, then render the fresh response.
    fn set_voice(
        &self,
        conn: &Connection,
        voice: VoiceProfile,
    ) -> Result<PersonaConfigResponse, Error>;
    /// Read the full relationship timeline.
    fn timeline(&self, conn: &Connection) -> Result<PersonaRelationshipTimeline, Error>;
    /// Record a relationship event, then return the fresh timeline.
    fn record_event(
        &self,
        conn: &Connection,
        input: &RecordPersonaEventInput,
    ) -> Result<PersonaRelationshipTimeline, Error>;
    /// Propose a keepsake, then return the fresh timeline.
    fn propose_keepsake(
        &self,
        conn: &Connection,
        input: &ProposePersonaKeepsakeInput,
    ) -> Result<PersonaRelationshipTimeline, Error>;
    /// Update a keepsake's status, then return the fresh timeline.
    fn update_keepsake_status(
        &self,
        conn: &Connection,
        input: &UpdatePersonaKeepsakeStatusInput,
    ) -> Result<PersonaRelationshipTimeline, Error>;
    /// Create a journal entry, then return the fresh timeline.
    fn create_journal_entry(
        &self,
        conn: &Connection,
        input: &CreatePersonaJournalEntryInput,
    ) -> Result<PersonaRelationshipTimeline, Error>;
    /// Delete a journal entry by id, then return the fresh timeline.
    fn delete_journal_entry(
        &self,
        conn: &Connection,
        id: &str,
    ) -> Result<PersonaRelationshipTimeline, Error>;
    /// Promote a journal entry, then return the fresh timeline.
    fn promote_journal_entry(
        &self,
        conn: &Connection,
        input: &PromotePersonaJournalEntryInput,
    ) -> Result<PersonaRelationshipTimeline, Error>;
    /// Upsert the global bond profile, then return the fresh timeline.
    fn update_bond_profile(
        &self,
        conn: &Connection,
        bond: &BondProfile,
    ) -> Result<PersonaRelationshipTimeline, Error>;
    /// Update relationship settings, then return the fresh timeline.
    fn update_relationship_settings(
        &self,
        conn: &Connection,
        input: &UpdatePersonaRelationshipSettingsInput,
    ) -> Result<PersonaRelationshipTimeline, Error>;
    /// Update badge visibility, then return the fresh timeline.
    fn update_badge_visibility(
        &self,
        conn: &Connection,
        input: &UpdatePersonaBadgeVisibilityInput,
    ) -> Result<PersonaRelationshipTimeline, Error>;
}

/// The SQLite-backed implementation (the only one in production). Each method
/// constructs a [`PersonaStore`] over the borrowed connection.
pub struct DbPersona;

impl PersonaService for DbPersona {
    fn voice_config(&self, conn: &Connection) -> Result<PersonaConfigResponse, Error> {
        let voice = PersonaStore::new(conn)
            .get_global_voice_profile()
            .map_err(Error::Database)?
            .unwrap_or_default();
        Ok(build_config_response(voice))
    }

    fn set_voice(
        &self,
        conn: &Connection,
        voice: VoiceProfile,
    ) -> Result<PersonaConfigResponse, Error> {
        let voice = voice.clamp();
        PersonaStore::new(conn)
            .upsert_global_voice_profile(&voice)
            .map_err(Error::Database)?;
        Ok(build_config_response(voice))
    }

    fn timeline(&self, conn: &Connection) -> Result<PersonaRelationshipTimeline, Error> {
        PersonaStore::new(conn)
            .relationship_timeline()
            .map_err(Error::Database)
    }

    fn record_event(
        &self,
        conn: &Connection,
        input: &RecordPersonaEventInput,
    ) -> Result<PersonaRelationshipTimeline, Error> {
        PersonaStore::new(conn)
            .record_event(input)
            .map_err(Error::Database)?;
        self.timeline(conn)
    }

    fn propose_keepsake(
        &self,
        conn: &Connection,
        input: &ProposePersonaKeepsakeInput,
    ) -> Result<PersonaRelationshipTimeline, Error> {
        PersonaStore::new(conn)
            .propose_keepsake(input)
            .map_err(Error::Database)?;
        self.timeline(conn)
    }

    fn update_keepsake_status(
        &self,
        conn: &Connection,
        input: &UpdatePersonaKeepsakeStatusInput,
    ) -> Result<PersonaRelationshipTimeline, Error> {
        PersonaStore::new(conn)
            .update_keepsake_status(input)
            .map_err(Error::Database)?;
        self.timeline(conn)
    }

    fn create_journal_entry(
        &self,
        conn: &Connection,
        input: &CreatePersonaJournalEntryInput,
    ) -> Result<PersonaRelationshipTimeline, Error> {
        PersonaStore::new(conn)
            .create_journal_entry(input)
            .map_err(Error::Database)?;
        self.timeline(conn)
    }

    fn delete_journal_entry(
        &self,
        conn: &Connection,
        id: &str,
    ) -> Result<PersonaRelationshipTimeline, Error> {
        PersonaStore::new(conn)
            .delete_journal_entry(id)
            .map_err(Error::Database)?;
        self.timeline(conn)
    }

    fn promote_journal_entry(
        &self,
        conn: &Connection,
        input: &PromotePersonaJournalEntryInput,
    ) -> Result<PersonaRelationshipTimeline, Error> {
        PersonaStore::new(conn)
            .promote_journal_entry(input)
            .map_err(Error::Database)?;
        self.timeline(conn)
    }

    fn update_bond_profile(
        &self,
        conn: &Connection,
        bond: &BondProfile,
    ) -> Result<PersonaRelationshipTimeline, Error> {
        PersonaStore::new(conn)
            .upsert_global_bond_profile(bond)
            .map_err(Error::Database)?;
        self.timeline(conn)
    }

    fn update_relationship_settings(
        &self,
        conn: &Connection,
        input: &UpdatePersonaRelationshipSettingsInput,
    ) -> Result<PersonaRelationshipTimeline, Error> {
        PersonaStore::new(conn)
            .update_relationship_settings(input)
            .map_err(Error::Database)?;
        self.timeline(conn)
    }

    fn update_badge_visibility(
        &self,
        conn: &Connection,
        input: &UpdatePersonaBadgeVisibilityInput,
    ) -> Result<PersonaRelationshipTimeline, Error> {
        PersonaStore::new(conn)
            .update_badge_visibility(input)
            .map_err(Error::Database)?;
        self.timeline(conn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        conn
    }

    #[test]
    fn voice_config_defaults_when_unset() {
        let conn = test_conn();
        let resp = DbPersona.voice_config(&conn).unwrap();
        // No row persisted yet → the default profile.
        assert_eq!(resp.voice, VoiceProfile::default());
        // Presets are the built-in set and the prompt block is non-empty.
        assert!(!resp.presets.is_empty());
        assert!(!resp.rendered_prompt.is_empty());
    }

    #[test]
    fn set_voice_clamps_persists_and_round_trips() {
        let conn = test_conn();
        let mut profile = VoiceProfile::default();
        profile.warmth = 99; // out of range → clamp to 5
        profile.directness = 1;
        let saved = DbPersona.set_voice(&conn, profile).unwrap();
        assert_eq!(saved.voice.warmth, 5, "clamped to ceiling");
        assert_eq!(saved.voice.directness, 1);
        // Re-read through the service: the persisted value comes back.
        let reread = DbPersona.voice_config(&conn).unwrap();
        assert_eq!(reread.voice.warmth, 5);
        assert_eq!(reread.voice.directness, 1);
    }

    #[test]
    fn record_event_returns_fresh_timeline() {
        use crate::agent::persona::PersonaEventKind;
        let conn = test_conn();
        // Empty to start.
        let before = DbPersona.timeline(&conn).unwrap();
        let input = RecordPersonaEventInput {
            kind: PersonaEventKind::PositiveFeedback,
            session_id: None,
            task_id: None,
            minutes: 0,
            weight: 1,
            note: Some("nice work".to_string()),
            evidence: vec![],
        };
        let after = DbPersona.record_event(&conn, &input).unwrap();
        // The mutate-then-reload composition surfaces the new event in the
        // returned timeline without a separate read call.
        assert_eq!(after.recent_events.len(), before.recent_events.len() + 1);
        assert_eq!(after.recent_events[0].kind, PersonaEventKind::PositiveFeedback);
        assert_eq!(after.recent_events[0].note.as_deref(), Some("nice work"));
    }
}
