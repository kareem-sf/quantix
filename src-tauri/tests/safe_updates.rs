use std::{
    fs, io,
    io::Cursor,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Barrier,
    },
};

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use garde::Validate;
use minisign::KeyPair;
use sha2::Digest;

use quantix_lib::{
    configure_tauri_builder, current_application_artifact_is_restorable, current_update_platform,
    ensure_quantix_setup, perform_authorized_update_restart,
    run_update_rollback_helper_with_launcher, update_platform_from_target,
    verify_signed_update_artifact, verify_signed_update_candidate, CreateTenderBackupCommand,
    CreateTenderCommand, DecideUpdateCommand, DeviceProtection, ImportTenderPackageCommand,
    InstalledApplicationArtifactKind, InstalledApplicationArtifactSet, ParseSourceArtifactCommand,
    QuantixHost, RegisterTenderContentCommand, RunBootstrapAgentCommand, RuntimeLayout,
    RuntimeReadinessState, SetupIssue, SetupPlatform, SetupState, SignedArtifactIdentity,
    StoragePermissions, TenderBackupState, TenderErrorCode, UpdateCandidate,
    UpdateCompatibilityManifest, UpdateDecision, UpdateDiagnostic, UpdateImpact, UpdatePlatform,
    UpdateReleaseInformation, UpdateState, MINIMUM_SETUP_FREE_SPACE_BYTES,
};

struct ReadySetupPlatform;

struct BlockingBackupSetupPlatform {
    block_backup: AtomicBool,
    backup_started: Barrier,
    release_backup: Barrier,
}

struct TamperableStorageSetupPlatform {
    unsafe_permissions: AtomicBool,
}

impl TamperableStorageSetupPlatform {
    fn new() -> Self {
        Self {
            unsafe_permissions: AtomicBool::new(false),
        }
    }
}

impl BlockingBackupSetupPlatform {
    fn new() -> Self {
        Self {
            block_backup: AtomicBool::new(false),
            backup_started: Barrier::new(2),
            release_backup: Barrier::new(2),
        }
    }
}

