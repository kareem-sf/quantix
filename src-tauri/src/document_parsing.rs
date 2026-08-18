use std::{fs, io::Read, path::PathBuf, time::Duration};

#[cfg(not(feature = "runtime-fixture"))]
use anydoc::ConvertError;
#[cfg(not(feature = "runtime-fixture"))]
use calamine::{open_workbook_auto_from_rs, Data, DataType, Reader};
use garde::Validate;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;
use unicode_bidi::{bidi_class, BidiClass};
use unicode_script::{Script, UnicodeScript};

use crate::{
    host::ParseTargetKey,
    process_supervisor::{ProcessSpec, ProcessTermination},
    runtime_readiness::{ocr_environment, python_executable},
    tender_store::{
        metadata_is_unsafe_storage_link, TenderCommandError, TenderErrorCode, TenderId,
    },
    QuantixHost,
};

pub(crate) const MARKDOWN_PIPELINE_VERSION: &str = "1";
const OCR_DOCUMENT_TIMEOUT_SECONDS: &str = "840";
pub(crate) const DOCUMENT_MAX_PAGES: u32 = 2_000;
const OCR_PROCESS_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const OCR_PROCESS_OUTPUT_LIMIT: usize = 64 * 1024;
#[cfg(not(feature = "runtime-fixture"))]
pub(crate) const MAX_MARKDOWN_BYTES: u64 = 64 * 1024 * 1024;
#[cfg(feature = "runtime-fixture")]
pub(crate) const MAX_MARKDOWN_BYTES: u64 = 64 * 1024;
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

