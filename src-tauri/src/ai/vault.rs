use std::{
    collections::BTreeMap,
    error::Error,
    fmt,
    fs::{File, OpenOptions},
    io::{self, Read, Write},
    os::windows::{
        ffi::OsStrExt,
        fs::{MetadataExt, OpenOptionsExt},
        io::AsRawHandle,
    },
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

#[cfg(feature = "runtime-fixture")]
use std::sync::atomic::{AtomicU8, Ordering};

use fs4::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use windows::Win32::{
    Foundation::HANDLE,
    Storage::FileSystem::{
        GetFileInformationByHandle, MoveFileExW, ReplaceFileW, BY_HANDLE_FILE_INFORMATION,
        FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        MOVEFILE_WRITE_THROUGH, REPLACE_FILE_FLAGS,
    },
};
use windows_core::PCWSTR;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use super::windows_dpapi::{protect_for_current_user, unprotect_for_current_user};
use crate::setup::validate_application_home_path;

const VAULT_FILE_NAME: &str = "ai-connections.vault";
const LOCK_FILE_NAME: &str = "ai-connections.vault.lock";
const STAGED_FILE_PREFIX: &str = ".ai-connections.vault.";
const STAGED_FILE_SUFFIX: &str = ".tmp";
const VAULT_SCHEMA_VERSION: u32 = 1;
const MAX_CLEAR_BYTES: usize = 4 * 1024 * 1024;
const MAX_CIPHERTEXT_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VaultError {
    Unavailable,
    Corrupt,
    Unsupported,
    Invalid,
    RevisionConflict,
}

impl fmt::Display for VaultError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "AI connection vault is unavailable",
            Self::Corrupt => "AI connection vault is corrupt",
            Self::Unsupported => "AI connection vault version is unsupported",
            Self::Invalid => "AI connection vault mutation is invalid",
            Self::RevisionConflict => "AI connection vault revision conflict",
        })
    }
}

impl Error for VaultError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultSnapshot {
    pub mutation_revision: u64,
    pub connection_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VaultLoadState {
    Missing,
    Ready(VaultSnapshot),
    Corrupt,
    Unsupported,
}

impl VaultLoadState {
    pub fn ready(&self) -> Option<&VaultSnapshot> {
        match self {
            Self::Ready(snapshot) => Some(snapshot),
            Self::Missing | Self::Corrupt | Self::Unsupported => None,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct VaultPayload {
    schema_version: u32,
    pub(crate) mutation_revision: u64,
    pub(crate) connections: BTreeMap<String, StoredAiConnection>,
}

#[derive(Deserialize)]
struct VaultVersionHeader {
    schema_version: u32,
}

impl Zeroize for VaultPayload {
    fn zeroize(&mut self) {
        self.schema_version.zeroize();
        self.mutation_revision.zeroize();
        self.connections.clear();
    }
}

impl ZeroizeOnDrop for VaultPayload {}

impl Drop for VaultPayload {
    fn drop(&mut self) {
        self.zeroize();
    }
}

impl fmt::Debug for VaultPayload {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("VaultPayload")
            .field("schema_version", &self.schema_version)
            .field("mutation_revision", &self.mutation_revision)
            .field("connection_count", &self.connections.len())
            .finish()
    }
}

impl VaultPayload {
    fn empty() -> Self {
        Self {
            schema_version: VAULT_SCHEMA_VERSION,
            mutation_revision: 0,
            connections: BTreeMap::new(),
        }
    }

    fn validate(&self) -> Result<(), VaultError> {
        if self.schema_version != VAULT_SCHEMA_VERSION {
            return Err(VaultError::Unsupported);
        }
        if self.connections.iter().any(|(key, connection)| {
            key != &connection.connection_id || !is_connection_id(key) || !connection.is_valid()
        }) {
            return Err(VaultError::Corrupt);
        }
        Ok(())
    }

    pub(crate) fn insert(&mut self, connection: StoredAiConnection) -> Result<(), VaultError> {
        if !connection.is_valid() || self.connections.contains_key(&connection.connection_id) {
            return Err(VaultError::Invalid);
        }
        self.connections
            .insert(connection.connection_id.clone(), connection);
        Ok(())
    }

    fn secret_free_snapshot(&self) -> VaultSnapshot {
        VaultSnapshot {
            mutation_revision: self.mutation_revision,
            connection_ids: self.connections.keys().cloned().collect(),
        }
    }
}

#[derive(Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(deny_unknown_fields)]
pub(crate) struct StoredAiConnection {
    pub(crate) connection_id: String,
    pub(crate) credential: StoredCredential,
}

impl fmt::Debug for StoredAiConnection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("StoredAiConnection([REDACTED])")
    }
}

impl StoredAiConnection {
    fn is_valid(&self) -> bool {
        is_connection_id(&self.connection_id) && self.credential.is_valid()
    }
}

#[derive(Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(deny_unknown_fields)]
pub(crate) struct StoredCredential {
    pub(crate) values: Vec<SecretNameValue>,
}

impl fmt::Debug for StoredCredential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("StoredCredential([REDACTED])")
    }
}

