//! Local in-process LLM runtime (S1): MiniCPM5-1B-GGUF via mistralrs.
pub mod paths;
pub mod download;
pub mod engine;
#[cfg(test)]
mod spike_test;