#[derive(Debug, Clone, PartialEq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct SearchEvidenceSemanticCommand {
    #[garde(length(bytes, min = 32, max = 32), ascii)]
    pub tender_id: String,
    #[garde(length(bytes, min = 1, max = 512))]
    pub query: String,
    #[garde(range(min = 0.0, max = 2.0))]
    pub distance_threshold: f32,
    #[garde(range(min = 1, max = 100))]
    pub limit: u32,
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
    pub pipeline_version: Option<String>,
    pub markdown_sha256: Option<String>,
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
    pub pipeline_version: Option<String>,
    pub markdown_sha256: Option<String>,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct EvidenceSemanticSearchHit {
    pub distance: f32,
    pub artifact_id: String,
    pub version: u32,
    pub package_path: String,
    pub location: EvidenceLocation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct EvidenceSemanticSearchResult {
    pub query: String,
    pub matches: Vec<EvidenceSemanticSearchHit>,
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
    pub markdown_bytes: Vec<u8>,
    pub language: EvidenceLanguage,
    pub direction: TextDirection,
    pub locations: Vec<EvidenceLocation>,
    pub embeddings: Vec<Vec<f32>>,
}

enum ConversionOutcome {
    Prepared(PreparedParseOutput),
    Interrupted,
    Failed(ParseExceptionCode),
    Quarantined(ParseExceptionCode),
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
        let outcome = self.convert_source_artifact(&job, cancellation).await;

        match outcome {
            ConversionOutcome::Prepared(mut prepared) => {
                if let Err(exception) = validate_prepared_output(&job, &prepared) {
                    return store
                        .lock()
                        .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
                        .fail_parse(&job, ParseState::Quarantined, exception);
                }
                let texts = prepared
                    .locations
                    .iter()
                    .map(|location| location.original_text.clone())
                    .collect();
                prepared.embeddings =
                    match crate::embedding::embed_evidence_locations(self, texts).await {
                        Ok(embeddings) => embeddings,
                        Err(_) => {
                            return store
                                .lock()
                                .map_err(|_| {
                                    TenderCommandError::new(TenderErrorCode::StoreUnavailable)
                                })?
                                .fail_parse(
                                    &job,
                                    ParseState::Failed,
                                    ParseExceptionCode::ProcessFailed,
                                );
                        }
                    };
                let mut store = store
                    .lock()
                    .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
                match store.publish_parse(&job, prepared) {
                    Ok(result) => Ok(result),
                    Err(_) => store.fail_parse(
                        &job,
                        ParseState::Failed,
                        ParseExceptionCode::PublicationFailed,
                    ),
                }
            }
            ConversionOutcome::Interrupted => store
                .lock()
                .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
                .fail_parse(
                    &job,
                    ParseState::Interrupted,
                    ParseExceptionCode::Interrupted,
                ),
            ConversionOutcome::Failed(exception) => store
                .lock()
                .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
                .fail_parse(&job, ParseState::Failed, exception),
            ConversionOutcome::Quarantined(exception) => store
                .lock()
                .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
                .fail_parse(&job, ParseState::Quarantined, exception),
        }
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

    pub async fn search_evidence_semantic(
        &self,
        command: SearchEvidenceSemanticCommand,
    ) -> Result<EvidenceSemanticSearchResult, TenderCommandError> {
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        if command.query.trim().is_empty() || !command.distance_threshold.is_finite() {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        self.require_runtime_verified()?;
        crate::tender_store::require_setup(self)?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let query_embedding =
            crate::embedding::embed_search_query(self, command.query.clone()).await?;
        self.tender_store(&tender_id)?
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .search_evidence_semantic(&command, &query_embedding)
    }

    async fn convert_source_artifact(
        &self,
        job: &ParseJob,
        cancellation: tokio_util::sync::CancellationToken,
    ) -> ConversionOutcome {
        #[cfg(feature = "runtime-fixture")]
        {
            // Fixture tests own the conversion pipeline end to end through the
            // supervised OCR process so every deterministic scenario is
            // reproducible without real document bytes.
            return self.convert_scanned_document(job, cancellation).await;
        }
        #[cfg(not(feature = "runtime-fixture"))]
        {
            let digital = self.convert_digital_document(job, &cancellation).await;
            match digital {
                Ok((markdown, locations)) => match prepare_digital_output(&markdown, locations) {
                    Ok(prepared) => ConversionOutcome::Prepared(prepared),
                    Err(exception) => ConversionOutcome::Quarantined(exception),
                },
                Err(ConversionError::NeedsOcr) => {
                    if cancellation.is_cancelled() {
                        return ConversionOutcome::Interrupted;
                    }
                    self.convert_scanned_document(job, cancellation).await
                }
                Err(ConversionError::Malformed) => {
                    ConversionOutcome::Failed(ParseExceptionCode::MalformedOutput)
                }
                Err(ConversionError::OutputLimit) => {
                    ConversionOutcome::Failed(ParseExceptionCode::OutputLimitExceeded)
                }
            }
        }
    }

    #[cfg(not(feature = "runtime-fixture"))]
    async fn convert_digital_document(
        &self,
        job: &ParseJob,
        cancellation: &tokio_util::sync::CancellationToken,
    ) -> Result<(String, Option<Vec<EvidenceLocation>>), ConversionError> {
        let input_path = job.input_path.clone();
        let input_format = job.input_format.clone();
        let converted = tokio::task::spawn_blocking(
            move || -> Result<(String, Option<Vec<EvidenceLocation>>), ConversionError> {
                let bytes = fs::read(&input_path).map_err(|_| ConversionError::Malformed)?;
                if bytes.len() as u64 > crate::tender_intake::MAX_INTAKE_FILE_BYTES {
                    return Err(ConversionError::OutputLimit);
                }
                match input_format.as_str() {
                    "pdf" | "docx" => {
                        let converted =
                            anydoc::to_markdown(&input_path).map_err(map_anydoc_error)?;
                        Ok((converted, None))
                    }
                    "xlsx" => {
                        let (markdown, locations) = convert_spreadsheet(&bytes)?;
                        Ok((markdown, Some(locations)))
                    }
                    _ => Err(ConversionError::Malformed),
                }
            },
        )
        .await
        .map_err(|_| ConversionError::Malformed)?;
        if cancellation.is_cancelled() {
            return Err(ConversionError::NeedsOcr);
        }
        converted
    }

    async fn convert_scanned_document(
        &self,
        job: &ParseJob,
        cancellation: tokio_util::sync::CancellationToken,
    ) -> ConversionOutcome {
        let output = self
            .process_supervisor()
            .run(ocr_process_spec(self, job), cancellation)
            .await;
        match output {
            Ok(output)
                if output.termination == ProcessTermination::Exited
                    && output.exit_code == Some(0) =>
            {
                match validate_ocr_candidate_output(job) {
                    Ok(prepared) => ConversionOutcome::Prepared(prepared),
                    Err(exception) => ConversionOutcome::Quarantined(exception),
                }
            }
            Ok(output) => match output.termination {
                ProcessTermination::Cancelled | ProcessTermination::TimedOut => {
                    ConversionOutcome::Interrupted
                }
                ProcessTermination::OutputLimitExceeded => {
                    ConversionOutcome::Failed(ParseExceptionCode::OutputLimitExceeded)
                }
                ProcessTermination::Exited => {
                    ConversionOutcome::Failed(ParseExceptionCode::ProcessFailed)
                }
            },
            Err(_) => ConversionOutcome::Failed(ParseExceptionCode::ProcessFailed),
        }
    }
}

#[cfg(not(feature = "runtime-fixture"))]
enum ConversionError {
    NeedsOcr,
    Malformed,
    OutputLimit,
}

#[cfg(not(feature = "runtime-fixture"))]
fn map_anydoc_error(error: ConvertError) -> ConversionError {
    match error {
        ConvertError::Unsupported(_) => ConversionError::NeedsOcr,
        ConvertError::ResourceLimit { .. } => ConversionError::OutputLimit,
        _ => ConversionError::Malformed,
    }
}

fn ocr_process_spec(host: &QuantixHost, job: &ParseJob) -> ProcessSpec {
    let arguments = vec![
        host.runtime_layout()
            .ocr_project()
            .join("ocr_document.py")
            .into_os_string(),
        std::ffi::OsString::from("--input"),
        job.input_path.clone().into_os_string(),
        std::ffi::OsString::from("--output-dir"),
        job.candidate_directory.clone().into_os_string(),
        std::ffi::OsString::from("--artifacts-path"),
        host.application_home()
            .join("models")
            .join("ocr")
            .into_os_string(),
        std::ffi::OsString::from("--document-timeout"),
        std::ffi::OsString::from(OCR_DOCUMENT_TIMEOUT_SECONDS),
        std::ffi::OsString::from("--max-file-size"),
        std::ffi::OsString::from(crate::tender_intake::MAX_INTAKE_FILE_BYTES.to_string()),
        std::ffi::OsString::from("--max-num-pages"),
        std::ffi::OsString::from(DOCUMENT_MAX_PAGES.to_string()),
        std::ffi::OsString::from("--num-threads"),
        // OCR conversion holds a rendered page image plus the detection and
        // recognition tensors at once. A single worker keeps that memory
        // footprint bounded on ordinary desktop hardware.
        std::ffi::OsString::from("1"),
    ];
    ProcessSpec {
        executable: python_executable(host.application_home()),
        arguments,
        current_directory: Some(job.staging_root.clone()),
        environment: ocr_environment(host.application_home()),
        inherit_environment: false,
        stdin: Vec::new(),
        timeout: OCR_PROCESS_TIMEOUT,
        stdout_limit: OCR_PROCESS_OUTPUT_LIMIT,
        stderr_limit: OCR_PROCESS_OUTPUT_LIMIT,
    }
}

fn validate_prepared_output(
    job: &ParseJob,
    prepared: &PreparedParseOutput,
) -> Result<(), ParseExceptionCode> {
    let markdown = std::str::from_utf8(&prepared.markdown_bytes)
        .map_err(|_| ParseExceptionCode::MalformedOutput)?;
    if prepared.locations.is_empty()
        || prepared.locations.len() > MAX_EVIDENCE_LOCATIONS
        || markdown.is_empty()
        || prepared.markdown_bytes.len() as u64 > MAX_MARKDOWN_BYTES
    {
        return Err(ParseExceptionCode::LossDetected);
    }
    for location in &prepared.locations {
        for region in &location.provenance {
            if region.page_number == 0
                || matches!(
                    (region.char_start, region.char_end),
                    (Some(start), Some(end)) if end < start
                )
                || matches!(
                    (region.char_start, region.char_end),
                    (Some(_), None) | (None, Some(_))
                )
            {
                return Err(ParseExceptionCode::MalformedOutput);
            }
            if let Some(end) = region.char_end {
                if usize::try_from(end)
                    .ok()
                    .is_none_or(|end| end > markdown.chars().count())
                {
                    return Err(ParseExceptionCode::MalformedOutput);
                }
            }
        }
    }
    if job
        .input_path
        .extension()
        .and_then(|extension| extension.to_str())
        != Some(job.input_format.as_str())
    {
        return Err(ParseExceptionCode::MalformedOutput);
    }
    Ok(())
}

#[cfg(not(feature = "runtime-fixture"))]
fn prepare_digital_output(
    markdown: &str,
    spreadsheet_locations: Option<Vec<EvidenceLocation>>,
) -> Result<PreparedParseOutput, ParseExceptionCode> {
    if markdown.trim().is_empty() || markdown.len() as u64 > MAX_MARKDOWN_BYTES {
        return Err(ParseExceptionCode::LossDetected);
    }
    let locations = match spreadsheet_locations {
        Some(locations) => locations,
        None => extract_markdown_locations(markdown)?,
    };
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
        markdown_bytes: markdown.as_bytes().to_vec(),
        language,
        direction,
        locations,
        embeddings: Vec::new(),
    })
}

