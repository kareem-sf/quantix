use std::{
    collections::{HashMap, HashSet},
    ffi::OsString,
    fs,
    io::Read,
    path::PathBuf,
    time::Duration,
};

use garde::Validate;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use ts_rs::TS;
use unicode_bidi::{bidi_class, BidiClass};
use unicode_script::{Script, UnicodeScript};

use crate::{
    host::ParseTargetKey,
    process_supervisor::{ProcessSpec, ProcessTermination},
    runtime_readiness::{docling_environment, docling_executable},
    tender_store::{
        metadata_is_unsafe_storage_link, TenderCommandError, TenderErrorCode, TenderId,
    },
    QuantixHost,
};

const DOCLING_DOCUMENT_SCHEMA_VERSION: &str = "1.10.0";
const DOCLING_DOCUMENT_TIMEOUT_SECONDS: &str = "840";
const DOCLING_PROCESS_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const DOCLING_PROCESS_OUTPUT_LIMIT: usize = 64 * 1024;
#[cfg(not(feature = "runtime-fixture"))]
pub(crate) const MAX_DOCLING_JSON_BYTES: u64 = 64 * 1024 * 1024;
#[cfg(feature = "runtime-fixture")]
pub(crate) const MAX_DOCLING_JSON_BYTES: u64 = 64 * 1024;
pub(crate) const MAX_EVIDENCE_LOCATIONS: usize = 100_000;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct ParseSourceArtifactCommand {
    #[garde(length(bytes, min = 32, max = 32), ascii)]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32), ascii)]
    pub artifact_id: String,
    #[garde(range(min = 1))]
    pub version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct SearchEvidenceCommand {
    #[garde(length(bytes, min = 32, max = 32), ascii)]
    pub tender_id: String,
    #[garde(length(bytes, min = 1, max = 512))]
    pub query: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ParseState {
    NotRequested,
    Running,
    Parsed,
    Failed,
    Interrupted,
    Quarantined,
    Unsupported,
}

impl ParseState {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::NotRequested => "not_requested",
            Self::Running => "running",
            Self::Parsed => "parsed",
            Self::Failed => "failed",
            Self::Interrupted => "interrupted",
            Self::Quarantined => "quarantined",
            Self::Unsupported => "unsupported",
        }
    }

    pub(crate) fn parse(value: &str) -> Result<Self, TenderCommandError> {
        match value {
            "not_requested" => Ok(Self::NotRequested),
            "running" => Ok(Self::Running),
            "parsed" => Ok(Self::Parsed),
            "failed" => Ok(Self::Failed),
            "interrupted" => Ok(Self::Interrupted),
            "quarantined" => Ok(Self::Quarantined),
            "unsupported" => Ok(Self::Unsupported),
            _ => Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ParseExceptionCode {
    Unsupported,
    ProcessFailed,
    PublicationFailed,
    Interrupted,
    MalformedOutput,
    OutputLimitExceeded,
    LossDetected,
}

impl ParseExceptionCode {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Unsupported => "unsupported",
            Self::ProcessFailed => "process_failed",
            Self::PublicationFailed => "publication_failed",
            Self::Interrupted => "interrupted",
            Self::MalformedOutput => "malformed_output",
            Self::OutputLimitExceeded => "output_limit_exceeded",
            Self::LossDetected => "loss_detected",
        }
    }

    pub(crate) fn parse(value: &str) -> Result<Self, TenderCommandError> {
        match value {
            "unsupported" => Ok(Self::Unsupported),
            "process_failed" => Ok(Self::ProcessFailed),
            "publication_failed" => Ok(Self::PublicationFailed),
            "interrupted" => Ok(Self::Interrupted),
            "malformed_output" => Ok(Self::MalformedOutput),
            "output_limit_exceeded" => Ok(Self::OutputLimitExceeded),
            "loss_detected" => Ok(Self::LossDetected),
            _ => Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum EvidenceLocationKind {
    Section,
    Paragraph,
    Table,
    Sheet,
    Cell,
}

impl EvidenceLocationKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Section => "section",
            Self::Paragraph => "paragraph",
            Self::Table => "table",
            Self::Sheet => "sheet",
            Self::Cell => "cell",
        }
    }

    pub(crate) fn parse(value: &str) -> Result<Self, TenderCommandError> {
        match value {
            "section" => Ok(Self::Section),
            "paragraph" => Ok(Self::Paragraph),
            "table" => Ok(Self::Table),
            "sheet" => Ok(Self::Sheet),
            "cell" => Ok(Self::Cell),
            _ => Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum EvidenceLanguage {
    Arabic,
    English,
    Mixed,
    Undetermined,
}

impl EvidenceLanguage {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Arabic => "arabic",
            Self::English => "english",
            Self::Mixed => "mixed",
            Self::Undetermined => "undetermined",
        }
    }

    pub(crate) fn parse(value: &str) -> Result<Self, TenderCommandError> {
        match value {
            "arabic" => Ok(Self::Arabic),
            "english" => Ok(Self::English),
            "mixed" => Ok(Self::Mixed),
            "undetermined" => Ok(Self::Undetermined),
            _ => Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum TextDirection {
    LeftToRight,
    RightToLeft,
    Mixed,
    Neutral,
}

impl TextDirection {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::LeftToRight => "left_to_right",
            Self::RightToLeft => "right_to_left",
            Self::Mixed => "mixed",
            Self::Neutral => "neutral",
        }
    }

    pub(crate) fn parse(value: &str) -> Result<Self, TenderCommandError> {
        match value {
            "left_to_right" => Ok(Self::LeftToRight),
            "right_to_left" => Ok(Self::RightToLeft),
            "mixed" => Ok(Self::Mixed),
            "neutral" => Ok(Self::Neutral),
            _ => Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct EvidenceBoundingBox {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub coordinate_origin: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct EvidenceRegion {
    pub page_number: u32,
    pub char_start: Option<u32>,
    pub char_end: Option<u32>,
    pub bounding_box: Option<EvidenceBoundingBox>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct EvidenceLocation {
    pub ordinal: u32,
    pub kind: EvidenceLocationKind,
    pub structural_path: String,
    pub provenance: Vec<EvidenceRegion>,
    pub section: Option<String>,
    pub paragraph_number: Option<u32>,
    pub table_number: Option<u32>,
    pub sheet_name: Option<String>,
    pub cell_range: Option<String>,
    pub original_text: String,
    pub translated_text: Option<String>,
    pub language: EvidenceLanguage,
    pub direction: TextDirection,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct DocumentParseResult {
    pub attempt_id: String,
    pub artifact_id: String,
    pub version: u32,
    pub state: ParseState,
    pub exception: Option<ParseExceptionCode>,
    pub location_count: u32,
    pub language: EvidenceLanguage,
    pub direction: TextDirection,
    pub docling_schema_version: Option<String>,
    pub docling_json_sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct EvidenceDocument {
    pub artifact_id: String,
    pub version: u32,
    pub state: ParseState,
    pub exception: Option<ParseExceptionCode>,
    pub language: EvidenceLanguage,
    pub direction: TextDirection,
    pub docling_schema_version: Option<String>,
    pub docling_json_sha256: Option<String>,
    pub locations: Vec<EvidenceLocation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct EvidenceSearchHit {
    pub artifact_id: String,
    pub version: u32,
    pub package_path: String,
    pub location: EvidenceLocation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct EvidenceSearchResult {
    pub query: String,
    pub matches: Vec<EvidenceSearchHit>,
}

pub(crate) struct ParseJob {
    pub attempt_id: String,
    pub tender_id: TenderId,
    pub artifact_id: String,
    pub version: u32,
    pub input_format: String,
    pub staging_root: PathBuf,
    pub input_path: PathBuf,
    pub candidate_directory: PathBuf,
}

pub(crate) struct PreparedParseOutput {
    pub json_bytes: Vec<u8>,
    pub schema_version: String,
    pub language: EvidenceLanguage,
    pub direction: TextDirection,
    pub locations: Vec<EvidenceLocation>,
}

struct StagingCleanup(PathBuf);

impl Drop for StagingCleanup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct ActiveParseCleanup {
    host: QuantixHost,
    key: ParseTargetKey,
}

impl Drop for ActiveParseCleanup {
    fn drop(&mut self) {
        self.host.finish_active_parse(&self.key);
    }
}

impl QuantixHost {
    pub async fn parse_source_artifact(
        &self,
        command: ParseSourceArtifactCommand,
    ) -> Result<DocumentParseResult, TenderCommandError> {
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        self.require_runtime_verified()?;
        crate::tender_store::require_setup(self)?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let activity_key =
            ParseTargetKey::new(&command.tender_id, &command.artifact_id, command.version);
        let cancellation = self.begin_active_parse(activity_key.clone())?;
        let _active_parse_cleanup = ActiveParseCleanup {
            host: self.clone(),
            key: activity_key,
        };
        let store = match self.tender_store(&tender_id) {
            Ok(store) => store,
            Err(error) => return Err(error),
        };
        let job = match store.lock() {
            Ok(mut store) => store.begin_parse(&command, &tender_id),
            Err(_) => Err(TenderCommandError::new(TenderErrorCode::StoreUnavailable)),
        };
        let job = match job {
            Ok(job) => job,
            Err(error) => return Err(error),
        };
        let _staging_cleanup = StagingCleanup(job.staging_root.clone());
        let output = self
            .process_supervisor()
            .run(docling_process_spec(self, &job), cancellation)
            .await;

        let result = match output {
            Ok(output)
                if output.termination == ProcessTermination::Exited
                    && output.exit_code == Some(0) =>
            {
                match validate_candidate_output(&job) {
                    Ok(prepared) => {
                        let mut store = store.lock().map_err(|_| {
                            TenderCommandError::new(TenderErrorCode::StoreUnavailable)
                        })?;
                        match store.publish_parse(&job, prepared) {
                            Ok(result) => Ok(result),
                            Err(_) => store.fail_parse(
                                &job,
                                ParseState::Failed,
                                ParseExceptionCode::PublicationFailed,
                            ),
                        }
                    }
                    Err(exception) => store
                        .lock()
                        .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
                        .fail_parse(&job, ParseState::Quarantined, exception),
                }
            }
            Ok(output) => {
                let (state, exception) = match output.termination {
                    ProcessTermination::Cancelled | ProcessTermination::TimedOut => {
                        (ParseState::Interrupted, ParseExceptionCode::Interrupted)
                    }
                    ProcessTermination::OutputLimitExceeded => (
                        ParseState::Quarantined,
                        ParseExceptionCode::OutputLimitExceeded,
                    ),
                    ProcessTermination::Exited => {
                        (ParseState::Failed, ParseExceptionCode::ProcessFailed)
                    }
                };
                store
                    .lock()
                    .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
                    .fail_parse(&job, state, exception)
            }
            Err(_) => store
                .lock()
                .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
                .fail_parse(&job, ParseState::Failed, ParseExceptionCode::ProcessFailed),
        };
        result
    }

    pub fn cancel_source_artifact_parse(
        &self,
        command: ParseSourceArtifactCommand,
    ) -> Result<bool, TenderCommandError> {
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        crate::tender_store::require_setup(self)?;
        TenderId::parse(&command.tender_id)?;
        Ok(self.cancel_active_parse(&ParseTargetKey::new(
            &command.tender_id,
            &command.artifact_id,
            command.version,
        )))
    }

    pub fn inspect_evidence(
        &self,
        command: ParseSourceArtifactCommand,
    ) -> Result<EvidenceDocument, TenderCommandError> {
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        crate::tender_store::require_setup(self)?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        self.tender_store(&tender_id)?
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .evidence_document(&command)
    }

    pub fn search_evidence(
        &self,
        command: SearchEvidenceCommand,
    ) -> Result<EvidenceSearchResult, TenderCommandError> {
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        if command.query.trim().is_empty() {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        crate::tender_store::require_setup(self)?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        self.tender_store(&tender_id)?
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .search_evidence(&command)
    }
}

fn docling_process_spec(host: &QuantixHost, job: &ParseJob) -> ProcessSpec {
    let arguments = vec![
        OsString::from("convert"),
        job.input_path.clone().into_os_string(),
        OsString::from("--from"),
        OsString::from(&job.input_format),
        OsString::from("--to"),
        OsString::from("json"),
        OsString::from("--image-export-mode"),
        OsString::from("placeholder"),
        OsString::from("--artifacts-path"),
        host.application_home()
            .join("models")
            .join("docling")
            .into_os_string(),
        OsString::from("--no-enable-remote-services"),
        OsString::from("--no-allow-external-plugins"),
        OsString::from("--abort-on-error"),
        OsString::from("--document-timeout"),
        OsString::from(DOCLING_DOCUMENT_TIMEOUT_SECONDS),
        OsString::from("--num-threads"),
        OsString::from("2"),
        OsString::from("--device"),
        OsString::from("cpu"),
        OsString::from("--quiet"),
        OsString::from("--output"),
        job.candidate_directory.clone().into_os_string(),
    ];
    ProcessSpec {
        executable: docling_executable(host.application_home()),
        arguments,
        current_directory: Some(job.staging_root.clone()),
        environment: docling_environment(host.application_home()),
        inherit_environment: false,
        stdin: Vec::new(),
        timeout: DOCLING_PROCESS_TIMEOUT,
        stdout_limit: DOCLING_PROCESS_OUTPUT_LIMIT,
        stderr_limit: DOCLING_PROCESS_OUTPUT_LIMIT,
    }
}

fn validate_candidate_output(job: &ParseJob) -> Result<PreparedParseOutput, ParseExceptionCode> {
    for directory in [&job.staging_root, &job.candidate_directory] {
        let metadata =
            fs::symlink_metadata(directory).map_err(|_| ParseExceptionCode::MalformedOutput)?;
        if metadata_is_unsafe_storage_link(&metadata) || !metadata.is_dir() {
            return Err(ParseExceptionCode::MalformedOutput);
        }
    }
    let input_metadata =
        fs::symlink_metadata(&job.input_path).map_err(|_| ParseExceptionCode::MalformedOutput)?;
    if metadata_is_unsafe_storage_link(&input_metadata) || !input_metadata.is_file() {
        return Err(ParseExceptionCode::MalformedOutput);
    }
    let mut entries =
        fs::read_dir(&job.candidate_directory).map_err(|_| ParseExceptionCode::MalformedOutput)?;
    let entry = entries
        .next()
        .ok_or(ParseExceptionCode::MalformedOutput)?
        .map_err(|_| ParseExceptionCode::MalformedOutput)?;
    if entries.next().is_some() {
        return Err(ParseExceptionCode::MalformedOutput);
    }
    let metadata =
        fs::symlink_metadata(entry.path()).map_err(|_| ParseExceptionCode::MalformedOutput)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(ParseExceptionCode::MalformedOutput);
    }
    if metadata.len() > MAX_DOCLING_JSON_BYTES {
        return Err(ParseExceptionCode::OutputLimitExceeded);
    }
    let expected_name = job
        .input_path
        .file_stem()
        .and_then(|name| name.to_str())
        .map(|name| format!("{name}.json"))
        .ok_or(ParseExceptionCode::MalformedOutput)?;
    if entry.file_name() != expected_name.as_str() {
        return Err(ParseExceptionCode::MalformedOutput);
    }
    let mut file = fs::File::open(entry.path()).map_err(|_| ParseExceptionCode::MalformedOutput)?;
    let mut bytes = Vec::with_capacity(usize::try_from(metadata.len()).unwrap_or(0));
    file.by_ref()
        .take(MAX_DOCLING_JSON_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| ParseExceptionCode::MalformedOutput)?;
    if bytes.len() as u64 != metadata.len() || bytes.len() as u64 > MAX_DOCLING_JSON_BYTES {
        return Err(ParseExceptionCode::OutputLimitExceeded);
    }
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|_| ParseExceptionCode::MalformedOutput)?;
    let validator = jsonschema::validator_for(&docling_document_schema())
        .map_err(|_| ParseExceptionCode::MalformedOutput)?;
    if !validator.is_valid(&value) {
        return Err(ParseExceptionCode::MalformedOutput);
    }
    let expected_mimetype = match job.input_format.as_str() {
        "pdf" => "application/pdf",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        _ => return Err(ParseExceptionCode::MalformedOutput),
    };
    if value.pointer("/origin/mimetype").and_then(Value::as_str) != Some(expected_mimetype) {
        return Err(ParseExceptionCode::MalformedOutput);
    }
    let expected_stem = job
        .input_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or(ParseExceptionCode::MalformedOutput)?;
    let expected_filename = format!("{expected_stem}.{}", job.input_format);
    if value.get("name").and_then(Value::as_str) != Some(expected_stem)
        || value.pointer("/origin/filename").and_then(Value::as_str)
            != Some(expected_filename.as_str())
    {
        return Err(ParseExceptionCode::MalformedOutput);
    }
    let locations = extract_locations(&value)?;
    if locations.is_empty() || locations.len() > MAX_EVIDENCE_LOCATIONS {
        return Err(ParseExceptionCode::LossDetected);
    }
    let combined = locations
        .iter()
        .map(|location| location.original_text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let (language, direction) = classify_text(&combined);
    Ok(PreparedParseOutput {
        json_bytes: bytes,
        schema_version: DOCLING_DOCUMENT_SCHEMA_VERSION.into(),
        language,
        direction,
        locations,
    })
}

fn docling_document_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "required": [
            "schema_name", "version", "name", "origin", "body",
            "groups", "texts", "tables", "pages"
        ],
        "properties": {
            "schema_name": { "const": "DoclingDocument" },
            "version": { "const": DOCLING_DOCUMENT_SCHEMA_VERSION },
            "name": { "type": "string", "minLength": 1, "maxLength": 1024 },
            "origin": {
                "type": "object",
                "required": ["mimetype", "filename"],
                "properties": {
                    "mimetype": { "type": "string", "minLength": 1, "maxLength": 255 },
                    "filename": { "type": "string", "maxLength": 2048 }
                }
            },
            "body": {
                "type": "object",
                "required": ["self_ref", "children"],
                "properties": {
                    "self_ref": { "const": "#/body" },
                    "children": { "$ref": "#/$defs/children" }
                }
            },
            "groups": {
                "type": "array",
                "maxItems": MAX_EVIDENCE_LOCATIONS,
                "items": {
                    "type": "object",
                    "required": ["self_ref", "parent", "children", "name", "label"],
                    "properties": {
                        "self_ref": { "$ref": "#/$defs/self_ref" },
                        "parent": { "$ref": "#/$defs/reference" },
                        "children": { "$ref": "#/$defs/children" },
                        "name": { "type": "string", "minLength": 1, "maxLength": 2048 },
                        "label": { "type": "string", "minLength": 1, "maxLength": 100 }
                    }
                }
            },
            "texts": {
                "type": "array",
                "maxItems": MAX_EVIDENCE_LOCATIONS,
                "items": {
                    "type": "object",
                    "required": ["self_ref", "parent", "label", "text"],
                    "properties": {
                        "self_ref": { "$ref": "#/$defs/self_ref" },
                        "parent": { "$ref": "#/$defs/reference" },
                        "label": { "type": "string", "minLength": 1, "maxLength": 100 },
                        "orig": { "type": "string", "maxLength": 1000000 },
                        "text": { "type": "string", "maxLength": 1000000 },
                        "prov": { "$ref": "#/$defs/provenance" }
                    }
                }
            },
            "tables": {
                "type": "array",
                "maxItems": MAX_EVIDENCE_LOCATIONS,
                "items": {
                    "type": "object",
                    "required": ["self_ref", "parent", "label", "data"],
                    "properties": {
                        "self_ref": { "$ref": "#/$defs/self_ref" },
                        "parent": { "$ref": "#/$defs/reference" },
                        "label": { "const": "table" },
                        "data": {
                            "type": "object",
                            "required": ["num_rows", "num_cols", "table_cells"],
                            "properties": {
                                "num_rows": { "type": "integer", "minimum": 1 },
                                "num_cols": { "type": "integer", "minimum": 1 },
                                "table_cells": {
                                    "type": "array",
                                    "minItems": 1,
                                    "maxItems": MAX_EVIDENCE_LOCATIONS,
                                    "items": { "$ref": "#/$defs/table_cell" }
                                }
                            }
                        },
                        "prov": { "$ref": "#/$defs/provenance" }
                    }
                }
            },
            "pictures": { "$ref": "#/$defs/ignored_items" },
            "key_value_items": { "$ref": "#/$defs/ignored_items" },
            "form_items": { "$ref": "#/$defs/ignored_items" },
            "pages": {
                "type": "object",
                "maxProperties": 100000,
                "propertyNames": { "pattern": "^[1-9][0-9]*$" },
                "additionalProperties": {
                    "type": "object",
                    "required": ["page_no", "size"],
                    "properties": {
                        "page_no": { "type": "integer", "minimum": 1 },
                        "size": {
                            "type": "object",
                            "required": ["width", "height"],
                            "properties": {
                                "width": { "type": "number", "exclusiveMinimum": 0 },
                                "height": { "type": "number", "exclusiveMinimum": 0 }
                            }
                        }
                    }
                }
            }
        },
        "$defs": {
            "self_ref": {
                "type": "string",
                "minLength": 1,
                "maxLength": 2048,
                "pattern": "^#/[a-z_]+/[0-9]+$"
            },
            "reference": {
                "type": "object",
                "required": ["$ref"],
                "properties": {
                    "$ref": { "type": "string", "minLength": 1, "maxLength": 2048 }
                }
            },
            "children": {
                "type": "array",
                "maxItems": MAX_EVIDENCE_LOCATIONS,
                "items": { "$ref": "#/$defs/reference" }
            },
            "bounding_box": {
                "type": "object",
                "required": ["l", "t", "r", "b", "coord_origin"],
                "properties": {
                    "l": { "type": "number" },
                    "t": { "type": "number" },
                    "r": { "type": "number" },
                    "b": { "type": "number" },
                    "coord_origin": { "enum": ["TOPLEFT", "BOTTOMLEFT"] }
                }
            },
            "provenance": {
                "type": "array",
                "maxItems": 10000,
                "items": {
                    "type": "object",
                    "required": ["page_no", "charspan", "bbox"],
                    "properties": {
                        "page_no": { "type": "integer", "minimum": 1 },
                        "charspan": {
                            "type": "array",
                            "prefixItems": [
                                { "type": "integer", "minimum": 0 },
                                { "type": "integer", "minimum": 0 }
                            ],
                            "minItems": 2,
                            "maxItems": 2
                        },
                        "bbox": { "$ref": "#/$defs/bounding_box" }
                    }
                }
            },
            "table_cell": {
                "type": "object",
                "required": [
                    "start_row_offset_idx", "end_row_offset_idx",
                    "start_col_offset_idx", "end_col_offset_idx", "text"
                ],
                "properties": {
                    "start_row_offset_idx": { "type": "integer", "minimum": 0 },
                    "end_row_offset_idx": { "type": "integer", "minimum": 1 },
                    "start_col_offset_idx": { "type": "integer", "minimum": 0 },
                    "end_col_offset_idx": { "type": "integer", "minimum": 1 },
                    "text": { "type": "string", "maxLength": 1000000 }
                }
            },
            "ignored_items": {
                "type": "array",
                "maxItems": MAX_EVIDENCE_LOCATIONS,
                "items": {
                    "type": "object",
                    "required": ["self_ref", "parent"],
                    "properties": {
                        "self_ref": { "$ref": "#/$defs/self_ref" },
                        "parent": { "$ref": "#/$defs/reference" }
                    }
                }
            }
        }
    })
}
#[derive(Clone, Default)]
struct StructureContext {
    section: Option<String>,
    sheet_name: Option<String>,
}

struct ExtractionState<'a> {
    groups: HashMap<String, &'a Value>,
    texts: HashMap<String, &'a Value>,
    tables: HashMap<String, &'a Value>,
    ignored_items: HashMap<String, &'a Value>,
    pages: &'a serde_json::Map<String, Value>,
    visited: HashSet<String>,
    locations: Vec<EvidenceLocation>,
    paragraph_number: u32,
    table_number: u32,
}

impl<'a> ExtractionState<'a> {
    fn new(value: &'a Value) -> Result<Self, ParseExceptionCode> {
        let pages = value
            .get("pages")
            .and_then(Value::as_object)
            .ok_or(ParseExceptionCode::MalformedOutput)?;
        for (key, page) in pages {
            if required_u64(page, "page_no")?.to_string() != *key {
                return Err(ParseExceptionCode::MalformedOutput);
            }
        }
        Ok(Self {
            groups: index_items(value, "groups")?,
            texts: index_items(value, "texts")?,
            tables: index_items(value, "tables")?,
            ignored_items: ["pictures", "key_value_items", "form_items"]
                .into_iter()
                .map(|field| index_items(value, field))
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .flatten()
                .collect(),
            pages,
            visited: HashSet::new(),
            locations: Vec::new(),
            paragraph_number: 0,
            table_number: 0,
        })
    }

    fn walk(
        &mut self,
        reference: &str,
        expected_parent: &str,
        context: &StructureContext,
    ) -> Result<(), ParseExceptionCode> {
        if !self.visited.insert(reference.to_owned()) {
            return Err(ParseExceptionCode::MalformedOutput);
        }
        if let Some(group) = self.groups.get(reference).copied() {
            require_parent(group, expected_parent)?;
            let label = required_str(group, "label")?;
            let name = required_str(group, "name")?.to_owned();
            let children = child_references(group)?;
            let mut child_context = context.clone();
            match label {
                "section" => child_context.section = Some(name),
                "sheet" => {
                    child_context.sheet_name = Some(name.clone());
                    let (language, direction) = classify_text(&name);
                    self.locations.push(EvidenceLocation {
                        ordinal: next_ordinal(&self.locations)?,
                        kind: EvidenceLocationKind::Sheet,
                        structural_path: reference.to_owned(),
                        provenance: Vec::new(),
                        section: context.section.clone(),
                        paragraph_number: None,
                        table_number: None,
                        sheet_name: Some(name.clone()),
                        cell_range: None,
                        original_text: name,
                        translated_text: None,
                        language,
                        direction,
                    });
                }
                _ => {}
            }
            for child in children {
                self.walk(&child, reference, &child_context)?;
            }
            return Ok(());
        }
        if let Some(text) = self.texts.get(reference).copied() {
            require_parent(text, expected_parent)?;
            self.push_text(text, reference, context)?;
            return Ok(());
        }
        if let Some(table) = self.tables.get(reference).copied() {
            require_parent(table, expected_parent)?;
            self.push_table(table, reference, context)?;
            return Ok(());
        }
        if let Some(item) = self.ignored_items.get(reference) {
            require_parent(item, expected_parent)?;
            return Ok(());
        }
        Err(ParseExceptionCode::MalformedOutput)
    }

    fn push_text(
        &mut self,
        text: &Value,
        reference: &str,
        context: &StructureContext,
    ) -> Result<(), ParseExceptionCode> {
        let label = required_str(text, "label")?;
        let original_text = text
            .get("orig")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .unwrap_or(required_str(text, "text")?)
            .to_owned();
        if original_text.trim().is_empty() {
            return Ok(());
        }
        let kind = if label == "section_header" {
            EvidenceLocationKind::Section
        } else {
            self.paragraph_number = self.paragraph_number.saturating_add(1);
            EvidenceLocationKind::Paragraph
        };
        let provenance = evidence_provenance(text, self.pages)?;
        let (language, direction) = classify_text(&original_text);
        self.locations.push(EvidenceLocation {
            ordinal: next_ordinal(&self.locations)?,
            kind,
            structural_path: reference.to_owned(),
            provenance,
            section: context.section.clone(),
            paragraph_number: (kind == EvidenceLocationKind::Paragraph)
                .then_some(self.paragraph_number),
            table_number: None,
            sheet_name: context.sheet_name.clone(),
            cell_range: None,
            original_text,
            translated_text: None,
            language,
            direction,
        });
        Ok(())
    }

    fn push_table(
        &mut self,
        table: &Value,
        reference: &str,
        context: &StructureContext,
    ) -> Result<(), ParseExceptionCode> {
        if required_str(table, "label")? != "table" {
            return Err(ParseExceptionCode::MalformedOutput);
        }
        self.table_number = self.table_number.saturating_add(1);
        let table_number = self.table_number;
        let cells = table
            .pointer("/data/table_cells")
            .and_then(Value::as_array)
            .ok_or(ParseExceptionCode::MalformedOutput)?;
        if cells.is_empty() {
            return Err(ParseExceptionCode::LossDetected);
        }
        let num_rows = required_u64(
            table
                .get("data")
                .ok_or(ParseExceptionCode::MalformedOutput)?,
            "num_rows",
        )?;
        let num_cols = required_u64(
            table
                .get("data")
                .ok_or(ParseExceptionCode::MalformedOutput)?,
            "num_cols",
        )?;
        for cell in cells {
            let start_row = required_u64(cell, "start_row_offset_idx")?;
            let end_row = required_u64(cell, "end_row_offset_idx")?;
            let start_col = required_u64(cell, "start_col_offset_idx")?;
            let end_col = required_u64(cell, "end_col_offset_idx")?;
            if start_row >= end_row
                || start_col >= end_col
                || end_row > num_rows
                || end_col > num_cols
            {
                return Err(ParseExceptionCode::MalformedOutput);
            }
        }
        let table_text = cells
            .iter()
            .map(|cell| required_str(cell, "text"))
            .collect::<Result<Vec<_>, _>>()?
            .join("\n");
        let provenance = evidence_provenance(table, self.pages)?;
        let cell_provenance = provenance
            .iter()
            .map(|region| EvidenceRegion {
                page_number: region.page_number,
                char_start: None,
                char_end: None,
                bounding_box: None,
            })
            .collect::<Vec<_>>();
        let (language, direction) = classify_text(&table_text);
        self.locations.push(EvidenceLocation {
            ordinal: next_ordinal(&self.locations)?,
            kind: EvidenceLocationKind::Table,
            structural_path: reference.to_owned(),
            provenance,
            section: context.section.clone(),
            paragraph_number: None,
            table_number: Some(table_number),
            sheet_name: context.sheet_name.clone(),
            cell_range: None,
            original_text: table_text,
            translated_text: None,
            language,
            direction,
        });
        for (cell_index, cell) in cells.iter().enumerate() {
            let original_text = required_str(cell, "text")?.to_owned();
            let cell_range = cell_range(cell).ok_or(ParseExceptionCode::MalformedOutput)?;
            let (language, direction) = classify_text(&original_text);
            self.locations.push(EvidenceLocation {
                ordinal: next_ordinal(&self.locations)?,
                kind: EvidenceLocationKind::Cell,
                structural_path: format!("{reference}/data/table_cells/{cell_index}"),
                provenance: cell_provenance.clone(),
                section: context.section.clone(),
                paragraph_number: None,
                table_number: Some(table_number),
                sheet_name: context.sheet_name.clone(),
                cell_range: Some(cell_range),
                original_text,
                translated_text: None,
                language,
                direction,
            });
        }
        Ok(())
    }

    fn finish(self) -> Result<Vec<EvidenceLocation>, ParseExceptionCode> {
        if self
            .groups
            .keys()
            .chain(self.texts.keys())
            .chain(self.tables.keys())
            .chain(self.ignored_items.keys())
            .any(|reference| !self.visited.contains(reference))
        {
            return Err(ParseExceptionCode::LossDetected);
        }
        Ok(self.locations)
    }
}

fn extract_locations(value: &Value) -> Result<Vec<EvidenceLocation>, ParseExceptionCode> {
    let body = value
        .get("body")
        .ok_or(ParseExceptionCode::MalformedOutput)?;
    if required_str(body, "self_ref")? != "#/body" {
        return Err(ParseExceptionCode::MalformedOutput);
    }
    let children = child_references(body)?;
    let mut state = ExtractionState::new(value)?;
    let context = StructureContext::default();
    for child in children {
        state.walk(&child, "#/body", &context)?;
    }
    state.finish()
}

fn index_items<'a>(
    value: &'a Value,
    field: &str,
) -> Result<HashMap<String, &'a Value>, ParseExceptionCode> {
    let Some(items) = value.get(field) else {
        return Ok(HashMap::new());
    };
    let items = items
        .as_array()
        .ok_or(ParseExceptionCode::MalformedOutput)?;
    let mut indexed = HashMap::new();
    for (index, item) in items.iter().enumerate() {
        let reference = required_str(item, "self_ref")?.to_owned();
        if reference != format!("#/{field}/{index}") {
            return Err(ParseExceptionCode::MalformedOutput);
        }
        if indexed.insert(reference, item).is_some() {
            return Err(ParseExceptionCode::MalformedOutput);
        }
    }
    Ok(indexed)
}

fn child_references(value: &Value) -> Result<Vec<String>, ParseExceptionCode> {
    value
        .get("children")
        .and_then(Value::as_array)
        .ok_or(ParseExceptionCode::MalformedOutput)?
        .iter()
        .map(|child| {
            child
                .get("$ref")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .ok_or(ParseExceptionCode::MalformedOutput)
        })
        .collect()
}

fn require_parent(value: &Value, expected_parent: &str) -> Result<(), ParseExceptionCode> {
    if value
        .get("parent")
        .and_then(|parent| parent.get("$ref"))
        .and_then(Value::as_str)
        == Some(expected_parent)
    {
        Ok(())
    } else {
        Err(ParseExceptionCode::MalformedOutput)
    }
}

fn evidence_provenance(
    value: &Value,
    pages: &serde_json::Map<String, Value>,
) -> Result<Vec<EvidenceRegion>, ParseExceptionCode> {
    let Some(provenance) = value.get("prov") else {
        return Ok(Vec::new());
    };
    let provenance = provenance
        .as_array()
        .ok_or(ParseExceptionCode::MalformedOutput)?;
    let mut regions = Vec::with_capacity(provenance.len());
    for region in provenance {
        let region = region
            .as_object()
            .ok_or(ParseExceptionCode::MalformedOutput)?;
        let page_number: u32 = region
            .get("page_no")
            .and_then(Value::as_u64)
            .and_then(|value| value.try_into().ok())
            .ok_or(ParseExceptionCode::MalformedOutput)?;
        if !pages.contains_key(&page_number.to_string()) {
            return Err(ParseExceptionCode::MalformedOutput);
        }
        let (char_start, char_end) = match region.get("charspan") {
            Some(span) => {
                let span = span
                    .as_array()
                    .filter(|span| span.len() == 2)
                    .ok_or(ParseExceptionCode::MalformedOutput)?;
                let start: u32 = span
                    .first()
                    .and_then(Value::as_u64)
                    .and_then(|value| value.try_into().ok())
                    .ok_or(ParseExceptionCode::MalformedOutput)?;
                let end: u32 = span
                    .get(1)
                    .and_then(Value::as_u64)
                    .and_then(|value| value.try_into().ok())
                    .filter(|end| *end >= start)
                    .ok_or(ParseExceptionCode::MalformedOutput)?;
                (Some(start), Some(end))
            }
            None => (None, None),
        };
        let bounding_box = match region.get("bbox") {
            Some(bbox) => Some(EvidenceBoundingBox {
                left: required_number(bbox, "l")?,
                top: required_number(bbox, "t")?,
                right: required_number(bbox, "r")?,
                bottom: required_number(bbox, "b")?,
                coordinate_origin: required_str(bbox, "coord_origin")?.to_owned(),
            }),
            None => None,
        };
        regions.push(EvidenceRegion {
            page_number,
            char_start,
            char_end,
            bounding_box,
        });
    }
    Ok(regions)
}
fn required_number(value: &Value, field: &str) -> Result<f64, ParseExceptionCode> {
    value
        .get(field)
        .and_then(Value::as_f64)
        .ok_or(ParseExceptionCode::MalformedOutput)
}

fn required_u64(value: &Value, field: &str) -> Result<u64, ParseExceptionCode> {
    value
        .get(field)
        .and_then(Value::as_u64)
        .ok_or(ParseExceptionCode::MalformedOutput)
}
fn required_str<'a>(value: &'a Value, field: &str) -> Result<&'a str, ParseExceptionCode> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or(ParseExceptionCode::MalformedOutput)
}

fn cell_range(cell: &Value) -> Option<String> {
    let start_row = cell.get("start_row_offset_idx")?.as_u64()?;
    let end_row = cell.get("end_row_offset_idx")?.as_u64()?;
    let start_column = cell.get("start_col_offset_idx")?.as_u64()?;
    let end_column = cell.get("end_col_offset_idx")?.as_u64()?;
    if end_row <= start_row || end_column <= start_column {
        return None;
    }
    let start = format!(
        "{}{}",
        spreadsheet_column(start_column.try_into().ok()?)?,
        start_row + 1
    );
    let end = format!(
        "{}{}",
        spreadsheet_column(end_column.checked_sub(1)?.try_into().ok()?)?,
        end_row
    );
    Some(if start == end {
        start
    } else {
        format!("{start}:{end}")
    })
}

fn spreadsheet_column(mut index: u32) -> Option<String> {
    let mut characters = Vec::new();
    loop {
        let remainder = u8::try_from(index % 26).ok()?;
        characters.push(char::from(b'A' + remainder));
        if index < 26 {
            break;
        }
        index = index.checked_div(26)?.checked_sub(1)?;
    }
    characters.reverse();
    Some(characters.into_iter().collect())
}

fn next_ordinal(locations: &[EvidenceLocation]) -> Result<u32, ParseExceptionCode> {
    u32::try_from(locations.len() + 1).map_err(|_| ParseExceptionCode::OutputLimitExceeded)
}

fn classify_text(text: &str) -> (EvidenceLanguage, TextDirection) {
    let mut arabic = 0_usize;
    let mut english = 0_usize;
    let mut right_to_left = false;
    let mut left_to_right = false;
    for character in text.chars() {
        if character.is_alphabetic() {
            match character.script() {
                Script::Arabic => arabic += 1,
                Script::Latin => english += 1,
                _ => {}
            }
        }
        match bidi_class(character) {
            BidiClass::R | BidiClass::AL => right_to_left = true,
            BidiClass::L => left_to_right = true,
            _ => {}
        }
    }
    let language = match (arabic > 0, english > 0) {
        (true, true) => EvidenceLanguage::Mixed,
        (true, false) => EvidenceLanguage::Arabic,
        (false, true) => EvidenceLanguage::English,
        (false, false) => EvidenceLanguage::Undetermined,
    };
    let direction = match (right_to_left, left_to_right) {
        (true, true) => TextDirection::Mixed,
        (true, false) => TextDirection::RightToLeft,
        (false, true) => TextDirection::LeftToRight,
        (false, false) => TextDirection::Neutral,
    };
    (language, direction)
}
