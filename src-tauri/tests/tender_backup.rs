use std::{
    fs::{File, OpenOptions},
    io::{self, Write},
    path::Path,
    process::Command,
    sync::{Arc, Barrier},
    thread,
};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

use quantix_lib::{
    ensure_quantix_setup, CreateTenderBackupCommand, CreateTenderCommand, DeletionReceipt,
    PrepareTenderRecoveryCommand, ProviderCleanupStatus, ProviderReferenceDiscoveryState,
    PurgeRecoveryRequiredTenderCommand, QuantixHost, RegisterTenderContentCommand,
    ResolveTenderRecoveryCommand, ReviseTenderCommand, SetupPlatform, SetupState,
    StoragePermissions, TenderBackupState, TenderDeletionSourceState, TenderErrorCode,
    TenderIntegrityState, TenderRecoveryDecision, TenderRecoveryState,
    TrashRecoveryRequiredTenderCommand, TrashedTenderDecisionCommand, TrashedTenderState,
    MINIMUM_SETUP_FREE_SPACE_BYTES,
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
}

#[test]
fn approval_rejects_a_valid_same_identity_candidate_substituted_after_the_offer() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Manifest-Bound Backup".into(),
        })
        .expect("create Tender");
    let backup = host
        .create_tender_backup(CreateTenderBackupCommand {
            tender_id: tender.tender_id.clone(),
        })
        .expect("create backup");
    let current = host
        .revise_tender(ReviseTenderCommand {
            tender_id: tender.tender_id.clone(),
            name: "Different Valid Same-ID History".into(),
        })
        .expect("revise current Tender");
    let offer = host
        .prepare_tender_recovery(PrepareTenderRecoveryCommand {
            tender_id: tender.tender_id.clone(),
            backup_id: backup.backup_id.clone(),
        })
        .expect("prepare exact backup candidate");
    host.close_tender(&tender.tender_id).expect("close Tender");
    let candidate = application_home
        .join("staging")
        .join(format!("recovery-{}", offer.recovery_id));
    std::fs::remove_dir_all(&candidate).expect("remove offered candidate for substitution fixture");
    copy_directory(
        &application_home.join("tenders").join(&tender.tender_id),
        &candidate,
    );

    let error = host
        .resolve_tender_recovery(ResolveTenderRecoveryCommand {
            tender_id: tender.tender_id.clone(),
            recovery_id: offer.recovery_id,
            decision: TenderRecoveryDecision::ApproveReplacement,
            rationale: "Approve only the manifest-bound candidate that was offered".into(),
        })
        .expect_err("same-ID candidate substitution must fail exact manifest verification");
    assert_eq!(error.code, TenderErrorCode::IntegrityFailed);
    assert_eq!(
        host.open_tender(&tender.tender_id)
            .expect("substitution cannot mutate current Tender"),
        current
    );
}

#[test]
fn approval_invalidates_an_offer_when_the_current_tender_changes() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Backup Baseline".into(),
        })
        .expect("create Tender");
    let backup = host
        .create_tender_backup(CreateTenderBackupCommand {
            tender_id: tender.tender_id.clone(),
        })
        .expect("create backup");
    host.revise_tender(ReviseTenderCommand {
        tender_id: tender.tender_id.clone(),
        name: "State Shown In Offer".into(),
    })
    .expect("revise before offer");
    let offer = host
        .prepare_tender_recovery(PrepareTenderRecoveryCommand {
            tender_id: tender.tender_id.clone(),
            backup_id: backup.backup_id,
        })
        .expect("prepare recovery offer");
    let latest = host
        .revise_tender(ReviseTenderCommand {
            tender_id: tender.tender_id.clone(),
            name: "Material Change After Offer".into(),
        })
        .expect("revise after offer");

    let error = host
        .resolve_tender_recovery(ResolveTenderRecoveryCommand {
            tender_id: tender.tender_id.clone(),
            recovery_id: offer.recovery_id.clone(),
            decision: TenderRecoveryDecision::ApproveReplacement,
            rationale: "This rationale was based on stale current-state evidence".into(),
        })
        .expect_err("material current change invalidates stale approval evidence");
    assert_eq!(error.code, TenderErrorCode::InvalidCommand);
    assert_eq!(
        host.open_tender(&tender.tender_id)
            .expect("stale approval cannot overwrite current Tender"),
        latest
    );
    let failed = host
        .inspect_tender_recoveries(&tender.tender_id)
        .expect("inspect invalidated offer")
        .into_iter()
        .find(|record| record.recovery_id == offer.recovery_id)
        .expect("invalidated recovery record");
    assert_eq!(failed.state, TenderRecoveryState::Failed);
    assert_eq!(failed.diagnostic_code.as_deref(), Some("current_changed"));
    assert!(failed.decision_record.is_none());
}

#[test]
fn approval_invalidates_an_offer_when_the_current_tender_becomes_unreadable() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Readable Current At Offer".into(),
        })
        .expect("create Tender");
    let backup = host
        .create_tender_backup(CreateTenderBackupCommand {
            tender_id: tender.tender_id.clone(),
        })
        .expect("create backup");
    let offer = host
        .prepare_tender_recovery(PrepareTenderRecoveryCommand {
            tender_id: tender.tender_id.clone(),
            backup_id: backup.backup_id,
        })
        .expect("prepare recovery offer");
    host.close_tender(&tender.tender_id).expect("close Tender");
    let database = application_home
        .join("tenders")
        .join(&tender.tender_id)
        .join("tender.sqlite");
    OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(&database)
        .expect("open current database")
        .sync_all()
        .expect("make current database unreadable");

    let error = host
        .resolve_tender_recovery(ResolveTenderRecoveryCommand {
            tender_id: tender.tender_id.clone(),
            recovery_id: offer.recovery_id.clone(),
            decision: TenderRecoveryDecision::ApproveReplacement,
            rationale: "Approval cannot outlive loss of current-state identity evidence".into(),
        })
        .expect_err("unreadable current state invalidates the offer");
    assert_eq!(error.code, TenderErrorCode::InvalidCommand);
    assert_eq!(
        std::fs::metadata(database)
            .expect("current database remains")
            .len(),
        0
    );
    let failed = host
        .inspect_tender_recoveries(&tender.tender_id)
        .expect("inspect invalidated offer")
        .into_iter()
        .find(|record| record.recovery_id == offer.recovery_id)
        .expect("invalidated recovery record");
    assert_eq!(failed.state, TenderRecoveryState::Failed);
    assert_eq!(failed.diagnostic_code.as_deref(), Some("current_changed"));
    assert!(failed.decision_record.is_none());
}