fn validate_ocr_candidate_output(
    job: &ParseJob,
) -> Result<PreparedParseOutput, ParseExceptionCode> {
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
    let stem = job
        .input_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or(ParseExceptionCode::MalformedOutput)?;
    let expected_markdown = format!("{stem}.md");
    let expected_locations = format!("{stem}.locations.json");
    let mut entries = fs::read_dir(&job.candidate_directory)
        .map_err(|_| ParseExceptionCode::MalformedOutput)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| ParseExceptionCode::MalformedOutput)?;
    entries.sort_by_key(fs::DirEntry::file_name);
    let names = entries
        .iter()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    if names != [expected_locations.clone(), expected_markdown.clone()] {
        return Err(ParseExceptionCode::MalformedOutput);
    }
    let markdown_path = job.candidate_directory.join(&expected_markdown);
    let locations_path = job.candidate_directory.join(&expected_locations);
    for path in [&markdown_path, &locations_path] {
        let metadata =
            fs::symlink_metadata(path).map_err(|_| ParseExceptionCode::MalformedOutput)?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(ParseExceptionCode::MalformedOutput);
        }
    }
    let markdown_bytes = read_limited_file(&markdown_path, MAX_MARKDOWN_BYTES)?;
    let markdown =
        std::str::from_utf8(&markdown_bytes).map_err(|_| ParseExceptionCode::MalformedOutput)?;
    let locations_bytes = read_limited_file(&locations_path, MAX_MARKDOWN_BYTES)?;
    let locations = parse_ocr_locations(markdown, &locations_bytes)?;
    let combined = locations
        .iter()
        .map(|location| location.original_text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let (language, direction) = classify_text(&combined);
    Ok(PreparedParseOutput {
        markdown_bytes,
        language,
        direction,
        locations,
        embeddings: Vec::new(),
    })
}

