//! Dense embedder (bge-m3 int8) and cross-encoder reranker (bge-reranker-v2-m3 int8)
//! wrappers over fastembed, plus small vector helpers (cosine, pack/unpack).

use std::path::Path;
use std::sync::Mutex;

use anyhow::{anyhow, Context};
use fastembed::{
    InitOptionsUserDefined, Pooling, RerankInitOptionsUserDefined, TextEmbedding, TextRerank,
    TokenizerFiles, UserDefinedEmbeddingModel, UserDefinedRerankingModel,
};

use ort::session::builder::AutoDevicePolicy;

use super::models::{is_ready, model_dir, ModelSpec, BGE_M3_INT8, BGE_RERANKER_V2_M3_INT8};

// Never let ORT auto-register CoreML on macOS: with dynamic shapes it recompiles per
// batch shape and balloons to tens of GB. Plain CPU is predictable.
const MAX_LENGTH: usize = 512;
const BATCH_SIZE: usize = 16;

fn read_tokenizer_files(dir: &Path) -> anyhow::Result<TokenizerFiles> {
    let rd = |name: &str| {
        std::fs::read(dir.join(name)).with_context(|| format!("read {}", dir.join(name).display()))
    };
    Ok(TokenizerFiles {
        tokenizer_file: rd("tokenizer.json")?,
        config_file: rd("config.json")?,
        special_tokens_map_file: rd("special_tokens_map.json")?,
        tokenizer_config_file: rd("tokenizer_config.json")?,
    })
}

fn require_ready(root: &Path, spec: &ModelSpec) -> anyhow::Result<()> {
    if !is_ready(root, spec) {
        return Err(anyhow!("model {} not downloaded under {}", spec.id, root.display()));
    }
    Ok(())
}

fn l2_normalize(v: &mut [f32]) {
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if n > 1e-12 {
        v.iter_mut().for_each(|x| *x /= n);
    }
}

pub struct Embedder {
    inner: Mutex<TextEmbedding>,
}

impl Embedder {
    pub fn load(models_root: &Path) -> anyhow::Result<Self> {
        require_ready(models_root, &BGE_M3_INT8)?;
        let dir = model_dir(models_root, &BGE_M3_INT8);
        let onnx = std::fs::read(dir.join("onnx/model_int8.onnx")).context("read bge-m3 onnx")?;
        let model = UserDefinedEmbeddingModel::new(onnx, read_tokenizer_files(&dir)?)
            .with_pooling(Pooling::Cls);
        let engine = TextEmbedding::try_new_from_user_defined(
            model,
            InitOptionsUserDefined::default()
                .with_max_length(MAX_LENGTH)
                .with_auto_device(AutoDevicePolicy::PreferCPU),
        )
        .map_err(|e| anyhow!("load bge-m3: {e}"))?;
        Ok(Self { inner: Mutex::new(engine) })
    }

    pub fn model_id(&self) -> &'static str {
        BGE_M3_INT8.id
    }

    pub fn dim(&self) -> usize {
        1024
    }

    /// L2-normalized dense embeddings, one per input text.
    pub fn embed(&self, texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let mut g = self.inner.lock().map_err(|_| anyhow!("embedder mutex poisoned"))?;
        let mut out = g
            .embed(texts, Some(BATCH_SIZE))
            .map_err(|e| anyhow!("embed: {e}"))?;
        out.iter_mut().for_each(|v| l2_normalize(v));
        Ok(out)
    }
}

pub struct Reranker {
    inner: Mutex<TextRerank>,
}