#[test]
fn concurrent_approvals_are_serialized_and_only_one_can_replace_the_tender() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Concurrent Recovery Baseline".into(),
        })
        .expect("create Tender");
    let expected = host
        .open_tender(&tender.tender_id)
        .expect("inspect backup state");
    let backup = host
        .create_tender_backup(CreateTenderBackupCommand {
            tender_id: tender.tender_id.clone(),
        })
        .expect("create backup");
    host.revise_tender(ReviseTenderCommand {
        tender_id: tender.tender_id.clone(),
        name: "Current Before Concurrent Approval".into(),
    })
    .expect("revise current Tender");
    let offers = (0..2)
        .map(|_| {
            host.prepare_tender_recovery(PrepareTenderRecoveryCommand {
                tender_id: tender.tender_id.clone(),
                backup_id: backup.backup_id.clone(),
            })
            .expect("prepare independent recovery offer")
        })
        .collect::<Vec<_>>();
    let barrier = Arc::new(Barrier::new(3));
    let handles = offers
        .into_iter()
        .map(|offer| {
            let host = host.clone();
            let tender_id = tender.tender_id.clone();
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                host.resolve_tender_recovery(ResolveTenderRecoveryCommand {
                    tender_id,
                    recovery_id: offer.recovery_id,
                    decision: TenderRecoveryDecision::ApproveReplacement,
                    rationale: "Approve this exact verified replacement under exclusive ownership"
                        .into(),
                })
            })
        })
        .collect::<Vec<_>>();
    barrier.wait();
    let outcomes = handles
        .into_iter()
        .map(|handle| handle.join().expect("approval thread"))
        .collect::<Vec<_>>();
    assert_eq!(outcomes.iter().filter(|outcome| outcome.is_ok()).count(), 1);
    assert_eq!(
        outcomes.iter().filter(|outcome| outcome.is_err()).count(),
        1
    );
    assert_eq!(
        host.open_tender(&tender.tender_id)
            .expect("open exactly recovered Tender"),
        expected
    );
    let recoveries = host
        .inspect_tender_recoveries(&tender.tender_id)
        .expect("inspect serialized decisions");
    assert_eq!(
        recoveries
            .iter()
            .filter(|record| record.state == TenderRecoveryState::Applied)
            .count(),
        1
    );
    assert_eq!(
        recoveries
            .iter()
            .filter(|record| record.diagnostic_code.as_deref() == Some("current_changed"))
            .count(),
        1
    );
}

#[test]
fn insufficient_recovery_space_preserves_the_current_tender_and_records_diagnostics() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Recovery Space Bound".into(),
        })
        .expect("create Tender");
    let current = host.open_tender(&tender.tender_id).expect("inspect Tender");
    let backup = host
        .create_tender_backup(CreateTenderBackupCommand {
            tender_id: tender.tender_id.clone(),
        })
        .expect("create backup");
    std::env::set_var(
        "QUANTIX_RECOVERY_AVAILABLE_SPACE_BYTES",
        format!("{}:0", tender.tender_id),
    );

    let error = host
        .prepare_tender_recovery(PrepareTenderRecoveryCommand {
            tender_id: tender.tender_id.clone(),
            backup_id: backup.backup_id,
        })
        .expect_err("recovery must preflight staging capacity");
    std::env::remove_var("QUANTIX_RECOVERY_AVAILABLE_SPACE_BYTES");
    assert_eq!(error.code, TenderErrorCode::InsufficientSpace);
    assert_eq!(
        host.open_tender(&tender.tender_id)
            .expect("space failure preserves current Tender"),
        current
    );
    let failed = host
        .inspect_tender_recoveries(&tender.tender_id)
        .expect("inspect recovery space diagnostic");
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].state, TenderRecoveryState::Failed);
    assert_eq!(
        failed[0].diagnostic_code.as_deref(),
        Some("insufficient_space")
    );
}

#[test]
fn candidate_damaged_after_offer_is_not_applied_and_becomes_a_failed_diagnostic() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Current Tender Preserved".into(),
        })
        .expect("create Tender");
    let backup = host
        .create_tender_backup(CreateTenderBackupCommand {
            tender_id: tender.tender_id.clone(),
        })
        .expect("create backup");
    let current = host
        .revise_tender(ReviseTenderCommand {
            tender_id: tender.tender_id.clone(),
            name: "Newer Current Tender".into(),
        })
        .expect("revise current Tender");
    let offer = host
        .prepare_tender_recovery(PrepareTenderRecoveryCommand {
            tender_id: tender.tender_id.clone(),
            backup_id: backup.backup_id,
        })
        .expect("prepare verified candidate");
    rusqlite::Connection::open(
        application_home
            .join("staging")
            .join(format!("recovery-{}", offer.recovery_id))
            .join("tender.sqlite"),
    )
    .expect("open staged candidate for corruption injection")
    .execute_batch("DROP TRIGGER audit_events_no_update")
    .expect("damage staged candidate schema");

    let error = host
        .resolve_tender_recovery(ResolveTenderRecoveryCommand {
            tender_id: tender.tender_id.clone(),
            recovery_id: offer.recovery_id.clone(),
            decision: TenderRecoveryDecision::ApproveReplacement,
            rationale: "Approve only if the candidate still verifies".into(),
        })
        .expect_err("damaged candidate cannot replace current Tender");
    assert_eq!(error.code, TenderErrorCode::IntegrityFailed);
    assert_eq!(
        host.open_tender(&tender.tender_id)
            .expect("failed approval preserves current Tender"),
        current
    );
    let failed = host
        .inspect_tender_recoveries(&tender.tender_id)
        .expect("inspect failed approval")
        .into_iter()
        .find(|record| record.recovery_id == offer.recovery_id)
        .expect("recovery diagnostic");
    assert_eq!(failed.state, TenderRecoveryState::Failed);
    assert_eq!(failed.diagnostic_code.as_deref(), Some("integrity_failed"));
}

#[test]
fn host_death_at_backup_publication_boundaries_preserves_the_tender_and_records_interruption() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Committed Before Backup Crash".into(),
        })
        .expect("create Tender");
    let baseline = host.open_tender(&tender.tender_id).expect("inspect Tender");
    host.close_tender(&tender.tender_id).expect("close Tender");
    drop(host);

    assert!(!run_storage_fixture(
        &application_home,
        &["backup", &tender.tender_id],
        "backup_after_verify",
    ));
    let restarted =
        QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    let records = restarted
        .inspect_tender_backups(&tender.tender_id)
        .expect("reconcile verified unpublished backup candidate");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].state, TenderBackupState::Failed);
    assert_eq!(records[0].diagnostic_code.as_deref(), Some("interrupted"));
    assert_eq!(
        restarted
            .open_tender(&tender.tender_id)
            .expect("backup interruption preserves Tender"),
        baseline
    );
    restarted
        .close_tender(&tender.tender_id)
        .expect("close Tender before second fixture");
    drop(restarted);

    assert!(!run_storage_fixture(
        &application_home,
        &["backup", &tender.tender_id],
        "backup_after_publish",
    ));
    let restarted =
        QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    let records = restarted
        .inspect_tender_backups(&tender.tender_id)
        .expect("reconcile published uncommitted backup candidate");
    assert_eq!(records.len(), 2);
    assert!(records.iter().all(|record| {
        record.state == TenderBackupState::Failed
            && record.diagnostic_code.as_deref() == Some("interrupted")
    }));
    assert_eq!(
        restarted
            .open_tender(&tender.tender_id)
            .expect("published backup interruption preserves Tender"),
        baseline
    );
}

