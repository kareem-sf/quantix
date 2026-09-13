use super::*;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

const RESET_ID: &str = "1234567890abcdef1234567890abcdef";

fn temporary() -> tempfile::TempDir {
    let target = Path::new(env!("CARGO_MANIFEST_DIR")).join("target");
    fs::create_dir_all(&target).unwrap();
    tempfile::Builder::new()
        .prefix("reset-test-")
        .tempdir_in(target)
        .unwrap()
}

fn stage(directory: &Path) -> PathBuf {
    let root = directory.join("home");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("pending-reset.json"), serde_json::to_vec(&json!({
        "format": 1, "reset_id": RESET_ID, "home": root, "phase": "ready",
        "credentials_cleared": true, "fingerprint": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        "confirmed_at": "2026-09-09T12:00:00Z", "detail": "Ready to close Quantix.",
        "credential_targets": [{"type": 1, "target": "private-owned-metadata"}]
    })).unwrap()).unwrap();
    root
}

fn journal(root: &Path) -> Value {
    serde_json::from_slice(&fs::read(root.join("pending-reset.json")).unwrap()).unwrap()
}

#[test]
fn pinned_directories_deny_write_handles_but_allow_child_file_creation() {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Foundation::ERROR_SHARING_VIOLATION;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE,
        FILE_SHARE_READ, FILE_SHARE_WRITE,
    };
    let directory = temporary();
    let root = stage(directory.path());
    let child = root.join("nested");
    fs::create_dir(&child).unwrap();
    let open_writer = |path: &Path| {
        fs::OpenOptions::new()
            .write(true)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .open(path)
    };
    drop(open_writer(&root).expect("the unpinned fixture allows directory write access"));
    drop(open_writer(&child).expect("the unpinned child allows directory write access"));
    let pinned = platform::pin_root(&child).unwrap();
    for path in [&root, &child] {
        let result = open_writer(path);
        assert!(
            result.is_err(),
            "a write-capable handle must not change a pinned directory into a reparse point"
        );
        assert_eq!(
            result.unwrap_err().raw_os_error(),
            Some(ERROR_SHARING_VIOLATION as i32)
        );
        fs::write(
            path.join("created-while-pinned"),
            b"ordinary child writes remain available",
        )
        .unwrap();
        assert_eq!(
            fs::read(path.join("created-while-pinned")).unwrap(),
            b"ordinary child writes remain available"
        );
    }
    drop(pinned);
    drop(open_writer(&child).expect("closing pins restores normal directory write access"));
    {
        // Descendant traversal uses DELETE rather than LIST access; its
        // sharing restrictions must be effective as well as the root pins.
        let _deleting = platform::deletion_handle(&child).unwrap();
        assert_eq!(
            open_writer(&child).unwrap_err().raw_os_error(),
            Some(ERROR_SHARING_VIOLATION as i32)
        );
        fs::write(
            child.join("created-during-traversal"),
            b"child creation is still allowed",
        )
        .unwrap();
    }
    {
        let _pins = platform::pin_root(&root).unwrap();
        let _lock = platform::workspace_lock(&root.join("workspace.lock")).unwrap();
        platform::write_control(&root, &journal(&root)).unwrap();
        platform::remove_entry(&root.join("created-while-pinned")).unwrap();
    }
    purge(&root, RESET_ID, Duration::from_secs(1)).unwrap();
}

#[test]
fn erases_every_owned_category_and_preserves_neighboring_original() {
    let directory = temporary();
    let root = stage(directory.path());
    let original = directory.path().join("original.xlsx");
    fs::write(&original, b"original supplied file").unwrap();
    for category in [
        "imports",
        "outputs",
        "backups",
        "runtime/webview",
        "logs",
        "cache",
        "ai-runtimes",
        "ai-components",
    ] {
        let folder = root.join(category);
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("private-data"), b"old Quantix data").unwrap();
    }
    fs::write(root.join("quantix.db"), b"old database").unwrap();
    purge(&root, RESET_ID, Duration::from_millis(300)).unwrap();
    assert_eq!(fs::read(original).unwrap(), b"original supplied file");
    let contents: Vec<_> = fs::read_dir(&root)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(contents, ["workspace.lock"]);
    assert_eq!(fs::metadata(root.join("workspace.lock")).unwrap().len(), 0);
}

#[test]
fn refuses_unconfirmed_wrong_id_and_wrong_home_without_deleting() {
    for (field, value) in [
        ("credentials_cleared", json!(false)),
        ("home", json!("C:\\wrong-home")),
        ("reset_id", json!("another-id")),
        ("phase", json!("credential_error")),
    ] {
        let directory = temporary();
        let root = stage(directory.path());
        let mut saved = journal(&root);
        saved[field] = value;
        fs::write(
            root.join("pending-reset.json"),
            serde_json::to_vec(&saved).unwrap(),
        )
        .unwrap();
        fs::write(root.join("private-data"), b"retained").unwrap();
        assert!(purge(&root, RESET_ID, Duration::ZERO).is_err());
        assert_eq!(fs::read(root.join("private-data")).unwrap(), b"retained");
        assert_eq!(journal(&root), saved);
    }
}