impl SetupPlatform for ReadySetupPlatform {
    fn available_space(&self, _path: &Path) -> io::Result<u64> {
        Ok(MINIMUM_SETUP_FREE_SPACE_BYTES * 4)
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

impl SetupPlatform for BlockingBackupSetupPlatform {
    fn available_space(&self, path: &Path) -> io::Result<u64> {
        if path.file_name().is_some_and(|name| name == "backups")
            && self.block_backup.load(Ordering::Acquire)
        {
            self.backup_started.wait();
            self.release_backup.wait();
        }
        Ok(MINIMUM_SETUP_FREE_SPACE_BYTES * 4)
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

impl SetupPlatform for TamperableStorageSetupPlatform {
    fn available_space(&self, _path: &Path) -> io::Result<u64> {
        Ok(MINIMUM_SETUP_FREE_SPACE_BYTES * 4)
    }

    fn is_writable(&self, _path: &Path) -> io::Result<bool> {
        Ok(true)
    }

    fn storage_permissions(&self, _path: &Path) -> io::Result<StoragePermissions> {
        Ok(if self.unsafe_permissions.load(Ordering::Acquire) {
            StoragePermissions::Unsafe
        } else {
            StoragePermissions::Restrictive
        })
    }

    fn device_protection(&self, _path: &Path) -> DeviceProtection {
        DeviceProtection::Protected
    }
}

fn ready_host() -> (tempfile::TempDir, QuantixHost) {
    let root = tempfile::tempdir().expect("temporary user home");
    let host = QuantixHost::with_setup_platform(
        root.path().join(".quantix"),
        Arc::new(ReadySetupPlatform),
    );
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    (root, host)
}

fn valid_offer(data_may_change: bool) -> UpdateCandidate {
    UpdateCandidate {
        current_version: "0.1.0".into(),
        version: "0.2.0".into(),
        platform: current_update_platform().expect("supported test platform"),
        artifact: SignedArtifactIdentity {
            sha256: "a".repeat(64),
            signature_sha256: "b".repeat(64),
        },
        compatibility: UpdateCompatibilityManifest {
            installation_schema_version: 8,
            tender_schema_version: 21,
            codex_version: "0.147.0".into(),
            docling_version: "2.118.0".into(),
            runtime_manifest_schema_version: 2,
        },
        release: UpdateReleaseInformation {
            published_at: "2026-08-12T10:00:00Z".into(),
            title: "Quantix 0.2.0".into(),
            notes: "Signed update with recovery hardening".into(),
        },
        impact: UpdateImpact {
            summary: if data_may_change {
                "Updates the Tender Store schema".into()
            } else {
                "Application-only update".into()
            },
            stored_data_may_change: data_may_change,
        },
    }
}

fn begin_fixture_installation(host: &QuantixHost, update_id: &str) -> std::path::PathBuf {
    let artifact = host
        .application_home()
        .parent()
        .expect("Application Home parent")
        .join(format!("prior-{update_id}.AppImage"));
    fs::write(&artifact, b"authenticated prior application")
        .expect("write prior application fixture");
    host.stage_application_recovery_point(
        update_id,
        "0.1.0",
        &InstalledApplicationArtifactSet {
            kind: InstalledApplicationArtifactKind::LinuxAppImage,
            root: artifact.clone(),
            launcher_relative: None,
        },
    )
    .expect("stage authenticated prior application");
    assert_eq!(
        host.begin_update_installation_after_recovery(update_id)
            .expect("persist Installing only after durable recovery")
            .state,
        UpdateState::Installing
    );
    artifact
}

fn prepare_restart_validation(host: &QuantixHost) -> String {
    let available = host.present_update(valid_offer(false)).expect("offer");
    let update_id = available.offer.expect("offer identity").update_id;
    host.decide_update(
        update_id.clone(),
        UpdateDecision::Approve,
        "Approve the exact release before Host-owned restart validation".into(),
    )
    .expect("approve exact offer");
    host.authorize_update_installation(&update_id)
        .expect("authorize installation");
    begin_fixture_installation(host, &update_id);
    assert_eq!(
        host.record_update_installed(&update_id)
            .expect("persist restart validation gate")
            .state,
        UpdateState::RestartValidationRequired
    );
    update_id
}

fn assert_failed_restart_preserves_recovery(
    host: &QuantixHost,
    update_id: &str,
    status: &quantix_lib::UpdateStatus,
) {
    assert_eq!(status.state, UpdateState::RepairRequired);
    assert_eq!(
        status.diagnostic,
        Some(UpdateDiagnostic::RestartValidationFailed)
    );
    assert!(host
        .application_home()
        .join("update-recovery")
        .join(update_id)
        .exists());
}

fn signed_candidate(candidate: &UpdateCandidate) -> (String, String) {
    let KeyPair { pk, sk } =
        KeyPair::generate_unencrypted_keypair().expect("generate test-only signing key");
    let signature = minisign::sign(
        None,
        &sk,
        Cursor::new(
            candidate
                .canonical_manifest_bytes()
                .expect("canonical update manifest"),
        ),
        None,
        None,
    )
    .expect("sign canonical update manifest")
    .into_string();
    (
        BASE64_STANDARD.encode(pk.to_box().expect("box test public key").to_string()),
        BASE64_STANDARD.encode(signature),
    )
}

#[test]
fn canonical_update_envelope_and_downloaded_artifact_require_the_release_key() {
    let candidate = valid_offer(false);
    let (public_key, signature) = signed_candidate(&candidate);

    verify_signed_update_candidate(&candidate, &signature, &public_key)
        .expect("release-signed canonical manifest");

    let mut tampered = candidate.clone();
    tampered.impact.summary = "forged low-impact description".into();
    assert_eq!(
        verify_signed_update_candidate(&tampered, &signature, &public_key)
            .expect_err("manifest fields are bound to the release signature")
            .diagnostic,
        UpdateDiagnostic::ArtifactTampered
    );

    let (wrong_key, _) = signed_candidate(&candidate);
    assert_eq!(
        verify_signed_update_candidate(&candidate, &signature, &wrong_key)
            .expect_err("another Minisign key cannot authorize the release")
            .diagnostic,
        UpdateDiagnostic::WrongSigningKey
    );
    assert_eq!(
        verify_signed_update_candidate(&candidate, "", &public_key)
            .expect_err("unsigned manifests fail closed")
            .diagnostic,
        UpdateDiagnostic::UnsignedArtifact
    );

    let artifact = b"signed updater artifact";
    let KeyPair { pk, sk } =
        KeyPair::generate_unencrypted_keypair().expect("generate artifact signing key");
    let artifact_signature = minisign::sign(None, &sk, Cursor::new(artifact), None, None)
        .expect("sign artifact")
        .into_string();
    let artifact_public_key = pk.to_box().expect("box artifact key").to_string();
    let artifact_signature = BASE64_STANDARD.encode(artifact_signature);
    let artifact_public_key = BASE64_STANDARD.encode(artifact_public_key);
    let artifact_sha256: String = sha2::Sha256::digest(artifact)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();

    verify_signed_update_artifact(
        artifact,
        &artifact_signature,
        &artifact_public_key,
        &artifact_sha256,
    )
    .expect("artifact signature and identity match");
    assert_eq!(
        verify_signed_update_artifact(
            b"tampered updater artifact",
            &artifact_signature,
            &artifact_public_key,
            &artifact_sha256,
        )
        .expect_err("changed bytes cannot install")
        .diagnostic,
        UpdateDiagnostic::ArtifactTampered
    );
}

#[test]
fn updater_targets_require_an_exact_supported_os_and_architecture_pair() {
    assert!(
        !current_application_artifact_is_restorable(),
        "an unpackaged test executable must not be claimed as a restorable installed bundle"
    );
    assert_eq!(
        update_platform_from_target("windows-x86_64"),
        Some(UpdatePlatform::WindowsX86_64)
    );
    assert_eq!(
        update_platform_from_target("x86_64-pc-windows-msvc"),
        Some(UpdatePlatform::WindowsX86_64)
    );
    assert_eq!(
        update_platform_from_target("darwin-aarch64"),
        Some(UpdatePlatform::MacOsAarch64)
    );
    assert_eq!(
        update_platform_from_target("aarch64-apple-darwin"),
        Some(UpdatePlatform::MacOsAarch64)
    );
    assert_eq!(
        update_platform_from_target("linux-x86_64"),
        Some(UpdatePlatform::UbuntuX86_64)
    );
    assert_eq!(
        update_platform_from_target("x86_64-unknown-linux-gnu"),
        Some(UpdatePlatform::UbuntuX86_64)
    );
    assert_eq!(update_platform_from_target("windows-aarch64"), None);
    assert_eq!(update_platform_from_target("linux-aarch64"), None);
    assert_eq!(update_platform_from_target("darwin-x86_64"), None);
}

#[test]
fn recovery_staging_rejects_an_unprovable_or_incomplete_application_layout() {
    let (root, host) = ready_host();
    let offered = host.present_update(valid_offer(false)).expect("offer");
    let update_id = offered.offer.expect("offer identity").update_id;
    host.decide_update(
        update_id.clone(),
        UpdateDecision::Approve,
        "Approve only if the prior packaged application can be recovered".into(),
    )
    .expect("approve exact update");
    host.authorize_update_installation(&update_id)
        .expect("enter installing state");

    let incomplete = root.path().join("incomplete-installation");
    fs::create_dir(&incomplete).expect("create incomplete bundle");
    fs::write(incomplete.join("quantix.exe"), b"application")
        .expect("write incomplete application");
    assert_eq!(
        host.stage_application_recovery_point(
            &update_id,
            "0.1.0",
            &InstalledApplicationArtifactSet {
                kind: InstalledApplicationArtifactKind::WindowsBundle,
                root: incomplete,
                launcher_relative: Some("missing-launcher.exe".into()),
            },
        )
        .expect_err("an unprovable package layout must fail before installation")
        .diagnostic,
        UpdateDiagnostic::InstallationFailed
    );
    let recursive = root.path().join(".quantix/installed-quantix");
    fs::create_dir_all(recursive.join("runtime")).expect("create nested package layout");
    fs::write(recursive.join("quantix.exe"), b"nested application").expect("write nested launcher");
    assert_eq!(
        host.stage_application_recovery_point(
            &update_id,
            "0.1.0",
            &InstalledApplicationArtifactSet {
                kind: InstalledApplicationArtifactKind::WindowsBundle,
                root: recursive,
                launcher_relative: Some("quantix.exe".into()),
            },
        )
        .expect_err("recovery cannot recursively stage an installation inside Application Home")
        .diagnostic,
        UpdateDiagnostic::InstallationFailed
    );
    begin_fixture_installation(&host, &update_id);
    host.record_update_failure(&update_id, UpdateDiagnostic::InstallationFailed)
        .expect("release the failed fixture installation lease");
}

#[test]
fn authorization_crash_before_recovery_never_persists_an_irreparable_installing_state() {
    let (root, host) = ready_host();
    let application_home = root.path().join(".quantix");
    let offered = host.present_update(valid_offer(false)).expect("offer");
    let update_id = offered.offer.expect("offer identity").update_id;
    host.decide_update(
        update_id.clone(),
        UpdateDecision::Approve,
        "Approve only if recovery becomes durable before Installing".into(),
    )
    .expect("approve exact update");
    assert_eq!(
        host.authorize_update_installation(&update_id)
            .expect("claim exclusive authorization lease")
            .state,
        UpdateState::Approved
    );
    drop(host);

    let restarted =
        QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&restarted).state, SetupState::Ready);
    let status = restarted
        .inspect_update_status()
        .expect("inspect persisted state");
    assert_eq!(status.state, UpdateState::Approved);
    assert!(!application_home
        .join("update-recovery")
        .join(update_id)
        .exists());
}

#[test]
fn recovery_staging_reconciles_only_authenticated_or_precommit_partial_state() {
    let (root, host) = ready_host();
    let application_home = root.path().join(".quantix");
    let offered = host.present_update(valid_offer(false)).expect("offer");
    let update_id = offered.offer.expect("offer identity").update_id;
    host.decide_update(
        update_id.clone(),
        UpdateDecision::Approve,
        "Approve only after recovery staging is durably reconciled".into(),
    )
    .expect("approve exact update");
    host.authorize_update_installation(&update_id)
        .expect("enter installing state");
    let bundle = root.path().join("reconcilable-installation");
    fs::create_dir_all(bundle.join("runtime")).expect("create valid packaged runtime");
    fs::write(bundle.join("quantix.exe"), b"prior application").expect("write launcher");
    fs::write(bundle.join("runtime/resource.bin"), b"prior resource").expect("write resource");
    let staging = application_home
        .join("staging")
        .join(format!("update-recovery-{update_id}"));
    fs::create_dir(&staging).expect("create interrupted precommit staging");
    fs::write(
        staging.join("partial.bin"),
        b"unauthenticated partial state",
    )
    .expect("write interrupted staging bytes");
    let artifacts = InstalledApplicationArtifactSet {
        kind: InstalledApplicationArtifactKind::WindowsBundle,
        root: bundle,
        launcher_relative: Some("quantix.exe".into()),
    };

    host.stage_application_recovery_point(&update_id, "0.1.0", &artifacts)
        .expect("discard precommit partial staging and create an authenticated recovery point");
    let published = application_home.join("update-recovery").join(&update_id);
    fs::create_dir(&staging).expect("create stale staging beside the published recovery point");
    fs::write(staging.join("stale.bin"), b"stale partial state")
        .expect("write stale staging bytes");
    host.stage_application_recovery_point(&update_id, "0.1.0", &artifacts)
        .expect("remove stale staging only after validating the published immutable point");
    assert!(!staging.exists());
    fs::rename(&published, &staging)
        .expect("simulate interruption after committing the immutable recovery fact");
    host.stage_application_recovery_point(&update_id, "0.1.0", &artifacts)
        .expect("publish and reuse the authenticated committed staging point");
    assert!(published.join("manifest.json").is_file());
    assert!(!staging.exists());
    fs::remove_dir_all(&published)
        .expect("simulate loss after the immutable recovery fact was committed");
    host.stage_application_recovery_point(&update_id, "0.1.0", &artifacts)
        .expect("rebuild only bytes that exactly match the immutable recovery fact");
    assert!(published.join("manifest.json").is_file());
}

#[test]
fn macos_app_bundle_and_linux_appimage_recovery_restore_the_complete_supported_artifact() {
    let (mac_root, mac_host) = ready_host();
    let mac_offer = mac_host
        .present_update(valid_offer(false))
        .expect("mac offer");
    let mac_update_id = mac_offer.offer.expect("mac offer identity").update_id;
    mac_host
        .decide_update(
            mac_update_id.clone(),
            UpdateDecision::Approve,
            "Approve the exact recoverable macOS application bundle".into(),
        )
        .expect("approve macOS fixture update");
    mac_host
        .authorize_update_installation(&mac_update_id)
        .expect("authorize macOS fixture install");
    let app = mac_root.path().join("Quantix.app");
    fs::create_dir_all(app.join("Contents/MacOS")).expect("create app executable directory");
    fs::create_dir_all(app.join("Contents/Resources/runtime"))
        .expect("create app resource directory");
    fs::write(
        app.join("Contents/MacOS/quantix"),
        b"prior macOS executable",
    )
    .expect("write macOS executable");
    fs::write(
        app.join("Contents/Resources/runtime/model.bin"),
        b"prior macOS resource",
    )
    .expect("write macOS resource");
    mac_host
        .stage_application_recovery_point(
            &mac_update_id,
            "0.1.0",
            &InstalledApplicationArtifactSet {
                kind: InstalledApplicationArtifactKind::MacOsBundle,
                root: app.clone(),
                launcher_relative: Some("Contents/MacOS/quantix".into()),
            },
        )
        .expect("stage complete macOS app bundle");
    mac_host
        .begin_update_installation_after_recovery(&mac_update_id)
        .expect("persist macOS Installing after durable recovery");
    mac_host
        .record_update_failure(&mac_update_id, UpdateDiagnostic::InstallationInterrupted)
        .expect("require macOS rollback");
    fs::remove_dir_all(&app).expect("simulate deleted macOS app bundle");
    mac_host
        .restore_application_recovery_point(&mac_update_id)
        .expect("restore complete macOS app bundle");
    assert_eq!(
        fs::read(app.join("Contents/MacOS/quantix")).expect("read restored macOS executable"),
        b"prior macOS executable"
    );
    assert_eq!(
        fs::read(app.join("Contents/Resources/runtime/model.bin"))
            .expect("read restored macOS resource"),
        b"prior macOS resource"
    );

    let (linux_root, linux_host) = ready_host();
    let linux_offer = linux_host
        .present_update(valid_offer(false))
        .expect("Linux offer");
    let linux_update_id = linux_offer.offer.expect("Linux offer identity").update_id;
    linux_host
        .decide_update(
            linux_update_id.clone(),
            UpdateDecision::Approve,
            "Approve the exact restorable Linux AppImage".into(),
        )
        .expect("approve Linux fixture update");
    linux_host
        .authorize_update_installation(&linux_update_id)
        .expect("authorize Linux fixture install");
    let app_image = linux_root.path().join("Quantix.AppImage");
    fs::write(&app_image, b"prior Linux AppImage").expect("write AppImage");
    linux_host
        .stage_application_recovery_point(
            &linux_update_id,
            "0.1.0",
            &InstalledApplicationArtifactSet {
                kind: InstalledApplicationArtifactKind::LinuxAppImage,
                root: app_image.clone(),
                launcher_relative: None,
            },
        )
        .expect("stage exact Linux AppImage");
    linux_host
        .begin_update_installation_after_recovery(&linux_update_id)
        .expect("persist Linux Installing after durable recovery");
    linux_host
        .record_update_failure(&linux_update_id, UpdateDiagnostic::InstallationInterrupted)
        .expect("require Linux rollback");
    fs::remove_file(&app_image).expect("simulate deleted AppImage");
    linux_host
        .restore_application_recovery_point(&linux_update_id)
        .expect("restore missing exact AppImage");
    assert_eq!(
        fs::read(app_image).expect("read restored AppImage"),
        b"prior Linux AppImage"
    );
}

#[test]
fn valid_signed_update_is_presented_and_requires_exact_backup_before_installation() {
    let (_root, host) = ready_host();
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Cairo Hospital".into(),
        })
        .expect("create Tender");
    host.register_tender_content(RegisterTenderContentCommand {
        tender_id: tender.tender_id.clone(),
        logical_id: "conditions".into(),
        media_type: "text/plain".into(),
        bytes: b"controlled source".to_vec(),
    })
    .expect("register canonical content");