#[test]
fn host_death_during_recovery_rolls_back_or_completes_only_the_approved_replacement() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Verified Recovery Source".into(),
        })
        .expect("create Tender");
    let backed_up = host.open_tender(&tender.tender_id).expect("inspect source");
    let backup = host
        .create_tender_backup(CreateTenderBackupCommand {
            tender_id: tender.tender_id.clone(),
        })
        .expect("create verified backup");
    let current = host
        .revise_tender(ReviseTenderCommand {
            tender_id: tender.tender_id.clone(),
            name: "Current State Before Recovery Crash".into(),
        })
        .expect("revise current Tender");
    host.close_tender(&tender.tender_id).expect("close Tender");
    drop(host);

    assert!(!run_storage_fixture(
        &application_home,
        &["prepare-recovery", &tender.tender_id, &backup.backup_id],
        "recovery_after_verify",
    ));
    let restarted =
        QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    let interrupted_prepare = restarted
        .inspect_tender_recoveries(&tender.tender_id)
        .expect("reconcile interrupted preparation");
    assert_eq!(interrupted_prepare.len(), 1);
    assert_eq!(interrupted_prepare[0].state, TenderRecoveryState::Failed);
    assert_eq!(
        restarted
            .open_tender(&tender.tender_id)
            .expect("preparation crash preserves current Tender"),
        current
    );
    let offer = restarted
        .prepare_tender_recovery(PrepareTenderRecoveryCommand {
            tender_id: tender.tender_id.clone(),
            backup_id: backup.backup_id.clone(),
        })
        .expect("prepare replacement for rollback boundary");
    let rollback_recovery_id = offer.recovery_id.clone();
    restarted
        .close_tender(&tender.tender_id)
        .expect("close current Tender");
    drop(restarted);

    assert!(!run_storage_fixture(
        &application_home,
        &["apply-recovery", &tender.tender_id, &offer.recovery_id],
        "recovery_after_current_retained",
    ));
    let restarted =
        QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(
        restarted
            .open_tender(&tender.tender_id)
            .expect("restart rolls back incomplete replacement"),
        current
    );
    let rolled_back = restarted
        .inspect_tender_recoveries(&tender.tender_id)
        .expect("inspect interrupted approved replacement")
        .into_iter()
        .find(|record| record.recovery_id == rollback_recovery_id)
        .expect("rolled-back recovery record");
    assert_eq!(rolled_back.state, TenderRecoveryState::Failed);
    assert_eq!(
        rolled_back
            .decision_record
            .expect("approval remains attributable after interruption")
            .decision,
        TenderRecoveryDecision::ApproveReplacement
    );
    let offer = restarted
        .prepare_tender_recovery(PrepareTenderRecoveryCommand {
            tender_id: tender.tender_id.clone(),
            backup_id: backup.backup_id,
        })
        .expect("prepare replacement for publish boundary");
    let published_recovery_id = offer.recovery_id.clone();
    restarted
        .close_tender(&tender.tender_id)
        .expect("close current Tender");
    drop(restarted);

    assert!(!run_storage_fixture(
        &application_home,
        &["apply-recovery", &tender.tender_id, &offer.recovery_id],
        "recovery_after_publish",
    ));
    let restarted =
        QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(
        restarted
            .open_tender(&tender.tender_id)
            .expect("restart completes explicitly approved replacement"),
        backed_up
    );
    let published = restarted
        .inspect_tender_recoveries(&tender.tender_id)
        .expect("inspect reconciled recovery decisions")
        .into_iter()
        .find(|record| record.recovery_id == published_recovery_id)
        .expect("published recovery record");
    assert_eq!(published.state, TenderRecoveryState::Applied);
    assert_eq!(
        published
            .decision_record
            .expect("published approval remains immutable")
            .decision,
        TenderRecoveryDecision::ApproveReplacement
    );
}

#[test]
fn failed_publish_and_restore_stays_applying_until_restart_restores_the_current_tender() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Rollback Failure Baseline".into(),
        })
        .expect("create Tender");
    let backup = host
        .create_tender_backup(CreateTenderBackupCommand {
            tender_id: tender.tender_id.clone(),
        })
        .expect("create backup");
    let current = host
        .revise_tender(ReviseTenderCommand {
            tender_id: tender.tender_id.clone(),
            name: "Current Must Be Restored".into(),
        })
        .expect("revise current Tender");
    let offer = host
        .prepare_tender_recovery(PrepareTenderRecoveryCommand {
            tender_id: tender.tender_id.clone(),
            backup_id: backup.backup_id,
        })
        .expect("prepare recovery");
    std::env::set_var(
        "QUANTIX_RECOVERY_IO_FAILURE",
        format!("{}:publish_and_restore", tender.tender_id),
    );

    let error = host
        .resolve_tender_recovery(ResolveTenderRecoveryCommand {
            tender_id: tender.tender_id.clone(),
            recovery_id: offer.recovery_id.clone(),
            decision: TenderRecoveryDecision::ApproveReplacement,
            rationale: "Approve the exact candidate while retaining rollback evidence".into(),
        })
        .expect_err("injected publication and restoration failure");
    std::env::remove_var("QUANTIX_RECOVERY_IO_FAILURE");
    assert_eq!(error.code, TenderErrorCode::StoreUnavailable);
    let applying = host
        .inspect_tender_recoveries(&tender.tender_id)
        .expect("inspect nonterminal recovery")
        .into_iter()
        .find(|record| record.recovery_id == offer.recovery_id)
        .expect("applying recovery");
    assert_eq!(applying.state, TenderRecoveryState::Applying);
    assert_eq!(
        applying
            .decision_record
            .expect("approval is durable before publication")
            .decision,
        TenderRecoveryDecision::ApproveReplacement
    );
    assert_eq!(
        host.open_tender(&tender.tender_id)
            .expect_err("partial layout stays fail-closed")
            .code,
        TenderErrorCode::RecoveryRequired
    );
    drop(host);

    let restarted =
        QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(
        restarted
            .open_tender(&tender.tender_id)
            .expect("startup restores retained current Tender"),
        current
    );
    let failed = restarted
        .inspect_tender_recoveries(&tender.tender_id)
        .expect("inspect reconciled recovery")
        .into_iter()
        .find(|record| record.recovery_id == offer.recovery_id)
        .expect("reconciled recovery record");
    assert_eq!(failed.state, TenderRecoveryState::Failed);
    assert_eq!(failed.diagnostic_code.as_deref(), Some("interrupted"));
    assert_eq!(
        failed
            .decision_record
            .expect("failed outcome retains approval fact")
            .decision,
        TenderRecoveryDecision::ApproveReplacement
    );
}

#[test]
fn post_publish_verification_failure_restores_the_retained_current_tender() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Post-Publish Rollback Baseline".into(),
        })
        .expect("create Tender");
    let backup = host
        .create_tender_backup(CreateTenderBackupCommand {
            tender_id: tender.tender_id.clone(),
        })
        .expect("create backup");
    let current = host
        .revise_tender(ReviseTenderCommand {
            tender_id: tender.tender_id.clone(),
            name: "Retained Current Must Win".into(),
        })
        .expect("revise current Tender");
    let offer = host
        .prepare_tender_recovery(PrepareTenderRecoveryCommand {
            tender_id: tender.tender_id.clone(),
            backup_id: backup.backup_id,
        })
        .expect("prepare recovery");
    std::env::set_var(
        "QUANTIX_RECOVERY_IO_FAILURE",
        format!("{}:damage_after_publish", tender.tender_id),
    );
    let result = host.resolve_tender_recovery(ResolveTenderRecoveryCommand {
        tender_id: tender.tender_id.clone(),
        recovery_id: offer.recovery_id.clone(),
        decision: TenderRecoveryDecision::ApproveReplacement,
        rationale: "Apply only if the published candidate still verifies".into(),
    });
    std::env::remove_var("QUANTIX_RECOVERY_IO_FAILURE");

    assert_eq!(
        result.expect_err("damaged publication must roll back").code,
        TenderErrorCode::IntegrityFailed
    );
    assert_eq!(
        host.open_tender(&tender.tender_id)
            .expect("retained current is restored immediately"),
        current
    );
    let failed = host
        .inspect_tender_recoveries(&tender.tender_id)
        .expect("inspect rolled-back recovery")
        .into_iter()
        .find(|record| record.recovery_id == offer.recovery_id)
        .expect("failed recovery record");
    assert_eq!(failed.state, TenderRecoveryState::Failed);
    assert_eq!(failed.diagnostic_code.as_deref(), Some("integrity_failed"));
    assert_eq!(
        failed
            .decision_record
            .expect("approval remains attributable")
            .decision,
        TenderRecoveryDecision::ApproveReplacement
    );
}