#[test]
fn locked_file_retains_failure_and_metadata_then_explicit_retry_finishes() {
    use std::os::windows::fs::OpenOptionsExt;
    let directory = temporary();
    let root = stage(directory.path());
    let private_file = root.join("private-data");
    fs::write(&private_file, b"retained until closed").unwrap();
    let held = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&private_file)
        .unwrap();
    assert!(purge(&root, RESET_ID, Duration::from_millis(25)).is_err());
    let saved = journal(&root);
    assert_eq!(saved["phase"], "failed");
    assert_eq!(saved["credentials_cleared"], true);
    assert_eq!(
        saved["credential_targets"][0]["target"],
        "private-owned-metadata"
    );
    assert!(private_file.exists());
    drop(held);
    purge(&root, RESET_ID, Duration::from_millis(300)).unwrap();
    assert!(!private_file.exists());
    assert!(!root.join("pending-reset.json").exists());
}

#[test]
fn workspace_lock_blocks_purge_until_backend_releases() {
    let directory = temporary();
    let root = stage(directory.path());
    fs::write(root.join("private-data"), b"retained").unwrap();
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(root.join("workspace.lock"))
        .unwrap();
    lock.lock().unwrap();
    assert!(purge(&root, RESET_ID, Duration::from_millis(25)).is_err());
    assert!(root.join("private-data").exists());
    assert_eq!(journal(&root)["phase"], "ready");
    drop(lock);
    purge(&root, RESET_ID, Duration::from_millis(300)).unwrap();
}

#[test]
fn child_junction_is_removed_without_traversing_its_target() {
    let directory = temporary();
    let root = stage(directory.path());
    let outside = directory.path().join("original-folder");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("original"), b"preserved").unwrap();
    junction(&outside, &root.join("linked-folder"));
    purge(&root, RESET_ID, Duration::from_millis(300)).unwrap();
    assert_eq!(fs::read(outside.join("original")).unwrap(), b"preserved");
    assert!(!root.join("linked-folder").exists());
}

#[test]
fn root_and_journal_links_are_rejected() {
    let directory = temporary();
    let root = stage(directory.path());
    let alias = directory.path().join("linked-home");
    junction(&root, &alias);
    assert!(purge(&alias, RESET_ID, Duration::ZERO).is_err());
    assert!(root.join("pending-reset.json").exists());
    let original = directory.path().join("original-journal");
    fs::rename(root.join("pending-reset.json"), &original).unwrap();
    fs::hard_link(&original, root.join("pending-reset.json")).unwrap();
    assert!(purge(&root, RESET_ID, Duration::ZERO).is_err());
    assert!(original.exists());
}

fn junction(target: &Path, link: &Path) {
    let status = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", "$ErrorActionPreference = 'Stop'; New-Item -ItemType Junction -Path $env:QUANTIX_TEST_LINK -Target $env:QUANTIX_TEST_TARGET | Out-Null"])
        .env("QUANTIX_TEST_LINK", link).env("QUANTIX_TEST_TARGET", target)
        .status().unwrap();
    assert!(status.success());
}

#[test]
fn read_only_hard_link_is_removed_without_changing_original() {
    let directory = temporary();
    let root = stage(directory.path());
    let original = directory.path().join("original");
    fs::write(&original, b"preserved").unwrap();
    fs::hard_link(&original, root.join("imported-link")).unwrap();
    let mut permissions = fs::metadata(&original).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&original, permissions).unwrap();
    let result = purge(&root, RESET_ID, Duration::from_millis(300));
    assert!(fs::metadata(&original).unwrap().permissions().readonly());
    assert_eq!(fs::read(&original).unwrap(), b"preserved");
    // The test fixture owns this original; release its attribute for TempDir cleanup.
    // This Windows-only test clears the read-only attribute; no Unix mode bits apply.
    let mut permissions = fs::metadata(&original).unwrap().permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    permissions.set_readonly(false);
    fs::set_permissions(&original, permissions).unwrap();
    result.unwrap();
}

fn python(script: &str, root: &Path) -> std::process::Command {
    use std::os::windows::process::CommandExt;
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut command = std::process::Command::new(project.join("backend/.venv/Scripts/python.exe"));
    command
        .args(["-c", script])
        .arg(root)
        .current_dir(project)
        .env("PYTHONPATH", project.join("backend"))
        .creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
    command
}

