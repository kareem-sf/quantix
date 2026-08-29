use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;
use ts_rs::TS;

const MAX_BINARY_BYTES: u64 = 400 * 1024 * 1024;
const STAGING_PREFIX: &str = ".staging-";
const PROVENANCE_FILE: &str = "provenance.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CodexRelease {
    pub version: &'static str,
    pub primary_url: &'static str,
    pub fallback_url: &'static str,
    pub sha256: &'static str,
}

pub const CODEX_RELEASE: CodexRelease = CodexRelease {
    version: "0.151.0",
    primary_url: "https://github.com/openai/codex/releases/download/rust-v0.151.0/codex-x86_64-pc-windows-msvc.exe",
    fallback_url: "https://releases.openai.com/codex/releases/0.151.0/codex-x86_64-pc-windows-msvc.exe",
    sha256: "cf68265897197ac5f3bff6a10c168eec159842b353129726da5e3ed6b91ef0f4",
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ManagedCodexRuntimeState {
    Ready,
    NotInstalled,
    InterruptedPreparation,
    InstallFailed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct ManagedCodexRuntimeStatus {
    pub state: ManagedCodexRuntimeState,
    pub version: Option<String>,
    pub summary: String,
}

#[derive(Debug)]
pub enum ManagedRuntimeError {
    Cancelled,
    DownloadFailed(String),
    IntegrityFailed,
    Io(std::io::Error),
}

impl fmt::Display for ManagedRuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => write!(formatter, "cancelled"),
            Self::DownloadFailed(detail) => write!(formatter, "download failed: {detail}"),
            Self::IntegrityFailed => write!(formatter, "integrity check failed"),
            Self::Io(error) => write!(formatter, "io error: {error}"),
        }
    }
}

pub fn codex_runtime_directory(application_home: &Path, version: &str) -> PathBuf {
    application_home
        .join("runtimes")
        .join("codex")
        .join(version)
}

pub fn codex_binary_path(application_home: &Path, version: &str) -> PathBuf {
    codex_runtime_directory(application_home, version).join(executable_name("codex"))
}

fn staging_path(application_home: &Path, version: &str) -> PathBuf {
    codex_runtime_directory(application_home, version)
        .join(format!("{STAGING_PREFIX}{version}.exe"))
}

fn executable_name(name: &str) -> String {
    if std::env::consts::EXE_EXTENSION.is_empty() {
        name.to_owned()
    } else {
        format!("{name}.{}", std::env::consts::EXE_EXTENSION)
    }
}

pub fn inspect_managed_codex_runtime(application_home: &Path) -> ManagedCodexRuntimeStatus {
    inspect_release(application_home, &CODEX_RELEASE)
}

pub fn inspect_release(
    application_home: &Path,
    release: &CodexRelease,
) -> ManagedCodexRuntimeStatus {
    let binary = codex_binary_path(application_home, release.version);
    if binary.is_file() {
        return match binary_sha256(&binary) {
            Some(digest) if digest == release.sha256 => ManagedCodexRuntimeStatus {
                state: ManagedCodexRuntimeState::Ready,
                version: Some(release.version.to_owned()),
                summary: "The ChatGPT assistant is ready.".to_owned(),
            },
            Some(_) => ManagedCodexRuntimeStatus {
                state: ManagedCodexRuntimeState::InstallFailed,
                version: Some(release.version.to_owned()),
                summary: "The downloaded assistant did not pass its safety check. Nothing was changed; trying again replaces it.".to_owned(),
            },
            None => ManagedCodexRuntimeStatus {
                state: ManagedCodexRuntimeState::InstallFailed,
                version: Some(release.version.to_owned()),
                summary: "The downloaded assistant could not be checked. Trying again replaces it.".to_owned(),
            },
        };
    }
    if staging_path(application_home, release.version).is_file() {
        return ManagedCodexRuntimeStatus {
            state: ManagedCodexRuntimeState::InterruptedPreparation,
            version: None,
            summary: "The last download was interrupted. Quantix cleans up and finishes the next time you try.".to_owned(),
        };
    }
    ManagedCodexRuntimeStatus {
        state: ManagedCodexRuntimeState::NotInstalled,
        version: None,
        summary: "The ChatGPT assistant is not downloaded yet.".to_owned(),
    }
}

pub async fn prepare_managed_codex_runtime(
    application_home: &Path,
    cancellation: CancellationToken,
) -> Result<ManagedCodexRuntimeStatus, ManagedRuntimeError> {
    prepare_release_from(application_home, &CODEX_RELEASE, &network_fetch, cancellation).await
}

pub type FetchFuture = Pin<
    Box<dyn std::future::Future<Output = Result<Download, String>> + Send>,
>;