#[test]
fn restart_rolls_back_a_published_candidate_that_no_longer_verifies() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Restart Rollback Baseline".into(),
        })
        .expect("create Tender");
    let backup = host
        .create_tender_backup(CreateTenderBackupCommand {
            tender_id: tender.tender_id.clone(),
        })
        .expect("create backup");
    let current = host
        .revise_tender(ReviseTenderCommand {
            tender_id: tender.tender_id.clone(),
            name: "Original Current Survives Restart".into(),
        })
        .expect("revise current Tender");
    let offer = host
        .prepare_tender_recovery(PrepareTenderRecoveryCommand {
            tender_id: tender.tender_id.clone(),
            backup_id: backup.backup_id,
        })
        .expect("prepare recovery");
    host.close_tender(&tender.tender_id).expect("close Tender");
    drop(host);

    assert!(!run_storage_fixture(
        &application_home,
        &["apply-recovery", &tender.tender_id, &offer.recovery_id],
        "recovery_after_publish",
    ));
    OpenOptions::new()
        .append(true)
        .open(
            application_home
                .join("tenders")
                .join(&tender.tender_id)
                .join("tender.sqlite"),
        )
        .expect("open published candidate")
        .write_all(b"post-publication-corruption")
        .expect("damage published candidate");

    let restarted =
        QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(
        restarted
            .open_tender(&tender.tender_id)
            .expect("restart restores the retained current Tender"),
        current
    );
    let failed = restarted
        .inspect_tender_recoveries(&tender.tender_id)
        .expect("inspect startup rollback")
        .into_iter()
        .find(|record| record.recovery_id == offer.recovery_id)
        .expect("failed recovery record");
    assert_eq!(failed.state, TenderRecoveryState::Failed);
    assert_eq!(failed.diagnostic_code.as_deref(), Some("integrity_failed"));
}

#[test]
fn restart_timeout_restores_current_and_records_the_exact_limit() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Restart Deadline Baseline".into(),
        })
        .expect("create Tender");
    let backup = host
        .create_tender_backup(CreateTenderBackupCommand {
            tender_id: tender.tender_id.clone(),
        })
        .expect("create backup");
    let current = host
        .revise_tender(ReviseTenderCommand {
            tender_id: tender.tender_id.clone(),
            name: "Current Restored After Deadline".into(),
        })
        .expect("revise current Tender");
    let offer = host
        .prepare_tender_recovery(PrepareTenderRecoveryCommand {
            tender_id: tender.tender_id.clone(),
            backup_id: backup.backup_id,
        })
        .expect("prepare recovery");
    host.close_tender(&tender.tender_id).expect("close Tender");
    drop(host);
    assert!(!run_storage_fixture(
        &application_home,
        &["apply-recovery", &tender.tender_id, &offer.recovery_id],
        "recovery_after_publish",
    ));

    std::env::set_var(
        "QUANTIX_STORAGE_OPERATION_TIMEOUT",
        format!("{}:expired", tender.tender_id),
    );
    let restarted =
        QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    let reopened = restarted.open_tender(&tender.tender_id);
    std::env::remove_var("QUANTIX_STORAGE_OPERATION_TIMEOUT");
    assert_eq!(
        reopened.expect("startup restores retained current after timeout"),
        current
    );
    let failed = restarted
        .inspect_tender_recoveries(&tender.tender_id)
        .expect("inspect timeout recovery")
        .into_iter()
        .find(|record| record.recovery_id == offer.recovery_id)
        .expect("failed recovery record");
    assert_eq!(failed.state, TenderRecoveryState::Failed);
    assert_eq!(
        failed.diagnostic_code.as_deref(),
        Some("operation_timed_out")
    );
}

#[test]
fn backup_and_recovery_deadlines_fail_without_publishing_partial_state() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Bounded Storage Operation".into(),
        })
        .expect("create Tender");
    std::env::set_var(
        "QUANTIX_STORAGE_OPERATION_TIMEOUT",
        format!("{}:expired", tender.tender_id),
    );
    let timed_out = host.create_tender_backup(CreateTenderBackupCommand {
        tender_id: tender.tender_id.clone(),
    });
    std::env::remove_var("QUANTIX_STORAGE_OPERATION_TIMEOUT");
    assert_eq!(
        timed_out.expect_err("expired backup deadline").code,
        TenderErrorCode::OperationTimedOut
    );
    let failed_backup = host
        .inspect_tender_backups(&tender.tender_id)
        .expect("inspect timed-out backup")
        .into_iter()
        .next()
        .expect("failed backup record");
    assert_eq!(failed_backup.state, TenderBackupState::Failed);
    assert_eq!(
        failed_backup.diagnostic_code.as_deref(),
        Some("operation_timed_out")
    );

    let backup = host
        .create_tender_backup(CreateTenderBackupCommand {
            tender_id: tender.tender_id.clone(),
        })
        .expect("create recovery source without expired deadline");
    std::env::set_var(
        "QUANTIX_STORAGE_OPERATION_TIMEOUT",
        format!("{}:expired", tender.tender_id),
    );
    let timed_out = host.prepare_tender_recovery(PrepareTenderRecoveryCommand {
        tender_id: tender.tender_id.clone(),
        backup_id: backup.backup_id,
    });
    std::env::remove_var("QUANTIX_STORAGE_OPERATION_TIMEOUT");
    assert_eq!(
        timed_out.expect_err("expired recovery deadline").code,
        TenderErrorCode::OperationTimedOut
    );
    let failed_recovery = host
        .inspect_tender_recoveries(&tender.tender_id)
        .expect("inspect timed-out recovery")
        .into_iter()
        .next()
        .expect("failed recovery record");
    assert_eq!(failed_recovery.state, TenderRecoveryState::Failed);
    assert_eq!(
        failed_recovery.diagnostic_code.as_deref(),
        Some("operation_timed_out")
    );
    assert_eq!(
        host.open_tender(&tender.tender_id)
            .expect("deadline failures preserve current Tender"),
        tender
    );
}

#[test]
fn unsafe_archive_entry_is_rejected_before_recovery_is_offered() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Path Safe Tender".into(),
        })
        .expect("create Tender");
    let backup = host
        .create_tender_backup(CreateTenderBackupCommand {
            tender_id: tender.tender_id.clone(),
        })
        .expect("create backup");
    let archive_path = application_home
        .join("backups")
        .join(format!("{}.qtbackup", backup.backup_id));
    let archive = File::options()
        .read(true)
        .write(true)
        .open(&archive_path)
        .expect("open archive for unsafe entry injection");
    let mut archive = ZipWriter::new_append(archive).expect("append archive entry");
    archive
        .start_file(
            "../outside",
            SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
        )
        .expect("start unsafe archive entry");
    archive
        .write_all(b"must never extract")
        .expect("write unsafe entry");
    archive.finish().expect("finish injected archive");
    let archive_size = std::fs::metadata(&archive_path)
        .expect("inspect injected archive")
        .len();
    rusqlite::Connection::open(application_home.join("installation.sqlite"))
        .expect("open installation catalogue for fixture binding")
        .execute(
            "UPDATE tender_backups SET archive_size_bytes = ?2 WHERE backup_id = ?1",
            rusqlite::params![
                backup.backup_id,
                i64::try_from(archive_size).expect("archive size fits installation catalogue")
            ],
        )
        .expect("bind injected archive size");

    let error = host
        .prepare_tender_recovery(PrepareTenderRecoveryCommand {
            tender_id: tender.tender_id.clone(),
            backup_id: backup.backup_id,
        })
        .expect_err("unsafe archive must not be offered");
    assert_eq!(error.code, TenderErrorCode::IntegrityFailed);
    assert_eq!(
        host.inspect_tender_recoveries(&tender.tender_id)
            .expect("inspect failed recovery")[0]
            .state,
        TenderRecoveryState::Failed
    );
    assert!(!application_home.join("outside").exists());
}

