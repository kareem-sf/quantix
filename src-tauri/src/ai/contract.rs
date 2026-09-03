use std::{
    fmt::Write,
    net::{Ipv4Addr, Ipv6Addr},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ts_rs::TS;
use unicode_normalization::UnicodeNormalization;
use url::Host;

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
const MAX_DATA_DESTINATION_BYTES: usize = 2_048;
const MAX_TIMESTAMP_BYTES: usize = 128;
const MAX_ACCOUNT_ID_BYTES: usize = 4 * 1024;

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
    Grok,
    OpenAi,
    Anthropic,
    GoogleGemini,
    XAi,
    OpenAiCompatible,
    AnthropicCompatible,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum AiNetworkDestinationClass {
    Public,
    Private,
    Loopback,
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
            | (AiConnectionMethod::AccountLogin, AiProviderKind::Grok)
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
        if matches!(
            &credential,
            CompatibleCredentialKind::ApiKeyHeader { name }
                if headers.iter().any(|header| header.eq_ignore_ascii_case(name))
        ) {
            return Err(AiContractError::InvalidCredentialPlacement);
        }

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

        let host = url.host().ok_or(AiContractError::InvalidEndpoint)?;
        if classify_endpoint_host(&host) == EndpointHostClass::Forbidden {
            return Err(AiContractError::InvalidEndpoint);
        }
        let is_loopback_literal = matches!(
            &host,
            Host::Ipv4(address) if *address == Ipv4Addr::LOCALHOST
        ) || matches!(&host, Host::Ipv6(address) if *address == Ipv6Addr::LOCALHOST);
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

        let base_url = url.to_string().trim_end_matches('/').to_owned();
        if base_url.is_empty() || base_url.len() > MAX_ENDPOINT_URL_BYTES {
            return Err(AiContractError::InvalidEndpoint);
        }

        Ok(Self {
            base_url,
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
            Self::AccountLogin {
                provider,
                account_id,
            } => {
                validate_method_provider(AiConnectionMethod::AccountLogin, *provider)?;
                if account_id.is_empty() || account_id.len() > MAX_ACCOUNT_ID_BYTES {
                    return Err(AiContractError::InvalidMetadata);
                }
                Ok(())
            }
            Self::DirectProviderKey { provider } => {
                validate_method_provider(AiConnectionMethod::DirectProviderKey, *provider)
            }
            Self::OpenAiCompatible { provider, endpoint } => {
                validate_method_provider(AiConnectionMethod::OpenAiCompatible, *provider)?;
                let canonical = CompatibleEndpointConfiguration::parse(
                    &endpoint.base_url,
                    endpoint.credential.clone(),
                    endpoint.custom_header_names.clone(),
                    endpoint.custom_query_names.clone(),
                    &endpoint.model_id,
                )?;
                if &canonical != endpoint {
                    return Err(AiContractError::InvalidEndpoint);
                }
                Ok(())
            }
            Self::AnthropicCompatible { provider, endpoint } => {
                validate_method_provider(AiConnectionMethod::AnthropicCompatible, *provider)?;
                let canonical = CompatibleEndpointConfiguration::parse(
                    &endpoint.base_url,
                    endpoint.credential.clone(),
                    endpoint.custom_header_names.clone(),
                    endpoint.custom_query_names.clone(),
                    &endpoint.model_id,
                )?;
                if &canonical != endpoint {
                    return Err(AiContractError::InvalidEndpoint);
                }
                Ok(())
            }
        }
    }

    pub fn method(&self) -> AiConnectionMethod {
        match self {
            Self::AccountLogin { .. } => AiConnectionMethod::AccountLogin,
            Self::DirectProviderKey { .. } => AiConnectionMethod::DirectProviderKey,
            Self::OpenAiCompatible { .. } => AiConnectionMethod::OpenAiCompatible,
            Self::AnthropicCompatible { .. } => AiConnectionMethod::AnthropicCompatible,
        }
    }

    pub fn provider(&self) -> AiProviderKind {
        match self {
            Self::AccountLogin { provider, .. }
            | Self::DirectProviderKey { provider }
            | Self::OpenAiCompatible { provider, .. }
            | Self::AnthropicCompatible { provider, .. } => *provider,
        }
    }

    pub fn data_destination(&self) -> Result<&str, AiContractError> {
        self.validate()?;
        match self {
            Self::AccountLogin { provider, .. } => match provider {
                AiProviderKind::Codex => Ok("https://chatgpt.com"),
                AiProviderKind::Grok => Ok("https://api.x.ai"),
                _ => Err(AiContractError::InvalidPairing),
            },
            Self::DirectProviderKey { provider } => match provider {
                AiProviderKind::OpenAi => Ok("https://api.openai.com"),
                AiProviderKind::Anthropic => Ok("https://api.anthropic.com"),
                AiProviderKind::GoogleGemini => Ok("https://generativelanguage.googleapis.com"),
                AiProviderKind::XAi => Ok("https://api.x.ai"),
                AiProviderKind::Codex
                | AiProviderKind::Grok
                | AiProviderKind::OpenAiCompatible
                | AiProviderKind::AnthropicCompatible => Err(AiContractError::InvalidPairing),
            },
            Self::OpenAiCompatible { endpoint, .. }
            | Self::AnthropicCompatible { endpoint, .. } => Ok(&endpoint.base_url),
        }
    }

    pub fn endpoint_fingerprint(&self) -> Result<String, AiContractError> {
        Ok(sha256_hex(self.data_destination()?.as_bytes()))
    }

    pub fn accepts_destination_class(&self, destination_class: AiNetworkDestinationClass) -> bool {
        match self {
            Self::AccountLogin { .. } | Self::DirectProviderKey { .. } => {
                destination_class == AiNetworkDestinationClass::Public
            }
            Self::OpenAiCompatible { endpoint, .. }
            | Self::AnthropicCompatible { endpoint, .. } => {
                let Some(host_class) = reqwest::Url::parse(&endpoint.base_url)
                    .ok()
                    .and_then(|url| url.host().map(|host| classify_endpoint_host(&host)))
                else {
                    return false;
                };
                match host_class {
                    EndpointHostClass::Dns => matches!(
                        destination_class,
                        AiNetworkDestinationClass::Public | AiNetworkDestinationClass::Private
                    ),
                    EndpointHostClass::Public => {
                        destination_class == AiNetworkDestinationClass::Public
                    }
                    EndpointHostClass::Private => {
                        destination_class == AiNetworkDestinationClass::Private
                    }
                    EndpointHostClass::Loopback => {
                        destination_class == AiNetworkDestinationClass::Loopback
                    }
                    EndpointHostClass::Forbidden => false,
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EndpointHostClass {
    Dns,
    Public,
    Private,
    Loopback,
    Forbidden,
}

fn classify_endpoint_host(host: &Host<&str>) -> EndpointHostClass {
    match host {
        Host::Domain(_) => EndpointHostClass::Dns,
        Host::Ipv4(address) => classify_ipv4(*address),
        Host::Ipv6(address) => classify_ipv6(*address),
    }
}

fn classify_ipv4(address: Ipv4Addr) -> EndpointHostClass {
    if address.is_loopback() {
        return EndpointHostClass::Loopback;
    }
    if address.is_private() {
        return EndpointHostClass::Private;
    }
    let octets = address.octets();
    let shared = octets[0] == 100 && (octets[1] & 0xc0) == 0x40;
    let protocol_assignment = octets[0] == 192 && octets[1] == 0 && octets[2] == 0;
    let documentation = (octets[0] == 192 && octets[1] == 0 && octets[2] == 2)
        || (octets[0] == 198 && octets[1] == 51 && octets[2] == 100)
        || (octets[0] == 203 && octets[1] == 0 && octets[2] == 113);
    let benchmark = octets[0] == 198 && matches!(octets[1], 18 | 19);
    let deprecated_relay = octets[0] == 192 && octets[1] == 88 && octets[2] == 99;
    if address.is_unspecified()
        || address.is_link_local()
        || address.is_multicast()
        || address.is_broadcast()
        || octets[0] == 0
        || octets[0] >= 240
        || shared
        || protocol_assignment
        || documentation
        || benchmark
        || deprecated_relay
    {
        EndpointHostClass::Forbidden
    } else {
        EndpointHostClass::Public
    }
}

fn classify_ipv6(address: Ipv6Addr) -> EndpointHostClass {
    if address.is_loopback() {
        return EndpointHostClass::Loopback;
    }
    let segments = address.segments();
    let ipv4_mapped = segments[..5] == [0, 0, 0, 0, 0] && segments[5] == 0xffff;
    let ipv4_compatible = segments[..6] == [0, 0, 0, 0, 0, 0];
    let aws_imds = segments == [0xfd00, 0x0ec2, 0, 0, 0, 0, 0, 0x0254];
    if aws_imds {
        return EndpointHostClass::Forbidden;
    }
    let unique_local = (segments[0] & 0xfe00) == 0xfc00;
    if unique_local {
        return EndpointHostClass::Private;
    }
    let link_local = (segments[0] & 0xffc0) == 0xfe80;
    let site_local = (segments[0] & 0xffc0) == 0xfec0;
    let ietf_special = segments[0] == 0x2001 && segments[1] <= 0x01ff;
    let documentation = segments[0] == 0x2001 && segments[1] == 0x0db8;
    let discard_only = segments[0] == 0x0100 && segments[1..4] == [0, 0, 0];
    let nat64 = segments[0] == 0x0064 && segments[1] == 0xff9b;
    let six_to_four = segments[0] == 0x2002;
    let iana_reserved_3f = (segments[0] & 0xff00) == 0x3f00;
    let global_unicast = (segments[0] & 0xe000) == 0x2000;
    if address.is_unspecified()
        || address.is_multicast()
        || ipv4_mapped
        || ipv4_compatible
        || link_local
        || site_local
        || ietf_special
        || documentation
        || discard_only
        || nat64
        || six_to_four
        || iana_reserved_3f
        || !global_unicast
    {
        EndpointHostClass::Forbidden
    } else {
        EndpointHostClass::Public
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
        let model = Self {
            model_id: value.model_id,
            reported_model_id: value.reported_model_id,
            display_name: normalize_label(&value.display_name)?,
            capabilities: value.capabilities,
            reasoning_options: value.reasoning_options,
        };
        model.validate()?;
        Ok(model)
    }
}

impl AiModelView {
    fn validate(&self) -> Result<(), AiContractError> {
        validate_model_id(&self.model_id)?;
        if let Some(reported_model_id) = &self.reported_model_id {
            validate_model_id(reported_model_id)?;
            if reported_model_id != &self.model_id {
                return Err(AiContractError::InvalidCatalogue);
            }
        } else if self.capabilities.reroute_detection == CapabilitySupport::Supported {
            return Err(AiContractError::InvalidCatalogue);
        }
        if normalize_label(&self.display_name)? != self.display_name
            || self.reasoning_options.len() > MAX_REASONING_OPTIONS
        {
            return Err(AiContractError::InvalidCatalogue);
        }
        for (index, option) in self.reasoning_options.iter().enumerate() {
            if normalize_label(&option.label)? != option.label
                || normalize_description(&option.description)? != option.description
                || self.reasoning_options[..index]
                    .iter()
                    .any(|prior| prior.selection == option.selection)
            {
                return Err(AiContractError::InvalidCatalogue);
            }
        }
        if self.capabilities.reasoning == CapabilitySupport::Unsupported
            && !self.reasoning_options.is_empty()
        {
            return Err(AiContractError::InvalidCatalogue);
        }
        if self.capabilities.reasoning == CapabilitySupport::Supported
            && self
                .reasoning_options
                .iter()
                .any(|option| option.selection == AiReasoningSelection::Unsupported)
        {
            return Err(AiContractError::InvalidCatalogue);
        }
        Ok(())
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
    pub destination_class: AiNetworkDestinationClass,
    pub models: Vec<AiModelView>,
    pub tested_model_id: String,
    pub tested_reasoning: AiReasoningSelection,
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
    destination_class: AiNetworkDestinationClass,
    models: Vec<AiModelView>,
    tested_model_id: String,
    tested_reasoning: AiReasoningSelection,
    observed_at: String,
}

impl TryFrom<AiProbeEvidenceInput> for AiProbeEvidence {
    type Error = AiContractError;

    fn try_from(value: AiProbeEvidenceInput) -> Result<Self, Self::Error> {
        let evidence = Self {
            connection_id: value.connection_id,
            execution_revision: value.execution_revision,
            provider: value.provider,
            endpoint_fingerprint: value.endpoint_fingerprint,
            adapter_version: value.adapter_version,
            destination_class: value.destination_class,
            models: value.models,
            tested_model_id: value.tested_model_id,
            tested_reasoning: value.tested_reasoning,
            observed_at: value.observed_at,
        };
        evidence.validate()?;
        Ok(evidence)
    }
}

impl AiProbeEvidence {
    pub fn validate(&self) -> Result<(), AiContractError> {
        if self.models.is_empty() || self.models.len() > MAX_MODELS {
            return Err(AiContractError::InvalidCatalogue);
        }
        validate_bounded_metadata(&self.endpoint_fingerprint, MAX_ENDPOINT_FINGERPRINT_BYTES)?;
        validate_bounded_metadata(&self.adapter_version, MAX_ADAPTER_VERSION_BYTES)?;
        validate_bounded_metadata(&self.observed_at, MAX_TIMESTAMP_BYTES)?;
        validate_model_id(&self.tested_model_id)?;
        for (index, model) in self.models.iter().enumerate() {
            model.validate()?;
            if self.models[..index]
                .iter()
                .any(|prior| prior.model_id == model.model_id)
            {
                return Err(AiContractError::InvalidCatalogue);
            }
        }
        let tested_model = self
            .models
            .iter()
            .find(|model| model.model_id == self.tested_model_id)
            .ok_or(AiContractError::InvalidCatalogue)?;
        match tested_model.capabilities.reasoning {
            CapabilitySupport::Supported => {
                if !matches!(self.tested_reasoning, AiReasoningSelection::Effort { .. })
                    || !tested_model
                        .reasoning_options
                        .iter()
                        .any(|option| option.selection == self.tested_reasoning)
                {
                    return Err(AiContractError::InvalidCatalogue);
                }
            }
            CapabilitySupport::Unsupported => {
                if self.tested_reasoning != AiReasoningSelection::Unsupported
                    || !tested_model.reasoning_options.is_empty()
                {
                    return Err(AiContractError::InvalidCatalogue);
                }
            }
            CapabilitySupport::Unknown => return Err(AiContractError::InvalidCatalogue),
        }
        Ok(())
    }

    pub fn semantic_projection(&self) -> AiProbeSemanticProjection<'_> {
        let mut models = self
            .models
            .iter()
            .map(|model| {
                let mut reasoning = model
                    .reasoning_options
                    .iter()
                    .map(|option| &option.selection)
                    .collect::<Vec<_>>();
                reasoning.sort_by(|left, right| {
                    reasoning_sort_key(left).cmp(&reasoning_sort_key(right))
                });
                AiModelSemanticProjection {
                    model_id: &model.model_id,
                    reported_model_id: model.reported_model_id.as_deref(),
                    capabilities: &model.capabilities,
                    reasoning,
                }
            })
            .collect::<Vec<_>>();
        models.sort_by(|left, right| left.model_id.cmp(right.model_id));
        AiProbeSemanticProjection {
            connection_id: self.connection_id.as_str(),
            execution_revision: self.execution_revision.get(),
            provider: self.provider,
            endpoint_fingerprint: &self.endpoint_fingerprint,
            adapter_version: &self.adapter_version,
            destination_class: self.destination_class,
            tested_model_id: &self.tested_model_id,
            tested_reasoning: &self.tested_reasoning,
            models,
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
    destination_class: AiNetworkDestinationClass,
    tested_model_id: &'a str,
    tested_reasoning: &'a AiReasoningSelection,
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
    evidence.validate()?;
    let bytes = serde_json_canonicalizer::to_vec(&evidence.semantic_projection())
        .map_err(|_| AiContractError::InvalidCatalogue)?;
    Ok(sha256_hex(&bytes))
}

fn reasoning_sort_key(selection: &AiReasoningSelection) -> (u8, &str) {
    match selection {
        AiReasoningSelection::Unsupported => (0, ""),
        AiReasoningSelection::Effort { id } => (1, id),
    }
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
    pub credential_generation: u64,
    pub method: AiConnectionMethod,
    pub provider: AiProviderKind,
    pub display_name: String,
    pub configuration: AiConnectionConfiguration,
    pub data_destination: String,
    pub endpoint_fingerprint: String,
    pub enabled: bool,
    pub status: AiConnectionStatus,
    pub secret_configured: bool,
    pub models: Vec<AiModelView>,
    pub adapter_version: Option<String>,
    pub catalogue_sha256: Option<String>,
    pub destination_class: Option<AiNetworkDestinationClass>,
    pub tested_model_id: Option<String>,
    pub tested_reasoning: Option<AiReasoningSelection>,
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
    pub destination_class: AiNetworkDestinationClass,
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
    pub destination_class: AiNetworkDestinationClass,
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
    destination_class: AiNetworkDestinationClass,
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
            destination_class: value.destination_class,
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
            (AiConnectionMethod::AccountLogin, AiProviderKind::Grok, true),
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
                AiConnectionMethod::DirectProviderKey,
                AiProviderKind::Grok,
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
    fn configuration_validation_rejects_auth_overlap_noncanonical_endpoints_and_bad_accounts() {
        assert!(CompatibleEndpointConfiguration::parse(
            "https://example.com/v1",
            CompatibleCredentialKind::ApiKeyHeader {
                name: "x-api-key".to_owned(),
            },
            vec!["X-API-KEY".to_owned()],
            Vec::new(),
            "model",
        )
        .is_err());

        let mut endpoint = CompatibleEndpointConfiguration::parse(
            "https://example.com/v1",
            CompatibleCredentialKind::Bearer,
            Vec::new(),
            Vec::new(),
            "model",
        )
        .unwrap();
        endpoint.base_url.push('/');
        assert!(AiConnectionConfiguration::OpenAiCompatible {
            provider: AiProviderKind::OpenAiCompatible,
            endpoint,
        }
        .validate()
        .is_err());

        for account_id in [String::new(), "a".repeat(4_097)] {
            assert!(AiConnectionConfiguration::AccountLogin {
                provider: AiProviderKind::Codex,
                account_id,
            }
            .validate()
            .is_err());
        }
    }

    #[test]
    fn compatible_endpoint_bounds_the_final_canonical_url() {
        let raw = format!("https://example.com/{}", "é".repeat(600));
        assert!(raw.len() <= MAX_ENDPOINT_URL_BYTES);
        assert!(CompatibleEndpointConfiguration::parse(
            &raw,
            CompatibleCredentialKind::Bearer,
            Vec::new(),
            Vec::new(),
            "model",
        )
        .is_err());
    }

    #[test]
    fn network_destination_classification_is_structural_and_fail_closed() {
        let endpoint = |base_url: &str| {
            CompatibleEndpointConfiguration::parse(
                base_url,
                CompatibleCredentialKind::Bearer,
                Vec::new(),
                Vec::new(),
                "model",
            )
            .unwrap()
        };
        let configuration = |endpoint| AiConnectionConfiguration::OpenAiCompatible {
            provider: AiProviderKind::OpenAiCompatible,
            endpoint,
        };

        let ipv6_loopback = configuration(endpoint("http://[::1]:11434/v1"));
        assert!(ipv6_loopback.accepts_destination_class(AiNetworkDestinationClass::Loopback));
        assert!(!ipv6_loopback.accepts_destination_class(AiNetworkDestinationClass::Public));

        let private = configuration(endpoint("https://10.0.0.1/v1"));
        assert!(private.accepts_destination_class(AiNetworkDestinationClass::Private));
        assert!(!private.accepts_destination_class(AiNetworkDestinationClass::Public));

        let public = configuration(endpoint("https://8.8.8.8/v1"));
        assert!(public.accepts_destination_class(AiNetworkDestinationClass::Public));
        assert!(!public.accepts_destination_class(AiNetworkDestinationClass::Private));

        for public_ipv6 in [
            "https://[2001:200::1]/v1",
            "https://[2606:4700:4700::1111]/v1",
        ] {
            let public = configuration(endpoint(public_ipv6));
            assert!(public.accepts_destination_class(AiNetworkDestinationClass::Public));
            assert!(!public.accepts_destination_class(AiNetworkDestinationClass::Private));
        }

        let dns = configuration(endpoint("https://models.example/v1"));
        assert!(dns.accepts_destination_class(AiNetworkDestinationClass::Public));
        assert!(dns.accepts_destination_class(AiNetworkDestinationClass::Private));
        assert!(!dns.accepts_destination_class(AiNetworkDestinationClass::Loopback));

        let direct = AiConnectionConfiguration::DirectProviderKey {
            provider: AiProviderKind::OpenAi,
        };
        assert!(direct.accepts_destination_class(AiNetworkDestinationClass::Public));
        assert!(!direct.accepts_destination_class(AiNetworkDestinationClass::Private));

        for forbidden in [
            "https://0.0.0.0/v1",
            "https://169.254.1.1/v1",
            "https://192.0.2.1/v1",
            "https://198.18.0.1/v1",
            "https://224.0.0.1/v1",
            "https://255.255.255.255/v1",
            "https://[::]/v1",
            "https://[fe80::1]/v1",
            "https://[2001:db8::1]/v1",
            "https://[2001:5::1]/v1",
            "https://[2002::1]/v1",
            "https://[3f00::1]/v1",
            "https://[3ffe::1]/v1",
            "https://[3fff::1]/v1",
            "https://[3fff:0fff::1]/v1",
            "https://[ff02::1]/v1",
            "https://[fd00:ec2::254]/v1",
            "https://[::ffff:127.0.0.1]/v1",
        ] {
            assert!(
                CompatibleEndpointConfiguration::parse(
                    forbidden,
                    CompatibleCredentialKind::Bearer,
                    Vec::new(),
                    Vec::new(),
                    "model",
                )
                .is_err(),
                "forbidden literal accepted: {forbidden}"
            );
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
    fn catalogue_hash_canonicalizes_order_and_includes_the_tested_pair() {
        let mut value = serde_json::to_value(probe_evidence(
            "2026-08-24T12:00:00Z",
            "Visible model",
            "Low",
        ))
        .unwrap();
        value["tested_model_id"] = serde_json::json!("gpt-test");
        value["tested_reasoning"] = serde_json::json!({"kind":"effort","id":"low"});
        let mut sibling = value["models"][0].clone();
        sibling["model_id"] = serde_json::json!("gpt-sibling");
        sibling["reported_model_id"] = serde_json::json!("gpt-sibling");
        sibling["reasoning_options"] = serde_json::json!([
            {
                "selection":{"kind":"effort","id":"high"},
                "label":"High",
                "description":"High effort"
            },
            {
                "selection":{"kind":"effort","id":"medium"},
                "label":"Medium",
                "description":"Medium effort"
            }
        ]);
        value["models"].as_array_mut().unwrap().push(sibling);
        let first: AiProbeEvidence = serde_json::from_value(value.clone()).unwrap();

        value["models"].as_array_mut().unwrap().reverse();
        value["models"][0]["reasoning_options"]
            .as_array_mut()
            .unwrap()
            .reverse();
        let reordered: AiProbeEvidence = serde_json::from_value(value).unwrap();
        assert_eq!(
            catalogue_sha256(&first).unwrap(),
            catalogue_sha256(&reordered).unwrap()
        );

        let mut different_pair = first;
        different_pair.tested_model_id = "gpt-sibling".to_owned();
        different_pair.tested_reasoning = AiReasoningSelection::Effort {
            id: "medium".to_owned(),
        };
        assert_ne!(
            catalogue_sha256(&different_pair).unwrap(),
            catalogue_sha256(&reordered).unwrap()
        );

        let mut private_destination = reordered.clone();
        private_destination.destination_class = AiNetworkDestinationClass::Private;
        assert_ne!(
            catalogue_sha256(&private_destination).unwrap(),
            catalogue_sha256(&reordered).unwrap()
        );
        let round_trip: AiProbeEvidence =
            serde_json::from_str(&serde_json::to_string(&private_destination).unwrap()).unwrap();
        assert_eq!(
            round_trip.destination_class,
            AiNetworkDestinationClass::Private
        );
    }

    #[test]
    fn probe_evidence_rejects_duplicates_incoherence_and_false_reroute_proof() {
        let base = serde_json::to_value(probe_evidence(
            "2026-08-24T12:00:00Z",
            "Visible model",
            "Low",
        ))
        .unwrap();

        let mut duplicate_model = base.clone();
        let model = duplicate_model["models"][0].clone();
        duplicate_model["models"]
            .as_array_mut()
            .unwrap()
            .push(model);
        assert!(serde_json::from_value::<AiProbeEvidence>(duplicate_model).is_err());

        let mut duplicate_reasoning = base.clone();
        let option = duplicate_reasoning["models"][0]["reasoning_options"][0].clone();
        duplicate_reasoning["models"][0]["reasoning_options"]
            .as_array_mut()
            .unwrap()
            .push(option);
        assert!(serde_json::from_value::<AiProbeEvidence>(duplicate_reasoning).is_err());

        let mut supported_unsupported = base.clone();
        supported_unsupported["tested_reasoning"] = serde_json::json!({"kind":"unsupported"});
        assert!(serde_json::from_value::<AiProbeEvidence>(supported_unsupported).is_err());

        let mut missing_reported = base.clone();
        missing_reported["models"][0]["reported_model_id"] = serde_json::Value::Null;
        assert!(serde_json::from_value::<AiProbeEvidence>(missing_reported).is_err());

        let mut rerouted = base;
        rerouted["models"][0]["reported_model_id"] = serde_json::json!("different-model");
        assert!(serde_json::from_value::<AiProbeEvidence>(rerouted).is_err());
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
    fn reasoning_option_count_limit_rejects_33_options() {
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
            "destination_class": "public",
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
    fn active_configuration_rejects_non_hex_catalogue_sha256() {
        let value = serde_json::json!({
            "connection_id": "0123456789abcdef0123456789abcdef",
            "execution_revision": 1,
            "provider": "open_ai",
            "endpoint_fingerprint": "direct-openai",
            "model_id": "model-1",
            "reasoning": {"kind": "unsupported"},
            "adapter_version": "worker-v1",
            "catalogue_sha256": "z".repeat(64),
            "destination_class": "public",
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

    #[test]
    fn configuration_identity_accessors_are_canonical() {
        let direct = AiConnectionConfiguration::DirectProviderKey {
            provider: AiProviderKind::OpenAi,
        };
        assert_eq!(direct.method(), AiConnectionMethod::DirectProviderKey);
        assert_eq!(direct.provider(), AiProviderKind::OpenAi);
        assert_eq!(direct.data_destination().unwrap(), "https://api.openai.com");
        assert_eq!(
            direct.endpoint_fingerprint().unwrap(),
            sha256_hex(b"https://api.openai.com")
        );

        let compatible = AiConnectionConfiguration::OpenAiCompatible {
            provider: AiProviderKind::OpenAiCompatible,
            endpoint: CompatibleEndpointConfiguration::parse(
                "https://identity.example/v1/",
                CompatibleCredentialKind::Bearer,
                Vec::new(),
                Vec::new(),
                "model",
            )
            .unwrap(),
        };
        assert_eq!(
            compatible.data_destination().unwrap(),
            "https://identity.example/v1"
        );
    }

    fn ready_connection_view() -> AiConnectionView {
        AiConnectionView {
            connection_id: "0123456789abcdef0123456789abcdef".to_owned(),
            execution_revision: 1,
            credential_generation: 1,
            method: AiConnectionMethod::DirectProviderKey,
            provider: AiProviderKind::OpenAi,
            display_name: "Engineering OpenAI".to_owned(),
            configuration: AiConnectionConfiguration::DirectProviderKey {
                provider: AiProviderKind::OpenAi,
            },
            data_destination: "https://api.openai.com".to_owned(),
            endpoint_fingerprint: sha256_hex(b"https://api.openai.com"),
            enabled: true,
            status: AiConnectionStatus::Ready,
            secret_configured: true,
            models: probe_evidence("2026-08-24T12:00:00Z", "Visible model", "Low").models,
            adapter_version: Some("worker-v1".to_owned()),
            catalogue_sha256: Some("a".repeat(64)),
            destination_class: Some(AiNetworkDestinationClass::Public),
            tested_model_id: Some("gpt-test".to_owned()),
            tested_reasoning: Some(AiReasoningSelection::Effort {
                id: "low".to_owned(),
            }),
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
            destination_class: AiNetworkDestinationClass::Public,
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
            tested_model_id: "gpt-test".to_owned(),
            tested_reasoning: AiReasoningSelection::Effort {
                id: "low".to_owned(),
            },
            observed_at: observed_at.to_owned(),
        }
    }
}