    let available = host
        .present_update(valid_offer(true))
        .expect("present compatible update");

    assert_eq!(available.state, UpdateState::AwaitingApproval);
    let presented = available.offer.expect("exact update offer");
    assert_eq!(presented.version, "0.2.0");
    assert_eq!(presented.artifact.sha256, "a".repeat(64));
    assert_eq!(presented.artifact.signature_sha256, "b".repeat(64));
    assert_eq!(presented.compatibility.tender_schema_version, 21);
    assert!(presented.impact.stored_data_may_change);

    let approved = host
        .decide_update(
            presented.update_id.clone(),
            UpdateDecision::Approve,
            "Install the exact signed and compatible release".into(),
        )
        .expect("Engineer approves exact offer");
    assert_eq!(approved.state, UpdateState::Approved);
    let blocked = host
        .authorize_update_installation(&presented.update_id)
        .expect_err("stored-data update needs an exact verified backup");
    assert_eq!(blocked.diagnostic, UpdateDiagnostic::VerifiedBackupRequired);

    host.create_tender_backup(CreateTenderBackupCommand {
        tender_id: tender.tender_id,
    })
    .expect("create exact verified backup");
    let authorized = host
        .authorize_update_installation(&presented.update_id)
        .expect("authorize approved compatible quiescent update");
    assert_eq!(authorized.state, UpdateState::Approved);
    begin_fixture_installation(&host, &presented.update_id);
}