#[test]
fn backup_from_another_tender_cannot_be_prepared_or_merged() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let source = host
        .create_tender(CreateTenderCommand {
            name: "Source Identity".into(),
        })
        .expect("create source Tender");
    let target = host
        .create_tender(CreateTenderCommand {
            name: "Target Identity".into(),
        })
        .expect("create target Tender");
    let target_before = host.open_tender(&target.tender_id).expect("inspect target");
    let backup = host
        .create_tender_backup(CreateTenderBackupCommand {
            tender_id: source.tender_id,
        })
        .expect("back up source Tender");

    let error = host
        .prepare_tender_recovery(PrepareTenderRecoveryCommand {
            tender_id: target.tender_id.clone(),
            backup_id: backup.backup_id,
        })
        .expect_err("backup identity cannot cross Tender boundary");
    assert_eq!(error.code, TenderErrorCode::InvalidCommand);
    assert!(host
        .inspect_tender_recoveries(&target.tender_id)
        .expect("inspect target recovery history")
        .is_empty());
    assert_eq!(
        host.open_tender(&target.tender_id)
            .expect("identity collision cannot mutate target"),
        target_before
    );
}

#[test]
fn rejected_recovery_preserves_current_state_and_records_the_decision() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Backed Up State".into(),
        })
        .expect("create Tender");
    let backup = host
        .create_tender_backup(CreateTenderBackupCommand {
            tender_id: tender.tender_id.clone(),
        })
        .expect("create backup");
    let current = host
        .revise_tender(ReviseTenderCommand {
            tender_id: tender.tender_id.clone(),
            name: "Current State Kept".into(),
        })
        .expect("revise current Tender");
    let offer = host
        .prepare_tender_recovery(PrepareTenderRecoveryCommand {
            tender_id: tender.tender_id.clone(),
            backup_id: backup.backup_id,
        })
        .expect("prepare recovery offer");

    let rejected = host
        .resolve_tender_recovery(ResolveTenderRecoveryCommand {
            tender_id: tender.tender_id.clone(),
            recovery_id: offer.recovery_id,
            decision: TenderRecoveryDecision::Reject,
            rationale: "The current audited revision remains authoritative".into(),
        })
        .expect("reject recovery offer");
    assert_eq!(rejected.state, TenderRecoveryState::Rejected);
    let decision = rejected
        .decision_record
        .as_ref()
        .expect("immutable Engineer decision");
    assert_eq!(decision.decision, TenderRecoveryDecision::Reject);
    assert_eq!(decision.decided_by, "engineer_user");
    assert_eq!(decision.manifest_sha256, backup.manifest_sha256.unwrap());
    assert_eq!(
        decision.current_audit_chain_head.as_deref(),
        Some(current.audit_chain_head.as_str())
    );
    assert!(rusqlite::Connection::open(application_home.join("installation.sqlite"))
        .expect("open installation facts for immutability injection")
        .execute(
            "UPDATE tender_recovery_decisions SET rationale = 'rewritten' WHERE recovery_id = ?1",
            [&rejected.recovery_id],
        )
        .is_err());
    assert_eq!(
        host.open_tender(&tender.tender_id)
            .expect("rejection preserves current Tender"),
        current
    );
}

#[test]
fn approved_verified_backup_replaces_a_recovery_required_tender_and_purge_removes_retained_store() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Recoverable Tender".into(),
        })
        .expect("create Tender");
    host.register_tender_content(RegisterTenderContentCommand {
        tender_id: tender.tender_id.clone(),
        logical_id: "baseline".into(),
        media_type: "text/plain".into(),
        bytes: b"complete backup content".to_vec(),
    })
    .expect("register content");
    let expected = host.open_tender(&tender.tender_id).expect("inspect source");
    let backup = host
        .create_tender_backup(CreateTenderBackupCommand {
            tender_id: tender.tender_id.clone(),
        })
        .expect("create backup");
    remove_registered_content(&application_home, &tender.tender_id);
    assert_eq!(
        host.inspect_tender_integrity(&tender.tender_id)
            .expect("inspect corrupted current Tender")
            .state,
        TenderIntegrityState::RecoveryRequired
    );
    let offer = host
        .prepare_tender_recovery(PrepareTenderRecoveryCommand {
            tender_id: tender.tender_id.clone(),
            backup_id: backup.backup_id,
        })
        .expect("prepare verified replacement");

    let applied = host
        .resolve_tender_recovery(ResolveTenderRecoveryCommand {
            tender_id: tender.tender_id.clone(),
            recovery_id: offer.recovery_id,
            decision: TenderRecoveryDecision::ApproveReplacement,
            rationale: "Replace the incomplete current store with the verified backup".into(),
        })
        .expect("approve exact replacement");
    assert_eq!(applied.state, TenderRecoveryState::Applied);
    assert_eq!(
        host.open_tender(&tender.tender_id)
            .expect("open recovered Tender"),
        expected
    );
    assert_eq!(
        host.inspect_tender_integrity(&tender.tender_id)
            .expect("verify replacement")
            .state,
        TenderIntegrityState::Ready
    );
    let retained = application_home
        .join("trash")
        .join(format!("recovery-replaced-{}", applied.recovery_id));
    assert!(retained.exists());

    remove_registered_content(&application_home, &tender.tender_id);
    assert_eq!(
        host.inspect_tender_integrity(&tender.tender_id)
            .expect("latch recovered Tender after a second corruption")
            .state,
        TenderIntegrityState::RecoveryRequired
    );
    let trashed = host
        .trash_recovery_required_tender(TrashRecoveryRequiredTenderCommand {
            tender_id: tender.tender_id.clone(),
            rationale: "Remove the damaged recovered Store".into(),
        })
        .expect("trash damaged recovered Store");
    host.purge_recovery_required_tender(PurgeRecoveryRequiredTenderCommand {
        tender_id: tender.tender_id,
        rationale: "Remove every retained Quantix recovery copy".into(),
        confirmation_tender_name: "Recoverable Tender".into(),
    })
    .expect("purge damaged recovered Store and retained recovery copy");
    assert!(!retained.exists());
    assert!(!application_home
        .join("trash")
        .join(trashed.relative_path)
        .exists());
}

#[test]
fn corrupted_backup_is_rejected_without_touching_the_tender_and_records_diagnostics() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Unchanged Live Tender".into(),
        })
        .expect("create Tender");
    let backup = host
        .create_tender_backup(CreateTenderBackupCommand {
            tender_id: tender.tender_id.clone(),
        })
        .expect("create verified backup");
    let before = host.open_tender(&tender.tender_id).expect("inspect Tender");
    OpenOptions::new()
        .append(true)
        .open(
            application_home
                .join("backups")
                .join(format!("{}.qtbackup", backup.backup_id)),
        )
        .expect("open backup for corruption injection")
        .write_all(b"corrupt")
        .expect("inject corruption");

    let error = host
        .prepare_tender_recovery(PrepareTenderRecoveryCommand {
            tender_id: tender.tender_id.clone(),
            backup_id: backup.backup_id,
        })
        .expect_err("altered backup must not become a recovery offer");
    assert_eq!(error.code, TenderErrorCode::IntegrityFailed);
    let recoveries = host
        .inspect_tender_recoveries(&tender.tender_id)
        .expect("inspect attributable recovery diagnostic");
    assert_eq!(recoveries.len(), 1);
    assert_eq!(recoveries[0].state, TenderRecoveryState::Failed);
    assert_eq!(
        recoveries[0].diagnostic_code.as_deref(),
        Some("integrity_failed")
    );
    assert_eq!(
        host.open_tender(&tender.tender_id)
            .expect("failed recovery cannot mutate current Tender"),
        before
    );
}