pub type Fetcher = dyn Fn(&'static str) -> FetchFuture + Send + Sync;

pub enum Download {
    Network(reqwest::Response),
    #[cfg(feature = "runtime-fixture")]
    #[cfg_attr(feature = "runtime-fixture", allow(dead_code))]
    Bytes(std::vec::IntoIter<Vec<u8>>),
}

impl Download {
    pub async fn next_chunk(&mut self) -> Option<Result<Vec<u8>, String>> {
        match self {
            Self::Network(response) => response
                .chunk()
                .await
                .map_err(|error| error.to_string())
                .transpose()
                .map(|chunk| chunk.map(|bytes| bytes.to_vec())),
            #[cfg(feature = "runtime-fixture")]
            Self::Bytes(chunks) => chunks.next().map(Ok),
        }
    }
}

pub fn network_fetch(url: &'static str) -> FetchFuture {
    Box::pin(async move {
        let client = reqwest::Client::builder()
            .user_agent(concat!("quantix/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|error| error.to_string())?;
        let response = client
            .get(url)
            .send()
            .await
            .and_then(|response| response.error_for_status())
            .map_err(|error| error.to_string())?;
        Ok(Download::Network(response))
    })
}

pub async fn prepare_release_from(
    application_home: &Path,
    release: &CodexRelease,
    fetch: &Fetcher,
    cancellation: CancellationToken,
) -> Result<ManagedCodexRuntimeStatus, ManagedRuntimeError> {
    let binary = codex_binary_path(application_home, release.version);
    if let Some(digest) = binary_sha256(&binary) {
        if digest == release.sha256 {
            return Ok(inspect_release(application_home, release));
        }
    }
    let staging = staging_path(application_home, release.version);
    let directory = staging.parent().expect("staging has a parent");
    if staging.is_file() {
        fs::remove_file(&staging).map_err(ManagedRuntimeError::Io)?;
    }
    if let Some(parent) = directory.parent() {
        fs::create_dir_all(parent).map_err(ManagedRuntimeError::Io)?;
    }
    fs::create_dir_all(directory).map_err(ManagedRuntimeError::Io)?;
    if let Err(error) = download_to_staging(release, fetch, &staging, &cancellation).await {
        let _ = fs::remove_file(&staging);
        return Err(error);
    }
    let digest = binary_sha256(&staging).ok_or(ManagedRuntimeError::IntegrityFailed)?;
    if digest != release.sha256 {
        let _ = fs::remove_file(&staging);
        return Err(ManagedRuntimeError::IntegrityFailed);
    }
    if binary.is_file() {
        fs::remove_file(&binary).map_err(ManagedRuntimeError::Io)?;
    }
    fs::rename(&staging, &binary).map_err(ManagedRuntimeError::Io)?;
    let provenance = serde_json::json!({
        "version": release.version,
        "sha256": release.sha256,
        "installed_at_epoch_ms": SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|since| since.as_millis() as u64)
            .unwrap_or_default(),
    });
    fs::write(
        codex_runtime_directory(application_home, release.version).join(PROVENANCE_FILE),
        provenance.to_string(),
    )
    .map_err(ManagedRuntimeError::Io)?;
    Ok(inspect_release(application_home, release))
}

async fn download_to_staging(
    release: &CodexRelease,
    fetch: &Fetcher,
    staging: &Path,
    cancellation: &CancellationToken,
) -> Result<(), ManagedRuntimeError> {
    let mut last_error = String::from("both download sources failed");
    'attempts: for url in [release.primary_url, release.fallback_url] {
        let mut download = match fetch(url).await {
            Ok(download) => download,
            Err(error) => {
                last_error = error;
                continue 'attempts;
            }
        };
        let partial = staging.with_extension(format!("partial-{}", url.len()));
        let mut output = match fs::File::create(&partial) {
            Ok(output) => output,
            Err(error) => return Err(ManagedRuntimeError::Io(error)),
        };
        let mut total: u64 = 0;
        use std::io::Write;
        loop {
            if cancellation.is_cancelled() {
                let _ = fs::remove_file(&partial);
                return Err(ManagedRuntimeError::Cancelled);
            }
            match download.next_chunk().await {
                Some(Ok(chunk)) => {
                    total += chunk.len() as u64;
                    if total > MAX_BINARY_BYTES {
                        let _ = fs::remove_file(&partial);
                        return Err(ManagedRuntimeError::DownloadFailed(
                            "download exceeded the size limit".to_owned(),
                        ));
                    }
                    if let Err(error) = output.write_all(&chunk) {
                        let _ = fs::remove_file(&partial);
                        return Err(ManagedRuntimeError::Io(error));
                    }
                }
                Some(Err(error)) => {
                    let _ = fs::remove_file(&partial);
                    last_error = error;
                    continue 'attempts;
                }
                None => break,
            }
        }
        if let Err(error) = output.flush() {
            let _ = fs::remove_file(&partial);
            return Err(ManagedRuntimeError::Io(error));
        }
        drop(output);
        if let Err(error) = fs::rename(&partial, staging) {
            let _ = fs::remove_file(&partial);
            return Err(ManagedRuntimeError::Io(error));
        }
        return Ok(());
    }
    Err(ManagedRuntimeError::DownloadFailed(last_error))
}

fn binary_sha256(path: &Path) -> Option<String> {
    let bytes = fs::read(path).ok()?;
    Some(hex_sha256(&bytes))
}

fn hex_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write;
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blob_fetch(bytes: Vec<u8>) -> impl Fn(&'static str) -> FetchFuture + Send + Sync {
        move |_| {
            let chunks: Vec<Vec<u8>> = bytes
                .chunks(64 * 1024)
                .map(<[u8]>::to_vec)
                .collect();
            Box::pin(async move { Ok(Download::Bytes(chunks.into_iter())) }) as FetchFuture
        }
    }

    fn failing_fetch() -> impl Fn(&'static str) -> FetchFuture + Send + Sync {
        |_| Box::pin(async { Err("unreachable".to_owned()) }) as FetchFuture
    }

    fn test_release() -> (CodexRelease, Vec<u8>) {
        let bytes: Vec<u8> = (0..256 * 1024u32).map(|index| (index % 251) as u8).collect();
        let release = CodexRelease {
            version: "test-1.0.0",
            primary_url: "https://example.invalid/primary",
            fallback_url: "https://example.invalid/fallback",
            sha256: Box::leak(hex_sha256(&bytes).into_boxed_str()),
        };
        (release, bytes)
    }

    fn home() -> PathBuf {
        let base = std::env::temp_dir().join(format!(
            "quantix-managed-runtime-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&base).unwrap();
        base
    }

    #[tokio::test]
    async fn fresh_home_installs_and_short_circuits() {
        let home = home();
        let (release, bytes) = test_release();
        assert_eq!(
            inspect_release(&home, &release).state,
            ManagedCodexRuntimeState::NotInstalled
        );
        let fetch = blob_fetch(bytes.clone());
        let status = prepare_release_from(&home, &release, &fetch, CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(status.state, ManagedCodexRuntimeState::Ready);
        let fetch_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let counted_calls = std::sync::Arc::clone(&fetch_calls);
        let counted = move |_url: &'static str| {
            counted_calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            blob_fetch(bytes.clone())("x")
        };
        let status = prepare_release_from(&home, &release, &counted, CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(status.state, ManagedCodexRuntimeState::Ready);
        assert_eq!(
            fetch_calls.load(std::sync::atomic::Ordering::SeqCst),
            0
        );
        assert!(codex_runtime_directory(&home, release.version)
            .join(PROVENANCE_FILE)
            .is_file());
    }

    #[tokio::test]
    async fn hash_mismatch_fails_closed() {
        let home = home();
        let (release, mut bytes) = test_release();
        bytes[0] ^= 0xFF;
        let fetch = blob_fetch(bytes);
        let error = prepare_release_from(&home, &release, &fetch, CancellationToken::new())
            .await
            .unwrap_err();
        assert!(matches!(error, ManagedRuntimeError::IntegrityFailed));
        assert!(!codex_binary_path(&home, release.version).exists());
        assert!(!staging_path(&home, release.version).exists());
    }

    #[tokio::test]
    async fn cancellation_cleans_staging() {
        let home = home();
        let (release, bytes) = test_release();
        let fetch = blob_fetch(bytes);
        let cancellation = CancellationToken::new();
        cancellation.cancel();
        let error = prepare_release_from(&home, &release, &fetch, cancellation)
            .await
            .unwrap_err();
        assert!(matches!(error, ManagedRuntimeError::Cancelled));
        assert!(!staging_path(&home, release.version).exists());
        assert!(!codex_binary_path(&home, release.version).exists());
    }

    #[tokio::test]
    async fn leftover_staging_reports_interrupted_then_converges() {
        let home = home();
        let (release, bytes) = test_release();
        fs::create_dir_all(codex_runtime_directory(&home, release.version)).unwrap();
        fs::write(staging_path(&home, release.version), b"junk").unwrap();
        assert_eq!(
            inspect_release(&home, &release).state,
            ManagedCodexRuntimeState::InterruptedPreparation
        );
        let fetch = blob_fetch(bytes);
        let status = prepare_release_from(&home, &release, &fetch, CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(status.state, ManagedCodexRuntimeState::Ready);
    }

    #[tokio::test]
    async fn corrupt_published_binary_is_replaced() {
        let home = home();
        let (release, bytes) = test_release();
        let binary = codex_binary_path(&home, release.version);
        fs::create_dir_all(binary.parent().unwrap()).unwrap();
        let mut corrupt = bytes.clone();
        corrupt[7] ^= 0x5A;
        fs::write(&binary, corrupt).unwrap();
        assert_eq!(
            inspect_release(&home, &release).state,
            ManagedCodexRuntimeState::InstallFailed
        );
        let fetch = blob_fetch(bytes);
        let status = prepare_release_from(&home, &release, &fetch, CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(status.state, ManagedCodexRuntimeState::Ready);
        assert_eq!(
            binary_sha256(&binary).unwrap(),
            release.sha256
        );
    }

    #[tokio::test]
    async fn download_failure_tries_fallback_then_fails() {
        let home = home();
        let (release, _bytes) = test_release();
        let fetch = failing_fetch();
        let error = prepare_release_from(&home, &release, &fetch, CancellationToken::new())
            .await
            .unwrap_err();
        assert!(matches!(error, ManagedRuntimeError::DownloadFailed(_)));
        assert!(!staging_path(&home, release.version).exists());
    }
}
