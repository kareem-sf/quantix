use crate::{tender_store::TenderCommandError, QuantixHost};

#[cfg(not(feature = "runtime-fixture"))]
use crate::tender_store::TenderErrorCode;

pub(crate) const EMBEDDING_DIMENSIONS: usize = 384;

#[cfg(not(feature = "runtime-fixture"))]
use std::{path::Path, sync::Mutex};

#[cfg(not(feature = "runtime-fixture"))]
use fastembed::{
    InitOptionsUserDefined, Pooling, TextEmbedding, TokenizerFiles, UserDefinedEmbeddingModel,
};

#[cfg(not(feature = "runtime-fixture"))]
static EMBEDDING_MODEL: Mutex<Option<TextEmbedding>> = Mutex::new(None);

#[cfg(not(feature = "runtime-fixture"))]
const EMBEDDING_BATCH_SIZE: usize = 32;
#[cfg(not(feature = "runtime-fixture"))]
const EMBEDDING_MAX_TOKENS: usize = 512;

pub(crate) async fn embed_evidence_locations(
    host: &QuantixHost,
    texts: Vec<String>,
) -> Result<Vec<Vec<f32>>, TenderCommandError> {
    #[cfg(feature = "runtime-fixture")]
    {
        let _ = host;
        Ok(texts
            .iter()
            .map(|text| deterministic_embedding(text))
            .collect())
    }
    #[cfg(not(feature = "runtime-fixture"))]
    {
        let model_directory = model_directory(host);
        tokio::task::spawn_blocking(move || embed(&model_directory, texts, "passage: "))
            .await
            .map_err(|_| runtime_error())?
    }
}

pub(crate) async fn embed_search_query(
    host: &QuantixHost,
    query: String,
) -> Result<Vec<f32>, TenderCommandError> {
    #[cfg(feature = "runtime-fixture")]
    {
        let _ = host;
        Ok(deterministic_embedding(&query))
    }
    #[cfg(not(feature = "runtime-fixture"))]
    {
        let model_directory = model_directory(host);
        let mut embeddings =
            tokio::task::spawn_blocking(move || embed(&model_directory, vec![query], "query: "))
                .await
                .map_err(|_| runtime_error())??;
        embeddings.pop().ok_or_else(runtime_error)
    }
}

#[cfg(not(feature = "runtime-fixture"))]
fn model_directory(host: &QuantixHost) -> std::path::PathBuf {
    host.application_home()
        .join("models")
        .join("ocr")
        .join("embeddings")
}

#[cfg(not(feature = "runtime-fixture"))]
fn embed(
    model_directory: &Path,
    texts: Vec<String>,
    prefix: &str,
) -> Result<Vec<Vec<f32>>, TenderCommandError> {
    let mut model = EMBEDDING_MODEL.lock().map_err(|_| runtime_error())?;
    if model.is_none() {
        *model = Some(load_model(model_directory)?);
    }
    let inputs = texts
        .iter()
        .map(|text| format!("{prefix}{}", text.trim()))
        .collect::<Vec<_>>();
    let embeddings = model
        .as_mut()
        .ok_or_else(runtime_error)?
        .embed(inputs, Some(EMBEDDING_BATCH_SIZE))
        .map_err(|_| runtime_error())?;
    validate_embeddings(&embeddings, texts.len())?;
    Ok(embeddings)
}

#[cfg(not(feature = "runtime-fixture"))]
fn load_model(model_directory: &Path) -> Result<TextEmbedding, TenderCommandError> {
    let tokenizer_files = TokenizerFiles {
        tokenizer_file: read_model_file(model_directory, "tokenizer.json")?,
        config_file: read_model_file(model_directory, "config.json")?,
        special_tokens_map_file: read_model_file(model_directory, "special_tokens_map.json")?,
        tokenizer_config_file: read_model_file(model_directory, "tokenizer_config.json")?,
    };
    let model = UserDefinedEmbeddingModel::new(
        read_model_file(model_directory, "model.onnx")?,
        tokenizer_files,
    )
    .with_pooling(Pooling::Mean);
    TextEmbedding::try_new_from_user_defined(
        model,
        InitOptionsUserDefined::new()
            .with_max_length(EMBEDDING_MAX_TOKENS)
            .with_intra_threads(2),
    )
    .map_err(|_| runtime_error())
}

#[cfg(not(feature = "runtime-fixture"))]
fn read_model_file(model_directory: &Path, name: &str) -> Result<Vec<u8>, TenderCommandError> {
    std::fs::read(model_directory.join(name)).map_err(|_| runtime_error())
}

#[cfg(not(feature = "runtime-fixture"))]
fn validate_embeddings(
    embeddings: &[Vec<f32>],
    expected_count: usize,
) -> Result<(), TenderCommandError> {
    if embeddings.len() != expected_count
        || embeddings.iter().any(|embedding| {
            embedding.len() != EMBEDDING_DIMENSIONS
                || embedding.iter().any(|value| !value.is_finite())
        })
    {
        return Err(runtime_error());
    }
    Ok(())
}

#[cfg(feature = "runtime-fixture")]
fn deterministic_embedding(text: &str) -> Vec<f32> {
    let mut embedding = vec![0.0_f32; EMBEDDING_DIMENSIONS];
    for token in text.split(|character: char| !character.is_alphanumeric()) {
        if token.is_empty() {
            continue;
        }
        let mut hash = 0xcbf29ce484222325_u64;
        for byte in token.to_lowercase().as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        let index = hash as usize % EMBEDDING_DIMENSIONS;
        embedding[index] += if hash & (1 << 63) == 0 { 1.0 } else { -1.0 };
    }
    let magnitude = embedding
        .iter()
        .map(|value| value * value)
        .sum::<f32>()
        .sqrt();
    if magnitude > 0.0 {
        for value in &mut embedding {
            *value /= magnitude;
        }
    }
    embedding
}

#[cfg(not(feature = "runtime-fixture"))]
fn runtime_error() -> TenderCommandError {
    TenderCommandError::new(TenderErrorCode::RuntimeRequired)
}