fn read_limited_file(path: &std::path::Path, limit: u64) -> Result<Vec<u8>, ParseExceptionCode> {
    let metadata = fs::symlink_metadata(path).map_err(|_| ParseExceptionCode::MalformedOutput)?;
    if metadata.len() > limit {
        return Err(ParseExceptionCode::OutputLimitExceeded);
    }
    let mut file = fs::File::open(path).map_err(|_| ParseExceptionCode::MalformedOutput)?;
    let mut bytes = Vec::with_capacity(usize::try_from(metadata.len()).unwrap_or(0));
    file.by_ref()
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| ParseExceptionCode::MalformedOutput)?;
    if bytes.len() as u64 != metadata.len() || bytes.len() as u64 > limit {
        return Err(ParseExceptionCode::OutputLimitExceeded);
    }
    Ok(bytes)
}

fn parse_ocr_locations(
    markdown: &str,
    locations_bytes: &[u8],
) -> Result<Vec<EvidenceLocation>, ParseExceptionCode> {
    let value: Value =
        serde_json::from_slice(locations_bytes).map_err(|_| ParseExceptionCode::MalformedOutput)?;
    if value.get("schema_version").and_then(Value::as_u64) != Some(1) {
        return Err(ParseExceptionCode::MalformedOutput);
    }
    let entries = value
        .get("locations")
        .and_then(Value::as_array)
        .ok_or(ParseExceptionCode::MalformedOutput)?;
    if entries.is_empty() || entries.len() > MAX_EVIDENCE_LOCATIONS {
        return Err(ParseExceptionCode::LossDetected);
    }
    let markdown_chars = markdown.chars().count();
    let mut locations = Vec::with_capacity(entries.len());
    for entry in entries {
        let entry = entry
            .as_object()
            .ok_or(ParseExceptionCode::MalformedOutput)?;
        let kind = match entry
            .get("kind")
            .and_then(Value::as_str)
            .ok_or(ParseExceptionCode::MalformedOutput)?
        {
            "section" => EvidenceLocationKind::Section,
            "paragraph" => EvidenceLocationKind::Paragraph,
            "table" => EvidenceLocationKind::Table,
            "sheet" => EvidenceLocationKind::Sheet,
            "cell" => EvidenceLocationKind::Cell,
            _ => return Err(ParseExceptionCode::MalformedOutput),
        };
        let text = entry
            .get("text")
            .and_then(Value::as_str)
            .ok_or(ParseExceptionCode::MalformedOutput)?;
        if text.trim().is_empty() {
            return Err(ParseExceptionCode::MalformedOutput);
        }
        let translated_text = entry
            .get("translated_text")
            .and_then(Value::as_str)
            .filter(|translated| !translated.is_empty())
            .map(str::to_owned);
        let provenance = match entry.get("provenance") {
            Some(provenance) => {
                let regions = provenance
                    .as_array()
                    .ok_or(ParseExceptionCode::MalformedOutput)?;
                let mut parsed = Vec::with_capacity(regions.len());
                for region in regions {
                    let region = region
                        .as_object()
                        .ok_or(ParseExceptionCode::MalformedOutput)?;
                    let page_number: u32 = region
                        .get("page")
                        .and_then(Value::as_u64)
                        .and_then(|page| page.try_into().ok())
                        .filter(|page| *page > 0)
                        .ok_or(ParseExceptionCode::MalformedOutput)?;
                    let (char_start, char_end) =
                        match (region.get("char_start"), region.get("char_end")) {
                            (None, None) => (None, None),
                            (Some(start), Some(end)) => {
                                let start: u32 = start
                                    .as_u64()
                                    .and_then(|start| start.try_into().ok())
                                    .ok_or(ParseExceptionCode::MalformedOutput)?;
                                let end: u32 = end
                                    .as_u64()
                                    .and_then(|end| end.try_into().ok())
                                    .ok_or(ParseExceptionCode::MalformedOutput)?;
                                if end < start
                                    || usize::try_from(end)
                                        .ok()
                                        .is_none_or(|end| end > markdown_chars)
                                {
                                    return Err(ParseExceptionCode::MalformedOutput);
                                }
                                (Some(start), Some(end))
                            }
                            _ => return Err(ParseExceptionCode::MalformedOutput),
                        };
                    parsed.push(EvidenceRegion {
                        page_number,
                        char_start,
                        char_end,
                        bounding_box: parse_bounding_box(region.get("bbox"))?,
                    });
                }
                parsed
            }
            None => {
                let page_number: u32 = match entry.get("page") {
                    Some(page) => page
                        .as_u64()
                        .and_then(|page| page.try_into().ok())
                        .filter(|page| *page > 0)
                        .ok_or(ParseExceptionCode::MalformedOutput)?,
                    None => 1,
                };
                let (char_start, char_end) = match (entry.get("char_start"), entry.get("char_end"))
                {
                    (None, None) => (None, None),
                    (Some(start), Some(end)) => {
                        let start: u32 = start
                            .as_u64()
                            .and_then(|start| start.try_into().ok())
                            .ok_or(ParseExceptionCode::MalformedOutput)?;
                        let end: u32 = end
                            .as_u64()
                            .and_then(|end| end.try_into().ok())
                            .ok_or(ParseExceptionCode::MalformedOutput)?;
                        if end < start
                            || usize::try_from(end)
                                .ok()
                                .is_none_or(|end| end > markdown_chars)
                        {
                            return Err(ParseExceptionCode::MalformedOutput);
                        }
                        (Some(start), Some(end))
                    }
                    _ => return Err(ParseExceptionCode::MalformedOutput),
                };
                vec![EvidenceRegion {
                    page_number,
                    char_start,
                    char_end,
                    bounding_box: parse_bounding_box(entry.get("bbox"))?,
                }]
            }
        };
        let section = entry
            .get("section")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        let sheet_name = entry
            .get("sheet_name")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        let cell_range = entry
            .get("cell_range")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        let paragraph_number = entry
            .get("paragraph_number")
            .and_then(Value::as_u64)
            .and_then(|number| number.try_into().ok())
            .filter(|number: &u32| *number > 0);
        let table_number = entry
            .get("table_number")
            .and_then(Value::as_u64)
            .and_then(|number| number.try_into().ok())
            .filter(|number: &u32| *number > 0);
        let (language, direction) = classify_text(text);
        locations.push(EvidenceLocation {
            ordinal: next_ordinal(&locations)?,
            kind,
            structural_path: format!("markdown://{}", first_provenance_page(&provenance)),
            provenance,
            section,
            paragraph_number,
            table_number,
            sheet_name,
            cell_range,
            original_text: text.to_owned(),
            translated_text,
            language,
            direction,
        });
    }
    Ok(locations)
}