impl Reranker {
    pub fn load(models_root: &Path) -> anyhow::Result<Self> {
        require_ready(models_root, &BGE_RERANKER_V2_M3_INT8)?;
        let dir = model_dir(models_root, &BGE_RERANKER_V2_M3_INT8);
        let model = UserDefinedRerankingModel::new(
            dir.join("onnx/model_int8.onnx"),
            read_tokenizer_files(&dir)?,
        );
        let opts = RerankInitOptionsUserDefined::default()
            .with_max_length(MAX_LENGTH)
            .with_auto_device(AutoDevicePolicy::PreferCPU);
        let engine = TextRerank::try_new_from_user_defined(model, opts)
            .map_err(|e| anyhow!("load reranker: {e}"))?;
        Ok(Self { inner: Mutex::new(engine) })
    }

    /// `(input index, score)` pairs, best first.
    pub fn rerank(&self, query: &str, docs: &[String]) -> anyhow::Result<Vec<(usize, f32)>> {
        if docs.is_empty() {
            return Ok(Vec::new());
        }
        let refs: Vec<&str> = docs.iter().map(String::as_str).collect();
        let mut g = self.inner.lock().map_err(|_| anyhow!("reranker mutex poisoned"))?;
        let res = g
            .rerank(query, &refs, false, Some(BATCH_SIZE))
            .map_err(|e| anyhow!("rerank: {e}"))?;
        let mut v: Vec<(usize, f32)> = res.into_iter().map(|r| (r.index, r.score)).collect();
        v.sort_by(|a, b| b.1.total_cmp(&a.1));
        Ok(v)
    }
}

/// Cosine similarity; 0.0 for mismatched lengths or zero vectors.
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let (mut dot, mut na, mut nb) = (0f32, 0f32, 0f32);
    for (x, y) in a.iter().zip(b) {
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    let d = na.sqrt() * nb.sqrt();
    if d <= 1e-12 { 0.0 } else { dot / d }
}

/// Little-endian f32 packing.
pub fn pack(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

pub fn unpack(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_unpack_roundtrip() {
        let v = vec![0.0, 1.5, -2.25, 1e-7, f32::MAX];
        assert_eq!(unpack(&pack(&v)), v);
        assert_eq!(pack(&v).len(), v.len() * 4);
    }

    #[test]
    fn cosine_identical_and_orthogonal() {
        let a = vec![0.6, 0.8, 0.0];
        let b = vec![0.0, 0.0, 1.0];
        assert!((cosine(&a, &a) - 1.0).abs() < 1e-6);
        assert!(cosine(&a, &b).abs() < 1e-6);
    }

    fn root() -> std::path::PathBuf {
        std::path::PathBuf::from(std::env::var("UCLAW_TEST_MODELS").expect("UCLAW_TEST_MODELS"))
    }

    #[test]
    #[ignore]
    fn bge_m3_cross_lingual() {
        let t = std::time::Instant::now();
        let e = Embedder::load(&root()).unwrap();
        eprintln!("load: {:?}", t.elapsed());
        let t = std::time::Instant::now();
        let v = e
            .embed(&[
                "我喜欢打乒乓球".to_string(),
                "The user enjoys playing table tennis".to_string(),
                "The user ran ls -a in a terminal".to_string(),
            ])
            .unwrap();
        eprintln!("embed 3 texts: {:?}", t.elapsed());
        assert_eq!(v[0].len(), e.dim());
        let n: f32 = v[0].iter().map(|x| x * x).sum();
        assert!((n - 1.0).abs() < 1e-3);
        let rel = cosine(&v[0], &v[1]);
        let irr = cosine(&v[0], &v[2]);
        eprintln!("related={rel} unrelated={irr}");
        assert!(rel > irr);
    }

    #[test]
    #[ignore]
    fn reranker_orders_relevant_first() {
        let t = std::time::Instant::now();
        let r = Reranker::load(&root()).unwrap();
        eprintln!("load: {:?}", t.elapsed());
        let docs = vec!["用户喜欢吃鱼".to_string(), "用户的儿子叫刘书维".to_string()];
        let res = r.rerank("我儿子叫什么名字", &docs).unwrap();
        eprintln!("{res:?}");
        assert_eq!(res[0].0, 1);
    }
}