#[test]
fn missing_source_content_blocks_backup_and_records_recovery_required() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
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
        .expect("open Tender Store for corruption injection");
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

    let error = host
        .create_tender_backup(CreateTenderBackupCommand {
            tender_id: tender.tender_id.clone(),
        })
        .expect_err("incomplete source cannot be backed up");
    assert_eq!(error.code, TenderErrorCode::RecoveryRequired);
    let records = host
        .inspect_tender_backups(&tender.tender_id)
        .expect("inspect failed backup diagnostic");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].state, TenderBackupState::Failed);
    assert_eq!(
        records[0].diagnostic_code.as_deref(),
        Some("recovery_required")
    );
    assert_eq!(
        host.open_tender(&tender.tender_id)
            .expect_err("failed source integrity latches Recovery Required")
            .code,
        TenderErrorCode::RecoveryRequired
    );
}

#[test]
fn insufficient_backup_space_preserves_the_tender_and_records_a_diagnostic() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Space Bounded Backup".into(),
        })
        .expect("create Tender");
    let before = host
        .open_tender(&tender.tender_id)
        .expect("inspect Tender before backup");
    std::env::set_var(
        "QUANTIX_BACKUP_AVAILABLE_SPACE_BYTES",
        format!("{}:0", tender.tender_id),
    );

    let error = host
        .create_tender_backup(CreateTenderBackupCommand {
            tender_id: tender.tender_id.clone(),
        })
        .expect_err("insufficient space must fail before publication");
    std::env::remove_var("QUANTIX_BACKUP_AVAILABLE_SPACE_BYTES");
    assert_eq!(error.code, TenderErrorCode::InsufficientSpace);
    let records = host
        .inspect_tender_backups(&tender.tender_id)
        .expect("inspect attributable backup diagnostic");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].state, TenderBackupState::Failed);
    assert_eq!(
        records[0].diagnostic_code.as_deref(),
        Some("insufficient_space")
    );
    assert_eq!(
        host.open_tender(&tender.tender_id)
            .expect("failed backup cannot mutate Tender"),
        before
    );
}

#[test]
fn verified_recovery_waits_for_explicit_approval_then_replaces_without_merging() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Original Tender State".into(),
        })
        .expect("create Tender");
    host.register_tender_content(RegisterTenderContentCommand {
        tender_id: tender.tender_id.clone(),
        logical_id: "baseline".into(),
        media_type: "text/plain".into(),
        bytes: b"verified baseline".to_vec(),
    })
    .expect("register canonical content");
    let backed_up = host
        .open_tender(&tender.tender_id)
        .expect("inspect backed-up state");
    let backup = host
        .create_tender_backup(CreateTenderBackupCommand {
            tender_id: tender.tender_id.clone(),
        })
        .expect("create verified backup");
    let changed = host
        .revise_tender(ReviseTenderCommand {
            tender_id: tender.tender_id.clone(),
            name: "Changed After Backup".into(),
        })
        .expect("change current Tender after backup");

    let offer = host
        .prepare_tender_recovery(PrepareTenderRecoveryCommand {
            tender_id: tender.tender_id.clone(),
            backup_id: backup.backup_id,
        })
        .expect("prepare and verify recovery candidate");
    assert_eq!(offer.state, TenderRecoveryState::AwaitingApproval);
    assert_eq!(offer.backup_source, Some(backed_up.clone()));
    assert_eq!(offer.current_source, Some(changed.clone()));
    assert_eq!(
        host.open_tender(&tender.tender_id)
            .expect("preparation cannot mutate current Tender"),
        changed
    );

    let applied = host
        .resolve_tender_recovery(ResolveTenderRecoveryCommand {
            tender_id: tender.tender_id.clone(),
            recovery_id: offer.recovery_id,
            decision: TenderRecoveryDecision::ApproveReplacement,
            rationale: "Restore the last independently verified complete backup".into(),
        })
        .expect("apply exact approved recovery candidate");
    assert_eq!(applied.state, TenderRecoveryState::Applied);
    assert_eq!(
        host.open_tender(&tender.tender_id)
            .expect("open recovered Tender"),
        backed_up
    );
    assert_eq!(
        host.inspect_tender_integrity(&tender.tender_id)
            .expect("verify recovered Tender")
            .state,
        TenderIntegrityState::Ready
    );
    assert_eq!(
        host.inspect_tender(&tender.tender_id)
            .expect("inspect recovered content")
            .content_object_count,
        1
    );
}

#[test]
fn verified_backup_records_a_complete_snapshot_without_mutating_the_tender() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Cairo Metro Backup".into(),
        })
        .expect("create Tender");
    host.register_tender_content(RegisterTenderContentCommand {
        tender_id: tender.tender_id.clone(),
        logical_id: "employer-requirements".into(),
        media_type: "text/plain".into(),
        bytes: b"immutable contractual requirements".to_vec(),
    })
    .expect("register canonical content");
    let before = host
        .open_tender(&tender.tender_id)
        .expect("inspect source before backup");

    let backup = host
        .create_tender_backup(CreateTenderBackupCommand {
            tender_id: tender.tender_id.clone(),
        })
        .expect("create and re-verify complete backup");

    assert_eq!(backup.tender_id, tender.tender_id);
    assert_eq!(backup.state, TenderBackupState::Ready);
    assert_eq!(backup.source, Some(before.clone()));
    assert_eq!(backup.content_object_count, 1);
    assert_eq!(
        host.open_tender(&tender.tender_id)
            .expect("backup must not mutate source Tender"),
        before
    );
    drop(host);

    let restarted =
        QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&restarted).state, SetupState::Ready);
    assert_eq!(
        restarted
            .inspect_tender_backups(&tender.tender_id)
            .expect("inspect durable backup record"),
        vec![backup]
    );
}

fn remove_registered_content(application_home: &Path, tender_id: &str) {
    let tender_root = application_home.join("tenders").join(tender_id);
    let connection = rusqlite::Connection::open(tender_root.join("tender.sqlite"))
        .expect("open Tender Store for corruption injection");
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
}

fn run_storage_fixture(application_home: &Path, arguments: &[&str], failpoint: &str) -> bool {
    Command::new(env!("CARGO_BIN_EXE_quantix-storage-fixture"))
        .arg(application_home)
        .args(arguments)
        .env("QUANTIX_STORAGE_FAILPOINT", failpoint)
        .status()
        .expect("run supervised storage fixture")
        .success()
}

fn copy_directory(source: &Path, target: &Path) {
    std::fs::create_dir(target).expect("create substituted candidate root");
    for entry in walkdir::WalkDir::new(source).into_iter().skip(1) {
        let entry = entry.expect("walk source Tender");
        let relative = entry
            .path()
            .strip_prefix(source)
            .expect("source-relative path");
        let destination = target.join(relative);
        if entry.file_type().is_dir() {
            std::fs::create_dir(&destination).expect("copy Tender directory");
        } else if entry.file_type().is_file() {
            std::fs::copy(entry.path(), &destination).expect("copy Tender file");
        } else {
            panic!("Tender fixture cannot copy linked storage");
        }
    }
}

