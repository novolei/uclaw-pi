// SPDX-License-Identifier: AGPL-3.0-or-later
//! Desk-pet personas (S4) — **system-prompt-only**.
//!
//! The S4 spike confirmed mistralrs 0.8.1 has no runtime LoRA adapter switch
//! (see `local_llm/s4_spike_test.rs`), so a persona is purely a sprite set +
//! a system prompt that shapes the pet's voice. No adapter / LoRA fields.
//!
//! `character` selects the sprite set the desktop window renders (the same
//! roster the in-app `PetWidget` already ships); multiple personas may reuse
//! one sprite set with a different voice.

use serde::{Deserialize, Serialize};

/// A desk-pet persona: a sprite set + a voice (system prompt). No adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PetPersona {
    /// Stable id used by `set_pet_persona` / `persona_by_id` (kebab-case).
    pub id: String,
    /// Human-facing name shown in the Settings dropdown.
    pub display_name: String,
    /// Sprite set the desktop window renders: "astro" | "clawby" | ...
    pub character: String,
    /// The voice — prepended as the system message for `pet_chat`.
    pub system_prompt: String,
}

/// The built-in persona roster. The FIRST entry is the default.
///
/// `astro`/`clawby` reuse the existing in-app sprite roster; `sprout` and
/// `pixel` are MiniCPM-Desk-Pet-inspired voices that reuse those same sprites
/// (system-prompt-only — no new assets required to function).
pub fn builtin_personas() -> Vec<PetPersona> {
    vec![
        PetPersona {
            id: "astro".to_string(),
            display_name: "Astro".to_string(),
            character: "astro".to_string(),
            system_prompt: "You are Astro, an upbeat, encouraging desktop \
                companion. Keep replies short, warm, and helpful — one or two \
                sentences. You live on the user's desktop and cheer them on. \
                Match the user's language (reply in Chinese if they write in \
                Chinese)."
                .to_string(),
        },
        PetPersona {
            id: "clawby".to_string(),
            display_name: "Clawby".to_string(),
            character: "clawby".to_string(),
            system_prompt: "You are Clawby, a playful, slightly cheeky desktop \
                pet. Be witty and concise — a quick quip or a short helpful \
                answer, never a wall of text. Match the user's language."
                .to_string(),
        },
        PetPersona {
            id: "sprout".to_string(),
            display_name: "Sprout".to_string(),
            character: "astro".to_string(),
            system_prompt: "You are Sprout, a gentle, calm desk companion \
                inspired by a tiny growing plant. Speak softly and kindly in \
                one or two short sentences. Encourage small steps and rest. \
                Match the user's language."
                .to_string(),
        },
        PetPersona {
            id: "pixel".to_string(),
            display_name: "Pixel".to_string(),
            character: "clawby".to_string(),
            system_prompt: "You are Pixel, a curious, geeky little desk buddy \
                who loves tech and tidy answers. Be precise and brief — get to \
                the point in a sentence or two. Match the user's language."
                .to_string(),
        },
    ]
}

/// Look up a persona by id. `None` if no built-in has that id.
pub fn persona_by_id(id: &str) -> Option<PetPersona> {
    builtin_personas().into_iter().find(|p| p.id == id)
}

/// The default persona (first in the roster). Always present.
pub fn default_persona() -> PetPersona {
    builtin_personas()
        .into_iter()
        .next()
        .expect("builtin_personas() must be non-empty")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn registry_is_non_empty() {
        assert!(!builtin_personas().is_empty());
    }

    #[test]
    fn ids_are_unique() {
        let personas = builtin_personas();
        let ids: HashSet<&str> = personas.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids.len(), personas.len(), "persona ids must be unique");
    }

    #[test]
    fn every_persona_has_a_voice_and_character() {
        for p in builtin_personas() {
            assert!(!p.system_prompt.trim().is_empty(), "{} has empty prompt", p.id);
            assert!(!p.character.trim().is_empty(), "{} has empty character", p.id);
            assert!(!p.display_name.trim().is_empty(), "{} has empty name", p.id);
        }
    }

    #[test]
    fn lookup_hits_and_misses() {
        assert_eq!(persona_by_id("astro").unwrap().display_name, "Astro");
        assert_eq!(persona_by_id("clawby").unwrap().character, "clawby");
        assert!(persona_by_id("does-not-exist").is_none());
    }

    #[test]
    fn default_is_first_entry() {
        assert_eq!(default_persona().id, builtin_personas()[0].id);
        assert_eq!(default_persona().id, "astro");
    }
}
