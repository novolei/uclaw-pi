// SPDX-License-Identifier: MIT
//! `MemUEmbedder` — bucket_seal's real [`Embedder`], backed by memU's local
//! FastEmbed model (`bge-small-en-v1.5`, [`EMBEDDING_DIM`] = 384). Reuses the
//! already-running memU Python subprocess (`MemUClient::embed_text`) so no new
//! embedding stack / dependency is needed.
//!
//! Integration-only: `embed` calls a live memU subprocess, so it is not
//! unit-tested here; the dimension guard is the contract callers rely on
//! (ingest / seal treat `Err` as "don't persist the row").

use std::sync::Arc;

use anyhow::{anyhow, Result};
use async_trait::async_trait;

use super::{Embedder, EMBEDDING_DIM};
use crate::memu::client::MemUClient;

/// Real embedder backed by memU's FastEmbed (`bge-small-en-v1.5`, 384-dim).
pub struct MemUEmbedder {
    client: Arc<MemUClient>,
}

impl MemUEmbedder {
    pub fn new(client: Arc<MemUClient>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl Embedder for MemUEmbedder {
    fn name(&self) -> &'static str {
        "memu_fastembed"
    }

    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let mut vectors = self
            .client
            .embed_text(&[text])
            .await
            .map_err(|e| anyhow!("memu embed: {}", e))?;
        let v = vectors
            .pop()
            .ok_or_else(|| anyhow!("memu embed: empty result"))?;
        if v.len() != EMBEDDING_DIM {
            anyhow::bail!("memu embed: {} dims, expected {}", v.len(), EMBEDDING_DIM);
        }
        Ok(v)
    }
}