#[cfg(unix)]
fn create_directory_link(target: &Path, link: &Path) {
    std::os::unix::fs::symlink(target, link).expect("create directory symlink fixture");
}

#[cfg(unix)]
fn remove_directory_link(link: &Path) {
    std::fs::remove_file(link).expect("remove directory symlink fixture");
}

#[cfg(windows)]
fn create_directory_link(target: &Path, link: &Path) {
    junction::create(target, link).expect("create directory junction fixture");
}

#[cfg(windows)]
fn remove_directory_link(link: &Path) {
    junction::delete(link).expect("remove directory junction fixture");
}

#[test]
fn recovery_required_tender_can_be_trashed_and_restored_without_opening_corrupt_store() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Recovery Bytes Retained".into(),
        })
        .expect("create Tender");
    host.register_tender_content(RegisterTenderContentCommand {
        tender_id: tender.tender_id.clone(),
        logical_id: "required".into(),
        media_type: "text/plain".into(),
        bytes: b"bytes that remain opaque during recovery deletion".to_vec(),
    })
    .expect("register canonical content");
    host.list_tenders()
        .expect("seed catalogue before corruption");
    let tender_root = application_home.join("tenders").join(&tender.tender_id);
    let database = tender_root.join("tender.sqlite");
    remove_registered_content(&application_home, &tender.tender_id);
    let backup_error = host
        .create_tender_backup(CreateTenderBackupCommand {
            tender_id: tender.tender_id.clone(),
        })
        .expect_err("missing content must latch recovery_required");
    assert_eq!(backup_error.code, TenderErrorCode::RecoveryRequired);
    let database_bytes = std::fs::read(&database).expect("read latched corrupt-store bytes");

    let trashed = host
        .trash_recovery_required_tender(TrashRecoveryRequiredTenderCommand {
            tender_id: tender.tender_id.clone(),
            rationale: "Remove the damaged store while preserving its bytes".into(),
        })
        .expect("trash recovery-required store without opening it");
    assert_eq!(trashed.tender_name, "Recovery Bytes Retained");
    assert_eq!(
        trashed.deletion_source,
        TenderDeletionSourceState::RecoveryRequired
    );
    assert_eq!(
        trashed.integrity_code.as_deref(),
        Some("referenced_content_missing")
    );
    assert_eq!(trashed.state, TrashedTenderState::Trashed);
    assert_eq!(
        std::fs::read(
            application_home
                .join("trash")
                .join(&trashed.relative_path)
                .join("tender.sqlite")
        )
        .expect("read trashed opaque bytes"),
        database_bytes
    );
    assert!(!tender_root.exists());
    assert!(host
        .list_tenders()
        .expect("refresh active catalogue while recovery Tender is in Trash")
        .is_empty());

    let restored = host
        .restore_trashed_tender(TrashedTenderDecisionCommand {
            deletion_id: trashed.deletion_id,
            rationale: "Restore for a later approved recovery decision".into(),
        })
        .expect("restore recovery-required store without claiming repair");
    assert_eq!(restored.state, TrashedTenderState::Restored);
    assert_eq!(
        host.open_tender(&tender.tender_id)
            .expect_err("restored corrupt store remains recovery-required")
            .code,
        TenderErrorCode::RecoveryRequired
    );
    assert_eq!(
        std::fs::read(database).expect("read restored opaque bytes"),
        database_bytes
    );
    let projection = host
        .inspect_manager_workspace(quantix_lib::InspectManagerWorkspaceCommand { tender_id: None })
        .expect("inspect restored recovery-required Tender");
    assert_eq!(projection.catalogue.len(), 1);
    assert_eq!(projection.catalogue[0].name, "Recovery Bytes Retained");
    assert_eq!(
        projection.catalogue[0].state,
        quantix_lib::ManagerWorkspaceTenderState::RecoveryRequired
    );
}

#[test]
fn recovery_trash_rejects_mismatched_store_identity_and_path_traversal() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Identity Bound Recovery".into(),
        })
        .expect("create Tender");
    host.close_tender(&tender.tender_id)
        .expect("close Tender before identity substitution");
    let mismatched_store = rusqlite::Connection::open(
        application_home
            .join("tenders")
            .join(&tender.tender_id)
            .join("tender.sqlite"),
    )
    .expect("open Tender database");
    mismatched_store
        .execute_batch(
            "PRAGMA foreign_keys = OFF;
             DROP TRIGGER tender_identity_no_update;",
        )
        .expect("remove the Store's own identity guard for the hostile fixture");
    mismatched_store
        .execute(
            "UPDATE tender SET tender_id = ?1 WHERE singleton = 1",
            ["f".repeat(32)],
        )
        .expect("substitute mismatched Tender identity");
    drop(mismatched_store);

    let mismatch = host
        .trash_recovery_required_tender(TrashRecoveryRequiredTenderCommand {
            tender_id: tender.tender_id.clone(),
            rationale: "This mismatched Store must not be moved".into(),
        })
        .expect_err("readable Store identity mismatch must fail closed");
    assert_eq!(mismatch.code, TenderErrorCode::IntegrityFailed);
    assert!(application_home
        .join("tenders")
        .join(&tender.tender_id)
        .exists());

    let traversal = host
        .trash_recovery_required_tender(TrashRecoveryRequiredTenderCommand {
            tender_id: "../outside-quantix-tender-store".into(),
            rationale: "Traversal must never address managed storage".into(),
        })
        .expect_err("path traversal Tender identity must be rejected");
    assert_eq!(traversal.code, TenderErrorCode::InvalidCommand);
}

#[test]
fn recovery_trash_rejects_links_without_touching_their_external_target() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let external = user_home.path().join("external-controlled-copy");
    std::fs::create_dir(&external).expect("create external directory fixture");
    let external_file = external.join("outside.txt");
    std::fs::write(&external_file, b"outside Quantix control").expect("write external fixture");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Linked Recovery Store".into(),
        })
        .expect("create Tender");
    host.close_tender(&tender.tender_id)
        .expect("close Tender before adding hostile link");
    let link = application_home
        .join("tenders")
        .join(&tender.tender_id)
        .join("external-link");
    create_directory_link(&external, &link);

    let error = host
        .trash_recovery_required_tender(TrashRecoveryRequiredTenderCommand {
            tender_id: tender.tender_id.clone(),
            rationale: "Linked storage must fail closed".into(),
        })
        .expect_err("recovery Trash must reject links anywhere in the Store tree");
    assert_eq!(error.code, TenderErrorCode::IntegrityFailed);
    assert!(external_file.exists());
    assert!(application_home
        .join("tenders")
        .join(&tender.tender_id)
        .exists());
    remove_directory_link(&link);
}

