use std::path::PathBuf;

use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};

use crate::db;

/// Model metadata: fastembed enum, dimension, query prefix, passage prefix.
/// E5 models require "query: " / "passage: " prefixes for proper retrieval.
/// BGE and MiniLM don't use prefixes.
struct ModelInfo {
    model: EmbeddingModel,
    _dim: usize,
    query_prefix: &'static str,
    passage_prefix: &'static str,
}

/// Maps a config model string to model info including required prefixes.
fn resolve_model(model_name: &str) -> ModelInfo {
    match model_name {
        "intfloat/multilingual-e5-small" => ModelInfo {
            model: EmbeddingModel::MultilingualE5Small,
            _dim: 384,
            query_prefix: "query: ",
            passage_prefix: "passage: ",
        },
        "BAAI/bge-small-en-v1.5" => ModelInfo {
            model: EmbeddingModel::BGESmallENV15,
            _dim: 384,
            query_prefix: "",
            passage_prefix: "",
        },
        "sentence-transformers/all-MiniLM-L6-v2" => ModelInfo {
            model: EmbeddingModel::AllMiniLML6V2,
            _dim: 384,
            query_prefix: "",
            passage_prefix: "",
        },
        _ => {
            tracing::warn!(
                model_name,
                "unknown model, falling back to multilingual-e5-small"
            );
            ModelInfo {
                model: EmbeddingModel::MultilingualE5Small,
                _dim: 384,
                query_prefix: "query: ",
                passage_prefix: "passage: ",
            }
        }
    }
}

/// Returns the passage prefix for the given model (used by store to prepend to texts).
pub fn passage_prefix(model_name: &str) -> &'static str {
    resolve_model(model_name).passage_prefix
}

/// Returns the query prefix for the given model.
pub fn query_prefix(model_name: &str) -> &'static str {
    resolve_model(model_name).query_prefix
}

/// Returns the directory where embedding models are cached.
fn models_cache_dir() -> PathBuf {
    db::data_dir()
        .map(|d| d.join("models"))
        .unwrap_or_else(|_| PathBuf::from("/tmp/zm-models"))
}

/// Build execution providers: CUDA first (if feature enabled), then CPU fallback.
/// Providers that fail to register are silently skipped by ort.
fn execution_providers() -> Vec<ort::ep::ExecutionProviderDispatch> {
    let mut providers = Vec::new();

    #[cfg(feature = "cuda")]
    {
        providers.push(ort::ep::cuda::CUDA::default().build());
        tracing::info!("CUDA execution provider registered");
    }

    providers.push(ort::ep::cpu::CPU::default().build());
    providers
}

/// Initialize the embedding model. This may download the model on first use.
/// Must be called on a blocking-safe thread (tokio::spawn_blocking or Tokio::spawn).
pub fn init_model(model_name: &str) -> anyhow::Result<TextEmbedding> {
    let info = resolve_model(model_name);
    let model = info.model;
    let cache_dir = models_cache_dir();
    let providers = execution_providers();

    tracing::info!(
        ?model,
        ?cache_dir,
        num_providers = providers.len(),
        "initializing embedding model"
    );

    let options = InitOptions::new(model)
        .with_cache_dir(cache_dir)
        .with_show_download_progress(true)
        .with_execution_providers(providers);

    let embedding = TextEmbedding::try_new(options)?;
    tracing::info!("embedding model ready");
    Ok(embedding)
}

/// Embed a batch of texts. Returns one embedding per input text.
pub fn embed_batch(model: &mut TextEmbedding, texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>> {
    if texts.is_empty() {
        return Ok(Vec::new());
    }
    let embeddings = model.embed(texts.to_vec(), None)?;
    Ok(embeddings)
}

/// Embed a single query text with the appropriate model prefix.
pub fn embed_query(
    model: &mut TextEmbedding,
    text: &str,
    model_name: &str,
) -> anyhow::Result<Vec<f32>> {
    let prefix = query_prefix(model_name);
    let prefixed = format!("{prefix}{text}");
    let results = model.embed(vec![&prefixed], None)?;
    results
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("no embedding returned"))
}