#[test]
fn data_affecting_update_reopens_and_verifies_the_exact_backup_archive_under_lease() {
    let (root, host) = ready_host();
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Archive Revalidation Tender".into(),
        })
        .expect("create Tender");
    host.register_tender_content(RegisterTenderContentCommand {
        tender_id: tender.tender_id.clone(),
        logical_id: "scope".into(),
        media_type: "text/plain".into(),
        bytes: b"authenticated source bytes".to_vec(),
    })
    .expect("register canonical content");
    let backup = host
        .create_tender_backup(CreateTenderBackupCommand {
            tender_id: tender.tender_id,
        })
        .expect("create catalogue-ready exact backup");
    let archive = root
        .path()
        .join(".quantix/backups")
        .join(format!("{}.qtbackup", backup.backup_id));
    let mut tampered = fs::read(&archive).expect("read exact backup archive");
    tampered.extend_from_slice(b"post-verification-tamper");
    fs::write(&archive, tampered).expect("tamper archive without changing its Ready row");

    let offered = host
        .present_update(valid_offer(true))
        .expect("present data-affecting update");
    let update_id = offered.offer.expect("offer identity").update_id;
    host.decide_update(
        update_id.clone(),
        UpdateDecision::Approve,
        "Approve only if the actual backup archive still verifies".into(),
    )
    .expect("approve exact update");

    assert_eq!(
        host.authorize_update_installation(&update_id)
            .expect_err("a stale Ready row cannot authorize altered backup bytes")
            .diagnostic,
        UpdateDiagnostic::VerifiedBackupRequired
    );
}

#[test]
fn a_failed_backup_cannot_authorize_a_data_affecting_update() {
    let (root, host) = ready_host();
    let application_home = root.path().join(".quantix");
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Incomplete Backup Source".into(),
        })
        .expect("create Tender");
    host.register_tender_content(RegisterTenderContentCommand {
        tender_id: tender.tender_id.clone(),
        logical_id: "requirements".into(),
        media_type: "text/plain".into(),
        bytes: b"required immutable bytes".to_vec(),
    })
    .expect("register canonical content");
    let tender_root = application_home.join("tenders").join(&tender.tender_id);
    let connection = rusqlite::Connection::open(tender_root.join("tender.sqlite"))
        .expect("open Tender Store for failure injection");
    let integrity: String = connection
        .query_row("SELECT integrity FROM content_objects", [], |row| {
            row.get(0)
        })
        .expect("read canonical content integrity");
    drop(connection);
    cacache::remove_hash_sync(
        tender_root.join("content"),
        &integrity.parse().expect("parse stored integrity"),
    )
    .expect("remove referenced content bytes");

    host.create_tender_backup(CreateTenderBackupCommand {
        tender_id: tender.tender_id.clone(),
    })
    .expect_err("incomplete source cannot be backed up");
    assert_eq!(
        host.inspect_tender_backups(&tender.tender_id)
            .expect("inspect attributable backup failure")[0]
            .state,
        TenderBackupState::Failed
    );

    let available = host
        .present_update(valid_offer(true))
        .expect("present data-affecting update");
    let update_id = available.offer.expect("offer").update_id;
    host.decide_update(
        update_id.clone(),
        UpdateDecision::Approve,
        "Install only after exact Tender backups verify".into(),
    )
    .expect("approve exact offer");
    assert_eq!(
        host.authorize_update_installation(&update_id)
            .expect_err("failed backup cannot satisfy the installation gate")
            .diagnostic,
        UpdateDiagnostic::VerifiedBackupRequired
    );
}

#[test]
fn engineer_denial_is_terminal_and_cannot_install() {
    let (_root, host) = ready_host();
    let available = host
        .present_update(valid_offer(false))
        .expect("present compatible update");
    let update_id = available.offer.expect("offer").update_id;

    let denied = host
        .decide_update(
            update_id.clone(),
            UpdateDecision::Deny,
            "Reject this exact release after reviewing its impact".into(),
        )
        .expect("record Engineer denial");

    assert_eq!(denied.state, UpdateState::Denied);
    assert_eq!(
        host.authorize_update_installation(&update_id)
            .expect_err("denied offer cannot install")
            .diagnostic,
        UpdateDiagnostic::ApprovalRequired
    );
}

#[test]
fn update_decision_identity_is_host_owned_and_cannot_be_forged_by_the_renderer() {
    let forged = serde_json::json!({
        "update_id": "a".repeat(64),
        "decision": "approve",
        "rationale": "Forge an approval identity",
        "decided_by": "forged_manager",
        "acting_role": "release_bot"
    });
    assert!(serde_json::from_value::<DecideUpdateCommand>(forged).is_err());
    let blank_rationale = serde_json::from_value::<DecideUpdateCommand>(serde_json::json!({
        "update_id": "a".repeat(64),
        "decision": "approve",
        "rationale": "   "
    }))
    .expect("rationale shape parses before validation");
    assert!(blank_rationale.validate().is_err());

    let (root, host) = ready_host();
    let offered = host.present_update(valid_offer(false)).expect("offer");
    let update_id = offered.offer.expect("offer identity").update_id;
    let approved = host
        .decide_update(
            update_id,
            UpdateDecision::Approve,
            "Approve the exact signed release evidence".into(),
        )
        .expect("Host derives the one local Engineer User identity");
    let decision = approved
        .decision_history
        .first()
        .expect("immutable approval record");
    assert_eq!(decision.decided_by, "engineer_user");
    assert_eq!(decision.acting_role, "tendering_manager");
    assert_eq!(decision.update_id, decision.offer_sha256);
    assert_eq!(decision.preceding_hash, "0".repeat(64));
    assert_eq!(decision.current_hash.len(), 64);

    let installation = rusqlite::Connection::open(root.path().join(".quantix/installation.sqlite"))
        .expect("open installation catalogue");
    assert!(installation
        .execute(
            "UPDATE update_decisions SET rationale = 'rewritten' WHERE update_id = ?1",
            [&decision.update_id],
        )
        .is_err());
    assert!(installation
        .execute(
            "DELETE FROM update_decisions WHERE update_id = ?1",
            [&decision.update_id],
        )
        .is_err());
    assert!(installation
        .execute(
            "UPDATE update_operations SET offer_json = '{}' WHERE update_id = ?1",
            [&decision.update_id],
        )
        .is_err());
    assert!(installation
        .execute(
            "DELETE FROM update_operations WHERE update_id = ?1",
            [&decision.update_id],
        )
        .is_err());
}

#[test]
fn update_approval_history_remains_exact_across_later_state_transitions() {
    let (_root, host) = ready_host();
    let offered = host.present_update(valid_offer(false)).expect("offer");
    let update_id = offered.offer.expect("offer identity").update_id;
    let approved = host
        .decide_update(
            update_id.clone(),
            UpdateDecision::Approve,
            "Approve the exact release after reviewing signed evidence".into(),
        )
        .expect("record immutable approval");
    let approval = approved.decision_history[0].clone();

    host.authorize_update_installation(&update_id)
        .expect("authorize exact approval");
    begin_fixture_installation(&host, &update_id);
    let installed = host
        .record_update_installed(&update_id)
        .expect("record installation");

    assert_eq!(installed.decision_history, vec![approval]);
    assert_eq!(installed.state, UpdateState::RestartValidationRequired);
}