impl StoredCredential {
    fn is_valid(&self) -> bool {
        !self.values.is_empty() && self.values.iter().all(SecretNameValue::is_valid)
    }
}

#[derive(Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(deny_unknown_fields)]
pub(crate) struct SecretNameValue {
    pub(crate) name: SecretName,
    pub(crate) value: SecretValue,
}

impl fmt::Debug for SecretNameValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretNameValue([REDACTED])")
    }
}

impl SecretNameValue {
    fn is_valid(&self) -> bool {
        !self.name.0.is_empty() && !self.value.0.is_empty()
    }
}

#[derive(Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(transparent)]
pub(crate) struct SecretName(pub(crate) String);

impl fmt::Debug for SecretName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretName([REDACTED])")
    }
}

#[derive(Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(transparent)]
pub(crate) struct SecretValue(pub(crate) String);

impl fmt::Debug for SecretValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretValue([REDACTED])")
    }
}

pub struct AiConnectionVault {
    path: PathBuf,
    lock_path: PathBuf,
    #[cfg(feature = "runtime-fixture")]
    publication_fault: AtomicU8,
}

impl AiConnectionVault {
    pub fn new(application_home: &Path) -> Result<Self, VaultError> {
        let application_home = validate_application_home_path(application_home)
            .map_err(|_| VaultError::Unavailable)?;
        validate_application_home_handle(&application_home)?;
        Ok(Self {
            path: application_home.join(VAULT_FILE_NAME),
            lock_path: application_home.join(LOCK_FILE_NAME),
            #[cfg(feature = "runtime-fixture")]
            publication_fault: AtomicU8::new(PUBLISH_FAULT_NONE),
        })
    }

    pub fn load(&self) -> Result<VaultLoadState, VaultError> {
        let _process = vault_mutex().lock().map_err(|_| VaultError::Unavailable)?;
        let lock = self.open_lock()?;
        FileExt::lock(&lock).map_err(|_| VaultError::Unavailable)?;
        validate_file_handle(&lock)?;
        self.load_state_locked()
    }

    pub(crate) fn mutate<F>(
        &self,
        expected_revision: u64,
        mutation: F,
    ) -> Result<VaultSnapshot, VaultError>
    where
        F: FnOnce(&mut VaultPayload) -> Result<(), VaultError>,
    {
        let _process = vault_mutex().lock().map_err(|_| VaultError::Unavailable)?;
        let lock = self.open_lock()?;
        FileExt::lock(&lock).map_err(|_| VaultError::Unavailable)?;
        validate_file_handle(&lock)?;
        let mut payload = self.load_payload_locked()?;
        if payload.mutation_revision != expected_revision {
            return Err(VaultError::RevisionConflict);
        }
        mutation(&mut payload)?;
        payload.mutation_revision = payload
            .mutation_revision
            .checked_add(1)
            .ok_or(VaultError::Invalid)?;
        self.publish_locked(&payload)?;
        Ok(payload.secret_free_snapshot())
    }

    #[allow(dead_code)]
    pub(crate) fn mutate_current<F>(&self, mutation: F) -> Result<VaultSnapshot, VaultError>
    where
        F: FnOnce(&mut VaultPayload) -> Result<(), VaultError>,
    {
        let _process = vault_mutex().lock().map_err(|_| VaultError::Unavailable)?;
        let lock = self.open_lock()?;
        FileExt::lock(&lock).map_err(|_| VaultError::Unavailable)?;
        validate_file_handle(&lock)?;
        let mut payload = self.load_payload_locked()?;
        mutation(&mut payload)?;
        payload.mutation_revision = payload
            .mutation_revision
            .checked_add(1)
            .ok_or(VaultError::Invalid)?;
        self.publish_locked(&payload)?;
        Ok(payload.secret_free_snapshot())
    }

