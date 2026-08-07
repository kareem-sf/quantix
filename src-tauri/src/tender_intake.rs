use std::{
    collections::{HashSet, VecDeque},
    fs,
    io::{Cursor, Read, Seek, SeekFrom},
    path::{Component, Path, PathBuf},
    time::{Duration, Instant},
};

use garde::Validate;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ts_rs::TS;
use zip::{CompressionMethod, ZipArchive};

use crate::{ParseExceptionCode, ParseState, TenderCommandError, TenderErrorCode};

pub(crate) const MAX_INTAKE_FILE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_INTAKE_TOTAL_BYTES: u64 = 512 * 1024 * 1024;
const MAX_INTAKE_ENTRIES: usize = 10_000;
const MAX_INTAKE_NESTING: usize = 32;
const MAX_ARCHIVE_INDEX_BYTES: u64 = 16 * 1024 * 1024;
const MAX_EXPANSION_RATIO: u64 = 100;
const MIN_EXPANSION_CHECK_BYTES: u64 = 64 * 1024;
const MIN_FREE_SPACE_RESERVE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_INTAKE_DURATION: Duration = Duration::from_secs(15 * 60);
const MAX_PACKAGE_PATH_BYTES: usize = 2_048;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct ImportTenderPackageCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 1, max = 4096))]
    pub source_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct ChooseTenderPackageCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(skip)]
    pub source_kind: TenderPackageSourceKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct ConfirmSourceRelationshipCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub prior_artifact_id: String,
    #[garde(range(min = 1))]
    pub prior_version: u32,
    #[garde(length(bytes, min = 32, max = 32))]
    pub replacement_artifact_id: String,
    #[garde(range(min = 1))]
    pub replacement_version: u32,
    #[garde(skip)]
    pub relationship_kind: SourceRelationshipKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum TenderPackageSourceKind {
    Directory,
    ZipArchive,
}

impl TenderPackageSourceKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Directory => "directory",
            Self::ZipArchive => "zip_archive",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum RegistrationState {
    Registered,
    Exception,
}