#[test]
fn update_decision_history_is_globally_hash_chained_across_exact_offers() {
    let (_root, host) = ready_host();
    let first = host
        .present_update(valid_offer(false))
        .expect("first offer");
    let first_id = first.offer.expect("first offer identity").update_id;
    let denied = host
        .decide_update(
            first_id,
            UpdateDecision::Deny,
            "Reject the first exact signed offer".into(),
        )
        .expect("record first decision");
    let first_record = denied.decision_history[0].clone();
    let mut second_offer = valid_offer(false);
    second_offer.release.title = "Quantix 0.2.0 corrected release".into();
    let second = host
        .present_update(second_offer)
        .expect("second exact offer");
    let second_id = second.offer.expect("second offer identity").update_id;
    let approved = host
        .decide_update(
            second_id,
            UpdateDecision::Approve,
            "Approve the corrected exact signed offer".into(),
        )
        .expect("record second decision");
    let second_record = &approved.decision_history[0];

    assert_eq!(first_record.sequence, 1);
    assert_eq!(second_record.sequence, 2);
    assert_eq!(second_record.preceding_hash, first_record.current_hash);
    assert_ne!(second_record.current_hash, second_record.preceding_hash);
}

#[test]
fn restart_and_repair_actions_are_bound_to_the_exact_persisted_update_state() {
    let (_root, host) = ready_host();
    let offered = host.present_update(valid_offer(false)).expect("offer");
    let update_id = offered.offer.expect("offer identity").update_id;
    host.decide_update(
        update_id.clone(),
        UpdateDecision::Approve,
        "Approve the exact release and restart only after installation".into(),
    )
    .expect("approve exact update");
    assert_eq!(
        host.authorize_update_restart(&update_id)
            .expect_err("approval alone cannot request a restart")
            .diagnostic,
        UpdateDiagnostic::RestartValidationFailed
    );
    host.authorize_update_installation(&update_id)
        .expect("authorize installation");
    begin_fixture_installation(&host, &update_id);
    host.record_update_installed(&update_id)
        .expect("persist restart requirement");
    assert_eq!(
        host.authorize_update_restart(&update_id)
            .expect("the exact installed update can request restart")
            .state,
        UpdateState::RestartValidationRequired
    );
    assert_eq!(
        host.prepare_application_rollback(&update_id)
            .expect_err("rollback cannot be scheduled outside Repair Required")
            .diagnostic,
        UpdateDiagnostic::InstallationFailed
    );
}

#[test]
fn renderer_invokes_only_named_restart_and_repair_domain_actions() {
    let (_root, host) = ready_host();
    let offered = host.present_update(valid_offer(false)).expect("offer");
    let update_id = offered.offer.expect("offer identity").update_id;
    host.decide_update(
        update_id.clone(),
        UpdateDecision::Approve,
        "Approve the exact release before exercising the renderer command seam".into(),
    )
    .expect("approve exact update");
    host.authorize_update_installation(&update_id)
        .expect("authorize installation");
    begin_fixture_installation(&host, &update_id);
    host.record_update_installed(&update_id)
        .expect("persist explicit restart requirement");
    let restart_requested = AtomicBool::new(false);
    let restart = perform_authorized_update_restart(
        host.authorize_update_restart(&update_id)
            .expect("authorize the exact persisted restart action"),
        || restart_requested.store(true, Ordering::Release),
    );
    assert!(restart_requested.load(Ordering::Acquire));
    assert_eq!(restart.state, UpdateState::RestartValidationRequired);

    let app = configure_tauri_builder(tauri::test::mock_builder())
        .manage(host)
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("build Tauri command harness");
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("build renderer harness");
    let request = |command: &str| tauri::webview::InvokeRequest {
        cmd: command.into(),
        callback: tauri::ipc::CallbackFn(0),
        error: tauri::ipc::CallbackFn(1),
        url: "http://tauri.localhost".parse().expect("renderer URL"),
        body: tauri::ipc::InvokeBody::Json(serde_json::json!({
            "command": { "update_id": update_id.clone() }
        })),
        headers: Default::default(),
        invoke_key: tauri::test::INVOKE_KEY.into(),
    };

    let repair_error =
        tauri::test::get_ipc_response(&webview, request("retry_quantix_update_repair"))
            .expect_err("repair cannot run from Restart Required");
    assert_eq!(repair_error["diagnostic"], "installation_failed");
}

#[test]
fn blocked_setup_cannot_check_or_present_an_update() {
    let (root, host) = ready_host();
    fs::remove_dir(root.path().join(".quantix/models"))
        .expect("make the otherwise initialized Application Home incomplete");
    assert_eq!(
        ensure_quantix_setup(&host).state,
        SetupState::RepairRequired
    );
    assert_eq!(
        host.present_update(valid_offer(false))
            .expect_err("the Host rejects update work from a blocked Setup state")
            .diagnostic,
        UpdateDiagnostic::UpdaterUnavailable
    );

    let app = configure_tauri_builder(tauri::test::mock_builder())
        .manage(host)
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("build blocked Setup Tauri harness");
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("build blocked Setup renderer harness");
    let response = tauri::test::get_ipc_response(
        &webview,
        tauri::webview::InvokeRequest {
            cmd: "check_quantix_update".into(),
            callback: tauri::ipc::CallbackFn(0),
            error: tauri::ipc::CallbackFn(1),
            url: "http://tauri.localhost".parse().expect("renderer URL"),
            body: Default::default(),
            headers: Default::default(),
            invoke_key: tauri::test::INVOKE_KEY.into(),
        },
    )
    .expect_err("the adapter blocks before update configuration or network access");
    assert_eq!(response["diagnostic"], "updater_unavailable");
}

#[test]
fn unsafe_or_incompatible_offers_are_rejected_before_approval() {
    let (_root, host) = ready_host();
    let cases = [
        (
            {
                let mut offer = valid_offer(false);
                offer.version = "0.0.9".into();
                offer
            },
            UpdateDiagnostic::DowngradeRejected,
        ),
        (
            {
                let mut offer = valid_offer(false);
                offer.platform = match current_update_platform().expect("supported test platform") {
                    UpdatePlatform::WindowsX86_64 => UpdatePlatform::MacOsAarch64,
                    UpdatePlatform::MacOsAarch64 | UpdatePlatform::UbuntuX86_64 => {
                        UpdatePlatform::WindowsX86_64
                    }
                };
                offer
            },
            UpdateDiagnostic::UnsupportedPlatform,
        ),
        (
            {
                let mut offer = valid_offer(false);
                offer.compatibility.tender_schema_version = 22;
                offer
            },
            UpdateDiagnostic::TenderStoreIncompatible,
        ),
        (
            {
                let mut offer = valid_offer(false);
                offer.compatibility.codex_version = "0.148.0".into();
                offer
            },
            UpdateDiagnostic::CodexIncompatible,
        ),
        (
            {
                let mut offer = valid_offer(false);
                offer.compatibility.docling_version = "2.76.0".into();
                offer
            },
            UpdateDiagnostic::DoclingIncompatible,
        ),
        (
            {
                let mut offer = valid_offer(false);
                offer.compatibility.runtime_manifest_schema_version = 3;
                offer
            },
            UpdateDiagnostic::RuntimeIncompatible,
        ),
    ];

    for (offer, diagnostic) in cases {
        let rejected = host
            .present_update(offer)
            .expect_err("unsafe offer must fail closed");
        assert_eq!(rejected.diagnostic, diagnostic);
    }
}