#[test]
fn recovery_required_tender_can_be_purged_with_exact_name_and_complete_provider_discovery() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Purge Damaged Tender".into(),
        })
        .expect("create Tender");
    host.register_tender_content(RegisterTenderContentCommand {
        tender_id: tender.tender_id.clone(),
        logical_id: "required".into(),
        media_type: "text/plain".into(),
        bytes: b"purge fixture".to_vec(),
    })
    .expect("register canonical content");
    host.list_tenders()
        .expect("seed catalogue before corruption");
    remove_registered_content(&application_home, &tender.tender_id);
    host.create_tender_backup(CreateTenderBackupCommand {
        tender_id: tender.tender_id.clone(),
    })
    .expect_err("missing content must latch recovery_required");
    let wrong_name = host
        .purge_recovery_required_tender(PurgeRecoveryRequiredTenderCommand {
            tender_id: tender.tender_id.clone(),
            rationale: "Permanent removal requested".into(),
            confirmation_tender_name: "Wrong Name".into(),
        })
        .expect_err("permanent deletion requires exact Tender name");
    assert_eq!(wrong_name.code, TenderErrorCode::InvalidCommand);
    assert!(application_home
        .join("tenders")
        .join(&tender.tender_id)
        .exists());
    assert!(host
        .inspect_trashed_tenders()
        .expect("wrong confirmation must not move the Tender")
        .is_empty());

    let trashed = host
        .trash_recovery_required_tender(TrashRecoveryRequiredTenderCommand {
            tender_id: tender.tender_id.clone(),
            rationale: "Quarantine the damaged local store before permanent removal".into(),
        })
        .expect("trash recovery-required Tender before purge");
    assert_eq!(trashed.state, TrashedTenderState::Trashed);
    assert!(host
        .list_tenders()
        .expect("refresh active catalogue while damaged Tender is in Trash")
        .is_empty());

    let receipt: DeletionReceipt = host
        .purge_recovery_required_tender(PurgeRecoveryRequiredTenderCommand {
            tender_id: tender.tender_id.clone(),
            rationale: "Permanently remove the damaged local store".into(),
            confirmation_tender_name: "Purge Damaged Tender".into(),
        })
        .expect("purge recovery-required Tender without opening corrupt DB");
    assert!(receipt.local_deletion_completed);
    assert_eq!(
        receipt.deletion_source,
        TenderDeletionSourceState::RecoveryRequired
    );
    assert_eq!(
        receipt.integrity_code.as_deref(),
        Some("referenced_content_missing")
    );
    assert_eq!(
        receipt.provider_reference_discovery,
        ProviderReferenceDiscoveryState::Complete
    );
    assert_eq!(
        receipt.provider_cleanup_status,
        ProviderCleanupStatus::NotRequired
    );
    assert!(!application_home
        .join("tenders")
        .join(&tender.tender_id)
        .exists());
    assert!(host
        .inspect_trashed_tenders()
        .expect("direct purge leaves no Trash record")
        .is_empty());
    assert_eq!(
        host.inspect_deletion_receipts().expect("inspect receipt"),
        vec![receipt]
    );
}

#[test]
fn cold_recovery_purge_marks_provider_discovery_incomplete_when_database_is_unreadable() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    let application_home = user_home.path().join(".quantix");
    let external_package = user_home.path().join("Original Tender Package.pdf");
    std::fs::write(
        &external_package,
        b"external source remains outside Quantix",
    )
    .expect("write external Tender Package fixture");
    let host = QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
    let tender = host
        .create_tender(CreateTenderCommand {
            name: "Unreadable Recovery Store".into(),
        })
        .expect("create Tender");
    host.close_tender(&tender.tender_id)
        .expect("close Tender before corrupting its database");
    drop(host);

    std::fs::write(
        application_home
            .join("tenders")
            .join(&tender.tender_id)
            .join("tender.sqlite"),
        b"not a sqlite database",
    )
    .expect("replace Tender database with unreadable bytes");

    let restarted =
        QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
    assert_eq!(ensure_quantix_setup(&restarted).state, SetupState::Ready);
    let receipt = restarted
        .purge_recovery_required_tender(PurgeRecoveryRequiredTenderCommand {
            tender_id: tender.tender_id.clone(),
            rationale: "Permanently remove the unreadable local Store".into(),
            confirmation_tender_name: "Unreadable Recovery Store".into(),
        })
        .expect("cold Host diagnoses and purges the unreadable Store");
    assert!(receipt.local_deletion_completed);
    assert_eq!(
        receipt.provider_reference_discovery,
        ProviderReferenceDiscoveryState::Incomplete
    );
    assert_eq!(
        receipt.provider_cleanup_status,
        ProviderCleanupStatus::Incomplete
    );
    assert_eq!(receipt.provider_thread_count, 0);
    assert!(external_package.exists());
}

#[test]
fn recovery_trash_reconciles_decision_and_move_publication_boundaries_on_restart() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    for failpoint in ["recovery_trash_after_decision", "recovery_trash_after_move"] {
        let application_home = user_home.path().join(failpoint);
        let host =
            QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
        assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
        let tender = host
            .create_tender(CreateTenderCommand {
                name: format!("Recovery Trash crash fixture {failpoint}"),
            })
            .expect("create recovery Trash crash fixture Tender");
        host.register_tender_content(RegisterTenderContentCommand {
            tender_id: tender.tender_id.clone(),
            logical_id: "required".into(),
            media_type: "text/plain".into(),
            bytes: b"recovery Trash crash fixture".to_vec(),
        })
        .expect("register recovery Trash crash fixture content");
        remove_registered_content(&application_home, &tender.tender_id);
        drop(host);

        assert!(!run_storage_fixture(
            &application_home,
            &["trash-recovery", &tender.tender_id],
            failpoint,
        ));

        let restarted =
            QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
        assert_eq!(ensure_quantix_setup(&restarted).state, SetupState::Ready);
        let trash = restarted
            .inspect_trashed_tenders()
            .expect("startup reconciles interrupted recovery Trash move");
        assert_eq!(trash.len(), 1);
        assert_eq!(trash[0].state, TrashedTenderState::Trashed);
        assert_eq!(
            trash[0].deletion_source,
            TenderDeletionSourceState::RecoveryRequired
        );
        assert!(restarted
            .list_tenders()
            .expect("reconciled recovery Trash removes the active row")
            .is_empty());
    }
}

#[test]
fn recovery_purge_reconciles_every_quarantine_publication_boundary_on_restart() {
    let user_home = tempfile::tempdir().expect("temporary user home");
    for failpoint in ["purge_after_decision", "purge_after_local_delete"] {
        let application_home = user_home.path().join(failpoint);
        let tender_name = format!("Recovery crash fixture {failpoint}");
        let host =
            QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
        assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
        let tender = host
            .create_tender(CreateTenderCommand {
                name: tender_name.clone(),
            })
            .expect("create recovery crash fixture Tender");
        host.register_tender_content(RegisterTenderContentCommand {
            tender_id: tender.tender_id.clone(),
            logical_id: "required".into(),
            media_type: "text/plain".into(),
            bytes: b"recovery crash fixture".to_vec(),
        })
        .expect("register recovery crash fixture content");
        remove_registered_content(&application_home, &tender.tender_id);
        host.create_tender_backup(CreateTenderBackupCommand {
            tender_id: tender.tender_id.clone(),
        })
        .expect_err("missing content must latch recovery_required");
        host.trash_recovery_required_tender(TrashRecoveryRequiredTenderCommand {
            tender_id: tender.tender_id.clone(),
            rationale: "Quarantine the damaged Store before the crash fixture".into(),
        })
        .expect("move recovery crash fixture to Trash");
        drop(host);

        assert!(!run_storage_fixture(
            &application_home,
            &["purge-recovery", &tender.tender_id, &tender_name],
            failpoint,
        ));

        let restarted =
            QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
        assert_eq!(ensure_quantix_setup(&restarted).state, SetupState::Ready);
        let receipts = restarted
            .inspect_deletion_receipts()
            .expect("startup reconciles interrupted recovery purge");
        assert_eq!(receipts.len(), 1);
        assert!(receipts[0].local_deletion_completed);
        assert_eq!(
            receipts[0].deletion_source,
            TenderDeletionSourceState::RecoveryRequired
        );
        assert!(restarted
            .inspect_trashed_tenders()
            .expect("reconciled recovery Trash is empty")
            .is_empty());
    }
}
