use std::fmt::Write;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ts_rs::TS;
use unicode_normalization::UnicodeNormalization;

const MAX_CONNECTION_ID_BYTES: usize = 32;
const MAX_DISPLAY_NAME_BYTES: usize = 120;
const MAX_ENDPOINT_URL_BYTES: usize = 2_048;
const MAX_MODEL_ID_BYTES: usize = 256;
pub const MAX_CREDENTIAL_BYTES: usize = 16 * 1024;
const MAX_CUSTOM_FIELDS: usize = 16;
const MAX_CUSTOM_NAME_BYTES: usize = 128;
const MAX_CUSTOM_VALUE_BYTES: usize = 4 * 1024;
const MAX_MODELS: usize = 500;
const MAX_REASONING_OPTIONS: usize = 32;
const MAX_DESCRIPTION_BYTES: usize = 4 * 1024;
const MAX_ENDPOINT_FINGERPRINT_BYTES: usize = 128;
const MAX_ADAPTER_VERSION_BYTES: usize = 128;
const MAX_DATA_DESTINATION_BYTES: usize = 256;
const MAX_TIMESTAMP_BYTES: usize = 128;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AiContractError {
    #[error("the AI connection method and provider are incompatible")]
    InvalidPairing,
    #[error("the AI connection identifier is invalid")]
    InvalidConnectionId,
    #[error("the AI connection revision is invalid")]
    InvalidConnectionRevision,
    #[error("the credential generation is invalid")]
    InvalidCredentialGeneration,
    #[error("the AI display label is invalid")]
    InvalidLabel,
    #[error("the compatible endpoint is invalid")]
    InvalidEndpoint,
    #[error("the compatible endpoint credential placement is invalid")]
    InvalidCredentialPlacement,
    #[error("the custom header name is invalid")]
    InvalidHeaderName,
    #[error("the custom query name is invalid")]
    InvalidQueryName,
    #[error("the configured collection exceeds its fixed limit")]
    TooManyValues,
    #[error("the configured value exceeds its fixed byte limit")]
    ValueTooLarge,
    #[error("the model identifier is invalid")]
    InvalidModelId,
    #[error("the AI reasoning selection is invalid")]
    InvalidReasoningSelection,
    #[error("the capability catalogue is invalid")]
    InvalidCatalogue,
    #[error("the AI configuration metadata is invalid")]
    InvalidMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(try_from = "String", into = "String")]
pub struct AiConnectionId(String);