#[test]
fn signature_failures_and_interruption_preserve_the_prior_installation_as_repairable() {
    for diagnostic in [
        UpdateDiagnostic::UnsignedArtifact,
        UpdateDiagnostic::WrongSigningKey,
        UpdateDiagnostic::ArtifactTampered,
    ] {
        let (_root, host) = ready_host();
        let available = host.present_update(valid_offer(false)).expect("offer");
        let update_id = available.offer.expect("offer").update_id;
        host.decide_update(
            update_id.clone(),
            UpdateDecision::Approve,
            "Approve the exact authenticated release".into(),
        )
        .expect("approve exact update");
        host.authorize_update_installation(&update_id)
            .expect("start installation");

        let rejected = host
            .record_update_failure(&update_id, diagnostic)
            .expect("record attributable authentication failure");

        assert_eq!(rejected.state, UpdateState::Rejected);
        assert_eq!(rejected.diagnostic, Some(diagnostic));
    }

    let (_root, host) = ready_host();
    let available = host.present_update(valid_offer(false)).expect("offer");
    let update_id = available.offer.expect("offer").update_id;
    host.decide_update(
        update_id.clone(),
        UpdateDecision::Approve,
        "Approve the exact recoverable release".into(),
    )
    .expect("approve exact update");
    host.authorize_update_installation(&update_id)
        .expect("start installation");
    begin_fixture_installation(&host, &update_id);
    let repair = host
        .record_update_failure(&update_id, UpdateDiagnostic::InstallationInterrupted)
        .expect("record interrupted update");
    assert_eq!(repair.state, UpdateState::RepairRequired);
    assert_eq!(
        repair.diagnostic,
        Some(UpdateDiagnostic::InstallationInterrupted)
    );
}

#[test]
fn an_installation_lease_rejects_concurrent_update_work() {
    let (root, host) = ready_host();
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Quiescence Gate".into(),
        })
        .expect("create Tender");
    let first = host
        .present_update(valid_offer(false))
        .expect("first offer");
    let first_id = first.offer.expect("first offer identity").update_id;
    host.decide_update(
        first_id.clone(),
        UpdateDecision::Approve,
        "Approve the first exact release".into(),
    )
    .expect("approve first update");

    let mut second_offer = valid_offer(false);
    second_offer.version = "0.3.0".into();
    let second = host.present_update(second_offer).expect("second offer");
    let second_id = second.offer.expect("second offer identity").update_id;
    host.decide_update(
        second_id.clone(),
        UpdateDecision::Approve,
        "Approve the second exact release".into(),
    )
    .expect("approve second exact offer");
    host.authorize_update_installation(&first_id)
        .expect("claim installation lease");
    let runtime = tokio::runtime::Runtime::new().expect("runtime readiness check executor");
    assert_eq!(
        runtime.block_on(host.inspect_runtime_readiness()).state,
        RuntimeReadinessState::Preparing,
        "runtime probing cannot spawn children during installation"
    );

    let contender = QuantixHost::with_setup_platform(
        root.path().join(".quantix"),
        Arc::new(ReadySetupPlatform),
    );
    let blocked_setup = ensure_quantix_setup(&contender);
    assert_eq!(blocked_setup.state, SetupState::RepairRequired);
    assert_eq!(
        blocked_setup.issues,
        vec![SetupIssue::UpdateInstallationActive]
    );

    assert_eq!(
        contender
            .authorize_update_installation(&second_id)
            .expect_err("only one global quiescent installation may run")
            .diagnostic,
        UpdateDiagnostic::ActiveWork
    );
    assert_eq!(
        contender
            .create_tender_backup(CreateTenderBackupCommand {
                tender_id: tender.tender_id,
            })
            .expect_err("backup work cannot race an active update")
            .code,
        TenderErrorCode::InvalidCommand
    );
    begin_fixture_installation(&host, &first_id);
    host.record_update_failure(&first_id, UpdateDiagnostic::InstallationInterrupted)
        .expect("release first installation lease");
}

#[test]
fn application_home_lease_serializes_two_hosts_against_an_in_flight_backup() {
    let root = tempfile::tempdir().expect("temporary app root");
    let application_home = root.path().join(".quantix");
    let platform = Arc::new(BlockingBackupSetupPlatform::new());
    let host = QuantixHost::with_setup_platform(&application_home, platform.clone());
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Global Lease Tender".into(),
        })
        .expect("create Tender");
    let update = host.present_update(valid_offer(true)).expect("offer");
    let update_id = update.offer.expect("offer identity").update_id;
    host.decide_update(
        update_id.clone(),
        UpdateDecision::Approve,
        "Approve after the exact backup finishes".into(),
    )
    .expect("approve update");

    let contender = QuantixHost::with_setup_platform(&application_home, platform.clone());
    assert_eq!(ensure_quantix_setup(&contender).state, SetupState::Ready);
    platform.block_backup.store(true, Ordering::Release);
    let backup_host = host.clone();
    let backup = std::thread::spawn(move || {
        backup_host.create_tender_backup(CreateTenderBackupCommand {
            tender_id: tender.tender_id,
        })
    });
    platform.backup_started.wait();

    assert_eq!(
        contender
            .authorize_update_installation(&update_id)
            .expect_err("global lease rejects update during an in-flight backup")
            .diagnostic,
        UpdateDiagnostic::ActiveWork
    );
    platform.release_backup.wait();
    assert_eq!(
        backup
            .join()
            .expect("backup thread")
            .expect("exact backup completes before installation")
            .state,
        TenderBackupState::Ready
    );
    assert_eq!(
        contender
            .authorize_update_installation(&update_id)
            .expect("update revalidates the completed exact backup")
            .state,
        UpdateState::Approved
    );
    begin_fixture_installation(&contender, &update_id);
    contender
        .record_update_failure(&update_id, UpdateDiagnostic::InstallationInterrupted)
        .expect("release global update lease");
}