fn first_provenance_page(provenance: &[EvidenceRegion]) -> u32 {
    provenance.first().map_or(1, |region| region.page_number)
}

fn parse_bounding_box(
    value: Option<&Value>,
) -> Result<Option<EvidenceBoundingBox>, ParseExceptionCode> {
    let Some(bbox) = value else {
        return Ok(None);
    };
    let coordinates = bbox
        .as_array()
        .filter(|coordinates| coordinates.len() == 4 || coordinates.len() == 5)
        .ok_or(ParseExceptionCode::MalformedOutput)?;
    let numbers = coordinates[..4]
        .iter()
        .map(Value::as_f64)
        .collect::<Option<Vec<_>>>()
        .ok_or(ParseExceptionCode::MalformedOutput)?;
    let coordinate_origin = match coordinates.get(4) {
        Some(origin) => origin
            .as_str()
            .ok_or(ParseExceptionCode::MalformedOutput)?
            .to_owned(),
        None => "TOPLEFT".to_owned(),
    };
    Ok(Some(EvidenceBoundingBox {
        left: numbers[0],
        top: numbers[1],
        right: numbers[2],
        bottom: numbers[3],
        coordinate_origin,
    }))
}

#[cfg(not(feature = "runtime-fixture"))]
fn convert_spreadsheet(bytes: &[u8]) -> Result<(String, Vec<EvidenceLocation>), ConversionError> {
    let cursor = std::io::Cursor::new(bytes);
    let mut workbook =
        open_workbook_auto_from_rs(cursor).map_err(|_| ConversionError::Malformed)?;
    let mut markdown = String::new();
    let mut locations: Vec<EvidenceLocation> = Vec::new();
    let mut table_number = 0_u32;
    for sheet_name in workbook.sheet_names().to_vec() {
        let range = workbook
            .worksheet_range(&sheet_name)
            .map_err(|_| ConversionError::Malformed)?;
        let rows = range.rows().collect::<Vec<_>>();
        if rows.is_empty() {
            continue;
        }
        let columns = rows.iter().map(|row| row.len()).max().unwrap_or(0);
        let heading_start = markdown.chars().count();
        markdown.push_str(&format!("## {sheet_name}\n\n"));
        let heading_region = EvidenceRegion {
            page_number: 1,
            char_start: Some(
                heading_start
                    .try_into()
                    .map_err(|_| ConversionError::OutputLimit)?,
            ),
            char_end: Some(
                (heading_start + sheet_name.chars().count())
                    .try_into()
                    .map_err(|_| ConversionError::OutputLimit)?,
            ),
            bounding_box: None,
        };
        let (sheet_language, sheet_direction) = classify_text(&sheet_name);
        locations.push(EvidenceLocation {
            ordinal: next_ordinal(&locations).map_err(conversion_limit)?,
            kind: EvidenceLocationKind::Sheet,
            structural_path: format!("markdown://{heading_start}"),
            provenance: vec![heading_region],
            section: None,
            paragraph_number: None,
            table_number: None,
            sheet_name: Some(sheet_name.clone()),
            cell_range: None,
            original_text: sheet_name.clone(),
            translated_text: None,
            language: sheet_language,
            direction: sheet_direction,
        });
        let table_start = markdown.chars().count();
        let mut table_block = String::new();
        let mut cell_texts: Vec<(String, String)> = Vec::new();
        for (row_index, row) in rows.iter().enumerate() {
            let cells = (0..columns)
                .map(|column| {
                    let text = match row.get(column) {
                        Some(Data::Empty) | None => String::new(),
                        Some(cell) => cell
                            .as_string()
                            .unwrap_or_default()
                            .replace('|', "\\|")
                            .replace('\n', " "),
                    };
                    let column_index: u32 = column
                        .try_into()
                        .map_err(|_| ConversionError::OutputLimit)?;
                    let cell_range = format!(
                        "{}{}",
                        spreadsheet_column(column_index).ok_or(ConversionError::Malformed)?,
                        row_index + 1
                    );
                    Ok::<_, ConversionError>((text, cell_range))
                })
                .collect::<Result<Vec<_>, _>>()?;
            table_block.push_str(&format!(
                "| {} |\n",
                cells
                    .iter()
                    .map(|(text, _)| text.as_str())
                    .collect::<Vec<_>>()
                    .join(" | ")
            ));
            if row_index == 0 {
                table_block.push_str(&format!(
                    "| {} |\n",
                    (0..columns).map(|_| "---").collect::<Vec<_>>().join(" | ")
                ));
            }
            for (text, cell_range) in cells {
                if !text.trim().is_empty() {
                    cell_texts.push((text, cell_range));
                }
            }
            if cell_texts.len() > MAX_EVIDENCE_LOCATIONS {
                return Err(ConversionError::OutputLimit);
            }
        }
        markdown.push_str(&table_block);
        markdown.push('\n');
        let table_region = EvidenceRegion {
            page_number: 1,
            char_start: Some(
                table_start
                    .try_into()
                    .map_err(|_| ConversionError::OutputLimit)?,
            ),
            char_end: Some(
                (table_start + table_block.chars().count())
                    .try_into()
                    .map_err(|_| ConversionError::OutputLimit)?,
            ),
            bounding_box: None,
        };
        table_number = table_number.saturating_add(1);
        let table_text = cell_texts
            .iter()
            .map(|(text, _)| text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        if table_text.trim().is_empty() {
            continue;
        }
        let (table_language, table_direction) = classify_text(&table_text);
        locations.push(EvidenceLocation {
            ordinal: next_ordinal(&locations).map_err(conversion_limit)?,
            kind: EvidenceLocationKind::Table,
            structural_path: format!("markdown://{table_start}"),
            provenance: vec![table_region.clone()],
            section: None,
            paragraph_number: None,
            table_number: Some(table_number),
            sheet_name: Some(sheet_name.clone()),
            cell_range: None,
            original_text: table_text,
            translated_text: None,
            language: table_language,
            direction: table_direction,
        });
        for (text, cell_range) in cell_texts {
            let (cell_language, cell_direction) = classify_text(&text);
            locations.push(EvidenceLocation {
                ordinal: next_ordinal(&locations).map_err(conversion_limit)?,
                kind: EvidenceLocationKind::Cell,
                structural_path: format!("markdown://{table_start}"),
                provenance: vec![table_region.clone()],
                section: None,
                paragraph_number: None,
                table_number: Some(table_number),
                sheet_name: Some(sheet_name.clone()),
                cell_range: Some(cell_range),
                original_text: text,
                translated_text: None,
                language: cell_language,
                direction: cell_direction,
            });
        }
        if locations.len() > MAX_EVIDENCE_LOCATIONS {
            return Err(ConversionError::OutputLimit);
        }
    }
    if markdown.trim().is_empty() {
        return Err(ConversionError::Malformed);
    }
    Ok((markdown, locations))
}

#[cfg(not(feature = "runtime-fixture"))]
fn conversion_limit(_: ParseExceptionCode) -> ConversionError {
    ConversionError::OutputLimit
}

#[cfg(not(feature = "runtime-fixture"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MarkdownBlockKind {
    Section,
    Paragraph,
    Table,
}