impl AiConnectionId {
    pub fn parse(value: impl Into<String>) -> Result<Self, AiContractError> {
        let value = value.into();
        if value.len() != MAX_CONNECTION_ID_BYTES
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err(AiContractError::InvalidConnectionId);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for AiConnectionId {
    type Error = AiContractError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl From<AiConnectionId> for String {
    fn from(value: AiConnectionId) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(try_from = "u64", into = "u64")]
pub struct AiConnectionRevision(u64);

impl AiConnectionRevision {
    pub fn new(value: u64) -> Result<Self, AiContractError> {
        (value > 0)
            .then_some(Self(value))
            .ok_or(AiContractError::InvalidConnectionRevision)
    }

    pub fn get(self) -> u64 {
        self.0
    }
}

impl TryFrom<u64> for AiConnectionRevision {
    type Error = AiContractError;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<AiConnectionRevision> for u64 {
    fn from(value: AiConnectionRevision) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(try_from = "u64", into = "u64")]
pub struct CredentialGeneration(u64);

impl CredentialGeneration {
    pub fn new(value: u64) -> Result<Self, AiContractError> {
        (value > 0)
            .then_some(Self(value))
            .ok_or(AiContractError::InvalidCredentialGeneration)
    }

    pub fn get(self) -> u64 {
        self.0
    }
}

impl TryFrom<u64> for CredentialGeneration {
    type Error = AiContractError;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<CredentialGeneration> for u64 {
    fn from(value: CredentialGeneration) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum AiConnectionMethod {
    AccountLogin,
    DirectProviderKey,
    OpenAiCompatible,
    AnthropicCompatible,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum AiProviderKind {
    Codex,
    OpenAi,
    Anthropic,
    GoogleGemini,
    XAi,
    OpenAiCompatible,
    AnthropicCompatible,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum CapabilitySupport {
    Supported,
    Unsupported,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    try_from = "AiReasoningSelectionInput"
)]
pub enum AiReasoningSelection {
    Unsupported,
    Effort { id: String },
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum AiReasoningSelectionInput {
    Unsupported,
    Effort { id: String },
}

impl TryFrom<AiReasoningSelectionInput> for AiReasoningSelection {
    type Error = AiContractError;

    fn try_from(value: AiReasoningSelectionInput) -> Result<Self, Self::Error> {
        match value {
            AiReasoningSelectionInput::Unsupported => Ok(Self::Unsupported),
            AiReasoningSelectionInput::Effort { id } => {
                if id.is_empty() || id.len() > MAX_MODEL_ID_BYTES {
                    return Err(AiContractError::InvalidReasoningSelection);
                }
                Ok(Self::Effort { id })
            }
        }
    }
}

pub fn validate_method_provider(
    method: AiConnectionMethod,
    provider: AiProviderKind,
) -> Result<(), AiContractError> {
    let valid = matches!(
        (method, provider),
        (AiConnectionMethod::AccountLogin, AiProviderKind::Codex)
            | (
                AiConnectionMethod::DirectProviderKey,
                AiProviderKind::OpenAi
            )
            | (
                AiConnectionMethod::DirectProviderKey,
                AiProviderKind::Anthropic
            )
            | (
                AiConnectionMethod::DirectProviderKey,
                AiProviderKind::GoogleGemini
            )
            | (AiConnectionMethod::DirectProviderKey, AiProviderKind::XAi)
            | (
                AiConnectionMethod::OpenAiCompatible,
                AiProviderKind::OpenAiCompatible
            )
            | (
                AiConnectionMethod::AnthropicCompatible,
                AiProviderKind::AnthropicCompatible
            )
    );

    valid.then_some(()).ok_or(AiContractError::InvalidPairing)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    try_from = "CompatibleCredentialKindInput"
)]
pub enum CompatibleCredentialKind {
    Bearer,
    ApiKeyHeader { name: String },
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum CompatibleCredentialKindInput {
    Bearer,
    ApiKeyHeader { name: String },
}

impl TryFrom<CompatibleCredentialKindInput> for CompatibleCredentialKind {
    type Error = AiContractError;

    fn try_from(value: CompatibleCredentialKindInput) -> Result<Self, Self::Error> {
        let credential = match value {
            CompatibleCredentialKindInput::Bearer => Self::Bearer,
            CompatibleCredentialKindInput::ApiKeyHeader { name } => Self::ApiKeyHeader { name },
        };
        credential.validate()?;
        Ok(credential)
    }
}

impl CompatibleCredentialKind {
    pub fn validate(&self) -> Result<(), AiContractError> {
        match self {
            Self::Bearer => Ok(()),
            Self::ApiKeyHeader { name } => validate_credential_header_name(name),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(try_from = "CompatibleEndpointConfigurationInput")]
pub struct CompatibleEndpointConfiguration {
    pub base_url: String,
    pub credential: CompatibleCredentialKind,
    pub custom_header_names: Vec<String>,
    pub custom_query_names: Vec<String>,
    pub model_id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CompatibleEndpointConfigurationInput {
    base_url: String,
    credential: CompatibleCredentialKind,
    custom_header_names: Vec<String>,
    custom_query_names: Vec<String>,
    model_id: String,
}

impl TryFrom<CompatibleEndpointConfigurationInput> for CompatibleEndpointConfiguration {
    type Error = AiContractError;

    fn try_from(value: CompatibleEndpointConfigurationInput) -> Result<Self, Self::Error> {
        Self::parse(
            &value.base_url,
            value.credential,
            value.custom_header_names,
            value.custom_query_names,
            &value.model_id,
        )
    }
}

impl CompatibleEndpointConfiguration {
    pub fn parse(
        base_url: &str,
        credential: CompatibleCredentialKind,
        headers: Vec<String>,
        query: Vec<String>,
        model_id: &str,
    ) -> Result<Self, AiContractError> {
        if base_url.is_empty() || base_url.len() > MAX_ENDPOINT_URL_BYTES {
            return Err(AiContractError::InvalidEndpoint);
        }
        credential.validate()?;
        validate_model_id(model_id)?;
        validate_collection(&headers, validate_custom_header_name)?;
        validate_collection(&query, validate_custom_query_name)?;
        reject_duplicate_names(&headers, true)?;
        reject_duplicate_names(&query, false)?;

        let mut url =
            reqwest::Url::parse(base_url).map_err(|_| AiContractError::InvalidEndpoint)?;
        if url.cannot_be_a_base()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(AiContractError::InvalidEndpoint);
        }

        let is_loopback_literal = matches!(
            url.host_str(),
            Some("127.0.0.1") | Some("[::1]") | Some("::1")
        );
        match url.scheme() {
            "https" => {}
            "http" if is_loopback_literal => {}
            _ => return Err(AiContractError::InvalidEndpoint),
        }

        let normalized_path = normalize_path_prefix(url.path())?;
        url.set_path(&normalized_path);
        if (url.scheme() == "https" && url.port() == Some(443))
            || (url.scheme() == "http" && url.port() == Some(80))
        {
            url.set_port(None)
                .map_err(|_| AiContractError::InvalidEndpoint)?;
        }

        Ok(Self {
            base_url: url.to_string().trim_end_matches('/').to_owned(),
            credential,
            custom_header_names: headers,
            custom_query_names: query,
            model_id: model_id.to_owned(),
        })
    }

    pub fn endpoint_fingerprint(&self) -> String {
        sha256_hex(self.base_url.as_bytes())
    }
}

pub fn validate_custom_header_name(name: &str) -> Result<(), AiContractError> {
    validate_ascii_name(name, AiContractError::InvalidHeaderName)?;
    let lower = name.to_ascii_lowercase();
    let reserved = matches!(
        lower.as_str(),
        "authorization"
            | "proxy-authorization"
            | "host"
            | "connection"
            | "keep-alive"
            | "transfer-encoding"
            | "te"
            | "trailer"
            | "upgrade"
            | "content-length"
            | "cookie"
            | "set-cookie"
            | "forwarded"
    ) || lower.starts_with("proxy-")
        || lower.starts_with("sec-");
    (!reserved)
        .then_some(())
        .ok_or(AiContractError::InvalidHeaderName)
}

pub fn validate_custom_query_name(name: &str) -> Result<(), AiContractError> {
    validate_ascii_name(name, AiContractError::InvalidQueryName)
}

fn validate_credential_header_name(name: &str) -> Result<(), AiContractError> {
    validate_custom_header_name(name).map_err(|_| AiContractError::InvalidCredentialPlacement)
}

fn validate_ascii_name(name: &str, error: AiContractError) -> Result<(), AiContractError> {
    if name.is_empty()
        || name.len() > MAX_CUSTOM_NAME_BYTES
        || !name.bytes().all(is_http_token_byte)
    {
        return Err(error);
    }
    Ok(())
}

fn is_http_token_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'!' | b'#'
                | b'$'
                | b'%'
                | b'&'
                | b'\''
                | b'*'
                | b'+'
                | b'-'
                | b'.'
                | b'^'
                | b'_'
                | b'`'
                | b'|'
                | b'~'
        )
}

fn validate_collection(
    values: &[String],
    validator: fn(&str) -> Result<(), AiContractError>,
) -> Result<(), AiContractError> {
    if values.len() > MAX_CUSTOM_FIELDS {
        return Err(AiContractError::TooManyValues);
    }
    for value in values {
        if value.len() > MAX_CUSTOM_VALUE_BYTES {
            return Err(AiContractError::ValueTooLarge);
        }
        validator(value)?;
    }
    Ok(())
}

fn reject_duplicate_names(
    values: &[String],
    case_insensitive: bool,
) -> Result<(), AiContractError> {
    for (index, value) in values.iter().enumerate() {
        if values[..index].iter().any(|prior| {
            if case_insensitive {
                prior.eq_ignore_ascii_case(value)
            } else {
                prior == value
            }
        }) {
            return Err(if case_insensitive {
                AiContractError::InvalidHeaderName
            } else {
                AiContractError::InvalidQueryName
            });
        }
    }
    Ok(())
}

fn normalize_path_prefix(path: &str) -> Result<String, AiContractError> {
    if path.split('/').any(|segment| {
        segment == "." || segment == ".." || segment.to_ascii_lowercase().contains("%2e")
    }) {
        return Err(AiContractError::InvalidEndpoint);
    }
    let parts: Vec<_> = path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    Ok(if parts.is_empty() {
        "/".to_owned()
    } else {
        format!("/{}", parts.join("/"))
    })
}

pub fn normalize_label(value: &str) -> Result<String, AiContractError> {
    let normalized: String = value.nfc().collect();
    if normalized.is_empty()
        || normalized.trim().is_empty()
        || normalized.len() > MAX_DISPLAY_NAME_BYTES
    {
        return Err(AiContractError::InvalidLabel);
    }
    Ok(normalized)
}

fn normalize_description(value: &str) -> Result<String, AiContractError> {
    let normalized: String = value.nfc().collect();
    if normalized.is_empty() || normalized.len() > MAX_DESCRIPTION_BYTES {
        return Err(AiContractError::InvalidLabel);
    }
    Ok(normalized)
}

fn validate_bounded_metadata(value: &str, limit: usize) -> Result<(), AiContractError> {
    if value.is_empty() || value.len() > limit {
        return Err(AiContractError::InvalidMetadata);
    }
    Ok(())
}

fn validate_catalogue_sha256(value: &str) -> Result<(), AiContractError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
    {
        return Err(AiContractError::InvalidMetadata);
    }
    Ok(())
}

fn validate_model_id(value: &str) -> Result<(), AiContractError> {
    if value.is_empty() || value.len() > MAX_MODEL_ID_BYTES {
        return Err(AiContractError::InvalidModelId);
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "method",
    rename_all = "snake_case",
    try_from = "AiConnectionConfigurationInput"
)]
pub enum AiConnectionConfiguration {
    AccountLogin {
        provider: AiProviderKind,
        account_id: String,
    },
    DirectProviderKey {
        provider: AiProviderKind,
    },
    OpenAiCompatible {
        provider: AiProviderKind,
        endpoint: CompatibleEndpointConfiguration,
    },
    AnthropicCompatible {
        provider: AiProviderKind,
        endpoint: CompatibleEndpointConfiguration,
    },
}

#[derive(Deserialize)]
#[serde(tag = "method", rename_all = "snake_case", deny_unknown_fields)]
enum AiConnectionConfigurationInput {
    AccountLogin {
        provider: AiProviderKind,
        account_id: String,
    },
    DirectProviderKey {
        provider: AiProviderKind,
    },
    OpenAiCompatible {
        provider: AiProviderKind,
        endpoint: CompatibleEndpointConfiguration,
    },
    AnthropicCompatible {
        provider: AiProviderKind,
        endpoint: CompatibleEndpointConfiguration,
    },
}

impl TryFrom<AiConnectionConfigurationInput> for AiConnectionConfiguration {
    type Error = AiContractError;

    fn try_from(value: AiConnectionConfigurationInput) -> Result<Self, Self::Error> {
        let configuration = match value {
            AiConnectionConfigurationInput::AccountLogin {
                provider,
                account_id,
            } => Self::AccountLogin {
                provider,
                account_id,
            },
            AiConnectionConfigurationInput::DirectProviderKey { provider } => {
                Self::DirectProviderKey { provider }
            }
            AiConnectionConfigurationInput::OpenAiCompatible { provider, endpoint } => {
                Self::OpenAiCompatible { provider, endpoint }
            }
            AiConnectionConfigurationInput::AnthropicCompatible { provider, endpoint } => {
                Self::AnthropicCompatible { provider, endpoint }
            }
        };
        configuration.validate()?;
        Ok(configuration)
    }
}

impl AiConnectionConfiguration {
    pub fn validate(&self) -> Result<(), AiContractError> {
        match self {
            Self::AccountLogin { provider, .. } => {
                validate_method_provider(AiConnectionMethod::AccountLogin, *provider)
            }
            Self::DirectProviderKey { provider } => {
                validate_method_provider(AiConnectionMethod::DirectProviderKey, *provider)
            }
            Self::OpenAiCompatible { provider, endpoint } => {
                validate_method_provider(AiConnectionMethod::OpenAiCompatible, *provider)?;
                CompatibleEndpointConfiguration::parse(
                    &endpoint.base_url,
                    endpoint.credential.clone(),
                    endpoint.custom_header_names.clone(),
                    endpoint.custom_query_names.clone(),
                    &endpoint.model_id,
                )?;
                Ok(())
            }
            Self::AnthropicCompatible { provider, endpoint } => {
                validate_method_provider(AiConnectionMethod::AnthropicCompatible, *provider)?;
                CompatibleEndpointConfiguration::parse(
                    &endpoint.base_url,
                    endpoint.credential.clone(),
                    endpoint.custom_header_names.clone(),
                    endpoint.custom_query_names.clone(),
                    &endpoint.model_id,
                )?;
                Ok(())
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum AiStructuredOutputMode {
    NativeJsonSchema,
    Tool,
    Prompted,
    Unsupported,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct AiCapabilitySet {
    pub streaming: CapabilitySupport,
    pub tools: CapabilitySupport,
    pub images: CapabilitySupport,
    pub reasoning: CapabilitySupport,
    pub reroute_detection: CapabilitySupport,
    pub structured_output: AiStructuredOutputMode,
    pub context_window_tokens: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(try_from = "AiReasoningOptionInput")]
pub struct AiReasoningOption {
    pub selection: AiReasoningSelection,
    pub label: String,
    pub description: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AiReasoningOptionInput {
    selection: AiReasoningSelection,
    label: String,
    description: String,
}

impl TryFrom<AiReasoningOptionInput> for AiReasoningOption {
    type Error = AiContractError;

    fn try_from(value: AiReasoningOptionInput) -> Result<Self, Self::Error> {
        Ok(Self {
            selection: value.selection,
            label: normalize_label(&value.label)?,
            description: normalize_description(&value.description)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(try_from = "AiModelViewInput")]
pub struct AiModelView {
    pub model_id: String,
    pub reported_model_id: Option<String>,
    pub display_name: String,
    pub capabilities: AiCapabilitySet,
    pub reasoning_options: Vec<AiReasoningOption>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AiModelViewInput {
    model_id: String,
    reported_model_id: Option<String>,
    display_name: String,
    capabilities: AiCapabilitySet,
    reasoning_options: Vec<AiReasoningOption>,
}

impl TryFrom<AiModelViewInput> for AiModelView {
    type Error = AiContractError;

    fn try_from(value: AiModelViewInput) -> Result<Self, Self::Error> {
        validate_model_id(&value.model_id)?;
        if let Some(reported_model_id) = &value.reported_model_id {
            validate_model_id(reported_model_id)?;
        }
        if value.reasoning_options.len() > MAX_REASONING_OPTIONS {
            return Err(AiContractError::InvalidCatalogue);
        }
        Ok(Self {
            model_id: value.model_id,
            reported_model_id: value.reported_model_id,
            display_name: normalize_label(&value.display_name)?,
            capabilities: value.capabilities,
            reasoning_options: value.reasoning_options,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(try_from = "AiProbeEvidenceInput")]
pub struct AiProbeEvidence {
    pub connection_id: AiConnectionId,
    #[serde(rename = "execution_revision")]
    pub execution_revision: AiConnectionRevision,
    pub provider: AiProviderKind,
    pub endpoint_fingerprint: String,
    pub adapter_version: String,
    pub models: Vec<AiModelView>,
    pub observed_at: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AiProbeEvidenceInput {
    connection_id: AiConnectionId,
    execution_revision: AiConnectionRevision,
    provider: AiProviderKind,
    endpoint_fingerprint: String,
    adapter_version: String,
    models: Vec<AiModelView>,
    observed_at: String,
}

impl TryFrom<AiProbeEvidenceInput> for AiProbeEvidence {
    type Error = AiContractError;

    fn try_from(value: AiProbeEvidenceInput) -> Result<Self, Self::Error> {
        if value.models.len() > MAX_MODELS {
            return Err(AiContractError::InvalidCatalogue);
        }
        validate_bounded_metadata(&value.endpoint_fingerprint, MAX_ENDPOINT_FINGERPRINT_BYTES)?;
        validate_bounded_metadata(&value.adapter_version, MAX_ADAPTER_VERSION_BYTES)?;
        validate_bounded_metadata(&value.observed_at, MAX_TIMESTAMP_BYTES)?;
        Ok(Self {
            connection_id: value.connection_id,
            execution_revision: value.execution_revision,
            provider: value.provider,
            endpoint_fingerprint: value.endpoint_fingerprint,
            adapter_version: value.adapter_version,
            models: value.models,
            observed_at: value.observed_at,
        })
    }
}

impl AiProbeEvidence {
    pub fn semantic_projection(&self) -> AiProbeSemanticProjection<'_> {
        AiProbeSemanticProjection {
            connection_id: self.connection_id.as_str(),
            execution_revision: self.execution_revision.get(),
            provider: self.provider,
            endpoint_fingerprint: &self.endpoint_fingerprint,
            adapter_version: &self.adapter_version,
            models: self
                .models
                .iter()
                .map(|model| AiModelSemanticProjection {
                    model_id: &model.model_id,
                    reported_model_id: model.reported_model_id.as_deref(),
                    capabilities: &model.capabilities,
                    reasoning: model
                        .reasoning_options
                        .iter()
                        .map(|option| &option.selection)
                        .collect(),
                })
                .collect(),
        }
    }
}

#[derive(Serialize)]
pub struct AiProbeSemanticProjection<'a> {
    connection_id: &'a str,
    execution_revision: u64,
    provider: AiProviderKind,
    endpoint_fingerprint: &'a str,
    adapter_version: &'a str,
    models: Vec<AiModelSemanticProjection<'a>>,
}

#[derive(Serialize)]
struct AiModelSemanticProjection<'a> {
    model_id: &'a str,
    reported_model_id: Option<&'a str>,
    capabilities: &'a AiCapabilitySet,
    reasoning: Vec<&'a AiReasoningSelection>,
}

pub fn catalogue_sha256(evidence: &AiProbeEvidence) -> Result<String, AiContractError> {
    if evidence.models.len() > MAX_MODELS {
        return Err(AiContractError::InvalidCatalogue);
    }
    for model in &evidence.models {
        validate_model_id(&model.model_id)?;
        if let Some(reported_model_id) = &model.reported_model_id {
            validate_model_id(reported_model_id)?;
        }
    }
    let bytes = serde_json_canonicalizer::to_vec(&evidence.semantic_projection())
        .map_err(|_| AiContractError::InvalidCatalogue)?;
    Ok(sha256_hex(&bytes))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum AiConnectionStatus {
    Untested,
    Testing,
    Ready,
    Disabled,
    AuthenticationRequired,
    TemporarilyUnavailable,
    Incompatible,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct AiConnectionView {
    pub connection_id: String,
    pub execution_revision: u64,
    pub method: AiConnectionMethod,
    pub provider: AiProviderKind,
    pub display_name: String,
    pub enabled: bool,
    pub status: AiConnectionStatus,
    pub secret_configured: bool,
    pub models: Vec<AiModelView>,
    pub status_summary: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ActiveAiReadiness {
    NotConfigured,
    Ready,
    StaleRevision,
    Disabled,
    AuthenticationRequired,
    WorkerUnavailable,
    CapabilityChanged,
    VaultUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ActiveAiConfigurationView {
    pub connection_id: String,
    pub execution_revision: u64,
    pub provider: AiProviderKind,
    pub endpoint_fingerprint: String,
    pub model_id: String,
    pub reasoning: AiReasoningSelection,
    pub adapter_version: String,
    pub catalogue_sha256: String,
    pub capabilities: AiCapabilitySet,
    pub data_destination: String,
    pub activated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AccountLoginProgress {
    Idle,
    OpeningBrowser,
    AwaitingBrowser,
    AwaitingDeviceCode {
        verification_url: String,
        user_code: String,
    },
    Completing,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(try_from = "ActiveAiConfigurationInput")]
pub struct ActiveAiConfiguration {
    pub connection_id: AiConnectionId,
    #[serde(rename = "execution_revision")]
    pub execution_revision: AiConnectionRevision,
    pub provider: AiProviderKind,
    pub endpoint_fingerprint: String,
    pub model_id: String,
    pub reasoning: AiReasoningSelection,
    pub adapter_version: String,
    pub catalogue_sha256: String,
    pub capabilities: AiCapabilitySet,
    pub data_destination: String,
    pub activated_at: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ActiveAiConfigurationInput {
    connection_id: AiConnectionId,
    execution_revision: AiConnectionRevision,
    provider: AiProviderKind,
    endpoint_fingerprint: String,
    model_id: String,
    reasoning: AiReasoningSelection,
    adapter_version: String,
    catalogue_sha256: String,
    capabilities: AiCapabilitySet,
    data_destination: String,
    activated_at: String,
}

impl TryFrom<ActiveAiConfigurationInput> for ActiveAiConfiguration {
    type Error = AiContractError;

    fn try_from(value: ActiveAiConfigurationInput) -> Result<Self, Self::Error> {
        validate_bounded_metadata(&value.endpoint_fingerprint, MAX_ENDPOINT_FINGERPRINT_BYTES)?;
        validate_model_id(&value.model_id)?;
        validate_bounded_metadata(&value.adapter_version, MAX_ADAPTER_VERSION_BYTES)?;
        validate_catalogue_sha256(&value.catalogue_sha256)?;
        validate_bounded_metadata(&value.data_destination, MAX_DATA_DESTINATION_BYTES)?;
        validate_bounded_metadata(&value.activated_at, MAX_TIMESTAMP_BYTES)?;
        Ok(Self {
            connection_id: value.connection_id,
            execution_revision: value.execution_revision,
            provider: value.provider,
            endpoint_fingerprint: value.endpoint_fingerprint,
            model_id: value.model_id,
            reasoning: value.reasoning,
            adapter_version: value.adapter_version,
            catalogue_sha256: value.catalogue_sha256,
            capabilities: value.capabilities,
            data_destination: value.data_destination,
            activated_at: value.activated_at,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct AiRuntimeRequest {
    pub request_id: String,
    pub active_configuration: ActiveAiConfiguration,
    pub input_json: String,
    pub cancellation_requested: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum AiRuntimeEventKind {
    RunStarted,
    TurnStarted,
    UsageObserved,
    RateLimitObserved,
    ToolCallRequested,
    ToolResultObserved,
    Warning,
    Terminal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct AiRuntimeEvent {
    pub sequence: u64,
    pub kind: AiRuntimeEventKind,
    pub summary: String,
    pub correlation_id: Option<String>,
    pub opaque_reference: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct AiRuntimeUsage {
    pub input_tokens: Option<u64>,
    pub cached_input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub reasoning_output_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
    pub context_window_tokens: Option<u64>,
    pub elapsed_milliseconds: Option<u64>,
    pub rate_limit: Option<AiRuntimeRateLimit>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum AiRuntimeRateLimitState {
    Available,
    Exhausted,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct AiRuntimeRateLimitWindow {
    pub used_percent: Option<u32>,
    pub window_minutes: Option<u64>,
    pub resets_at_epoch_seconds: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct AiRuntimeRateLimit {
    pub state: AiRuntimeRateLimitState,
    pub primary: Option<AiRuntimeRateLimitWindow>,
    pub secondary: Option<AiRuntimeRateLimitWindow>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum AiRuntimeFailureCategory {
    AuthenticationRequired,
    QuotaExceeded,
    RateLimited,
    CapabilityMissing,
    InvalidRequest,
    ProtocolDrift,
    TimedOut,
    Cancelled,
    Transport,
    InvalidOutput,
    ModelRerouted,
    OutcomeIndeterminate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct AiRuntimeFailure {
    pub category: AiRuntimeFailureCategory,
    pub retry_safe: bool,
    pub required_user_action: String,
    pub redacted_detail: Option<String>,
    pub retry_after_milliseconds: Option<u64>,
}

impl AiRuntimeFailure {
    pub fn protocol_drift() -> Self {
        Self {
            category: AiRuntimeFailureCategory::ProtocolDrift,
            retry_safe: false,
            required_user_action: "Update the incompatible AI runtime before retrying.".to_owned(),
            redacted_detail: None,
            retry_after_milliseconds: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AiRuntimeResult {
    Completed {
        output_json: String,
        usage: AiRuntimeUsage,
    },
    Failed {
        failure: AiRuntimeFailure,
        usage: AiRuntimeUsage,
    },
    Cancelled {
        usage: AiRuntimeUsage,
    },
    Indeterminate {
        failure: AiRuntimeFailure,
        usage: AiRuntimeUsage,
    },
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn method_provider_matrix_is_closed() {
        let cases = [
            (
                AiConnectionMethod::AccountLogin,
                AiProviderKind::Codex,
                true,
            ),
            (
                AiConnectionMethod::DirectProviderKey,
                AiProviderKind::OpenAi,
                true,
            ),
            (
                AiConnectionMethod::DirectProviderKey,
                AiProviderKind::Anthropic,
                true,
            ),
            (
                AiConnectionMethod::DirectProviderKey,
                AiProviderKind::GoogleGemini,
                true,
            ),
            (
                AiConnectionMethod::DirectProviderKey,
                AiProviderKind::XAi,
                true,
            ),
            (
                AiConnectionMethod::OpenAiCompatible,
                AiProviderKind::OpenAiCompatible,
                true,
            ),
            (
                AiConnectionMethod::AnthropicCompatible,
                AiProviderKind::AnthropicCompatible,
                true,
            ),
            (
                AiConnectionMethod::AccountLogin,
                AiProviderKind::OpenAi,
                false,
            ),
            (
                AiConnectionMethod::DirectProviderKey,
                AiProviderKind::Codex,
                false,
            ),
            (
                AiConnectionMethod::OpenAiCompatible,
                AiProviderKind::AnthropicCompatible,
                false,
            ),
        ];

        for (method, provider, valid) in cases {
            assert_eq!(validate_method_provider(method, provider).is_ok(), valid);
        }
    }

    #[test]
    fn compatible_endpoint_policy_is_fail_closed() {
        let credential = CompatibleCredentialKind::Bearer;
        for rejected in [
            "http://localhost:11434/v1",
            "http://10.0.0.2/v1",
            "https://user:pass@example.com/v1",
            "https://example.com/v1?key=value",
            "https://example.com/v1#fragment",
        ] {
            assert!(CompatibleEndpointConfiguration::parse(
                rejected,
                credential.clone(),
                vec![],
                vec![],
                "test-model",
            )
            .is_err());
        }
        assert!(CompatibleEndpointConfiguration::parse(
            "http://127.0.0.1:11434/v1",
            credential.clone(),
            vec![],
            vec![],
            "test-model",
        )
        .is_ok());
        assert!(CompatibleEndpointConfiguration::parse(
            "http://[::1]:11434/v1",
            credential,
            vec![],
            vec![],
            "test-model",
        )
        .is_ok());
        for name in [
            "authorization",
            "host",
            "content-length",
            "proxy-authorization",
        ] {
            assert!(validate_custom_header_name(name).is_err());
        }
    }

    #[test]
    fn catalogue_hash_excludes_observation_time_and_presentation_labels() {
        let mut evidence = probe_evidence("2026-08-24T12:00:00Z", "Visible model", "Low");
        let first = catalogue_sha256(&evidence).unwrap();
        evidence.observed_at = "2026-08-24T12:01:00Z".to_owned();
        evidence.models[0].display_name = "Renamed model".to_owned();
        evidence.models[0].reasoning_options[0].label = "Economical".to_owned();

        assert_eq!(catalogue_sha256(&evidence).unwrap(), first);
    }

    #[test]
    fn connection_view_has_no_secret_or_default_surface() {
        let json = serde_json::to_string(&ready_connection_view()).unwrap();
        for forbidden in [
            "api_key",
            "access_token",
            "refresh_token",
            "header_value",
            "query_value",
            "is_default",
            "recommended",
        ] {
            assert!(
                !json.contains(forbidden),
                "forbidden projection field: {forbidden}"
            );
        }
    }

    #[test]
    fn persisted_contract_deserialization_rejects_invalid_identity_endpoint_and_pairing() {
        assert!(serde_json::from_str::<AiConnectionId>(r#""not-an-id""#).is_err());
        assert!(serde_json::from_str::<AiConnectionRevision>("0").is_err());
        assert!(serde_json::from_str::<CredentialGeneration>("0").is_err());
        assert!(serde_json::from_str::<CompatibleCredentialKind>(
            r#"{"kind":"api_key_header","name":"Host"}"#
        )
        .is_err());
        assert!(serde_json::from_str::<CompatibleEndpointConfiguration>(
            r#"{"base_url":"http://localhost:11434/v1","credential":{"kind":"bearer"},"custom_header_names":[],"custom_query_names":[],"model_id":"test-model"}"#
        )
        .is_err());
        assert!(serde_json::from_str::<AiConnectionConfiguration>(
            r#"{"method":"account_login","provider":"open_ai","account_id":"account"}"#
        )
        .is_err());
    }

    #[test]
    fn credential_header_and_tagged_unions_fail_closed() {
        for name in [
            "Host",
            "Content-Length",
            "Authorization",
            "Proxy-Authorization",
        ] {
            assert!(CompatibleCredentialKind::ApiKeyHeader {
                name: name.to_owned()
            }
            .validate()
            .is_err());
        }
        assert!(serde_json::from_str::<AiReasoningSelection>(
            r#"{"kind":"effort","id":"low","extra":true}"#
        )
        .is_err());
        assert!(serde_json::from_str::<AccountLoginProgress>(
            r#"{"kind":"awaiting_device_code","verification_url":"https://example.com","user_code":"abc","extra":true}"#
        )
        .is_err());
    }

    #[test]
    fn reasoning_effort_and_options_are_bounded_at_deserialization() {
        assert!(
            serde_json::from_str::<AiReasoningSelection>(r#"{"kind":"effort","id":""}"#).is_err()
        );
        assert!(
            serde_json::from_value::<AiReasoningSelection>(serde_json::json!({
                "kind": "effort",
                "id": "x".repeat(257)
            }))
            .is_err()
        );

        let mut value = serde_json::to_value(probe_evidence(
            "2026-08-24T12:00:00Z",
            "Visible model",
            "Low",
        ))
        .unwrap();
        let option = value["models"][0]["reasoning_options"][0].clone();
        value["models"][0]["reasoning_options"] = serde_json::Value::Array(vec![option; 33]);
        assert!(serde_json::from_value::<AiProbeEvidence>(value).is_err());
    }

    #[test]
    fn persisted_catalogue_normalizes_labels_and_enforces_bounds() {
        let mut value =
            serde_json::to_value(probe_evidence("2026-08-24T12:00:00Z", "e\u{301}", "Low"))
                .unwrap();
        value["models"][0]["reasoning_options"][0]["description"] =
            serde_json::Value::String("x".repeat(4_097));
        assert!(serde_json::from_value::<AiProbeEvidence>(value.clone()).is_err());

        value["models"][0]["reasoning_options"][0]["description"] =
            serde_json::Value::String("Bounded description".to_owned());
        let normalized = serde_json::from_value::<AiProbeEvidence>(value.clone()).unwrap();
        assert_eq!(normalized.models[0].display_name, "é");

        let model = value["models"][0].clone();
        value["models"] = serde_json::Value::Array(vec![model; 501]);
        assert!(serde_json::from_value::<AiProbeEvidence>(value).is_err());
    }

    #[test]
    fn persisted_active_configuration_rejects_unknown_and_oversized_metadata() {
        let mut value = serde_json::json!({
            "connection_id": "0123456789abcdef0123456789abcdef",
            "execution_revision": 1,
            "provider": "open_ai",
            "endpoint_fingerprint": "direct-openai",
            "model_id": "m".repeat(256),
            "reasoning": {"kind": "unsupported"},
            "adapter_version": "worker-v1",
            "catalogue_sha256": "a".repeat(64),
            "capabilities": {
                "streaming": "supported",
                "tools": "supported",
                "images": "unsupported",
                "reasoning": "unsupported",
                "reroute_detection": "unknown",
                "structured_output": "unsupported",
                "context_window_tokens": null
            },
            "data_destination": "quantix",
            "activated_at": "2026-08-24T12:00:00Z"
        });
        assert!(serde_json::from_value::<ActiveAiConfiguration>(value.clone()).is_ok());

        value["catalogue_sha256"] = serde_json::Value::String("z".repeat(64));
        assert!(serde_json::from_value::<ActiveAiConfiguration>(value.clone()).is_err());

        value["catalogue_sha256"] = serde_json::Value::String("a".repeat(64));
        value["model_id"] = serde_json::Value::String("m".repeat(257));
        assert!(serde_json::from_value::<ActiveAiConfiguration>(value.clone()).is_err());

        value["model_id"] = serde_json::Value::String("m".repeat(256));
        value["unexpected"] = serde_json::Value::Bool(true);
        assert!(serde_json::from_value::<ActiveAiConfiguration>(value).is_err());
    }

    #[test]
    fn valid_configuration_round_trips_through_the_persisted_contract() {
        let configuration = AiConnectionConfiguration::OpenAiCompatible {
            provider: AiProviderKind::OpenAiCompatible,
            endpoint: CompatibleEndpointConfiguration::parse(
                "https://example.com/v1/",
                CompatibleCredentialKind::ApiKeyHeader {
                    name: "x-api-key".to_owned(),
                },
                vec!["x-tenant".to_owned()],
                vec!["version".to_owned()],
                "model-1",
            )
            .unwrap(),
        };
        let json = serde_json::to_string(&configuration).unwrap();
        assert_eq!(
            serde_json::from_str::<AiConnectionConfiguration>(&json).unwrap(),
            configuration
        );
    }

    fn ready_connection_view() -> AiConnectionView {
        AiConnectionView {
            connection_id: "0123456789abcdef0123456789abcdef".to_owned(),
            execution_revision: 1,
            method: AiConnectionMethod::DirectProviderKey,
            provider: AiProviderKind::OpenAi,
            display_name: "Engineering OpenAI".to_owned(),
            enabled: true,
            status: AiConnectionStatus::Ready,
            secret_configured: true,
            models: probe_evidence("2026-08-24T12:00:00Z", "Visible model", "Low").models,
            status_summary: "Tested and ready.".to_owned(),
        }
    }

    fn probe_evidence(
        observed_at: &str,
        model_label: &str,
        reasoning_label: &str,
    ) -> AiProbeEvidence {
        AiProbeEvidence {
            connection_id: AiConnectionId::parse("0123456789abcdef0123456789abcdef").unwrap(),
            execution_revision: AiConnectionRevision::new(1).unwrap(),
            provider: AiProviderKind::OpenAi,
            endpoint_fingerprint: "direct-openai".to_owned(),
            adapter_version: "worker-v1".to_owned(),
            models: vec![AiModelView {
                model_id: "gpt-test".to_owned(),
                reported_model_id: Some("gpt-test".to_owned()),
                display_name: model_label.to_owned(),
                capabilities: AiCapabilitySet {
                    streaming: CapabilitySupport::Supported,
                    tools: CapabilitySupport::Supported,
                    images: CapabilitySupport::Unsupported,
                    reasoning: CapabilitySupport::Supported,
                    reroute_detection: CapabilitySupport::Supported,
                    structured_output: AiStructuredOutputMode::NativeJsonSchema,
                    context_window_tokens: Some(128_000),
                },
                reasoning_options: vec![AiReasoningOption {
                    selection: AiReasoningSelection::Effort {
                        id: "low".to_owned(),
                    },
                    label: reasoning_label.to_owned(),
                    description: "A bounded low-effort option.".to_owned(),
                }],
            }],
            observed_at: observed_at.to_owned(),
        }
    }
}