    #[allow(dead_code)]
    pub(crate) fn with_locked_payload<F, T>(&self, operation: F) -> Result<T, VaultError>
    where
        F: FnOnce(&VaultPayload) -> Result<T, VaultError>,
    {
        let _process = vault_mutex().lock().map_err(|_| VaultError::Unavailable)?;
        let lock = self.open_lock()?;
        FileExt::lock(&lock).map_err(|_| VaultError::Unavailable)?;
        validate_file_handle(&lock)?;
        let payload = self.load_payload_locked()?;
        operation(&payload)
    }

    #[cfg(feature = "runtime-fixture")]
    pub fn fixture_insert(
        &self,
        expected_revision: u64,
        connection_id: &str,
        secret: &Zeroizing<String>,
    ) -> Result<VaultSnapshot, VaultError> {
        let connection = fixture_connection(connection_id, secret)?;
        self.mutate(expected_revision, |payload| payload.insert(connection))
    }

    #[cfg(feature = "runtime-fixture")]
    pub fn fixture_insert_current(
        &self,
        connection_id: &str,
        secret: &Zeroizing<String>,
    ) -> Result<VaultSnapshot, VaultError> {
        let connection = fixture_connection(connection_id, secret)?;
        self.mutate_current(|payload| payload.insert(connection))
    }

    #[cfg(feature = "runtime-fixture")]
    pub fn fixture_fail_before_publish_once(&self) {
        self.publication_fault
            .store(PUBLISH_FAULT_BEFORE, Ordering::Release);
    }

    #[cfg(feature = "runtime-fixture")]
    pub fn fixture_fail_after_publish_once(&self) {
        self.publication_fault
            .store(PUBLISH_FAULT_AFTER, Ordering::Release);
    }

    fn load_state_locked(&self) -> Result<VaultLoadState, VaultError> {
        let ciphertext = match self.read_ciphertext_locked() {
            Ok(ciphertext) => ciphertext,
            Err(VaultError::Corrupt | VaultError::Invalid) => return Ok(VaultLoadState::Corrupt),
            Err(error) => return Err(error),
        };
        let Some(ciphertext) = ciphertext else {
            return Ok(VaultLoadState::Missing);
        };
        match decode_payload(&ciphertext) {
            Ok(payload) => Ok(VaultLoadState::Ready(payload.secret_free_snapshot())),
            Err(VaultError::Corrupt | VaultError::Invalid) => Ok(VaultLoadState::Corrupt),
            Err(VaultError::Unsupported) => Ok(VaultLoadState::Unsupported),
            Err(error) => Err(error),
        }
    }

    fn load_payload_locked(&self) -> Result<VaultPayload, VaultError> {
        let Some(ciphertext) = self.read_ciphertext_locked()? else {
            return Ok(VaultPayload::empty());
        };
        decode_payload(&ciphertext)
    }

    fn read_ciphertext_locked(&self) -> Result<Option<Vec<u8>>, VaultError> {
        let Some(mut file) = open_existing_validated_file(&self.path, false)? else {
            return Ok(None);
        };
        let metadata = file.metadata().map_err(|_| VaultError::Unavailable)?;
        if metadata.len() > MAX_CIPHERTEXT_BYTES as u64 {
            return Err(VaultError::Corrupt);
        }
        let mut ciphertext = Vec::with_capacity(metadata.len() as usize);
        (&mut file)
            .take((MAX_CIPHERTEXT_BYTES + 1) as u64)
            .read_to_end(&mut ciphertext)
            .map_err(|_| VaultError::Unavailable)?;
        if ciphertext.is_empty() || ciphertext.len() > MAX_CIPHERTEXT_BYTES {
            return Err(VaultError::Corrupt);
        }
        Ok(Some(ciphertext))
    }