#[cfg(not(feature = "runtime-fixture"))]
struct MarkdownBlock {
    kind: MarkdownBlockKind,
    start: usize,
    end: usize,
    text: String,
}

#[cfg(not(feature = "runtime-fixture"))]
fn extract_markdown_locations(markdown: &str) -> Result<Vec<EvidenceLocation>, ParseExceptionCode> {
    let blocks = markdown_blocks(markdown);
    if blocks.is_empty() {
        return Err(ParseExceptionCode::LossDetected);
    }
    let mut locations = Vec::new();
    let mut paragraph_number = 0_u32;
    let mut table_number = 0_u32;
    let mut section: Option<String> = None;
    for block in blocks {
        let span = EvidenceRegion {
            page_number: 1,
            char_start: Some(
                block
                    .start
                    .try_into()
                    .map_err(|_| ParseExceptionCode::OutputLimitExceeded)?,
            ),
            char_end: Some(
                block
                    .end
                    .try_into()
                    .map_err(|_| ParseExceptionCode::OutputLimitExceeded)?,
            ),
            bounding_box: None,
        };
        match block.kind {
            MarkdownBlockKind::Section => {
                let (language, direction) = classify_text(&block.text);
                locations.push(EvidenceLocation {
                    ordinal: next_ordinal(&locations)?,
                    kind: EvidenceLocationKind::Section,
                    structural_path: format!("markdown://{}/{}", block.start, block.end),
                    provenance: vec![span],
                    section: None,
                    paragraph_number: None,
                    table_number: None,
                    sheet_name: None,
                    cell_range: None,
                    original_text: block.text.clone(),
                    translated_text: None,
                    language,
                    direction,
                });
                section = Some(block.text);
            }
            MarkdownBlockKind::Paragraph => {
                paragraph_number = paragraph_number.saturating_add(1);
                let (language, direction) = classify_text(&block.text);
                locations.push(EvidenceLocation {
                    ordinal: next_ordinal(&locations)?,
                    kind: EvidenceLocationKind::Paragraph,
                    structural_path: format!("markdown://{}/{}", block.start, block.end),
                    provenance: vec![span],
                    section: section.clone(),
                    paragraph_number: Some(paragraph_number),
                    table_number: None,
                    sheet_name: None,
                    cell_range: None,
                    original_text: block.text.clone(),
                    translated_text: None,
                    language,
                    direction,
                });
            }
            MarkdownBlockKind::Table => {
                table_number = table_number.saturating_add(1);
                let rows = markdown_table_rows(&block.text);
                let table_text = rows
                    .iter()
                    .flat_map(|row| row.iter())
                    .filter(|cell| !cell.trim().is_empty())
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("\n");
                if table_text.trim().is_empty() {
                    return Err(ParseExceptionCode::LossDetected);
                }
                let (language, direction) = classify_text(&table_text);
                locations.push(EvidenceLocation {
                    ordinal: next_ordinal(&locations)?,
                    kind: EvidenceLocationKind::Table,
                    structural_path: format!("markdown://{}/{}", block.start, block.end),
                    provenance: vec![span.clone()],
                    section: section.clone(),
                    paragraph_number: None,
                    table_number: Some(table_number),
                    sheet_name: None,
                    cell_range: None,
                    original_text: table_text,
                    translated_text: None,
                    language,
                    direction,
                });
                for (row_index, row) in rows.iter().enumerate() {
                    for (column_index, cell) in row.iter().enumerate() {
                        if cell.trim().is_empty() {
                            continue;
                        }
                        let (cell_language, cell_direction) = classify_text(cell);
                        locations.push(EvidenceLocation {
                            ordinal: next_ordinal(&locations)?,
                            kind: EvidenceLocationKind::Cell,
                            structural_path: format!(
                                "markdown://{}/{}/row/{row_index}/cell/{column_index}",
                                block.start, block.end
                            ),
                            provenance: vec![span.clone()],
                            section: section.clone(),
                            paragraph_number: None,
                            table_number: Some(table_number),
                            sheet_name: None,
                            cell_range: None,
                            original_text: cell.clone(),
                            translated_text: None,
                            language: cell_language,
                            direction: cell_direction,
                        });
                    }
                }
            }
        }
        if locations.len() > MAX_EVIDENCE_LOCATIONS {
            return Err(ParseExceptionCode::LossDetected);
        }
    }
    Ok(locations)
}