#[tokio::test]
async fn restart_discovers_interrupted_installation_and_requires_repair() {
    let (root, host) = ready_host();
    let application_home = root.path().join(".quantix");
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Rollback Tender".into(),
        })
        .expect("create Tender before interrupted update");
    let available = host.present_update(valid_offer(false)).expect("offer");
    let update_id = available.offer.expect("offer identity").update_id;
    host.decide_update(
        update_id.clone(),
        UpdateDecision::Approve,
        "Approve the exact release with complete rollback evidence".into(),
    )
    .expect("approve exact offer");
    host.authorize_update_installation(&update_id)
        .expect("persist installing state");
    let installed_root = root.path().join("installed-quantix");
    fs::create_dir_all(installed_root.join("runtime/bin"))
        .expect("create packaged runtime directory");
    let application_artifact = installed_root.join("quantix.exe");
    let bundled_resource = installed_root.join("runtime/app-resource.bin");
    let bundled_sidecar = installed_root.join("runtime/bin/codex-x86_64-pc-windows-msvc.exe");
    fs::write(&application_artifact, b"verified prior Quantix application")
        .expect("write prior application artifact");
    fs::write(&bundled_resource, b"verified prior packaged resource")
        .expect("write packaged resource");
    fs::write(&bundled_sidecar, b"verified prior Codex sidecar").expect("write packaged sidecar");
    host.stage_application_recovery_point(
        &update_id,
        "0.1.0",
        &InstalledApplicationArtifactSet {
            kind: InstalledApplicationArtifactKind::WindowsBundle,
            root: installed_root.clone(),
            launcher_relative: Some("quantix.exe".into()),
        },
    )
    .expect("stage the complete exact installed application artifact set");
    host.begin_update_installation_after_recovery(&update_id)
        .expect("persist Installing after complete recovery is durable");
    let installation = rusqlite::Connection::open(application_home.join("installation.sqlite"))
        .expect("open installation catalogue");
    assert!(
        installation
            .execute(
                "UPDATE update_recovery_points SET destination_root = ?2 WHERE update_id = ?1",
                rusqlite::params![
                    &update_id,
                    root.path().join("forged.exe").display().to_string()
                ],
            )
            .is_err(),
        "authenticated recovery facts are immutable"
    );
    fs::write(&application_artifact, b"interrupted replacement")
        .expect("simulate interrupted update artifact");
    fs::remove_file(&bundled_resource).expect("remove old resource before type drift");
    fs::create_dir(&bundled_resource)
        .expect("simulate partial replacement changing a file into a directory");
    fs::remove_file(&bundled_sidecar).expect("simulate missing prior sidecar");
    fs::write(
        installed_root.join("new-version-only.dll"),
        b"partial extra",
    )
    .expect("simulate a partial new-version artifact");
    drop(host);

    let restarted =
        QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    let blocked = ensure_quantix_setup(&restarted);
    assert_eq!(blocked.state, SetupState::RepairRequired);
    assert_eq!(blocked.issues, vec![SetupIssue::UpdateInstallationActive]);
    let status = restarted
        .validate_update_after_restart("0.1.0")
        .await
        .expect("reconcile prior installation");

    assert_eq!(status.state, UpdateState::RepairRequired);
    assert_eq!(
        status.diagnostic,
        Some(UpdateDiagnostic::InstallationInterrupted)
    );
    let recovery_manifest = application_home
        .join("update-recovery")
        .join(&update_id)
        .join("manifest.json");
    let original_manifest = fs::read(&recovery_manifest).expect("read recovery manifest");
    let victim = root.path().join("unrelated-installation");
    fs::create_dir(&victim).expect("create unrelated installation");
    fs::write(victim.join("keep.bin"), b"unrelated application")
        .expect("write unrelated application");
    let mut tampered: serde_json::Value =
        serde_json::from_slice(&original_manifest).expect("decode recovery manifest");
    tampered["destination_root"] = serde_json::Value::String(victim.display().to_string());
    fs::write(
        &recovery_manifest,
        serde_json::to_vec(&tampered).expect("encode tampered manifest"),
    )
    .expect("tamper recovery destination");
    assert_eq!(
        restarted
            .restore_application_recovery_point(&update_id)
            .expect_err("mutable manifest cannot redirect an executable overwrite")
            .diagnostic,
        UpdateDiagnostic::InstallationFailed
    );
    assert_eq!(
        fs::read(victim.join("keep.bin")).expect("read protected unrelated application"),
        b"unrelated application"
    );
    assert_eq!(
        fs::read(&application_artifact).expect("read partial replacement"),
        b"interrupted replacement"
    );
    fs::write(&recovery_manifest, original_manifest).expect("restore authenticated manifest");
    fs::remove_dir_all(&installed_root)
        .expect("simulate updater deleting the complete prior destination");
    let rollback = restarted
        .prepare_application_rollback(&update_id)
        .expect("prepare the authenticated rollback helper launch");
    assert_eq!(
        rollback.helper_path,
        application_home
            .join("update-recovery")
            .join(&update_id)
            .join("artifacts/quantix.exe")
    );
    assert_eq!(
        rollback.arguments,
        vec![
            std::ffi::OsString::from("--quantix-update-rollback"),
            application_home.clone().into_os_string(),
            std::ffi::OsString::from(update_id.as_str()),
        ]
    );
    let launched = std::sync::Mutex::new(None);
    assert!(run_update_rollback_helper_with_launcher(
        [
            rollback.helper_path.clone().into_os_string(),
            rollback.arguments[0].clone(),
            rollback.arguments[1].clone(),
            rollback.arguments[2].clone(),
        ],
        |launcher| {
            *launched.lock().expect("record restored launcher") = Some(launcher.to_path_buf());
            Ok(())
        },
    ));
    assert_eq!(
        launched.into_inner().expect("read recorded launcher"),
        Some(
            application_artifact
                .canonicalize()
                .expect("canonical restored launcher path")
        )
    );
    assert_eq!(
        fs::read(&application_artifact).expect("read restored application artifact"),
        b"verified prior Quantix application"
    );
    assert_eq!(
        fs::read(&bundled_resource).expect("read restored packaged resource"),
        b"verified prior packaged resource"
    );
    assert_eq!(
        fs::read(&bundled_sidecar).expect("read restored packaged sidecar"),
        b"verified prior Codex sidecar"
    );
    assert!(!installed_root.join("new-version-only.dll").exists());

    drop(restarted);
    let rolled_back =
        QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    let blocked = ensure_quantix_setup(&rolled_back);
    assert_eq!(blocked.state, SetupState::RepairRequired);
    assert_eq!(blocked.issues, vec![SetupIssue::UpdateInstallationActive]);
    let status = rolled_back
        .validate_update_after_restart("0.1.0")
        .await
        .expect("validate the recovered prior installation");
    assert_eq!(status.state, UpdateState::RolledBack);
    assert_eq!(
        status.diagnostic,
        Some(UpdateDiagnostic::InstallationInterrupted)
    );
    assert_eq!(
        rolled_back
            .open_tender(&tender.tender_id)
            .expect("Tender work resumes only after rollback validation")
            .tender_id,
        tender.tender_id
    );
    assert!(!application_home
        .join("update-recovery")
        .join(&update_id)
        .exists());
}

#[tokio::test]
async fn restarted_update_fails_closed_when_runtime_revalidation_fails() {
    let (root, host) = ready_host();
    let application_home = root.path().join(".quantix");
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Cairo Hospital".into(),
        })
        .expect("create Tender before update");
    let available = host.present_update(valid_offer(false)).expect("offer");
    let update_id = available.offer.expect("offer identity").update_id;
    host.decide_update(
        update_id.clone(),
        UpdateDecision::Approve,
        "Approve the exact release before restart validation".into(),
    )
    .expect("approve exact offer");
    host.authorize_update_installation(&update_id)
        .expect("begin installation");
    begin_fixture_installation(&host, &update_id);
    host.record_update_installed(&update_id)
        .expect("wait for restart validation");
    drop(host);

    let restarted = QuantixHost::with_setup_platform_and_runtime(
        &application_home,
        Arc::new(ReadySetupPlatform),
        RuntimeLayout::bundled(root.path().join("missing-runtime-resources")),
    );
    let blocked = ensure_quantix_setup(&restarted);
    assert_eq!(blocked.state, SetupState::RepairRequired);
    assert_eq!(blocked.issues, vec![SetupIssue::UpdateInstallationActive]);
    assert_eq!(
        restarted
            .open_tender(&tender.tender_id)
            .expect_err("Tender work stays blocked before restart validation")
            .code,
        TenderErrorCode::SetupRequired
    );
    let status = restarted
        .validate_update_after_restart("0.2.0")
        .await
        .expect("run restart validation");

    assert_eq!(status.state, UpdateState::RepairRequired);
    assert_eq!(
        status.diagnostic,
        Some(UpdateDiagnostic::RestartValidationFailed)
    );
    assert_eq!(
        restarted
            .open_tender(&tender.tender_id)
            .expect_err("failed restart validation keeps Tender work blocked")
            .code,
        TenderErrorCode::SetupRequired
    );
}

#[tokio::test]
async fn restarted_update_rejects_a_tampered_installation_schema_and_preserves_recovery() {
    let (_root, host) = ready_host();
    let update_id = prepare_restart_validation(&host);
    let installation =
        rusqlite::Connection::open(host.application_home().join("installation.sqlite"))
            .expect("open installation catalogue");
    installation
        .pragma_update(None, "user_version", 9)
        .expect("simulate a newer unsupported installation schema");
    drop(installation);

    let status = host
        .validate_update_after_restart("0.2.0")
        .await
        .expect("restart validation remains an actionable update result");

    assert_failed_restart_preserves_recovery(&host, &update_id, &status);
}