    fn open_lock(&self) -> Result<File, VaultError> {
        for _ in 0..32 {
            match open_create_new_file(&self.lock_path, true) {
                Ok(file) => return Ok(file),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    if let Some(file) = open_existing_validated_file(&self.lock_path, true)? {
                        return Ok(file);
                    }
                }
                Err(_) => return Err(VaultError::Unavailable),
            }
        }
        Err(VaultError::Unavailable)
    }

    fn publish_locked(&self, payload: &VaultPayload) -> Result<(), VaultError> {
        let clear = Zeroizing::new(serde_json::to_vec(payload).map_err(|_| VaultError::Invalid)?);
        if clear.len() > MAX_CLEAR_BYTES {
            return Err(VaultError::Invalid);
        }
        let ciphertext = protect_for_current_user(clear).map_err(|_| VaultError::Unavailable)?;
        if ciphertext.is_empty() || ciphertext.len() > MAX_CIPHERTEXT_BYTES {
            return Err(VaultError::Invalid);
        }
        let expected_ciphertext_sha256 = Sha256::digest(&ciphertext);

        let (staged_path, mut staged) = self.create_staged_file()?;
        staged
            .write_all(&ciphertext)
            .and_then(|()| staged.flush())
            .and_then(|()| staged.sync_all())
            .map_err(|_| VaultError::Unavailable)?;
        validate_file_handle(&staged)?;
        drop(staged);

        let target_state = self.validated_target_state()?;
        let fault = self.take_publication_fault();
        let publication = if fault == PUBLISH_FAULT_BEFORE {
            Err(())
        } else {
            let result = match target_state {
                TargetState::Existing => replace_file(&self.path, staged_path.path()),
                TargetState::Missing => move_file_write_through(staged_path.path(), &self.path),
            }
            .map_err(|_| ());
            if fault == PUBLISH_FAULT_AFTER && result.is_ok() {
                Err(())
            } else {
                result
            }
        };

        let verified = self.reopen_and_verify(
            expected_ciphertext_sha256.as_slice(),
            payload.mutation_revision,
        );
        match (publication, verified) {
            (_, Ok(true)) => Ok(()),
            (Ok(()), Ok(false)) | (Err(()), Ok(false)) => Err(VaultError::Unavailable),
            (Ok(()) | Err(()), Err(error)) => Err(error),
        }
    }

    fn create_staged_file(&self) -> Result<(OwnedStagedPath, File), VaultError> {
        for _ in 0..32 {
            let mut random = [0u8; 16];
            getrandom::fill(&mut random).map_err(|_| VaultError::Unavailable)?;
            let suffix: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
            let path = self
                .path
                .with_file_name(format!("{STAGED_FILE_PREFIX}{suffix}{STAGED_FILE_SUFFIX}"));
            match open_create_new_file(&path, false) {
                Ok(file) => return Ok((OwnedStagedPath(path), file)),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(_) => return Err(VaultError::Unavailable),
            }
        }
        Err(VaultError::Unavailable)
    }

    fn validated_target_state(&self) -> Result<TargetState, VaultError> {
        Ok(
            if open_existing_validated_file(&self.path, false)?.is_some() {
                TargetState::Existing
            } else {
                TargetState::Missing
            },
        )
    }

    fn reopen_and_verify(
        &self,
        expected_ciphertext_sha256: &[u8],
        expected_mutation_revision: u64,
    ) -> Result<bool, VaultError> {
        let Some(ciphertext) = self.read_ciphertext_locked()? else {
            return Ok(false);
        };
        if Sha256::digest(&ciphertext).as_slice() != expected_ciphertext_sha256 {
            return Ok(false);
        }
        match decode_payload(&ciphertext) {
            Ok(payload) => Ok(payload.mutation_revision == expected_mutation_revision),
            Err(VaultError::Corrupt | VaultError::Unsupported | VaultError::Invalid) => Ok(false),
            Err(error) => Err(error),
        }
    }

    #[cfg(feature = "runtime-fixture")]
    fn take_publication_fault(&self) -> u8 {
        self.publication_fault
            .swap(PUBLISH_FAULT_NONE, Ordering::AcqRel)
    }

    #[cfg(not(feature = "runtime-fixture"))]
    fn take_publication_fault(&self) -> u8 {
        PUBLISH_FAULT_NONE
    }
}

const PUBLISH_FAULT_NONE: u8 = 0;
const PUBLISH_FAULT_BEFORE: u8 = 1;
const PUBLISH_FAULT_AFTER: u8 = 2;

enum TargetState {
    Missing,
    Existing,
}

struct OwnedStagedPath(PathBuf);

