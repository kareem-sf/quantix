use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock, TryLockError};
use std::time::{Duration, Instant};

use super::{extract_identity, ChatGptIdentity, IssuedTokens, TokenClient};
use serde::{Deserialize, Serialize};

const STORE_FILE: &str = "auth.json";
const STORE_VERSION: u64 = 2;
const REFRESH_MARGIN_MS: u64 = 120_000;

static CONNECTION_MUTATION_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
static TEMPORARY_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);
const MUTATION_LOCK_POLL: Duration = Duration::from_millis(10);

/// Serializes a complete connection mutation. Refresh/login/disconnect callers
/// hold this boundary from their initial read until the final save or clear so
/// a rotating refresh token cannot be used by a competing operation.
pub(crate) fn with_connection_mutation<T>(operation: impl FnOnce() -> T) -> T {
    let lock = connection_mutation_lock();
    let _guard = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    operation()
}

/// Acquires the same serialization boundary without allowing a bounded flow
/// to wait beyond its deadline for an unrelated connection mutation.
pub(crate) fn with_connection_mutation_before<T>(
    deadline: Instant,
    operation: impl FnOnce() -> T,
) -> Option<T> {
    let lock = connection_mutation_lock();
    loop {
        match lock.try_lock() {
            Ok(_guard) => return Some(operation()),
            Err(TryLockError::Poisoned(poisoned)) => {
                let _guard = poisoned.into_inner();
                return Some(operation());
            }
            Err(TryLockError::WouldBlock) => {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return None;
                }
                std::thread::sleep(remaining.min(MUTATION_LOCK_POLL));
            }
        }
    }
}

fn connection_mutation_lock() -> &'static Mutex<()> {
    CONNECTION_MUTATION_LOCK.get_or_init(|| Mutex::new(()))
}

#[derive(Debug)]
pub(crate) enum LoadState {
    Connected(Box<StoredConnection>),
    Absent,
    Unusable,
}

pub(crate) struct StoredConnection {
    pub access_token: String,
    pub refresh_token: String,
    pub id_token: String,
    pub expires_at_ms: u64,
    pub account_id: String,
    pub plan_type: Option<String>,
    pub compute_residency: Option<String>,
}

impl std::fmt::Debug for StoredConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoredConnection")
            .field("access_token_len", &self.access_token.len())
            .field("refresh_token_len", &self.refresh_token.len())
            .field("id_token_len", &self.id_token.len())
            .field("expires_at_ms", &self.expires_at_ms)
            .field("account_id", &self.account_id)
            .field("plan_type", &self.plan_type)
            .field("compute_residency", &self.compute_residency)
            .finish()
    }
}

#[derive(Serialize, Deserialize)]
struct StoreRecord {
    version: u64,
    access: String,
    refresh: String,
    id_token: String,
    expires_at_ms: u64,
    account_id: String,
    plan_type: Option<String>,
    compute_residency: Option<String>,
}

impl StoreRecord {
    fn from_connection(conn: &StoredConnection) -> Self {
        Self {
            version: STORE_VERSION,
            access: conn.access_token.clone(),
            refresh: conn.refresh_token.clone(),
            id_token: conn.id_token.clone(),
            expires_at_ms: conn.expires_at_ms,
            account_id: conn.account_id.clone(),
            plan_type: conn.plan_type.clone(),
            compute_residency: conn.compute_residency.clone(),
        }
    }

    fn into_connection(self) -> StoredConnection {
        StoredConnection {
            access_token: self.access,
            refresh_token: self.refresh,
            id_token: self.id_token,
            expires_at_ms: self.expires_at_ms,
            account_id: self.account_id,
            plan_type: self.plan_type,
            compute_residency: self.compute_residency,
        }
    }
}

impl StoredConnection {
    pub(crate) fn from_issued(
        issued: &IssuedTokens,
        identity: &ChatGptIdentity,
        now_ms: u64,
    ) -> Self {
        Self {
            access_token: issued.access_token.clone(),
            refresh_token: issued.refresh_token.clone(),
            id_token: issued.id_token.clone(),
            expires_at_ms: now_ms.saturating_add(issued.expires_in_secs.saturating_mul(1_000)),
            account_id: identity.account_id.clone(),
            plan_type: identity.plan_type.clone(),
            compute_residency: identity.compute_residency.clone(),
        }
    }
}

