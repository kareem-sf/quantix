use std::{
    collections::HashMap,
    fs,
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

use garde::Validate;
use rusqlite::{
    limits::Limit, params, Connection, OpenFlags, OptionalExtension, Transaction,
    TransactionBehavior,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use ts_rs::TS;

use crate::{setup::SetupState, QuantixHost};

#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};

#[cfg(test)]
static TENDER_STORE_OPEN_COUNT: AtomicUsize = AtomicUsize::new(0);

const TENDER_SCHEMA_VERSION: i64 = 1;
const MAX_TENDER_NAME_BYTES: usize = 200;
const MAX_CONTENT_BYTES: usize = 16 * 1024 * 1024;
const ZERO_AUDIT_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

const TENDER_SCHEMA: &str = r#"
CREATE TABLE tender (
  singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
  tender_id TEXT NOT NULL UNIQUE,
  current_revision INTEGER NOT NULL CHECK (current_revision > 0),
  created_at TEXT NOT NULL
);
CREATE TABLE tender_revisions (
  revision INTEGER PRIMARY KEY CHECK (revision > 0),
  tender_id TEXT NOT NULL,
  name TEXT NOT NULL CHECK (length(CAST(name AS BLOB)) BETWEEN 1 AND 200),
  created_at TEXT NOT NULL,
  FOREIGN KEY (tender_id) REFERENCES tender(tender_id)
);
CREATE TABLE content_objects (
  sha256 TEXT PRIMARY KEY CHECK (length(sha256) = 64),
  integrity TEXT NOT NULL UNIQUE,
  size_bytes INTEGER NOT NULL CHECK (size_bytes > 0)
);
CREATE TABLE content_versions (
  logical_id TEXT NOT NULL CHECK (length(CAST(logical_id AS BLOB)) BETWEEN 1 AND 100),
  revision INTEGER NOT NULL CHECK (revision > 0),
  sha256 TEXT NOT NULL,
  media_type TEXT NOT NULL CHECK (length(CAST(media_type AS BLOB)) BETWEEN 1 AND 100),
  created_at TEXT NOT NULL,
  PRIMARY KEY (logical_id, revision),
  FOREIGN KEY (sha256) REFERENCES content_objects(sha256)
);
CREATE TABLE content_heads (
  logical_id TEXT PRIMARY KEY,
  current_revision INTEGER NOT NULL CHECK (current_revision > 0),
  FOREIGN KEY (logical_id, current_revision)
    REFERENCES content_versions(logical_id, revision)
);
CREATE TABLE audit_events (
  sequence INTEGER PRIMARY KEY CHECK (sequence > 0),
  event_type TEXT NOT NULL,
  aggregate_revision INTEGER NOT NULL CHECK (aggregate_revision > 0),
  payload_json TEXT NOT NULL,
  preceding_hash TEXT NOT NULL CHECK (length(preceding_hash) = 64),
  current_hash TEXT NOT NULL UNIQUE CHECK (length(current_hash) = 64),
  created_at TEXT NOT NULL
);
CREATE TRIGGER tender_identity_no_update
BEFORE UPDATE OF singleton, tender_id, created_at ON tender
BEGIN
  SELECT RAISE(ABORT, 'Tender identity is immutable');
END;
CREATE TRIGGER tender_no_delete
BEFORE DELETE ON tender
BEGIN
  SELECT RAISE(ABORT, 'Tender identity is immutable');
END;
CREATE TRIGGER tender_revisions_no_update
BEFORE UPDATE ON tender_revisions
BEGIN
  SELECT RAISE(ABORT, 'Tender revisions are immutable');
END;
CREATE TRIGGER tender_revisions_no_delete
BEFORE DELETE ON tender_revisions
BEGIN
  SELECT RAISE(ABORT, 'Tender revisions are immutable');
END;
CREATE TRIGGER content_objects_no_update
BEFORE UPDATE ON content_objects
BEGIN
  SELECT RAISE(ABORT, 'Content Objects are immutable');
END;
CREATE TRIGGER content_objects_no_delete
BEFORE DELETE ON content_objects
BEGIN
  SELECT RAISE(ABORT, 'Content Objects are immutable');
END;
CREATE TRIGGER content_versions_no_update
BEFORE UPDATE ON content_versions
BEGIN
  SELECT RAISE(ABORT, 'Content versions are immutable');
END;
CREATE TRIGGER content_versions_no_delete
BEFORE DELETE ON content_versions
BEGIN
  SELECT RAISE(ABORT, 'Content versions are immutable');
END;
CREATE TRIGGER audit_events_no_update
BEFORE UPDATE ON audit_events
BEGIN
  SELECT RAISE(ABORT, 'Audit Events are immutable');
END;
CREATE TRIGGER audit_events_no_delete
BEFORE DELETE ON audit_events
BEGIN
  SELECT RAISE(ABORT, 'Audit Events are immutable');
END;
"#;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct CreateTenderCommand {
    #[garde(length(bytes, min = 1, max = 200))]
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct ReviseTenderCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 1, max = 200))]
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct OpenTenderCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct RegisterTenderContentCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 1, max = 100))]
    pub logical_id: String,
    #[garde(length(bytes, min = 1, max = 100))]
    pub media_type: String,
    #[garde(length(min = 1, max = 16777216))]
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct TenderSummary {
    pub tender_id: String,
    pub name: String,
    pub revision: u32,
    pub audit_event_count: u64,
    pub audit_chain_head: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct ContentVersionSummary {
    pub logical_id: String,
    pub revision: u32,
    pub sha256: String,
    pub size_bytes: u64,
    pub media_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct TenderInspection {
    pub summary: TenderSummary,
    pub content_object_count: u64,
    pub content_version_count: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum TenderErrorCode {
    IntegrityFailed,
    InvalidCommand,
    NotFound,
    SetupRequired,
    StoreUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct TenderCommandError {
    pub code: TenderErrorCode,
}

impl TenderCommandError {
    pub(crate) fn new(code: TenderErrorCode) -> Self {
        Self { code }
    }
}

impl std::fmt::Display for TenderCommandError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Tender command failed: {:?}", self.code)
    }
}

impl std::error::Error for TenderCommandError {}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct TenderId(String);

impl TenderId {
    pub(crate) fn parse(value: &str) -> Result<Self, TenderCommandError> {
        if value.len() == 32
            && value
                .chars()
                .all(|character| character.is_ascii_digit() || matches!(character, 'a'..='f'))
        {
            Ok(Self(value.to_owned()))
        } else {
            Err(TenderCommandError::new(TenderErrorCode::InvalidCommand))
        }
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

pub(crate) struct TenderStore {
    root: std::path::PathBuf,
    connection: Connection,
}

impl TenderStore {
    fn create(root: &Path, tender_id: &TenderId, name: &str) -> Result<Self, TenderCommandError> {
        fs::create_dir(root).map_err(store_unavailable)?;
        for directory in ["content", "runs", "staging"] {
            fs::create_dir(root.join(directory)).map_err(store_unavailable)?;
        }

        let mut connection = Connection::open(root.join("tender.sqlite")).map_err(sql_error)?;
        configure_writer(&connection)?;
        connection.execute_batch(TENDER_SCHEMA).map_err(sql_error)?;
        connection
            .pragma_update(None, "user_version", TENDER_SCHEMA_VERSION)
            .map_err(sql_error)?;

        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let created_at = sqlite_timestamp(&transaction)?;
        transaction
            .execute(
                "INSERT INTO tender (singleton, tender_id, current_revision, created_at) VALUES (1, ?1, 1, ?2)",
                params![tender_id.as_str(), created_at],
            )
            .map_err(sql_error)?;
        transaction
            .execute(
                "INSERT INTO tender_revisions (revision, tender_id, name, created_at) VALUES (1, ?1, ?2, ?3)",
                params![tender_id.as_str(), name, created_at],
            )
            .map_err(sql_error)?;
        append_audit_event(
            &transaction,
            tender_id.as_str(),
            "tender_created",
            1,
            json!({ "name": name }),
            &created_at,
        )?;
        transaction.commit().map_err(sql_error)?;

        Ok(Self {
            root: root.to_path_buf(),
            connection,
        })
    }

    fn open(root: &Path, expected_tender_id: &TenderId) -> Result<Self, TenderCommandError> {
        #[cfg(test)]
        TENDER_STORE_OPEN_COUNT.fetch_add(1, Ordering::SeqCst);
        validate_tender_store_layout(root)?;
        let database = root.join("tender.sqlite");
        let connection = Connection::open_with_flags(
            database,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|_| TenderCommandError::new(TenderErrorCode::NotFound))?;
        configure_writer(&connection)?;
        validate_store(&connection, expected_tender_id)?;
        Ok(Self {
            root: root.to_path_buf(),
            connection,
        })
    }

    fn summary(&self) -> Result<TenderSummary, TenderCommandError> {
        let (tender_id, revision, name): (String, u32, String) = self
            .connection
            .query_row(
                "SELECT tender.tender_id, tender.current_revision, tender_revisions.name
                 FROM tender
                 JOIN tender_revisions ON tender_revisions.revision = tender.current_revision
                 WHERE tender.singleton = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .map_err(sql_error)?;
        let (audit_event_count, audit_chain_head): (i64, Option<String>) = self
            .connection
            .query_row(
                "SELECT COUNT(*), MAX(current_hash) FILTER (
                   WHERE sequence = (SELECT MAX(sequence) FROM audit_events)
                 ) FROM audit_events",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(sql_error)?;

        Ok(TenderSummary {
            tender_id,
            name,
            revision,
            audit_event_count: audit_event_count
                .try_into()
                .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
            audit_chain_head: audit_chain_head.unwrap_or_else(|| ZERO_AUDIT_HASH.into()),
        })
    }

    fn revise(
        &mut self,
        command: &ReviseTenderCommand,
    ) -> Result<TenderSummary, TenderCommandError> {
        let name = command.name.trim();
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let (stored_tender_id, current_revision): (String, u32) = transaction
            .query_row(
                "SELECT tender_id, current_revision FROM tender WHERE singleton = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(sql_error)?;
        if stored_tender_id != command.tender_id {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let next_revision = current_revision
            .checked_add(1)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        let created_at = sqlite_timestamp(&transaction)?;
        transaction
            .execute(
                "INSERT INTO tender_revisions (revision, tender_id, name, created_at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![next_revision, stored_tender_id, name, created_at],
            )
            .map_err(sql_error)?;
        let advanced = transaction
            .execute(
                "UPDATE tender SET current_revision = ?1
                 WHERE singleton = 1 AND current_revision = ?2",
                params![next_revision, current_revision],
            )
            .map_err(sql_error)?;
        if advanced != 1 {
            return Err(TenderCommandError::new(TenderErrorCode::StoreUnavailable));
        }
        append_audit_event(
            &transaction,
            &stored_tender_id,
            "tender_revised",
            next_revision,
            json!({ "name": name }),
            &created_at,
        )?;
        transaction.commit().map_err(sql_error)?;
        self.summary()
    }

    fn register_content(
        &mut self,
        command: &RegisterTenderContentCommand,
    ) -> Result<ContentVersionSummary, TenderCommandError> {
        let content_root = self.root.join("content");
        let integrity = match cacache::write_hash_sync(&content_root, &command.bytes) {
            Ok(integrity) => integrity,
            Err(error) => {
                return self.reject_content_publication(
                    command,
                    "content_store_unavailable",
                    content_store_error(error),
                );
            }
        };
        let verified = match cacache::read_hash_sync(&content_root, &integrity) {
            Ok(verified) => verified,
            Err(error) => {
                return self.reject_content_publication(
                    command,
                    "content_verification_unavailable",
                    content_store_error(error),
                );
            }
        };
        if verified != command.bytes {
            return self.reject_content_publication(
                command,
                "content_digest_mismatch",
                TenderCommandError::new(TenderErrorCode::IntegrityFailed),
            );
        }
        let sha256 = sha256_hex(&command.bytes);
        let size_bytes = i64::try_from(command.bytes.len())
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;

        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let tender_id: String = transaction
            .query_row(
                "SELECT tender_id FROM tender WHERE singleton = 1",
                [],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if tender_id != command.tender_id {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let current_revision: Option<u32> = transaction
            .query_row(
                "SELECT current_revision FROM content_heads WHERE logical_id = ?1",
                [&command.logical_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_error)?;
        let revision = current_revision
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        let created_at = sqlite_timestamp(&transaction)?;
        transaction
            .execute(
                "INSERT INTO content_objects (sha256, integrity, size_bytes)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(sha256) DO NOTHING",
                params![sha256, integrity.to_string(), size_bytes],
            )
            .map_err(sql_error)?;
        let stored_object: (String, i64) = transaction
            .query_row(
                "SELECT integrity, size_bytes FROM content_objects WHERE sha256 = ?1",
                [&sha256],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(sql_error)?;
        if stored_object != (integrity.to_string(), size_bytes) {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        transaction
            .execute(
                "INSERT INTO content_versions (
                   logical_id, revision, sha256, media_type, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    command.logical_id,
                    revision,
                    sha256,
                    command.media_type,
                    created_at
                ],
            )
            .map_err(sql_error)?;
        transaction
            .execute(
                "INSERT INTO content_heads (logical_id, current_revision) VALUES (?1, ?2)
                 ON CONFLICT(logical_id) DO UPDATE SET current_revision = excluded.current_revision",
                params![command.logical_id, revision],
            )
            .map_err(sql_error)?;
        append_audit_event(
            &transaction,
            &tender_id,
            "content_version_registered",
            revision,
            json!({
                "logical_id": command.logical_id,
                "media_type": command.media_type,
                "sha256": sha256,
                "size_bytes": size_bytes.to_string(),
            }),
            &created_at,
        )?;
        transaction.commit().map_err(sql_error)?;

        Ok(ContentVersionSummary {
            logical_id: command.logical_id.clone(),
            revision,
            sha256,
            size_bytes: size_bytes
                .try_into()
                .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
            media_type: command.media_type.clone(),
        })
    }

    fn reject_content_publication<T>(
        &mut self,
        command: &RegisterTenderContentCommand,
        reason: &str,
        error: TenderCommandError,
    ) -> Result<T, TenderCommandError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let (tender_id, tender_revision): (String, u32) = transaction
            .query_row(
                "SELECT tender_id, current_revision FROM tender WHERE singleton = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(sql_error)?;
        if tender_id != command.tender_id {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let created_at = sqlite_timestamp(&transaction)?;
        append_audit_event(
            &transaction,
            &tender_id,
            "content_publication_failed",
            tender_revision,
            json!({
                "logical_id": command.logical_id,
                "reason": reason,
            }),
            &created_at,
        )?;
        transaction.commit().map_err(sql_error)?;
        Err(error)
    }

    fn record_command_denied(
        &mut self,
        expected_tender_id: &TenderId,
        command_name: &str,
    ) -> Result<(), TenderCommandError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let (tender_id, tender_revision): (String, u32) = transaction
            .query_row(
                "SELECT tender_id, current_revision FROM tender WHERE singleton = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(sql_error)?;
        if tender_id != expected_tender_id.as_str() {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let created_at = sqlite_timestamp(&transaction)?;
        append_audit_event(
            &transaction,
            &tender_id,
            "tender_command_denied",
            tender_revision,
            json!({
                "command": command_name,
                "reason": "invalid_command",
            }),
            &created_at,
        )?;
        transaction.commit().map_err(sql_error)
    }

    fn inspection(&self) -> Result<TenderInspection, TenderCommandError> {
        let (content_object_count, content_version_count): (i64, i64) = self
            .connection
            .query_row(
                "SELECT
                   (SELECT COUNT(*) FROM content_objects),
                   (SELECT COUNT(*) FROM content_versions)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(sql_error)?;
        Ok(TenderInspection {
            summary: self.summary()?,
            content_object_count: content_object_count
                .try_into()
                .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
            content_version_count: content_version_count
                .try_into()
                .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
        })
    }
}

pub(crate) type OpenTenderStores = Mutex<HashMap<TenderId, Arc<Mutex<TenderStore>>>>;

impl QuantixHost {
    pub fn create_tender(
        &self,
        command: CreateTenderCommand,
    ) -> Result<TenderSummary, TenderCommandError> {
        require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let name = command.name.trim();
        if name.is_empty() || name.len() > MAX_TENDER_NAME_BYTES {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }

        let tender_id = self.generate_tender_id()?;
        let stage_root = self
            .application_home()
            .join("staging")
            .join(format!("tender-{}", tender_id.as_str()));
        let final_root = self
            .application_home()
            .join("tenders")
            .join(tender_id.as_str());
        let store = match TenderStore::create(&stage_root, &tender_id, name) {
            Ok(store) => store,
            Err(error) => {
                let _ = fs::remove_dir_all(&stage_root);
                return Err(error);
            }
        };
        let summary = store.summary()?;
        drop(store);
        fs::rename(&stage_root, &final_root).map_err(store_unavailable)?;
        Ok(summary)
    }

    pub fn close_tender(&self, tender_id: &str) -> Result<(), TenderCommandError> {
        let tender_id = TenderId::parse(tender_id)?;
        let mut stores = self
            .open_tender_stores()
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        if stores
            .get(&tender_id)
            .is_some_and(|store| Arc::strong_count(store) != 1)
        {
            return Err(TenderCommandError::new(TenderErrorCode::StoreUnavailable));
        }
        stores.remove(&tender_id);
        Ok(())
    }

    pub fn revise_tender(
        &self,
        command: ReviseTenderCommand,
    ) -> Result<TenderSummary, TenderCommandError> {
        require_setup(self)?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        if command.validate().is_err() {
            return self.reject_tender_command(&tender_id, "revise_tender");
        }
        let name = command.name.trim();
        if name.is_empty() || name.len() > MAX_TENDER_NAME_BYTES {
            return self.reject_tender_command(&tender_id, "revise_tender");
        }
        let store = self.tender_store(&tender_id)?;
        let summary = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .revise(&command)?;
        Ok(summary)
    }

    pub fn register_tender_content(
        &self,
        command: RegisterTenderContentCommand,
    ) -> Result<ContentVersionSummary, TenderCommandError> {
        require_setup(self)?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        if command.validate().is_err()
            || command.bytes.len() > MAX_CONTENT_BYTES
            || !valid_logical_id(&command.logical_id)
            || !valid_media_type(&command.media_type)
        {
            return self.reject_tender_command(&tender_id, "register_tender_content");
        }
        let store = self.tender_store(&tender_id)?;
        let content = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .register_content(&command)?;
        Ok(content)
    }

    pub fn inspect_tender(&self, tender_id: &str) -> Result<TenderInspection, TenderCommandError> {
        require_setup(self)?;
        let tender_id = TenderId::parse(tender_id)?;
        let store = self.tender_store(&tender_id)?;
        let inspection = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .inspection()?;
        Ok(inspection)
    }

    pub fn open_tender(&self, tender_id: &str) -> Result<TenderSummary, TenderCommandError> {
        require_setup(self)?;
        let tender_id = TenderId::parse(tender_id)?;
        let store = self.tender_store(&tender_id)?;
        let summary = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .summary()?;
        Ok(summary)
    }

    pub fn list_tenders(&self) -> Result<Vec<TenderSummary>, TenderCommandError> {
        require_setup(self)?;
        let mut summaries = Vec::new();
        let entries =
            fs::read_dir(self.application_home().join("tenders")).map_err(store_unavailable)?;
        for entry in entries {
            let entry = entry.map_err(store_unavailable)?;
            let metadata = fs::symlink_metadata(entry.path()).map_err(store_unavailable)?;
            if metadata_is_unsafe_storage_link(&metadata) || !metadata.is_dir() {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            let tender_id = entry
                .file_name()
                .to_str()
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))
                .and_then(|value| {
                    TenderId::parse(value)
                        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))
                })?;
            summaries.push(self.open_tender(tender_id.as_str())?);
        }
        summaries.sort_by(|left, right| left.tender_id.cmp(&right.tender_id));
        let _ = self.replace_catalogue(&summaries);
        Ok(summaries)
    }

    fn tender_store(
        &self,
        tender_id: &TenderId,
    ) -> Result<Arc<Mutex<TenderStore>>, TenderCommandError> {
        let mut stores = self
            .open_tender_stores()
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        if let Some(store) = stores.get(tender_id) {
            return Ok(Arc::clone(store));
        }

        let root = self
            .application_home()
            .join("tenders")
            .join(tender_id.as_str());
        let store = Arc::new(Mutex::new(TenderStore::open(&root, tender_id)?));
        stores.insert(tender_id.clone(), Arc::clone(&store));
        Ok(store)
    }

    fn reject_tender_command<T>(
        &self,
        tender_id: &TenderId,
        command_name: &str,
    ) -> Result<T, TenderCommandError> {
        let store = self.tender_store(tender_id)?;
        store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .record_command_denied(tender_id, command_name)?;
        Err(TenderCommandError::new(TenderErrorCode::InvalidCommand))
    }

    fn replace_catalogue(&self, summaries: &[TenderSummary]) -> Result<(), TenderCommandError> {
        let _guard = self
            .catalogue_lock()
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        let mut connection = Connection::open(self.application_home().join("installation.sqlite"))
            .map_err(sql_error)?;
        connection
            .busy_timeout(Duration::from_secs(5))
            .map_err(sql_error)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        transaction
            .execute("DELETE FROM tender_catalogue", [])
            .map_err(sql_error)?;
        for summary in summaries {
            transaction
                .execute(
                    "INSERT INTO tender_catalogue (
                       tender_id, name, revision, audit_event_count, audit_chain_head
                     ) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        summary.tender_id,
                        summary.name,
                        summary.revision,
                        summary.audit_event_count as i64,
                        summary.audit_chain_head
                    ],
                )
                .map_err(sql_error)?;
        }
        transaction.commit().map_err(sql_error)
    }
}

fn require_setup(host: &QuantixHost) -> Result<(), TenderCommandError> {
    let outcome = host.ensure_setup();
    match outcome.state {
        SetupState::Ready | SetupState::Warning => Ok(()),
        _ => Err(TenderCommandError::new(TenderErrorCode::SetupRequired)),
    }
}

fn valid_logical_id(logical_id: &str) -> bool {
    !logical_id.is_empty()
        && logical_id.len() <= 100
        && logical_id.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
}

fn valid_media_type(media_type: &str) -> bool {
    !media_type.is_empty()
        && media_type.len() <= 100
        && media_type.is_ascii()
        && media_type.contains('/')
        && !media_type.chars().any(char::is_whitespace)
}

fn configure_writer(connection: &Connection) -> Result<(), TenderCommandError> {
    connection
        .busy_timeout(Duration::from_secs(5))
        .map_err(sql_error)?;
    connection
        .pragma_update(None, "foreign_keys", true)
        .map_err(sql_error)?;
    let journal_mode: String = connection
        .pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get(0))
        .map_err(sql_error)?;
    if !journal_mode.eq_ignore_ascii_case("wal") {
        return Err(TenderCommandError::new(TenderErrorCode::StoreUnavailable));
    }
    connection
        .pragma_update(None, "synchronous", "FULL")
        .map_err(sql_error)?;
    connection
        .set_limit(Limit::SQLITE_LIMIT_LENGTH, 16 * 1024 * 1024)
        .map_err(sql_error)?;
    connection
        .set_limit(Limit::SQLITE_LIMIT_SQL_LENGTH, 1024 * 1024)
        .map_err(sql_error)?;
    connection
        .set_limit(Limit::SQLITE_LIMIT_ATTACHED, 0)
        .map_err(sql_error)?;
    Ok(())
}

fn validate_tender_store_layout(root: &Path) -> Result<(), TenderCommandError> {
    let root_metadata = fs::symlink_metadata(root)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::NotFound))?;
    if metadata_is_unsafe_storage_link(&root_metadata) || !root_metadata.is_dir() {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }

    let parent = root
        .parent()
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    let canonical_root = fs::canonicalize(root).map_err(store_unavailable)?;
    let canonical_parent = fs::canonicalize(parent).map_err(store_unavailable)?;
    if canonical_root.parent() != Some(canonical_parent.as_path()) {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }

    for entry in fs::read_dir(root).map_err(store_unavailable)? {
        let entry = entry.map_err(store_unavailable)?;
        let metadata = fs::symlink_metadata(entry.path()).map_err(store_unavailable)?;
        if metadata_is_unsafe_storage_link(&metadata) {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let name = entry
            .file_name()
            .to_str()
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?
            .to_owned();
        let expected_type = match name.as_str() {
            "content" | "runs" | "staging" => Some(true),
            "tender.sqlite"
            | "tender.sqlite-journal"
            | "tender.sqlite-shm"
            | "tender.sqlite-wal" => Some(false),
            _ => None,
        };
        match expected_type {
            Some(true) if metadata.is_dir() => {}
            Some(false) if metadata.is_file() => {}
            _ => return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
        }
    }

    for directory in ["content", "runs", "staging"] {
        let metadata = fs::symlink_metadata(root.join(directory))
            .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        if metadata_is_unsafe_storage_link(&metadata) || !metadata.is_dir() {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
    }
    let database_metadata = fs::symlink_metadata(root.join("tender.sqlite"))
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    if metadata_is_unsafe_storage_link(&database_metadata) || !database_metadata.is_file() {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok(())
}

fn metadata_is_unsafe_storage_link(metadata: &fs::Metadata) -> bool {
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

fn validate_store(
    connection: &Connection,
    expected_tender_id: &TenderId,
) -> Result<(), TenderCommandError> {
    let version: i64 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(sql_error)?;
    if version != TENDER_SCHEMA_VERSION {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let expected_schema = Connection::open_in_memory().map_err(sql_error)?;
    expected_schema
        .execute_batch(TENDER_SCHEMA)
        .map_err(sql_error)?;
    if tender_schema_objects(connection)? != tender_schema_objects(&expected_schema)? {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let quick_check: String = connection
        .query_row("PRAGMA quick_check", [], |row| row.get(0))
        .map_err(sql_error)?;
    if quick_check != "ok" {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let foreign_key_failure: Option<i64> = connection
        .query_row(
            "SELECT 1 FROM pragma_foreign_key_check LIMIT 1",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(sql_error)?;
    if foreign_key_failure.is_some() {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let stored_tender_id: String = connection
        .query_row(
            "SELECT tender_id FROM tender WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    if stored_tender_id != expected_tender_id.as_str() {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    verify_audit_chain(connection)
}

type SchemaObject = (String, String, String, Option<String>);

fn tender_schema_objects(connection: &Connection) -> Result<Vec<SchemaObject>, TenderCommandError> {
    let mut statement = connection
        .prepare(
            "SELECT type, name, tbl_name, sql FROM sqlite_schema
             WHERE name NOT LIKE 'sqlite_%'
             ORDER BY type, name",
        )
        .map_err(sql_error)?;
    let objects = statement
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .map_err(sql_error)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(sql_error)?;
    Ok(objects)
}

fn append_audit_event(
    transaction: &Transaction<'_>,
    tender_id: &str,
    event_type: &str,
    aggregate_revision: u32,
    change: serde_json::Value,
    created_at: &str,
) -> Result<(), TenderCommandError> {
    let previous: Option<(i64, String)> = transaction
        .query_row(
            "SELECT sequence, current_hash FROM audit_events ORDER BY sequence DESC LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(sql_error)?;
    let sequence = previous.as_ref().map_or(1, |(sequence, _)| sequence + 1);
    let preceding_hash = previous
        .map(|(_, hash)| hash)
        .unwrap_or_else(|| ZERO_AUDIT_HASH.into());
    let payload = json!({
        "aggregate_revision": aggregate_revision.to_string(),
        "change": change,
        "created_at": created_at,
        "event_type": event_type,
        "preceding_hash": preceding_hash,
        "schema_version": "1",
        "sequence": sequence.to_string(),
        "tender_id": tender_id,
    });
    let payload_json = serde_json_canonicalizer::to_string(&payload)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
    let current_hash = sha256_hex(payload_json.as_bytes());
    transaction
        .execute(
            "INSERT INTO audit_events (
               sequence, event_type, aggregate_revision, payload_json,
               preceding_hash, current_hash, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                sequence,
                event_type,
                aggregate_revision,
                payload_json,
                preceding_hash,
                current_hash,
                created_at
            ],
        )
        .map_err(sql_error)?;
    Ok(())
}

fn verify_audit_chain(connection: &Connection) -> Result<(), TenderCommandError> {
    let tender_id: String = connection
        .query_row(
            "SELECT tender_id FROM tender WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    let mut statement = connection
        .prepare(
            "SELECT sequence, event_type, aggregate_revision, payload_json,
                    preceding_hash, current_hash, created_at
             FROM audit_events ORDER BY sequence",
        )
        .map_err(sql_error)?;
    let events = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, u32>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
            ))
        })
        .map_err(sql_error)?;
    let mut expected_sequence = 1_i64;
    let mut expected_preceding = ZERO_AUDIT_HASH.to_owned();
    for event in events {
        let (
            sequence,
            event_type,
            aggregate_revision,
            payload_json,
            preceding_hash,
            current_hash,
            created_at,
        ) = event.map_err(sql_error)?;
        let parsed: serde_json::Value = serde_json::from_str(&payload_json)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let canonical = serde_json_canonicalizer::to_string(&parsed)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let sequence_text = sequence.to_string();
        let aggregate_revision_text = aggregate_revision.to_string();
        if sequence != expected_sequence
            || payload_json != canonical
            || preceding_hash != expected_preceding
            || sha256_hex(payload_json.as_bytes()) != current_hash
            || parsed.get("sequence").and_then(serde_json::Value::as_str)
                != Some(sequence_text.as_str())
            || parsed.get("event_type").and_then(serde_json::Value::as_str)
                != Some(event_type.as_str())
            || parsed.get("created_at").and_then(serde_json::Value::as_str)
                != Some(created_at.as_str())
            || parsed
                .get("aggregate_revision")
                .and_then(serde_json::Value::as_str)
                != Some(aggregate_revision_text.as_str())
            || parsed
                .get("preceding_hash")
                .and_then(serde_json::Value::as_str)
                != Some(preceding_hash.as_str())
            || parsed.get("tender_id").and_then(serde_json::Value::as_str)
                != Some(tender_id.as_str())
            || parsed
                .get("schema_version")
                .and_then(serde_json::Value::as_str)
                != Some("1")
        {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        expected_sequence += 1;
        expected_preceding = current_hash;
    }
    if expected_sequence == 1 {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok(())
}

fn sqlite_timestamp(transaction: &Transaction<'_>) -> Result<String, TenderCommandError> {
    transaction
        .query_row("SELECT strftime('%Y-%m-%dT%H:%M:%fZ', 'now')", [], |row| {
            row.get(0)
        })
        .map_err(sql_error)
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn store_unavailable(_error: std::io::Error) -> TenderCommandError {
    TenderCommandError::new(TenderErrorCode::StoreUnavailable)
}

fn sql_error(_error: rusqlite::Error) -> TenderCommandError {
    TenderCommandError::new(TenderErrorCode::StoreUnavailable)
}

fn content_store_error(_error: cacache::Error) -> TenderCommandError {
    TenderCommandError::new(TenderErrorCode::StoreUnavailable)
}

#[cfg(test)]
mod tests {
    use std::{
        io,
        path::Path,
        sync::{Arc, Barrier},
    };

    use super::*;
    use crate::setup::{
        DeviceProtection, SetupPlatform, StoragePermissions, MINIMUM_SETUP_FREE_SPACE_BYTES,
    };

    struct ReadySetupPlatform;

    impl SetupPlatform for ReadySetupPlatform {
        fn available_space(&self, _path: &Path) -> io::Result<u64> {
            Ok(MINIMUM_SETUP_FREE_SPACE_BYTES)
        }

        fn is_writable(&self, _path: &Path) -> io::Result<bool> {
            Ok(true)
        }

        fn storage_permissions(&self, _path: &Path) -> io::Result<StoragePermissions> {
            Ok(StoragePermissions::Restrictive)
        }

        fn device_protection(&self, _path: &Path) -> DeviceProtection {
            DeviceProtection::Protected
        }
    }

    #[test]
    fn cold_open_has_one_writer_and_close_rejects_a_borrowed_store() {
        let user_home = tempfile::tempdir().expect("temporary user home");
        let application_home = user_home.path().join(".quantix");
        let host =
            QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
        assert_eq!(host.ensure_setup().state, SetupState::Ready);
        let tender = host
            .create_tender(CreateTenderCommand {
                name: "Observable writer ownership".into(),
            })
            .expect("create Tender");
        host.close_tender(&tender.tender_id).expect("close Tender");
        TENDER_STORE_OPEN_COUNT.store(0, Ordering::SeqCst);

        let barrier = Arc::new(Barrier::new(8));
        let opens = (0..8)
            .map(|_| {
                let host = host.clone();
                let tender_id = tender.tender_id.clone();
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    host.open_tender(&tender_id)
                })
            })
            .collect::<Vec<_>>();
        for open in opens {
            assert_eq!(
                open.join()
                    .expect("open thread")
                    .expect("serialized cold open"),
                tender
            );
        }
        assert_eq!(TENDER_STORE_OPEN_COUNT.load(Ordering::SeqCst), 1);

        let tender_id = TenderId::parse(&tender.tender_id).expect("valid Tender identity");
        let borrowed_store = host.tender_store(&tender_id).expect("borrow open writer");
        let close_error = host
            .close_tender(&tender.tender_id)
            .expect_err("borrowed writer cannot be closed");
        assert_eq!(close_error.code, TenderErrorCode::StoreUnavailable);
        drop(borrowed_store);
        host.close_tender(&tender.tender_id)
            .expect("close unborrowed writer");
    }
}