#[cfg(not(feature = "runtime-fixture"))]
fn markdown_blocks(markdown: &str) -> Vec<MarkdownBlock> {
    let lines = markdown.lines().collect::<Vec<_>>();
    let mut blocks = Vec::new();
    let mut index = 0_usize;
    let mut offset = 0_usize;
    while index < lines.len() {
        let line = lines[index];
        if line.trim().is_empty() {
            offset += line.chars().count() + 1;
            index += 1;
            continue;
        }
        if let Some(heading) = line.strip_prefix('#').filter(|rest| {
            rest.starts_with(' ')
                || (line.len() >= 2 && rest.chars().all(|character| character == '#'))
        }) {
            let text = heading.trim_start_matches('#').trim().to_owned();
            blocks.push(MarkdownBlock {
                kind: MarkdownBlockKind::Section,
                start: offset,
                end: offset + text.chars().count(),
                text,
            });
            offset += line.chars().count() + 1;
            index += 1;
            continue;
        }
        if line.starts_with("```") || line.starts_with("~~~") {
            let fence = &line[..3];
            let start = offset;
            let mut collected = Vec::new();
            index += 1;
            offset += line.chars().count() + 1;
            let mut closed = false;
            while index < lines.len() {
                let inner = lines[index];
                if inner.trim_start().starts_with(fence) {
                    closed = true;
                    offset += inner.chars().count() + 1;
                    index += 1;
                    break;
                }
                collected.push(inner);
                offset += inner.chars().count() + 1;
                index += 1;
            }
            if closed && !collected.iter().all(|inner| inner.trim().is_empty()) {
                let text = collected.join("\n");
                blocks.push(MarkdownBlock {
                    kind: MarkdownBlockKind::Paragraph,
                    start,
                    end: start + text.chars().count(),
                    text,
                });
            }
            continue;
        }
        if is_table_row(line) {
            let start = offset;
            let mut table_lines = Vec::new();
            while index < lines.len() && is_table_row(lines[index]) {
                table_lines.push(lines[index]);
                offset += lines[index].chars().count() + 1;
                index += 1;
            }
            let has_separator = table_lines.len() >= 2 && is_table_separator(table_lines[1]);
            let has_cells = table_lines
                .iter()
                .filter(|row| !is_table_separator(row))
                .any(|row| row.split('|').any(|cell| !cell.trim().is_empty()));
            if has_separator && has_cells {
                let text = table_lines.join("\n");
                blocks.push(MarkdownBlock {
                    kind: MarkdownBlockKind::Table,
                    start,
                    end: start + text.chars().count(),
                    text,
                });
                continue;
            }
            let text = table_lines.join(" ");
            blocks.push(MarkdownBlock {
                kind: MarkdownBlockKind::Paragraph,
                start,
                end: start + text.chars().count(),
                text,
            });
            continue;
        }
        let trimmed = line.trim_start();
        let is_break = trimmed == "---" || trimmed == "***" || trimmed == "___";
        if is_break {
            offset += line.chars().count() + 1;
            index += 1;
            continue;
        }
        if let Some(content) = trimmed
            .strip_prefix('>')
            .or_else(|| trimmed.strip_prefix("- "))
            .or_else(|| trimmed.strip_prefix("* "))
            .or_else(|| trimmed.strip_prefix("+ "))
            .or_else(|| numbered_prefix(trimmed).map(|(_, rest)| rest))
        {
            let start = offset;
            let text = content.trim().to_owned();
            blocks.push(MarkdownBlock {
                kind: MarkdownBlockKind::Paragraph,
                start,
                end: start + text.chars().count(),
                text,
            });
            offset += line.chars().count() + 1;
            index += 1;
            continue;
        }
        let start = offset;
        let mut paragraph = Vec::new();
        while index < lines.len() {
            let candidate = lines[index];
            if candidate.trim().is_empty() {
                break;
            }
            let candidate_trimmed = candidate.trim_start();
            if index > 0 && is_block_start(candidate_trimmed) {
                break;
            }
            paragraph.push(candidate);
            offset += candidate.chars().count() + 1;
            index += 1;
        }
        let text = paragraph.join("\n");
        blocks.push(MarkdownBlock {
            kind: MarkdownBlockKind::Paragraph,
            start,
            end: start + text.chars().count(),
            text,
        });
    }
    blocks
}