pub(crate) fn load(home: &Path) -> LoadState {
    let raw = match std::fs::read_to_string(home.join(STORE_FILE)) {
        Ok(raw) => raw,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return LoadState::Absent,
        Err(_) => return LoadState::Unusable,
    };
    match serde_json::from_str::<StoreRecord>(&raw) {
        Ok(record) if record.version == STORE_VERSION => {
            LoadState::Connected(Box::new(record.into_connection()))
        }
        _ => LoadState::Unusable,
    }
}

#[cfg(test)]
pub(crate) fn save(home: &Path, conn: &StoredConnection) -> io::Result<()> {
    with_connection_mutation(|| save_unlocked(home, conn))
}

pub(crate) fn save_unlocked(home: &Path, conn: &StoredConnection) -> io::Result<()> {
    let payload =
        serde_json::to_string(&StoreRecord::from_connection(conn)).map_err(io::Error::other)?;
    let tmp_path = unique_temporary_path(home);
    let target_path = home.join(STORE_FILE);
    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&tmp_path)?;
        file.write_all(payload.as_bytes())?;
        file.flush()?;
        file.sync_all()?;
        drop(file);
        atomic_replace_file(&target_path, &tmp_path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp_path);
    }
    result
}

#[cfg(test)]
pub(crate) fn clear(home: &Path) -> io::Result<()> {
    with_connection_mutation(|| clear_unlocked(home))
}

pub(crate) fn clear_unlocked(home: &Path) -> io::Result<()> {
    match std::fs::remove_file(home.join(STORE_FILE)) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

/// Refreshes the currently persisted connection as one serialized operation.
/// Callers retry with the returned value; a stale in-memory token is
/// deliberately not an input because another operation may have rotated it.
#[cfg_attr(feature = "runtime-fixture", allow(dead_code))]
pub(crate) fn refresh_connection(
    home: &Path,
    token_client: &TokenClient,
    now_ms: u64,
) -> Result<StoredConnection, ()> {
    with_connection_mutation(|| refresh_connection_unlocked(home, token_client, now_ms))
}

/// Refreshes while the caller already owns the connection mutation boundary.
/// This keeps auth observation, any network refresh, and a related settings
/// projection inside one lock without recursively acquiring the global mutex.
pub(crate) fn refresh_connection_unlocked(
    home: &Path,
    token_client: &TokenClient,
    now_ms: u64,
) -> Result<StoredConnection, ()> {
    let current = match load(home) {
        LoadState::Connected(connection) => *connection,
        LoadState::Absent | LoadState::Unusable => return Err(()),
    };
    if !needs_refresh(&current, now_ms) {
        return Ok(current);
    }
    let issued = token_client
        .refresh(&current.refresh_token)
        .map_err(|_| ())?;
    let identity = extract_identity(&issued.id_token).ok_or(())?;
    let refreshed = StoredConnection::from_issued(&issued, &identity, now_ms);
    save_unlocked(home, &refreshed).map_err(|_| ())?;
    Ok(refreshed)
}

fn unique_temporary_path(home: &Path) -> PathBuf {
    home.join(format!(
        ".auth.json.{}.{}.tmp",
        std::process::id(),
        TEMPORARY_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed),
    ))
}

#[cfg(windows)]
fn atomic_replace_file(target: &Path, replacement: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Storage::FileSystem::{
        MoveFileExW, ReplaceFileW, MOVEFILE_WRITE_THROUGH, REPLACEFILE_WRITE_THROUGH,
    };
    use windows_core::PCWSTR;

    let target_wide = target
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let replacement_wide = replacement
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let result = if target.exists() {
        unsafe {
            ReplaceFileW(
                PCWSTR(target_wide.as_ptr()),
                PCWSTR(replacement_wide.as_ptr()),
                PCWSTR::null(),
                REPLACEFILE_WRITE_THROUGH,
                None,
                None,
            )
        }
    } else {
        unsafe {
            MoveFileExW(
                PCWSTR(replacement_wide.as_ptr()),
                PCWSTR(target_wide.as_ptr()),
                MOVEFILE_WRITE_THROUGH,
            )
        }
    };
    result.map_err(|error| io::Error::other(error.to_string()))
}