#[test]
fn native_reset_then_real_repository_has_zero_tenders_accounts_and_default_settings() {
    let directory = temporary();
    let root = stage(directory.path());
    let seeded = python(r#"
import sys
from pathlib import Path
from quantix.repository import Repository
from quantix.ai_connections import AIConnectionService
repo = Repository(Path(sys.argv[1]))
repo.create_tender('Synthetic reset acceptance')
repo.set_setting('default_currency', 'USD')
repo.set_setting('preferences', 'Synthetic old preferences')
AIConnectionService(repo)
with repo.db.connect(write=True) as conn:
    conn.execute("INSERT INTO ai_connections VALUES(?,?,?,?)", ('synthetic-account', '{}', None, 'missing'))
assert len(repo.list_tenders()) == 1
for name in ['objects', 'extractions', 'outputs', 'backups', 'logs', 'cache', 'runtime/webview', 'ai-components', 'ai-runtimes']:
    folder = repo.home / name
    folder.mkdir(parents=True, exist_ok=True)
    (folder / 'synthetic-private-data').write_bytes(b'Synthetic private data')
"#, &root).output().unwrap();
    assert!(
        seeded.status.success(),
        "{}",
        String::from_utf8_lossy(&seeded.stderr)
    );
    purge(&root, RESET_ID, Duration::from_secs(3)).unwrap();
    assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
    let fresh = python(r#"
import json, sys
from pathlib import Path
from quantix.repository import Repository
from quantix.settings import SettingsService
from quantix.ai_connections import AIConnectionService
repo = Repository(Path(sys.argv[1]))
assert repo.list_tenders() == []
assert AIConnectionService(repo).list() == []
settings = SettingsService(repo).public()
assert settings.default_currency == 'EGP'
assert settings.preferences == ''
assert settings.provider_ready is False
assert not (repo.home / 'pending-reset.json').exists()
print(json.dumps({'tenders': 0, 'accounts': 0, 'currency': settings.default_currency, 'preferences': settings.preferences}))
"#, &root).output().unwrap();
    assert!(
        fresh.status.success(),
        "{}",
        String::from_utf8_lossy(&fresh.stderr)
    );
    let result: Value = serde_json::from_slice(&fresh.stdout).unwrap();
    assert_eq!(
        result,
        json!({"tenders": 0, "accounts": 0, "currency": "EGP", "preferences": ""})
    );
}

#[test]
fn installed_python_filelock_blocks_native_until_its_process_closes() {
    use std::io::{BufRead, BufReader, Write};
    let directory = temporary();
    let root = stage(directory.path());
    fs::write(
        root.join("private-data"),
        b"retained while backend owns lock",
    )
    .unwrap();
    let mut child = python(
        r#"
import sys
from pathlib import Path
from filelock import FileLock
with FileLock(Path(sys.argv[1]) / 'workspace.lock'):
    print('locked', flush=True)
    sys.stdin.readline()
"#,
        &root,
    )
    .stdin(std::process::Stdio::piped())
    .stdout(std::process::Stdio::piped())
    .spawn()
    .unwrap();
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    assert_eq!(line.trim(), "locked");
    let blocked = purge(&root, RESET_ID, Duration::from_millis(25));
    child.stdin.take().unwrap().write_all(b"release\n").unwrap();
    assert!(child.wait().unwrap().success());
    assert!(blocked.is_err());
    assert!(root.join("private-data").exists());
    assert_eq!(journal(&root)["phase"], "ready");
    purge(&root, RESET_ID, Duration::from_secs(1)).unwrap();
}

#[test]
fn competing_helper_does_not_overwrite_owner_or_recreate_completed_journal() {
    use std::os::windows::fs::OpenOptionsExt;
    let directory = temporary();
    let root = stage(directory.path());
    let data = root.join("private-data");
    fs::write(&data, b"old data").unwrap();
    let held = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&data)
        .unwrap();
    let owner_root = root.clone();
    let owner =
        std::thread::spawn(move || purge(&owner_root, RESET_ID, Duration::from_millis(300)));
    let until = std::time::Instant::now() + Duration::from_secs(2);
    while std::time::Instant::now() < until {
        if read_journal(&root)
            .ok()
            .flatten()
            .is_some_and(|saved| saved["phase"] == "deleting")
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(journal(&root)["phase"], "deleting");
    assert!(purge(&root, RESET_ID, Duration::from_millis(25)).is_err());
    assert_eq!(
        journal(&root)["phase"],
        "deleting",
        "a lock waiter must not replace the active owner's journal"
    );
    assert!(owner.join().unwrap().is_err());
    assert_eq!(journal(&root)["phase"], "failed");
    drop(held);
    let peer_root = root.clone();
    let peer = std::thread::spawn(move || purge(&peer_root, RESET_ID, Duration::from_secs(2)));
    let _ = purge(&root, RESET_ID, Duration::from_secs(2));
    let _ = peer.join().unwrap();
    assert!(!root.join("pending-reset.json").exists());
    assert!(!data.exists());
}