#[cfg(not(feature = "runtime-fixture"))]
fn numbered_prefix(line: &str) -> Option<(usize, &str)> {
    let digits = line
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .count();
    if digits == 0 {
        return None;
    }
    let rest = &line[digits..];
    let rest = rest.strip_prefix('.').or_else(|| rest.strip_prefix(')'))?;
    rest.strip_prefix(' ').map(|rest| (digits, rest))
}

#[cfg(not(feature = "runtime-fixture"))]
fn is_block_start(trimmed: &str) -> bool {
    let heading = trimmed
        .strip_prefix('#')
        .is_some_and(|rest| rest.starts_with(' '));
    heading
        || trimmed.starts_with("```")
        || trimmed.starts_with("~~~")
        || trimmed.starts_with('>')
        || trimmed.starts_with("- ")
        || trimmed.starts_with("* ")
        || trimmed.starts_with("+ ")
        || numbered_prefix(trimmed).is_some()
        || is_table_row(trimmed)
        || trimmed == "---"
        || trimmed == "***"
        || trimmed == "___"
}

#[cfg(not(feature = "runtime-fixture"))]
fn is_table_row(line: &str) -> bool {
    line.starts_with('|') && line.ends_with('|') && line.len() >= 2
}

#[cfg(not(feature = "runtime-fixture"))]
fn is_table_separator(line: &str) -> bool {
    let cells = line.split('|').filter(|cell| !cell.trim().is_empty());
    let mut saw_dash = false;
    for cell in cells {
        let trimmed = cell.trim();
        if !trimmed
            .chars()
            .all(|character| matches!(character, '-' | ':' | ' '))
        {
            return false;
        }
        if trimmed.contains('-') {
            saw_dash = true;
        }
    }
    saw_dash
}

#[cfg(not(feature = "runtime-fixture"))]
fn markdown_table_rows(block: &str) -> Vec<Vec<String>> {
    block
        .lines()
        .filter(|line| !is_table_separator(line))
        .map(|line| {
            let mut cells = line.split('|').collect::<Vec<_>>();
            if cells.first().is_some_and(|cell| cell.trim().is_empty()) {
                cells.remove(0);
            }
            if cells.last().is_some_and(|cell| cell.trim().is_empty()) {
                cells.pop();
            }
            cells
                .iter()
                .map(|cell| cell.trim().replace("\\|", "|").to_owned())
                .collect()
        })
        .collect()
}

fn next_ordinal(locations: &[EvidenceLocation]) -> Result<u32, ParseExceptionCode> {
    u32::try_from(locations.len() + 1).map_err(|_| ParseExceptionCode::OutputLimitExceeded)
}

#[cfg(not(feature = "runtime-fixture"))]
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