#[cfg(not(windows))]
fn atomic_replace_file(target: &Path, replacement: &Path) -> io::Result<()> {
    std::fs::rename(replacement, target)
}

pub(crate) fn needs_refresh(conn: &StoredConnection, now_ms: u64) -> bool {
    conn.expires_at_ms <= now_ms.saturating_add(REFRESH_MARGIN_MS)
}

#[cfg(test)]
mod tests {
    use super::{clear, load, needs_refresh, save, LoadState, StoredConnection};
    use crate::chatgpt_oauth::{refresh_connection, ChatGptIdentity, IssuedTokens, TokenClient};
    use std::fs;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    fn temp_home(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "quantix-chatgpt-store-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sample_connection() -> StoredConnection {
        StoredConnection {
            access_token: "access-1".to_string(),
            refresh_token: "refresh-1".to_string(),
            id_token: "id-1".to_string(),
            expires_at_ms: 5_000,
            account_id: "acc-1".to_string(),
            plan_type: Some("plus".to_string()),
            compute_residency: Some("us".to_string()),
        }
    }

    fn jwt_id_token(
        account_id: &str,
        plan_type: Option<&str>,
        compute_residency: Option<&str>,
    ) -> String {
        let header =
            crate::chatgpt_oauth::crypto::base64url_encode(br#"{"alg":"RS256","typ":"JWT"}"#);
        let payload = crate::chatgpt_oauth::crypto::base64url_encode(
            serde_json::json!({
                "chatgpt_account_id": account_id,
                "https://api.openai.com/auth": {
                    "chatgpt_plan_type": plan_type,
                    "chatgpt_compute_residency": compute_residency,
                },
            })
            .to_string()
            .as_bytes(),
        );
        format!("{header}.{payload}.signature")
    }

    #[test]
    fn save_then_load_roundtrips_exact_json_shape() {
        let home = temp_home("roundtrip");
        let connection = sample_connection();

        save(&home, &connection).unwrap();

        let raw = fs::read_to_string(home.join("auth.json")).unwrap();
        assert_eq!(
            raw,
            r#"{"version":2,"access":"access-1","refresh":"refresh-1","id_token":"id-1","expires_at_ms":5000,"account_id":"acc-1","plan_type":"plus","compute_residency":"us"}"#
        );

        match load(&home) {
            LoadState::Connected(loaded) => {
                assert_eq!(loaded.access_token, "access-1");
                assert_eq!(loaded.refresh_token, "refresh-1");
                assert_eq!(loaded.id_token, "id-1");
                assert_eq!(loaded.expires_at_ms, 5_000);
                assert_eq!(loaded.account_id, "acc-1");
                assert_eq!(loaded.plan_type.as_deref(), Some("plus"));
                assert_eq!(loaded.compute_residency.as_deref(), Some("us"));
            }
            other => panic!("expected connected state, got {other:?}"),
        }

        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn save_overwrites_existing_file_and_leaves_no_tmp() {
        let home = temp_home("overwrite");
        let mut connection = sample_connection();
        save(&home, &connection).unwrap();

        connection.access_token = "access-2".to_string();
        connection.plan_type = None;
        save(&home, &connection).unwrap();

        assert!(fs::read_dir(&home).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".tmp")
        }));
        match load(&home) {
            LoadState::Connected(loaded) => {
                assert_eq!(loaded.access_token, "access-2");
                assert_eq!(loaded.plan_type, None);
            }
            other => panic!("expected connected state, got {other:?}"),
        }

        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn clear_removes_file_and_is_idempotent() {
        let home = temp_home("clear");
        save(&home, &sample_connection()).unwrap();

        clear(&home).unwrap();
        assert!(!home.join("auth.json").exists());
        assert!(matches!(load(&home), LoadState::Absent));

        clear(&home).unwrap();

        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn missing_home_reports_absent() {
        let home = temp_home("absent");

        assert!(matches!(load(&home), LoadState::Absent));

        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn corrupt_files_report_unusable_without_deletion() {
        for name in ["truncated", "garbage", "wrong-version", "missing-fields"] {
            let home = temp_home(name);
            let body = match name {
                "truncated" => r#"{"version":1,"access":"a","refr"#.to_string(),
                "garbage" => "<html>not json</html>".to_string(),
                "wrong-version" => r#"{"version":3,"access":"a","refresh":"b","id_token":"c","expires_at_ms":1,"account_id":"d","plan_type":null,"compute_residency":null}"#.to_string(),
                _ => r#"{"version":1,"access":"a"}"#.to_string(),
            };
            fs::write(home.join("auth.json"), body).unwrap();

            assert!(matches!(load(&home), LoadState::Unusable), "case {name}");
            assert!(home.join("auth.json").exists(), "case {name}");

            let _ = fs::remove_dir_all(&home);
        }
    }

    #[test]
    fn from_issued_computes_expiry_from_now() {
        let issued = IssuedTokens {
            access_token: "at".to_string(),
            refresh_token: "rt".to_string(),
            id_token: "idt".to_string(),
            expires_in_secs: 3_600,
        };
        let identity = ChatGptIdentity {
            account_id: "acc-9".to_string(),
            plan_type: Some("team".to_string()),
            compute_residency: Some("eu".to_string()),
        };

        let connection = StoredConnection::from_issued(&issued, &identity, 1_000);

        assert_eq!(connection.access_token, "at");
        assert_eq!(connection.refresh_token, "rt");
        assert_eq!(connection.id_token, "idt");
        assert_eq!(connection.expires_at_ms, 3_601_000);
        assert_eq!(connection.account_id, "acc-9");
        assert_eq!(connection.plan_type.as_deref(), Some("team"));
        assert_eq!(connection.compute_residency.as_deref(), Some("eu"));
    }

    #[test]
    fn needs_refresh_true_when_one_hundred_twenty_seconds_or_less_remain() {
        let mut connection = sample_connection();

        connection.expires_at_ms = 200_000;
        assert!(!needs_refresh(&connection, 80_000 - 1));
        assert!(needs_refresh(&connection, 80_000));
        assert!(needs_refresh(&connection, 80_000 + 1));
        assert!(needs_refresh(&connection, 200_000));
        assert!(needs_refresh(&connection, 300_000));
    }

    #[test]
    fn serialized_refresh_reloads_and_persists_the_rotated_connection() {
        let home = temp_home("serialized-refresh");
        save(&home, &sample_connection()).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let issuer = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut buffer = [0_u8; 512];
            loop {
                let read = stream.read(&mut buffer).unwrap();
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..read]);
                if request.windows(4).any(|window| window == b"\r\n\r\n") {
                    break;
                }
            }
            let body = serde_json::json!({
                "access_token": "access-2",
                "refresh_token": "refresh-2",
                "id_token": jwt_id_token("acc-2", Some("team"), Some("eu")),
                "expires_in": 3600,
            })
            .to_string();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).unwrap();
            String::from_utf8(request).unwrap()
        });

        let client = TokenClient::new(&format!("http://127.0.0.1:{port}")).unwrap();
        let refreshed = refresh_connection(&home, &client, 1_000_000).unwrap();

        assert_eq!(refreshed.access_token, "access-2");
        assert_eq!(refreshed.refresh_token, "refresh-2");
        assert_eq!(refreshed.account_id, "acc-2");
        assert_eq!(refreshed.plan_type.as_deref(), Some("team"));
        assert_eq!(refreshed.compute_residency.as_deref(), Some("eu"));
        assert_eq!(refreshed.expires_at_ms, 4_600_000);
        assert!(issuer.join().unwrap().contains("refresh_token=refresh-1"));
        match load(&home) {
            LoadState::Connected(persisted) => {
                assert_eq!(persisted.refresh_token, "refresh-2");
                assert_eq!(persisted.account_id, "acc-2");
                assert_eq!(persisted.plan_type.as_deref(), Some("team"));
                assert_eq!(persisted.compute_residency.as_deref(), Some("eu"));
            }
            state => panic!("expected the rotated connection, got {state:?}"),
        }
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn bounded_connection_mutation_wait_stops_at_its_deadline() {
        let started = Instant::now();
        let waiter = with_connection_mutation(|| {
            std::thread::spawn(move || {
                super::with_connection_mutation_before(started + Duration::from_millis(50), || {
                    "entered"
                })
            })
            .join()
            .unwrap()
        });

        assert!(waiter.is_none());
        assert!(started.elapsed() < Duration::from_millis(500));
    }
}
