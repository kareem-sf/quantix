use std::io;
use std::path::Path;

use super::{ChatGptIdentity, IssuedTokens};
use serde::{Deserialize, Serialize};

const STORE_FILE: &str = "auth.json";
const STORE_TMP_FILE: &str = "auth.json.tmp";
const STORE_VERSION: u64 = 1;
const REFRESH_MARGIN_MS: u64 = 120_000;

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

pub(crate) fn save(home: &Path, conn: &StoredConnection) -> io::Result<()> {
    let payload =
        serde_json::to_string(&StoreRecord::from_connection(conn)).map_err(io::Error::other)?;
    let tmp_path = home.join(STORE_TMP_FILE);
    let target_path = home.join(STORE_FILE);
    std::fs::write(&tmp_path, payload)?;
    if std::fs::rename(&tmp_path, &target_path).is_ok() {
        return Ok(());
    }
    match std::fs::remove_file(&target_path) {
        Ok(()) => std::fs::rename(&tmp_path, &target_path),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            std::fs::rename(&tmp_path, &target_path)
        }
        Err(error) => {
            let _ = std::fs::remove_file(&tmp_path);
            Err(error)
        }
    }
}

pub(crate) fn clear(home: &Path) -> io::Result<()> {
    match std::fs::remove_file(home.join(STORE_FILE)) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

pub(crate) fn needs_refresh(conn: &StoredConnection, now_ms: u64) -> bool {
    conn.expires_at_ms <= now_ms.saturating_add(REFRESH_MARGIN_MS)
}

#[cfg(test)]
mod tests {
    use super::{clear, load, needs_refresh, save, LoadState, StoredConnection};
    use crate::chatgpt_oauth::{ChatGptIdentity, IssuedTokens};
    use std::fs;
    use std::path::PathBuf;

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
        }
    }

    #[test]
    fn save_then_load_roundtrips_exact_json_shape() {
        let home = temp_home("roundtrip");
        let connection = sample_connection();

        save(&home, &connection).unwrap();

        let raw = fs::read_to_string(home.join("auth.json")).unwrap();
        assert_eq!(
            raw,
            r#"{"version":1,"access":"access-1","refresh":"refresh-1","id_token":"id-1","expires_at_ms":5000,"account_id":"acc-1","plan_type":"plus"}"#
        );

        match load(&home) {
            LoadState::Connected(loaded) => {
                assert_eq!(loaded.access_token, "access-1");
                assert_eq!(loaded.refresh_token, "refresh-1");
                assert_eq!(loaded.id_token, "id-1");
                assert_eq!(loaded.expires_at_ms, 5_000);
                assert_eq!(loaded.account_id, "acc-1");
                assert_eq!(loaded.plan_type.as_deref(), Some("plus"));
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

        assert!(!home.join("auth.json.tmp").exists());
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
                "wrong-version" => r#"{"version":2,"access":"a","refresh":"b","id_token":"c","expires_at_ms":1,"account_id":"d","plan_type":null}"#.to_string(),
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
        };

        let connection = StoredConnection::from_issued(&issued, &identity, 1_000);

        assert_eq!(connection.access_token, "at");
        assert_eq!(connection.refresh_token, "rt");
        assert_eq!(connection.id_token, "idt");
        assert_eq!(connection.expires_at_ms, 3_601_000);
        assert_eq!(connection.account_id, "acc-9");
        assert_eq!(connection.plan_type.as_deref(), Some("team"));
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
}