#[tokio::test]
async fn restarted_update_rejects_tampered_application_home_layout_and_preserves_recovery() {
    let (_root, host) = ready_host();
    let update_id = prepare_restart_validation(&host);
    fs::write(
        host.application_home().join("unrecognized-update-artifact"),
        b"must not become part of the trusted Application Home layout",
    )
    .expect("tamper Application Home layout");

    let status = host
        .validate_update_after_restart("0.2.0")
        .await
        .expect("restart validation remains an actionable update result");

    assert_failed_restart_preserves_recovery(&host, &update_id, &status);
}

#[tokio::test]
async fn restarted_update_rejects_unsafe_storage_permissions_and_preserves_recovery() {
    let root = tempfile::tempdir().expect("temporary restart validation harness");
    let platform = Arc::new(TamperableStorageSetupPlatform::new());
    let host = QuantixHost::with_setup_platform(root.path().join(".quantix"), platform.clone());
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let update_id = prepare_restart_validation(&host);
    platform.unsafe_permissions.store(true, Ordering::Release);

    let status = host
        .validate_update_after_restart("0.2.0")
        .await
        .expect("restart validation remains an actionable update result");

    assert_failed_restart_preserves_recovery(&host, &update_id, &status);
}

#[tokio::test]
async fn in_flight_agent_work_on_one_host_blocks_update_authorization_on_another() {
    let root = tempfile::tempdir().expect("temporary agent update harness");
    let application_home = root.path().join(".quantix");
    let resources = root.path().join("resources");
    let codex = install_codex_fixture(&resources, "hang-before-thread");
    let host = QuantixHost::with_setup_platform_and_runtime(
        &application_home,
        Arc::new(ReadySetupPlatform),
        RuntimeLayout::bundled(&resources),
    );
    host.accept_runtime_fixture();
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Agent Update Race".into(),
        })
        .expect("create Tender");
    let update = host.present_update(valid_offer(false)).expect("offer");
    let update_id = update.offer.expect("offer identity").update_id;
    host.decide_update(
        update_id.clone(),
        UpdateDecision::Approve,
        "Approve only when Agent work is quiescent".into(),
    )
    .expect("approve update");

    let running_host = host.clone();
    let running_tender_id = tender.tender_id;
    let running = tokio::spawn(async move {
        running_host
            .run_bootstrap_agent(RunBootstrapAgentCommand {
                tender_id: running_tender_id,
                retry_of_run_id: None,
            })
            .await
    });
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while !codex.with_extension("thread-waiting").is_file() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("Agent work reaches the real provider adapter");

    let contender = QuantixHost::with_setup_platform_and_runtime(
        &application_home,
        Arc::new(ReadySetupPlatform),
        RuntimeLayout::bundled(resources),
    );
    contender.accept_runtime_fixture();
    assert_eq!(
        contender
            .authorize_update_installation(&update_id)
            .expect_err("in-flight Agent work owns the global ordinary-work lease")
            .diagnostic,
        UpdateDiagnostic::ActiveWork
    );
    running.abort();
    assert!(running
        .await
        .expect_err("stop hanging Agent fixture")
        .is_cancelled());
}

#[tokio::test]
async fn in_flight_docling_work_on_one_host_blocks_update_authorization_on_another() {
    let (root, host) = ready_host();
    let application_home = root.path().join(".quantix");
    install_docling_fixture(&application_home);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Docling Update Race".into(),
        })
        .expect("create Tender");
    let source = root.path().join("docling-update-race-source");
    fs::create_dir(&source).expect("create Docling update race source");
    fs::write(
        source.join("slow.pdf"),
        b"%PDF-1.7\nINTERRUPTED_PROCESS\n%%EOF",
    )
    .expect("write slow PDF source");
    let imported = host
        .import_tender_package(ImportTenderPackageCommand {
            tender_id: tender.tender_id.clone(),
            source_path: source.to_string_lossy().into_owned(),
        })
        .expect("import slow PDF source");
    let document = imported.documents.first().expect("registered slow PDF");
    let update = host.present_update(valid_offer(false)).expect("offer");
    let update_id = update.offer.expect("offer identity").update_id;
    host.decide_update(
        update_id.clone(),
        UpdateDecision::Approve,
        "Approve only when Docling work is quiescent".into(),
    )
    .expect("approve update");

    let parse_command = ParseSourceArtifactCommand {
        tender_id: tender.tender_id.clone(),
        artifact_id: document.artifact_id.clone(),
        version: document.version,
    };
    let observed_artifact_id = parse_command.artifact_id.clone();
    let observed_version = parse_command.version;
    let running_host = host.clone();
    let mut running =
        tokio::spawn(async move { running_host.parse_source_artifact(parse_command).await });
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if running.is_finished() {
                let early_result = (&mut running).await;
                panic!("Docling parse exited before its public Running state: {early_result:?}");
            }
            let register = host
                .inspect_document_register(&tender.tender_id)
                .expect("inspect the public Document Register during parsing");
            if register.documents.iter().any(|document| {
                document.artifact_id == observed_artifact_id
                    && document.version == observed_version
                    && document.parse_state == quantix_lib::ParseState::Running
            }) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("Docling parse reaches its public persisted in-flight state");

    let contender =
        QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(
        contender
            .authorize_update_installation(&update_id)
            .expect_err("in-flight Docling work owns the global ordinary-work lease")
            .diagnostic,
        UpdateDiagnostic::ActiveWork
    );
    running.abort();
    assert!(running
        .await
        .expect_err("stop hanging Docling fixture")
        .is_cancelled());
}

fn install_codex_fixture(resources: &Path, scenario: &str) -> std::path::PathBuf {
    let runtime_bin = resources.join("runtime").join("bin");
    fs::create_dir_all(&runtime_bin).expect("fake runtime bin");
    let codex = runtime_bin.join(executable_name("codex"));
    fs::copy(
        Path::new(env!("CARGO_BIN_EXE_quantix-runtime-fixture")),
        &codex,
    )
    .expect("copy fake app-server");
    fs::write(codex.with_extension("agent-scenario"), scenario)
        .expect("write fake app-server scenario");
    codex
}

fn install_docling_fixture(application_home: &Path) {
    let executable = application_home
        .join("runtimes")
        .join("docling")
        .join(if cfg!(windows) { "Scripts" } else { "bin" })
        .join(executable_name("docling"));
    fs::create_dir_all(executable.parent().expect("Docling executable parent"))
        .expect("Docling executable directory");
    fs::copy(
        Path::new(env!("CARGO_BIN_EXE_quantix-runtime-fixture")),
        &executable,
    )
    .expect("install Docling fixture");
    let models = application_home.join("models").join("docling");
    for profile in [
        "layout",
        "tableformer",
        "code_formula",
        "picture_classifier",
        "rapidocr",
    ] {
        let model = models.join(profile).join("model.bin");
        fs::create_dir_all(model.parent().expect("model parent")).expect("model directory");
        fs::write(model, format!("{profile} fixture model")).expect("model fixture");
    }
}

fn executable_name(name: &str) -> String {
    if std::env::consts::EXE_EXTENSION.is_empty() {
        name.to_owned()
    } else {
        format!("{name}.{}", std::env::consts::EXE_EXTENSION)
    }
}