impl RegistrationState {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Registered => "registered",
            Self::Exception => "exception",
        }
    }

    pub(crate) fn parse(value: &str) -> Result<Self, TenderCommandError> {
        match value {
            "registered" => Ok(Self::Registered),
            "exception" => Ok(Self::Exception),
            _ => Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum SupersessionState {
    Unconfirmed,
    Current,
    Superseded,
}

impl SupersessionState {
    pub(crate) fn parse(value: &str) -> Result<Self, TenderCommandError> {
        match value {
            "unconfirmed" => Ok(Self::Unconfirmed),
            "current" => Ok(Self::Current),
            "superseded" => Ok(Self::Superseded),
            _ => Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum SourceRelationshipKind {
    Addendum,
    Replacement,
}

impl SourceRelationshipKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Addendum => "addendum",
            Self::Replacement => "replacement",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum IntakeExceptionCode {
    Corrupt,
    DiskSpaceInsufficient,
    DuplicatePath,
    DurationExceeded,
    Encrypted,
    EntryCountExceeded,
    ExpansionRatioExceeded,
    FileSizeExceeded,
    MacroBearing,
    MemoryLimitExceeded,
    NestedArchive,
    NestingExceeded,
    PathTooLong,
    RegistrationFailed,
    TotalSizeExceeded,
    UnsafeLink,
    UnsafePath,
    Unsupported,
}

impl IntakeExceptionCode {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Corrupt => "corrupt",
            Self::DiskSpaceInsufficient => "disk_space_insufficient",
            Self::DuplicatePath => "duplicate_path",
            Self::DurationExceeded => "duration_exceeded",
            Self::Encrypted => "encrypted",
            Self::EntryCountExceeded => "entry_count_exceeded",
            Self::ExpansionRatioExceeded => "expansion_ratio_exceeded",
            Self::FileSizeExceeded => "file_size_exceeded",
            Self::MacroBearing => "macro_bearing",
            Self::MemoryLimitExceeded => "memory_limit_exceeded",
            Self::NestedArchive => "nested_archive",
            Self::NestingExceeded => "nesting_exceeded",
            Self::PathTooLong => "path_too_long",
            Self::RegistrationFailed => "registration_failed",
            Self::TotalSizeExceeded => "total_size_exceeded",
            Self::UnsafeLink => "unsafe_link",
            Self::UnsafePath => "unsafe_path",
            Self::Unsupported => "unsupported",
        }
    }

    pub(crate) fn parse(value: &str) -> Result<Self, TenderCommandError> {
        match value {
            "corrupt" => Ok(Self::Corrupt),
            "disk_space_insufficient" => Ok(Self::DiskSpaceInsufficient),
            "duplicate_path" => Ok(Self::DuplicatePath),
            "duration_exceeded" => Ok(Self::DurationExceeded),
            "encrypted" => Ok(Self::Encrypted),
            "entry_count_exceeded" => Ok(Self::EntryCountExceeded),
            "expansion_ratio_exceeded" => Ok(Self::ExpansionRatioExceeded),
            "file_size_exceeded" => Ok(Self::FileSizeExceeded),
            "macro_bearing" => Ok(Self::MacroBearing),
            "memory_limit_exceeded" => Ok(Self::MemoryLimitExceeded),
            "nested_archive" => Ok(Self::NestedArchive),
            "nesting_exceeded" => Ok(Self::NestingExceeded),
            "path_too_long" => Ok(Self::PathTooLong),
            "registration_failed" => Ok(Self::RegistrationFailed),
            "total_size_exceeded" => Ok(Self::TotalSizeExceeded),
            "unsafe_link" => Ok(Self::UnsafeLink),
            "unsafe_path" => Ok(Self::UnsafePath),
            "unsupported" => Ok(Self::Unsupported),
            _ => Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DocumentRegisterEntry {
    pub artifact_id: String,
    pub version: u32,
    pub package_path: String,
    pub language: String,
    pub document_type: String,
    pub media_type: Option<String>,
    pub sha256: Option<String>,
    pub size_bytes: u64,
    pub registration_state: RegistrationState,
    pub parse_state: ParseState,
    pub parse_exception: Option<ParseExceptionCode>,
    pub supersession_state: SupersessionState,
    pub exception: Option<IntakeExceptionCode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DocumentRegister {
    pub query_register_open: bool,
    pub documents: Vec<DocumentRegisterEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TenderPackageImportResult {
    pub intake_id: String,
    pub source_kind: TenderPackageSourceKind,
    pub discovered_count: u32,
    pub registered_count: u32,
    pub exception_count: u32,
    pub query_register_open: bool,
    pub documents: Vec<DocumentRegisterEntry>,
}

#[derive(Debug, Clone)]
pub(crate) struct PreparedIntake {
    pub source_kind: TenderPackageSourceKind,
    pub source_path: String,
    pub source_name: String,
    pub documents: Vec<PreparedDocument>,
}

#[derive(Debug, Clone)]
pub(crate) struct PreparedDocument {
    pub package_path: String,
    pub language: String,
    pub document_type: String,
    pub media_type: Option<String>,
    pub sha256: Option<String>,
    pub integrity: Option<String>,
    pub size_bytes: u64,
    pub exception: Option<IntakeExceptionCode>,
}

impl PreparedDocument {
    fn exception(package_path: String, size_bytes: u64, code: IntakeExceptionCode) -> Self {
        Self {
            package_path,
            language: "undetermined".into(),
            document_type: "unclassified".into(),
            media_type: None,
            sha256: None,
            integrity: None,
            size_bytes,
            exception: Some(code),
        }
    }
}

pub(crate) fn prepare_package(
    source: &Path,
    content_root: &Path,
) -> Result<PreparedIntake, TenderCommandError> {
    let started = Instant::now();
    let metadata = fs::symlink_metadata(source)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
    let source_name = source
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("Tender Package")
        .to_owned();
    let source_path = source.to_string_lossy().into_owned();

    if metadata_is_unsafe_link(&metadata) {
        let source_kind = if is_zip_path(source) {
            TenderPackageSourceKind::ZipArchive
        } else {
            TenderPackageSourceKind::Directory
        };
        return Ok(PreparedIntake {
            source_kind,
            source_path,
            source_name: source_name.clone(),
            documents: vec![PreparedDocument::exception(
                source_name,
                0,
                IntakeExceptionCode::UnsafeLink,
            )],
        });
    }

    let mut documents = if metadata.is_dir() {
        prepare_directory(source, content_root)?
    } else if metadata.is_file() && is_zip_path(source) {
        prepare_zip(source, content_root)?
    } else {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    };
    if started.elapsed() > MAX_INTAKE_DURATION {
        documents = vec![PreparedDocument::exception(
            "[package]".into(),
            0,
            IntakeExceptionCode::DurationExceeded,
        )];
    }
    Ok(PreparedIntake {
        source_kind: if metadata.is_dir() {
            TenderPackageSourceKind::Directory
        } else {
            TenderPackageSourceKind::ZipArchive
        },
        source_path,
        source_name,
        documents,
    })
}

fn prepare_directory(
    source: &Path,
    content_root: &Path,
) -> Result<Vec<PreparedDocument>, TenderCommandError> {
    let started = Instant::now();
    let mut queue = VecDeque::from([(source.to_path_buf(), PathBuf::new(), 0_usize)]);
    let mut documents = Vec::new();
    let mut total_bytes = 0_u64;
    let mut discovered_entries = 0_usize;

    while let Some((directory, relative_directory, depth)) = queue.pop_front() {
        if started.elapsed() > MAX_INTAKE_DURATION {
            documents.push(PreparedDocument::exception(
                package_marker(&relative_directory),
                0,
                IntakeExceptionCode::DurationExceeded,
            ));
            break;
        }
        let directory_entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(_) => {
                documents.push(PreparedDocument::exception(
                    package_marker(&relative_directory),
                    0,
                    IntakeExceptionCode::Corrupt,
                ));
                continue;
            }
        };
        let mut entries = Vec::new();
        for entry in directory_entries {
            match entry {
                Ok(entry) => entries.push(entry),
                Err(_) => documents.push(PreparedDocument::exception(
                    format!("{}/[unreadable-entry]", package_marker(&relative_directory)),
                    0,
                    IntakeExceptionCode::Corrupt,
                )),
            }
        }
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            discovered_entries = discovered_entries.saturating_add(1);
            if discovered_entries > MAX_INTAKE_ENTRIES {
                documents.push(PreparedDocument::exception(
                    "[package]".into(),
                    0,
                    IntakeExceptionCode::EntryCountExceeded,
                ));
                return Ok(documents);
            }
            let relative = relative_directory.join(entry.file_name());
            let package_path = portable_path(&relative);
            if package_path.len() > MAX_PACKAGE_PATH_BYTES {
                documents.push(PreparedDocument::exception(
                    package_path,
                    0,
                    IntakeExceptionCode::PathTooLong,
                ));
                continue;
            }
            let metadata = match fs::symlink_metadata(entry.path()) {
                Ok(metadata) => metadata,
                Err(_) => {
                    documents.push(PreparedDocument::exception(
                        package_path,
                        0,
                        IntakeExceptionCode::Corrupt,
                    ));
                    continue;
                }
            };
            if metadata_is_unsafe_link(&metadata) {
                documents.push(PreparedDocument::exception(
                    package_path,
                    metadata.len(),
                    IntakeExceptionCode::UnsafeLink,
                ));
            } else if metadata.is_dir() {
                if depth + 1 > MAX_INTAKE_NESTING {
                    documents.push(PreparedDocument::exception(
                        package_path,
                        0,
                        IntakeExceptionCode::NestingExceeded,
                    ));
                } else {
                    queue.push_back((entry.path(), relative, depth + 1));
                }
            } else if metadata.is_file() {
                let size = metadata.len();
                total_bytes = total_bytes.saturating_add(size);
                if total_bytes > MAX_INTAKE_TOTAL_BYTES {
                    documents.push(PreparedDocument::exception(
                        package_path,
                        size,
                        IntakeExceptionCode::TotalSizeExceeded,
                    ));
                } else if size > MAX_INTAKE_FILE_BYTES {
                    documents.push(PreparedDocument::exception(
                        package_path,
                        size,
                        IntakeExceptionCode::FileSizeExceeded,
                    ));
                } else {
                    let bytes = match read_bounded_file(&entry.path(), size) {
                        Ok(Some(bytes)) => bytes,
                        Ok(None) => {
                            documents.push(PreparedDocument::exception(
                                package_path,
                                size,
                                IntakeExceptionCode::FileSizeExceeded,
                            ));
                            continue;
                        }
                        Err(_) => {
                            documents.push(PreparedDocument::exception(
                                package_path,
                                size,
                                IntakeExceptionCode::Corrupt,
                            ));
                            continue;
                        }
                    };
                    documents.push(prepare_bytes(package_path, bytes, content_root));
                }
            } else {
                documents.push(PreparedDocument::exception(
                    package_path,
                    metadata.len(),
                    IntakeExceptionCode::UnsafeLink,
                ));
            }
        }
    }
    Ok(documents)
}

fn prepare_zip(
    source: &Path,
    content_root: &Path,
) -> Result<Vec<PreparedDocument>, TenderCommandError> {
    if fs::metadata(source).map_or(true, |metadata| metadata.len() > MAX_INTAKE_TOTAL_BYTES) {
        return Ok(vec![PreparedDocument::exception(
            source
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("[package]")
                .to_owned(),
            fs::metadata(source).map(|value| value.len()).unwrap_or(0),
            IntakeExceptionCode::TotalSizeExceeded,
        )]);
    }
    let Some(preflight) = zip_preflight(source) else {
        return Ok(vec![PreparedDocument::exception(
            source
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("[package]")
                .to_owned(),
            fs::metadata(source).map(|value| value.len()).unwrap_or(0),
            IntakeExceptionCode::Corrupt,
        )]);
    };
    if preflight.zip64 {
        return Ok(vec![PreparedDocument::exception(
            "[package]".into(),
            0,
            IntakeExceptionCode::Unsupported,
        )]);
    }
    if preflight.central_directory_size > MAX_ARCHIVE_INDEX_BYTES {
        return Ok(vec![PreparedDocument::exception(
            "[package]".into(),
            preflight.central_directory_size,
            IntakeExceptionCode::MemoryLimitExceeded,
        )]);
    }
    if preflight.entry_count > MAX_INTAKE_ENTRIES {
        return Ok(vec![PreparedDocument::exception(
            "[package]".into(),
            0,
            IntakeExceptionCode::EntryCountExceeded,
        )]);
    }
    let file = fs::File::open(source)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
    let mut archive = match ZipArchive::new(file) {
        Ok(archive) => archive,
        Err(_) => {
            return Ok(vec![PreparedDocument::exception(
                source
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("[package]")
                    .to_owned(),
                fs::metadata(source).map(|value| value.len()).unwrap_or(0),
                IntakeExceptionCode::Corrupt,
            )]);
        }
    };
    if archive.offset() != 0
        || archive.central_directory_start() != preflight.central_directory_offset
        || archive.comment() != preflight.comment.as_slice()
    {
        return Ok(vec![PreparedDocument::exception(
            "[package]".into(),
            0,
            IntakeExceptionCode::Corrupt,
        )]);
    }
    if archive.len() > preflight.entry_count {
        return Ok(vec![PreparedDocument::exception(
            "[package]".into(),
            0,
            IntakeExceptionCode::Corrupt,
        )]);
    }

    let started = Instant::now();
    let mut documents = (0..preflight.entry_count.saturating_sub(archive.len()))
        .map(|index| {
            PreparedDocument::exception(
                format!("[duplicate-path-{}]", index + 1),
                0,
                IntakeExceptionCode::DuplicatePath,
            )
        })
        .collect::<Vec<_>>();
    let mut paths = HashSet::new();
    let mut total_bytes = 0_u64;
    for index in 0..archive.len() {
        if started.elapsed() > MAX_INTAKE_DURATION {
            documents.push(PreparedDocument::exception(
                "[package]".into(),
                0,
                IntakeExceptionCode::DurationExceeded,
            ));
            break;
        }
        let mut entry = match archive.by_index(index) {
            Ok(entry) => entry,
            Err(_) => {
                documents.push(PreparedDocument::exception(
                    format!("[entry-{index}]"),
                    0,
                    IntakeExceptionCode::Corrupt,
                ));
                continue;
            }
        };
        if entry.is_dir() {
            continue;
        }
        let raw_name = entry.name().to_owned();
        let package_path = normalize_zip_path(&raw_name);
        let size = entry.size();
        let compressed_size = entry.compressed_size();
        let exception = if !zip_path_is_safe(&raw_name) || entry.enclosed_name().is_none() {
            Some(IntakeExceptionCode::UnsafePath)
        } else if package_path.len() > MAX_PACKAGE_PATH_BYTES {
            Some(IntakeExceptionCode::PathTooLong)
        } else if entry.is_symlink() {
            Some(IntakeExceptionCode::UnsafeLink)
        } else if entry.encrypted() {
            Some(IntakeExceptionCode::Encrypted)
        } else if !matches!(
            entry.compression(),
            CompressionMethod::Stored | CompressionMethod::Deflated
        ) {
            Some(IntakeExceptionCode::Unsupported)
        } else if path_depth(&package_path) > MAX_INTAKE_NESTING {
            Some(IntakeExceptionCode::NestingExceeded)
        } else if !paths.insert(package_path.to_ascii_lowercase()) {
            Some(IntakeExceptionCode::DuplicatePath)
        } else if size > MAX_INTAKE_FILE_BYTES {
            Some(IntakeExceptionCode::FileSizeExceeded)
        } else if size >= MIN_EXPANSION_CHECK_BYTES
            && size / compressed_size.max(1) > MAX_EXPANSION_RATIO
        {
            Some(IntakeExceptionCode::ExpansionRatioExceeded)
        } else if is_nested_archive(Path::new(&package_path)) {
            Some(IntakeExceptionCode::NestedArchive)
        } else if is_macro_path(Path::new(&package_path)) {
            Some(IntakeExceptionCode::MacroBearing)
        } else {
            None
        };
        total_bytes = total_bytes.saturating_add(size);
        let exception = exception.or_else(|| {
            (total_bytes > MAX_INTAKE_TOTAL_BYTES).then_some(IntakeExceptionCode::TotalSizeExceeded)
        });
        if let Some(exception) = exception {
            documents.push(PreparedDocument::exception(package_path, size, exception));
            continue;
        }

        let mut bytes = Vec::with_capacity(usize::try_from(size).unwrap_or(0));
        if entry
            .by_ref()
            .take(MAX_INTAKE_FILE_BYTES + 1)
            .read_to_end(&mut bytes)
            .is_err()
            || bytes.len() as u64 != size
        {
            documents.push(PreparedDocument::exception(
                package_path,
                size,
                IntakeExceptionCode::Corrupt,
            ));
            continue;
        }
        documents.push(prepare_bytes(package_path, bytes, content_root));
    }
    Ok(documents)
}

fn prepare_bytes(package_path: String, bytes: Vec<u8>, content_root: &Path) -> PreparedDocument {
    let path = Path::new(&package_path);
    if is_nested_archive(path) {
        return PreparedDocument::exception(
            package_path,
            bytes.len() as u64,
            IntakeExceptionCode::NestedArchive,
        );
    }
    if is_macro_path(path)
        || (matches!(extension(path).as_deref(), Some("docx" | "xlsx"))
            && office_package_has_macros(&bytes))
    {
        return PreparedDocument::exception(
            package_path,
            bytes.len() as u64,
            IntakeExceptionCode::MacroBearing,
        );
    }
    let Some((document_type, media_type)) = classify_supported(path, &bytes) else {
        let exception = if is_supported_extension(path) {
            IntakeExceptionCode::Corrupt
        } else {
            IntakeExceptionCode::Unsupported
        };
        return PreparedDocument::exception(package_path, bytes.len() as u64, exception);
    };

    let required_space = (bytes.len() as u64).saturating_add(MIN_FREE_SPACE_RESERVE_BYTES);
    if fs4::available_space(content_root).map_or(true, |available| available < required_space) {
        return PreparedDocument::exception(
            package_path,
            bytes.len() as u64,
            IntakeExceptionCode::DiskSpaceInsufficient,
        );
    }
    let integrity = match cacache::write_hash_sync(content_root, &bytes) {
        Ok(integrity) => integrity,
        Err(_) => {
            return PreparedDocument::exception(
                package_path,
                bytes.len() as u64,
                IntakeExceptionCode::RegistrationFailed,
            );
        }
    };
    let verified = match cacache::read_hash_sync(content_root, &integrity) {
        Ok(verified) => verified,
        Err(_) => {
            return PreparedDocument::exception(
                package_path,
                bytes.len() as u64,
                IntakeExceptionCode::RegistrationFailed,
            );
        }
    };
    if verified != bytes {
        return PreparedDocument::exception(
            package_path,
            bytes.len() as u64,
            IntakeExceptionCode::RegistrationFailed,
        );
    }

    PreparedDocument {
        package_path,
        language: "undetermined".into(),
        document_type: document_type.into(),
        media_type: Some(media_type.into()),
        sha256: Some(
            Sha256::digest(&bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
        ),
        integrity: Some(integrity.to_string()),
        size_bytes: bytes.len() as u64,
        exception: None,
    }
}

fn classify_supported(path: &Path, bytes: &[u8]) -> Option<(&'static str, &'static str)> {
    match extension(path).as_deref() {
        Some("pdf")
            if infer::get(bytes).is_some_and(|kind| kind.mime_type() == "application/pdf") =>
        {
            Some(("pdf_document", "application/pdf"))
        }
        Some("docx") if valid_office_package(bytes, "word/document.xml") => Some((
            "word_document",
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        )),
        Some("xlsx") if valid_office_package(bytes, "xl/workbook.xml") => Some((
            "spreadsheet",
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        )),
        _ => None,
    }
}

fn read_bounded_file(path: &Path, expected_size: u64) -> std::io::Result<Option<Vec<u8>>> {
    let file = fs::File::open(path)?;
    let mut bytes = Vec::with_capacity(usize::try_from(expected_size).unwrap_or(0));
    file.take(MAX_INTAKE_FILE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_INTAKE_FILE_BYTES {
        return Ok(None);
    }
    if bytes.len() as u64 != expected_size {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "source changed during intake",
        ));
    }
    Ok(Some(bytes))
}

fn valid_office_package(bytes: &[u8], required_entry: &str) -> bool {
    let Ok(mut archive) = ZipArchive::new(Cursor::new(bytes)) else {
        return false;
    };
    archive.by_name("[Content_Types].xml").is_ok() && archive.by_name(required_entry).is_ok()
}

fn office_package_has_macros(bytes: &[u8]) -> bool {
    let Ok(mut archive) = ZipArchive::new(Cursor::new(bytes)) else {
        return false;
    };
    (0..archive.len()).any(|index| {
        archive.by_index(index).is_ok_and(|entry| {
            normalize_zip_path(entry.name())
                .to_ascii_lowercase()
                .ends_with("vbaproject.bin")
        })
    })
}

fn is_supported_extension(path: &Path) -> bool {
    matches!(extension(path).as_deref(), Some("pdf" | "docx" | "xlsx"))
}

fn is_macro_path(path: &Path) -> bool {
    matches!(extension(path).as_deref(), Some("docm" | "xlsm" | "pptm"))
}

fn is_nested_archive(path: &Path) -> bool {
    matches!(
        extension(path).as_deref(),
        Some("zip" | "7z" | "rar" | "tar" | "gz" | "bz2" | "xz")
    )
}

fn is_zip_path(path: &Path) -> bool {
    extension(path).as_deref() == Some("zip")
}

fn extension(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
}

fn normalize_zip_path(value: &str) -> String {
    value.replace('\\', "/").trim_end_matches('/').to_owned()
}

fn zip_path_is_safe(value: &str) -> bool {
    if value.contains('\0') || value.starts_with(['/', '\\']) {
        return false;
    }
    let normalized = normalize_zip_path(value);
    let mut components = normalized.split('/');
    let first = components.next().unwrap_or_default();
    !first.contains(':')
        && !first.is_empty()
        && std::iter::once(first)
            .chain(components)
            .all(|component| !component.is_empty() && component != "." && component != "..")
}

fn path_depth(value: &str) -> usize {
    value
        .split('/')
        .filter(|component| !component.is_empty())
        .count()
}

fn portable_path(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value.to_string_lossy()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn package_marker(path: &Path) -> String {
    let path = portable_path(path);
    if path.is_empty() {
        "[package]".into()
    } else {
        path
    }
}

fn metadata_is_unsafe_link(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return true;
        }
    }
    false
}

struct ZipPreflight {
    entry_count: usize,
    central_directory_offset: u64,
    central_directory_size: u64,
    comment: Vec<u8>,
    zip64: bool,
}

fn zip_preflight(source: &Path) -> Option<ZipPreflight> {
    const EOCD_SIGNATURE: &[u8; 4] = b"PK\x05\x06";
    const MAX_EOCD_SEARCH_BYTES: u64 = u16::MAX as u64 + 22;

    let mut file = fs::File::open(source).ok()?;
    let length = file.seek(SeekFrom::End(0)).ok()?;
    let tail_length = length.min(MAX_EOCD_SEARCH_BYTES);
    file.seek(SeekFrom::End(-(tail_length as i64))).ok()?;
    let mut tail = vec![0; tail_length as usize];
    file.read_exact(&mut tail).ok()?;
    tail.windows(EOCD_SIGNATURE.len())
        .enumerate()
        .rev()
        .find_map(|(offset, window)| {
            if window != EOCD_SIGNATURE || offset + 22 > tail.len() {
                return None;
            }
            let comment_length =
                u16::from_le_bytes(tail.get(offset + 20..offset + 22)?.try_into().ok()?) as usize;
            if offset + 22 + comment_length != tail.len() {
                return None;
            }
            let disk_number =
                u16::from_le_bytes(tail.get(offset + 4..offset + 6)?.try_into().ok()?);
            let central_disk =
                u16::from_le_bytes(tail.get(offset + 6..offset + 8)?.try_into().ok()?);
            let entries_on_disk =
                u16::from_le_bytes(tail.get(offset + 8..offset + 10)?.try_into().ok()?);
            let total_entries =
                u16::from_le_bytes(tail.get(offset + 10..offset + 12)?.try_into().ok()?);
            let central_directory_size =
                u32::from_le_bytes(tail.get(offset + 12..offset + 16)?.try_into().ok()?);
            let central_directory_offset =
                u32::from_le_bytes(tail.get(offset + 16..offset + 20)?.try_into().ok()?);
            let zip64 = total_entries == u16::MAX
                || central_directory_size == u32::MAX
                || central_directory_offset == u32::MAX;
            let absolute_offset = length
                .checked_sub(tail_length)?
                .checked_add(offset as u64)?;
            let directory_ends_at_record = u64::from(central_directory_offset)
                .checked_add(u64::from(central_directory_size))
                == Some(absolute_offset);
            (disk_number == 0
                && central_disk == 0
                && entries_on_disk == total_entries
                && (zip64 || directory_ends_at_record))
                .then_some(ZipPreflight {
                    entry_count: usize::from(total_entries),
                    central_directory_offset: u64::from(central_directory_offset),
                    central_directory_size: u64::from(central_directory_size),
                    comment: tail.get(offset + 22..)?.to_vec(),
                    zip64,
                })
        })
}