impl OwnedStagedPath {
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for OwnedStagedPath {
    fn drop(&mut self) {
        remove_owned_stage(&self.0);
    }
}

fn decode_payload(ciphertext: &[u8]) -> Result<VaultPayload, VaultError> {
    if ciphertext.is_empty() || ciphertext.len() > MAX_CIPHERTEXT_BYTES {
        return Err(VaultError::Corrupt);
    }
    let clear = unprotect_for_current_user(ciphertext).map_err(|_| VaultError::Corrupt)?;
    if clear.len() > MAX_CLEAR_BYTES {
        return Err(VaultError::Corrupt);
    }
    let header: VaultVersionHeader =
        serde_json::from_slice(&clear).map_err(|_| VaultError::Corrupt)?;
    if header.schema_version != VAULT_SCHEMA_VERSION {
        return Err(VaultError::Unsupported);
    }
    let payload: VaultPayload = serde_json::from_slice(&clear).map_err(|_| VaultError::Corrupt)?;
    payload.validate()?;
    Ok(payload)
}

fn vault_mutex() -> &'static Mutex<()> {
    static MUTEX: OnceLock<Mutex<()>> = OnceLock::new();
    MUTEX.get_or_init(|| Mutex::new(()))
}

fn validate_application_home_handle(path: &Path) -> Result<(), VaultError> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .custom_flags((FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS).0);
    let directory = options.open(path).map_err(|_| VaultError::Unavailable)?;
    let metadata = directory.metadata().map_err(|_| VaultError::Unavailable)?;
    if !metadata.is_dir() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0 {
        return Err(VaultError::Unavailable);
    }
    Ok(())
}

fn open_create_new_file(path: &Path, readable: bool) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options
        .read(readable)
        .write(true)
        .create_new(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0);
    let file = options.open(path)?;
    validate_file_handle_io(&file)?;
    Ok(file)
}

fn open_existing_validated_file(path: &Path, writable: bool) -> Result<Option<File>, VaultError> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .write(writable)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0);
    let file = match options.open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(VaultError::Unavailable),
    };
    validate_file_handle(&file)?;
    Ok(Some(file))
}

fn validate_file_handle(file: &File) -> Result<(), VaultError> {
    validate_file_handle_io(file).map_err(|_| VaultError::Unavailable)
}

fn validate_file_handle_io(file: &File) -> io::Result<()> {
    let metadata = file.metadata()?;
    let mut handle_information = BY_HANDLE_FILE_INFORMATION::default();
    // SAFETY: the borrowed standard-library file handle remains valid for the call and
    // the output points to an initialized structure owned by this stack frame.
    unsafe { GetFileInformationByHandle(HANDLE(file.as_raw_handle()), &mut handle_information) }
        .map_err(|_| io::Error::other("could not inspect vault storage object"))?;
    if !metadata.is_file()
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0
        || handle_information.nNumberOfLinks != 1
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid vault storage object",
        ));
    }
    Ok(())
}

fn remove_owned_stage(path: &Path) {
    if matches!(open_existing_validated_file(path, false), Ok(Some(_))) {
        let _ = std::fs::remove_file(path);
    }
}

fn is_connection_id(value: &str) -> bool {
    value.len() == 32
        && value
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

#[cfg(feature = "runtime-fixture")]
fn fixture_connection(
    connection_id: &str,
    secret: &Zeroizing<String>,
) -> Result<StoredAiConnection, VaultError> {
    if !is_connection_id(connection_id) || secret.is_empty() {
        return Err(VaultError::Invalid);
    }
    Ok(StoredAiConnection {
        connection_id: connection_id.to_owned(),
        credential: StoredCredential {
            values: vec![SecretNameValue {
                name: SecretName("fixture".to_owned()),
                value: SecretValue(secret.to_string()),
            }],
        },
    })
}

fn replace_file(target: &Path, staged: &Path) -> windows_core::Result<()> {
    let target = wide_path(target);
    let staged = wide_path(staged);
    // SAFETY: both buffers are terminated and remain alive for the call; no backup is requested.
    unsafe {
        ReplaceFileW(
            PCWSTR(target.as_ptr()),
            PCWSTR(staged.as_ptr()),
            PCWSTR::null(),
            REPLACE_FILE_FLAGS(0),
            None,
            None,
        )
    }
}

fn move_file_write_through(staged: &Path, target: &Path) -> windows_core::Result<()> {
    let staged = wide_path(staged);
    let target = wide_path(target);
    // SAFETY: both buffers are terminated and remain alive for the call.
    unsafe {
        MoveFileExW(
            PCWSTR(staged.as_ptr()),
            PCWSTR(target.as_ptr()),
            MOVEFILE_WRITE_THROUGH,
        )
    }
}

fn wide_path(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}
