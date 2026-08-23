//! Secure operational diagnostics journal.
//!
//! The diagnostics API intentionally accepts only typed scalar facts.  It is
//! independent of the application database and can therefore be owned by each
//! [`QuantixHost`] without introducing process-global logging state.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tracing::Subscriber;
use tracing_subscriber::{layer::Context, prelude::*, Layer, Registry};
use ts_rs::TS;
use zip::write::SimpleFileOptions;

const SCHEMA_VERSION: u8 = 1;
pub const DIAGNOSTIC_REDACTION_POLICY_VERSION: u32 = 1;
const NORMAL_CAPACITY: usize = 4_096;
const CRITICAL_CAPACITY: usize = 512;
const ROTATE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_RETAIN_BYTES: u64 = 5 * 1024 * 1024 * 1024;
const DEEP_CAP_BYTES: u64 = 100 * 1024 * 1024;
const DEEP_TTL_SECONDS: u64 = 60 * 60;
const MAX_PAGE: usize = 200;
const RETENTION_INTERVAL: Duration = Duration::from_secs(15 * 60);

#[allow(dead_code)]
const PURGE_TARGET_BYTES: u64 = 4_500 * 1024 * 1024; // 4.5 GiB

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticSeverity {
    Debug,
    Info,
    Warning,
    Error,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticScope {
    Application,
    Tender,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum DiagnosticComponent {
    Startup,
    Package,
    Parsing,
    Embedding,
    Manager,
    AgentRun,
    Process,
    Provider,
    Database,
    Reconciliation,
    Backup,
    Recovery,
    Update,
    Export,
    Renderer,
    Diagnostics,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
pub struct DiagnosticCorrelation {
    pub operation_id: Option<String>,
    pub parent_operation_id: Option<String>,
    pub tender_id: Option<String>,
    pub run_id: Option<String>,
    pub task_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DiagnosticEvent {
    pub schema_version: u8,
    pub timestamp: String,
    pub severity: DiagnosticSeverity,
    pub scope: DiagnosticScope,
    pub component: DiagnosticComponent,
    pub event_name: String,
    pub summary: String,
    pub session_sequence: u64,
    pub correlation: DiagnosticCorrelation,
    pub duration_ms: Option<u64>,
    pub outcome: Option<String>,
    pub error_code: Option<String>,
    pub deep: bool,
    pub request_id: Option<String>,
    pub size_bytes: Option<u64>,
    pub redaction_count: u32,
    pub original_error_event: Option<String>,
    pub initiated_by: Option<String>,
    pub action: Option<String>,
    pub success: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DiagnosticHealth {
    pub enabled: bool,
    pub degraded: bool,
    pub writer_error: Option<String>,
    pub dropped_normal: u64,
    pub dropped_critical: u64,
    pub retained_bytes: u64,
    pub retained_files: u64,
    pub deep_active_tender: Option<String>,
    pub deep_remaining_seconds: Option<u64>,
    pub deep_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DiagnosticTimelineFilter {
    pub scope: Option<DiagnosticScope>,
    pub severity: Option<DiagnosticSeverity>,
    pub component: Option<DiagnosticComponent>,
    pub tender_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DiagnosticTimelinePage {
    pub events: Vec<DiagnosticTimelineEvent>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
    pub snapshot: String,
}

/// Renderer-facing timeline projection. Raw journal records stay private to
/// the host and are converted to this safe display shape at the boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DiagnosticTimelineEvent {
    pub event_id: String,
    pub occurred_at: String,
    pub severity: DiagnosticSeverity,
    pub component: String,
    pub title: String,
    pub detail: Option<String>,
    pub scope: DiagnosticScope,
    pub operation_id: Option<String>,
    pub parent_operation_id: Option<String>,
    pub outcome: Option<String>,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum DiagnosticsStatusState {
    Healthy,
    Degraded,
    Disabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum DiagnosticsDeepState {
    Idle,
    Running,
    Expired,
    Capped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DiagnosticsDeepStatus {
    pub state: DiagnosticsDeepState,
    pub session_id: Option<String>,
    pub started_at: Option<String>,
    pub ends_at: Option<String>,
    pub remaining_seconds: Option<u64>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DiagnosticsStatus {
    pub scope: DiagnosticScope,
    pub tender_id: Option<String>,
    pub state: DiagnosticsStatusState,
    pub retention_days: Option<u32>,
    pub retained_bytes: u64,
    pub retention_limit_bytes: u64,
    pub retained_event_count: Option<u32>,
    pub dropped_event_count: u32,
    pub degraded_reason: Option<String>,
    pub checked_at: String,
    pub deep: DiagnosticsDeepStatus,
    pub component: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct InspectDiagnosticsStatusCommand {
    pub scope: DiagnosticScope,
    pub tender_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct InspectDiagnosticTimelineCommand {
    pub scope: DiagnosticScope,
    pub tender_id: Option<String>,
    pub cursor: Option<String>,
    pub limit: Option<usize>,
    pub severity: Option<DiagnosticSeverity>,
    pub component: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct StartTenderDeepDiagnosticsCommand {
    pub tender_id: String,
    pub policy_revision: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct StopTenderDeepDiagnosticsCommand {
    pub tender_id: String,
    pub session_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct OpenDiagnosticLogsCommand {
    pub scope: DiagnosticScope,
    pub tender_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ExportDiagnosticsSupportBundleCommand {
    pub scope: DiagnosticScope,
    pub tender_id: Option<String>,
    pub include_deep: bool,
    pub policy_revision: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum RendererDiagnosticKind {
    SurfaceUnavailable,
    InteractionFailed,
    StateRecovered,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct RecordRendererDiagnosticCommand {
    pub kind: RendererDiagnosticKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DeepDiagnosticsSession {
    pub session_id: String,
    pub tender_id: String,
    pub started_at: String,
    pub expires_at: String,
    pub bytes_written: u64,
    pub cap_bytes: u64,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct OpenDiagnosticLogsResult {
    pub directory: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DiagnosticSupportBundleResult {
    pub path: String,
    pub sha256: String,
    pub contains_deep_events: bool,
    pub sensitive_data: bool,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RecordDiagnosticFact {
    pub severity: DiagnosticSeverity,
    pub scope: DiagnosticScope,
    pub component: DiagnosticComponent,
    pub event_name: String,
    pub summary: String,
    pub correlation: DiagnosticCorrelation,
    pub duration_ms: Option<u64>,
    pub outcome: Option<String>,
    pub error_code: Option<String>,
    pub deep: bool,
    pub request_id: Option<String>,
    pub size_bytes: Option<u64>,
    pub redaction_count: u32,
    pub original_error_event: Option<String>,
    pub initiated_by: Option<String>,
    pub action: Option<String>,
    pub success: Option<bool>,
}

impl RecordDiagnosticFact {
    pub(crate) fn new(
        severity: DiagnosticSeverity,
        component: DiagnosticComponent,
        event_name: &'static str,
        summary: &'static str,
    ) -> Self {
        Self {
            severity,
            scope: DiagnosticScope::Application,
            component,
            event_name: event_name.into(),
            summary: summary.into(),
            correlation: DiagnosticCorrelation::default(),
            duration_ms: None,
            outcome: None,
            error_code: None,
            deep: false,
            request_id: None,
            size_bytes: None,
            redaction_count: 0,
            original_error_event: None,
            initiated_by: None,
            action: None,
            success: None,
        }
    }
}

impl DiagnosticComponent {
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "startup" => Self::Startup,
            "package" => Self::Package,
            "parsing" => Self::Parsing,
            "embedding" => Self::Embedding,
            "manager" => Self::Manager,
            "agent-run" => Self::AgentRun,
            "process" => Self::Process,
            "provider" => Self::Provider,
            "database" => Self::Database,
            "reconciliation" => Self::Reconciliation,
            "backup" => Self::Backup,
            "recovery" => Self::Recovery,
            "update" => Self::Update,
            "export" => Self::Export,
            "renderer" => Self::Renderer,
            "diagnostics" => Self::Diagnostics,
            "other" => Self::Other,
            _ => return None,
        })
    }

    fn label(self) -> &'static str {
        match self {
            Self::Startup => "startup",
            Self::Package => "package",
            Self::Parsing => "parsing",
            Self::Embedding => "embedding",
            Self::Manager => "manager",
            Self::AgentRun => "agent-run",
            Self::Process => "process",
            Self::Provider => "provider",
            Self::Database => "database",
            Self::Reconciliation => "reconciliation",
            Self::Backup => "backup",
            Self::Recovery => "recovery",
            Self::Update => "update",
            Self::Export => "export",
            Self::Renderer => "renderer",
            Self::Diagnostics => "diagnostics",
            Self::Other => "other",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DeepState {
    session_id: String,
    tender_id: String,
    started: SystemTime,
    bytes: u64,
    capped: bool,
}

#[derive(Debug)]
struct JournalMessage {
    event: DiagnosticEvent,
}

thread_local! {
    static PENDING_TRACING_MESSAGE: RefCell<Option<JournalMessage>> = const { RefCell::new(None) };
    static TRACING_MESSAGE_ACCEPTED: Cell<bool> = const { Cell::new(false) };
}

#[derive(Debug)]
struct DiagnosticsLayer {
    inner: std::sync::Weak<Inner>,
}

impl<S> Layer<S> for DiagnosticsLayer
where
    S: Subscriber,
{
    fn on_event(&self, event: &tracing::Event<'_>, _context: Context<'_, S>) {
        if event.metadata().target() != "quantix::diagnostics" {
            return;
        }
        let Some(inner) = self.inner.upgrade() else {
            return;
        };
        let message = PENDING_TRACING_MESSAGE.with(|pending| pending.borrow_mut().take());
        let Some(message) = message else {
            return;
        };
        let accepted = enqueue_message(&inner, message);
        TRACING_MESSAGE_ACCEPTED.with(|result| result.set(accepted));
    }
}

#[derive(Debug)]
struct State {
    activated: bool,
    sequence: u64,
    deep_sequence: u64,
    normal: VecDeque<JournalMessage>,
    critical: VecDeque<JournalMessage>,
    dropped_normal: u64,
    dropped_critical: u64,
    reported_dropped_normal: u64,
    reported_dropped_critical: u64,
    writer_error: Option<String>,
    writer_active: bool,
    deep: Option<DeepState>,
    retired: HashSet<String>,
    deleting: HashSet<String>,
}

#[derive(Debug)]
struct Inner {
    root: PathBuf,
    state: Mutex<State>,
    wake: std::sync::Condvar,
    stop: Mutex<bool>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

/// A cloneable, host-local diagnostics writer/router.
#[derive(Clone)]
pub struct DiagnosticsStore {
    inner: Arc<Inner>,
    dispatch: tracing::Dispatch,
}

impl fmt::Debug for DiagnosticsStore {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DiagnosticsStore")
            .field("root", &self.inner.root)
            .finish_non_exhaustive()
    }
}

impl DiagnosticsStore {
    /// Construct a dormant store. No files or threads are created until activation.
    pub fn new(application_home: impl Into<PathBuf>) -> Self {
        let inner = Arc::new(Inner {
            root: application_home.into().join("logs"),
            state: Mutex::new(State {
                activated: false,
                sequence: 0,
                deep_sequence: 0,
                normal: VecDeque::with_capacity(NORMAL_CAPACITY),
                critical: VecDeque::with_capacity(CRITICAL_CAPACITY),
                dropped_normal: 0,
                dropped_critical: 0,
                reported_dropped_normal: 0,
                reported_dropped_critical: 0,
                writer_error: None,
                writer_active: false,
                deep: None,
                retired: HashSet::new(),
                deleting: HashSet::new(),
            }),
            wake: std::sync::Condvar::new(),
            stop: Mutex::new(false),
            worker: Mutex::new(None),
        });
        let subscriber = Registry::default().with(DiagnosticsLayer {
            inner: Arc::downgrade(&inner),
        });
        Self {
            inner,
            dispatch: tracing::Dispatch::new(subscriber),
        }
    }

    /// Idempotently start the bounded router/writer thread.
    pub fn activate(&self) -> io::Result<()> {
        let state = self.inner.state.lock().expect("diagnostics state");
        if state.activated {
            return Ok(());
        }
        drop(state);
        fs::create_dir_all(self.application_directory())?;
        fs::create_dir_all(self.inner.root.join("tenders"))?;
        *self.inner.stop.lock().expect("diagnostics stop") = false;
        let inner = Arc::clone(&self.inner);
        let handle = thread::Builder::new()
            .name("quantix-diagnostics".into())
            .spawn(move || writer_loop(inner))?;
        *self.inner.worker.lock().expect("diagnostics worker") = Some(handle);
        let mut state = self.inner.state.lock().expect("diagnostics state");
        state.activated = true;
        state.writer_error = None;
        Ok(())
    }

    pub fn application_directory(&self) -> PathBuf {
        self.inner.root.join("application")
    }
    pub fn tender_directory(&self, tender_id: &str) -> io::Result<PathBuf> {
        let safe = validate_id(tender_id)?;
        Ok(self.inner.root.join("tenders").join(safe))
    }

    pub fn record_application(&self, fact: RecordDiagnosticFact) -> bool {
        self.record(fact, DiagnosticScope::Application, None)
    }

    pub fn record_tender(&self, tender_id: &str, mut fact: RecordDiagnosticFact) -> bool {
        let Ok(id) = validate_id(tender_id) else {
            return false;
        };
        fact.scope = DiagnosticScope::Tender;
        fact.correlation.tender_id = Some(id.clone());
        self.record(fact, DiagnosticScope::Tender, Some(id))
    }

    fn record(
        &self,
        mut fact: RecordDiagnosticFact,
        scope: DiagnosticScope,
        tender_id: Option<String>,
    ) -> bool {
        if !matches!(
            fact.scope,
            DiagnosticScope::Application | DiagnosticScope::Tender
        ) {
            return false;
        }
        if scope == DiagnosticScope::Tender && tender_id.is_none() {
            return false;
        }
        let mut state = self.inner.state.lock().expect("diagnostics state");
        if !state.activated {
            return false;
        }
        if let Some(id) = tender_id.as_deref() {
            if state.retired.contains(id) || state.deleting.contains(id) {
                return false;
            }
            if state.deep.as_ref().is_some_and(|deep| {
                deep.started.elapsed().unwrap_or_default().as_secs() >= DEEP_TTL_SECONDS
            }) {
                state.deep = None;
            }
            if fact.deep && state.deep.as_ref().is_none_or(|deep| deep.tender_id != id) {
                return false;
            }
        } else {
            fact.correlation.tender_id = None;
            fact.deep = false;
        }
        if fact.deep {
            let Some(deep) = state.deep.as_mut() else {
                return false;
            };
            if deep.capped {
                return false;
            }
            let bytes = fact
                .summary
                .len()
                .saturating_add(fact.event_name.len())
                .saturating_add(512) as u64;
            if deep.bytes.saturating_add(bytes) > DEEP_CAP_BYTES {
                deep.capped = true;
                return false;
            }
            deep.bytes += bytes;
        }
        state.sequence = state.sequence.saturating_add(1);
        let event = DiagnosticEvent {
            schema_version: SCHEMA_VERSION,
            timestamp: now_timestamp(),
            severity: fact.severity,
            scope,
            component: fact.component,
            event_name: scrub_token(&fact.event_name, 80),
            summary: scrub_token(&fact.summary, 500),
            session_sequence: state.sequence,
            correlation: scrub_correlation(fact.correlation),
            duration_ms: fact.duration_ms,
            outcome: fact.outcome.map(|v| scrub_token(&v, 80)),
            error_code: fact.error_code.map(|v| normalize_error_code(&v)),
            deep: fact.deep,
            request_id: fact.request_id.map(|v| scrub_token(&v, 80)),
            size_bytes: fact.size_bytes,
            redaction_count: fact.redaction_count,
            original_error_event: fact.original_error_event.map(|v| scrub_token(&v, 100)),
            initiated_by: fact.initiated_by.map(|v| scrub_token(&v, 80)),
            action: fact.action.map(|v| scrub_token(&v, 120)),
            success: fact.success,
        };
        drop(state);
        TRACING_MESSAGE_ACCEPTED.with(|result| result.set(false));
        PENDING_TRACING_MESSAGE.with(|pending| {
            *pending.borrow_mut() = Some(JournalMessage { event });
        });
        tracing::dispatcher::with_default(&self.dispatch, || {
            tracing::event!(
                target: "quantix::diagnostics",
                tracing::Level::TRACE,
                typed_diagnostic = true
            );
        });
        PENDING_TRACING_MESSAGE.with(|pending| {
            pending.borrow_mut().take();
        });
        TRACING_MESSAGE_ACCEPTED.with(Cell::get)
    }

    pub fn inspect_health(&self) -> DiagnosticHealth {
        let state = self.inner.state.lock().expect("diagnostics state");
        let (retained_bytes, retained_files) = retention_usage(&self.inner.root);
        let (deep_active_tender, deep_remaining_seconds, deep_bytes) = match state.deep.as_ref() {
            Some(deep) => {
                let elapsed = deep.started.elapsed().unwrap_or_default().as_secs();
                if elapsed >= DEEP_TTL_SECONDS || deep.capped {
                    (None, None, deep.bytes)
                } else {
                    (
                        state.deep.as_ref().map(|value| value.tender_id.clone()),
                        Some(DEEP_TTL_SECONDS - elapsed),
                        deep.bytes,
                    )
                }
            }
            None => (None, None, 0),
        };
        DiagnosticHealth {
            enabled: state.activated,
            degraded: state.writer_error.is_some(),
            writer_error: state
                .writer_error
                .as_ref()
                .map(|_| "diagnostics_writer_unavailable".to_string()),
            dropped_normal: state.dropped_normal,
            dropped_critical: state.dropped_critical,
            retained_bytes,
            retained_files,
            deep_active_tender,
            deep_remaining_seconds,
            deep_bytes,
        }
    }

    /// Wait until every currently accepted diagnostic has been flushed. This is
    /// deliberately fixture-only so production callers cannot make workflow
    /// progress depend on the asynchronous journal writer.
    #[cfg(any(test, feature = "runtime-fixture"))]
    pub fn drain_for_test(&self) -> io::Result<()> {
        let mut state = self.inner.state.lock().expect("diagnostics state");
        loop {
            if let Some(error) = state.writer_error.as_deref() {
                return Err(io::Error::other(error.to_owned()));
            }
            if state.normal.is_empty() && state.critical.is_empty() && !state.writer_active {
                return Ok(());
            }
            state = self.inner.wake.wait(state).expect("diagnostics wake");
        }
    }

    pub fn inspect_status(
        &self,
        scope: DiagnosticScope,
        tender_id: Option<&str>,
    ) -> io::Result<DiagnosticsStatus> {
        let tender_id = tender_id.map(validate_id).transpose()?;
        let health = self.inspect_health();
        let dropped = health
            .dropped_normal
            .saturating_add(health.dropped_critical)
            .min(u64::from(u32::MAX)) as u32;
        let (state, degraded_reason) = if !health.enabled {
            (
                DiagnosticsStatusState::Disabled,
                Some("Diagnostics storage is unavailable".into()),
            )
        } else if health.degraded || dropped > 0 {
            (
                DiagnosticsStatusState::Degraded,
                Some(if health.degraded {
                    "Diagnostics storage reported an error; Tender work is unaffected".into()
                } else {
                    "Some diagnostics were dropped to protect Tender performance".into()
                }),
            )
        } else {
            (DiagnosticsStatusState::Healthy, None)
        };
        let deep = self
            .inner
            .state
            .lock()
            .expect("diagnostics state")
            .deep
            .clone();
        let deep = match deep.filter(|deep| tender_id.as_ref() == Some(&deep.tender_id)) {
            Some(deep) => {
                let elapsed = deep.started.elapsed().unwrap_or_default().as_secs();
                let remaining = DEEP_TTL_SECONDS.saturating_sub(elapsed);
                if deep.capped {
                    DiagnosticsDeepStatus {
                        state: DiagnosticsDeepState::Capped,
                        session_id: Some(deep.session_id.clone()),
                        started_at: Some(format_system_time(deep.started)),
                        ends_at: None,
                        remaining_seconds: Some(0),
                        detail: Some(
                            "Deep diagnostics reached 100 MiB; safe diagnostics continue".into(),
                        ),
                    }
                } else if remaining == 0 {
                    DiagnosticsDeepStatus {
                        state: DiagnosticsDeepState::Expired,
                        session_id: Some(deep.session_id.clone()),
                        started_at: Some(format_system_time(deep.started)),
                        ends_at: Some(format_system_time(
                            deep.started + Duration::from_secs(DEEP_TTL_SECONDS),
                        )),
                        remaining_seconds: Some(0),
                        detail: Some("Deep diagnostics expired; safe diagnostics continue".into()),
                    }
                } else {
                    DiagnosticsDeepStatus {
                        state: DiagnosticsDeepState::Running,
                        session_id: Some(deep.session_id.clone()),
                        started_at: Some(format_system_time(deep.started)),
                        ends_at: Some(format_system_time(
                            deep.started + Duration::from_secs(DEEP_TTL_SECONDS),
                        )),
                        remaining_seconds: Some(remaining),
                        detail: Some("Redacted protocol and process diagnostics are active".into()),
                    }
                }
            }
            None => DiagnosticsDeepStatus {
                state: DiagnosticsDeepState::Idle,
                session_id: None,
                started_at: None,
                ends_at: None,
                remaining_seconds: None,
                detail: None,
            },
        };
        Ok(DiagnosticsStatus {
            scope,
            tender_id,
            state,
            retention_days: Some(90),
            retained_bytes: health.retained_bytes,
            retention_limit_bytes: MAX_RETAIN_BYTES,
            // Counting every retained JSONL line can take seconds at the 5 GiB
            // retention ceiling. Timeline queries provide exact pages without
            // putting this optional status metric on the refresh path.
            retained_event_count: None,
            dropped_event_count: dropped,
            degraded_reason,
            checked_at: now_timestamp(),
            deep,
            component: None,
        })
    }

    pub fn retry(&self) -> io::Result<()> {
        self.shutdown();
        self.activate()
    }

    pub fn start_deep(&self, tender_id: &str) -> io::Result<DeepDiagnosticsSession> {
        let id = validate_id(tender_id)?;
        let mut state = self.inner.state.lock().expect("diagnostics state");
        if !state.activated {
            return Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "diagnostics inactive",
            ));
        }
        if state.retired.contains(&id) {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "tender diagnostics are closed",
            ));
        }
        if state.deep.as_ref().is_some_and(|deep| {
            deep.capped || deep.started.elapsed().unwrap_or_default().as_secs() >= DEEP_TTL_SECONDS
        }) {
            state.deep = None;
        }
        if state.deep.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "a deep diagnostics session is already active",
            ));
        }
        let started = SystemTime::now();
        state.deep_sequence = state.deep_sequence.saturating_add(1);
        let session_id = format!("deep-{}-{}", unix_millis(), state.deep_sequence);
        state.deep = Some(DeepState {
            session_id: session_id.clone(),
            tender_id: id.clone(),
            started,
            bytes: 0,
            capped: false,
        });
        Ok(deep_session(&session_id, &id, started, 0, true))
    }

    pub fn stop_deep(&self, tender_id: &str, session_id: &str) -> bool {
        let Ok(id) = validate_id(tender_id) else {
            return false;
        };
        let mut state = self.inner.state.lock().expect("diagnostics state");
        let was = state
            .deep
            .as_ref()
            .is_some_and(|deep| deep.tender_id == id && deep.session_id == session_id);
        if was {
            state.deep = None;
        }
        was
    }

    pub fn stop_deep_for_tender(&self, tender_id: &str) -> bool {
        let Ok(id) = validate_id(tender_id) else {
            return false;
        };
        let mut state = self.inner.state.lock().expect("diagnostics state");
        let was = state.deep.as_ref().is_some_and(|deep| deep.tender_id == id);
        if was {
            state.deep = None;
        }
        was
    }

    pub fn retire_tender(&self, tender_id: &str) -> bool {
        let Ok(id) = validate_id(tender_id) else {
            return false;
        };
        let mut state = self.inner.state.lock().expect("diagnostics state");
        if state.deep.as_ref().is_some_and(|deep| deep.tender_id == id) {
            state.deep = None;
        }
        state.retired.insert(id);
        drop(state);
        self.inner.wake.notify_one();
        true
    }

    /// Re-enable normal Tender journaling after a trash/archive restoration.
    pub fn resume_tender(&self, tender_id: &str) -> bool {
        let Ok(id) = validate_id(tender_id) else {
            return false;
        };
        self.inner
            .state
            .lock()
            .map(|mut state| state.retired.remove(&id))
            .unwrap_or(false)
    }

    /// Stop routing and join the writer. Safe to call repeatedly.
    pub fn shutdown(&self) {
        if let Ok(mut stop) = self.inner.stop.lock() {
            *stop = true;
        }
        self.inner.wake.notify_one();
        if let Ok(mut worker) = self.inner.worker.lock() {
            if let Some(handle) = worker.take() {
                let deadline = Instant::now() + Duration::from_secs(2);
                while !handle.is_finished() && Instant::now() < deadline {
                    thread::sleep(Duration::from_millis(20));
                }
                if handle.is_finished() {
                    let _ = handle.join();
                }
            }
        }
        if let Ok(mut state) = self.inner.state.lock() {
            state.activated = false;
        }
    }

    /// Permanently erase exactly one Tender journal after validating its path.
    pub fn delete_tender(&self, tender_id: &str) -> io::Result<()> {
        let id = validate_id(tender_id)?;
        self.retire_tender(&id);
        self.shutdown();
        let path = self.tender_directory(&id)?;
        let canonical_root =
            fs::canonicalize(&self.inner.root).unwrap_or_else(|_| self.inner.root.clone());
        if path.exists() {
            let canonical = fs::canonicalize(&path)?;
            if !canonical.starts_with(canonical_root.join("tenders")) {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "unsafe diagnostics path",
                ));
            }
            if fs::symlink_metadata(&path)?.file_type().is_symlink() {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "symlink diagnostics path",
                ));
            }
            fs::remove_dir_all(&path)?;
            if path.exists() {
                return Err(io::Error::other("diagnostics erasure could not be proven"));
            }
        }
        self.inner
            .state
            .lock()
            .expect("diagnostics state")
            .deleting
            .remove(&id);
        let _ = self.activate();
        Ok(())
    }

    pub fn inspect_timeline(
        &self,
        filter: DiagnosticTimelineFilter,
        cursor: Option<&str>,
        limit: usize,
    ) -> DiagnosticTimelinePage {
        let max = limit.clamp(1, MAX_PAGE);
        let mut events = read_events_for_filter(&self.inner.root, &filter);
        events.retain(|e| {
            filter.scope.is_none_or(|scope| match scope {
                DiagnosticScope::Application => e.scope == DiagnosticScope::Application,
                DiagnosticScope::Tender => {
                    e.scope == DiagnosticScope::Application
                        || filter.tender_id.as_ref().is_some_and(|id| {
                            e.scope == DiagnosticScope::Tender
                                && e.correlation.tender_id.as_ref() == Some(id)
                        })
                }
            }) && filter.severity.is_none_or(|v| v == e.severity)
                && filter.component.is_none_or(|v| v == e.component)
        });
        events.sort_by(|a, b| {
            b.timestamp
                .cmp(&a.timestamp)
                .then_with(|| b.session_sequence.cmp(&a.session_sequence))
        });
        let decoded_cursor = cursor.and_then(decode_cursor);
        let snapshot = decoded_cursor
            .as_ref()
            .map(|(snapshot, _)| snapshot.clone())
            .unwrap_or_else(now_timestamp);
        events.retain(|event| event.timestamp.as_str() <= snapshot.as_str());
        let start = decoded_cursor.map(|(_, offset)| offset).unwrap_or(0);
        let page = events
            .into_iter()
            .skip(start)
            .take(max)
            .map(project_timeline_event)
            .collect::<Vec<_>>();
        let next_cursor = if page.len() == max {
            Some(encode_cursor(&snapshot, start + page.len()))
        } else {
            None
        };
        DiagnosticTimelinePage {
            events: page,
            has_more: next_cursor.is_some(),
            next_cursor,
            snapshot,
        }
    }

    pub fn export_support_bundle(
        &self,
        tender_id: Option<&str>,
        days: u32,
        include_deep: bool,
        doctor_report: Option<&crate::doctor::QuantixDoctorReport>,
    ) -> io::Result<DiagnosticSupportBundleResult> {
        let tender = tender_id.map(validate_id).transpose()?;
        let days = days.clamp(1, 90);
        let application_home = self.inner.root.parent().unwrap_or(&self.inner.root);
        let exports = match tender.as_deref() {
            Some(tender_id) => application_home
                .join("exports")
                .join(tender_id)
                .join("support"),
            None => application_home
                .join("exports")
                .join("application")
                .join("support"),
        };
        fs::create_dir_all(&exports)?;
        let pseudonym = tender.as_deref().map(bundle_pseudonym);
        let path = exports.join(format!("diagnostics-{}.qdiag.zip", unix_millis()));
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        let mut zip = zip::ZipWriter::new(file);
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        let filter = DiagnosticTimelineFilter {
            scope: Some(if tender.is_some() {
                DiagnosticScope::Tender
            } else {
                DiagnosticScope::Application
            }),
            severity: None,
            component: None,
            tender_id: tender.clone(),
        };
        let mut events = read_events_for_filter(&self.inner.root, &filter)
            .into_iter()
            .filter(|e| {
                tender.as_ref().is_none_or(|id| {
                    e.scope == DiagnosticScope::Application
                        || e.correlation.tender_id.as_ref() == Some(id)
                })
            })
            .filter(|e| within_days(&e.timestamp, days))
            .filter(|e| include_deep || !e.deep)
            .collect::<Vec<_>>();
        events.sort_by(|left, right| {
            right
                .timestamp
                .cmp(&left.timestamp)
                .then_with(|| right.session_sequence.cmp(&left.session_sequence))
        });
        let mut truncated = events.len() > 100_000;
        events.truncate(100_000);
        let contains_deep = events.iter().any(|e| e.deep);
        let mut json = String::new();
        for mut event in events {
            if let (Some(id), Some(pseudo)) = (tender.as_ref(), pseudonym.as_ref()) {
                if event.correlation.tender_id.as_ref() == Some(id) {
                    event.correlation.tender_id = Some(pseudo.clone());
                }
            }
            let line = serde_json::to_string(&event).unwrap_or_default() + "\n";
            // Reserve one MiB for the manifest, health, runtime and Doctor
            // records while enforcing the 256 MiB uncompressed bundle limit.
            if json.len().saturating_add(line.len()) > 255 * 1024 * 1024 {
                truncated = true;
                break;
            }
            json.push_str(&line);
        }
        let manifest = serde_json::json!({
            "schemaVersion": SCHEMA_VERSION,
            "redactionPolicyVersion": DIAGNOSTIC_REDACTION_POLICY_VERSION,
            "days": days,
            "sensitiveData": contains_deep,
            "sensitiveDataLabel": contains_deep.then_some("CONTAINS REDACTED DEEP DIAGNOSTICS"),
            "tenderPseudonym": pseudonym,
            "truncated": truncated,
        });
        let health = self.inspect_health();
        let runtime = serde_json::json!({
            "application": "Quantix",
            "version": env!("CARGO_PKG_VERSION"),
            "operatingSystem": std::env::consts::OS,
            "architecture": std::env::consts::ARCH,
        });
        zip.start_file("events.jsonl", options).map_err(zip_io)?;
        zip.write_all(json.as_bytes())?;
        zip.start_file("manifest.json", options).map_err(zip_io)?;
        zip.write_all(manifest.to_string().as_bytes())?;
        zip.start_file("diagnostics-health.json", options)
            .map_err(zip_io)?;
        zip.write_all(
            serde_json::to_string(&health)
                .unwrap_or_default()
                .as_bytes(),
        )?;
        zip.start_file("runtime.json", options).map_err(zip_io)?;
        zip.write_all(runtime.to_string().as_bytes())?;
        zip.start_file("doctor-report.json", options)
            .map_err(zip_io)?;
        zip.write_all(
            serde_json::to_string(&doctor_report)
                .unwrap_or_else(|_| "null".to_string())
                .as_bytes(),
        )?;
        zip.finish().map_err(zip_io)?;
        let mut bytes = Vec::new();
        File::open(&path)?.read_to_end(&mut bytes)?;
        Ok(DiagnosticSupportBundleResult {
            path: path.to_string_lossy().into_owned(),
            sha256: hex_sha256(&bytes),
            contains_deep_events: contains_deep,
            sensitive_data: contains_deep,
            truncated,
        })
    }
}

impl Drop for DiagnosticsStore {
    fn drop(&mut self) {
        // The worker itself owns one Arc. The final external clone therefore
        // observes a count of two and can reliably signal/join it.
        if Arc::strong_count(&self.inner) != 2 {
            return;
        }
        self.shutdown();
    }
}

fn enqueue_message(inner: &Arc<Inner>, message: JournalMessage) -> bool {
    let critical = matches!(
        message.event.severity,
        DiagnosticSeverity::Warning | DiagnosticSeverity::Error | DiagnosticSeverity::Critical
    );
    let mut state = inner.state.lock().expect("diagnostics state");
    if !state.activated {
        return false;
    }
    if let Some(tender_id) = message.event.correlation.tender_id.as_deref() {
        if state.retired.contains(tender_id) || state.deleting.contains(tender_id) {
            return false;
        }
    }
    let accepted = if critical {
        if state.critical.len() < CRITICAL_CAPACITY {
            state.critical.push_back(message);
            true
        } else {
            state.dropped_critical = state.dropped_critical.saturating_add(1);
            false
        }
    } else if state.normal.len() < NORMAL_CAPACITY {
        state.normal.push_back(message);
        true
    } else {
        state.dropped_normal = state.dropped_normal.saturating_add(1);
        false
    };
    drop(state);
    inner.wake.notify_one();
    accepted
}

fn writer_loop(inner: Arc<Inner>) {
    let mut journals: HashMap<PathBuf, (File, u64, String, PathBuf)> = HashMap::new();
    let mut last_retention = Instant::now()
        .checked_sub(RETENTION_INTERVAL)
        .unwrap_or_else(Instant::now);
    loop {
        let mut guard = inner.state.lock().expect("diagnostics state");
        if guard.normal.is_empty()
            && guard.critical.is_empty()
            && !*inner.stop.lock().expect("diagnostics stop")
        {
            let (g, _) = inner
                .wake
                .wait_timeout(guard, Duration::from_secs(1))
                .expect("diagnostics wake");
            guard = g;
        }
        let mut batch = Vec::new();
        while let Some(m) = guard.critical.pop_front() {
            batch.push(m);
        }
        while let Some(m) = guard.normal.pop_front() {
            batch.push(m);
            if batch.len() >= 256 {
                break;
            }
        }
        let wrote_batch = !batch.is_empty();
        if wrote_batch {
            guard.writer_active = true;
        }
        let drop_report = if !batch.is_empty()
            && (guard.dropped_normal > guard.reported_dropped_normal
                || guard.dropped_critical > guard.reported_dropped_critical)
        {
            guard.sequence = guard.sequence.saturating_add(1);
            Some((
                JournalMessage {
                    event: DiagnosticEvent {
                        schema_version: SCHEMA_VERSION,
                        timestamp: now_timestamp(),
                        severity: DiagnosticSeverity::Warning,
                        scope: DiagnosticScope::Application,
                        component: DiagnosticComponent::Diagnostics,
                        event_name: "diagnostic_events_dropped".into(),
                        summary: "Events were dropped to protect Tender execution".into(),
                        session_sequence: guard.sequence,
                        correlation: DiagnosticCorrelation::default(),
                        duration_ms: None,
                        outcome: Some("degraded".into()),
                        error_code: Some("DIAGNOSTIC_QUEUE_DROPPED".into()),
                        deep: false,
                        request_id: None,
                        size_bytes: Some(
                            guard.dropped_normal.saturating_add(guard.dropped_critical),
                        ),
                        redaction_count: 0,
                        original_error_event: None,
                        initiated_by: None,
                        action: None,
                        success: Some(false),
                    },
                },
                guard.dropped_normal,
                guard.dropped_critical,
            ))
        } else {
            None
        };
        let stopping = *inner.stop.lock().expect("diagnostics stop");
        drop(guard);
        if let Some((report, normal, critical)) = drop_report {
            match append_event(&inner.root, &mut journals, report) {
                Ok(()) => {
                    let mut state = inner.state.lock().expect("diagnostics state");
                    state.reported_dropped_normal = normal;
                    state.reported_dropped_critical = critical;
                }
                Err(error) => {
                    inner.state.lock().expect("diagnostics state").writer_error =
                        Some(scrub_token(&error.to_string(), 200));
                }
            }
        }
        for message in batch {
            if let Err(error) = append_event(&inner.root, &mut journals, message) {
                inner.state.lock().expect("diagnostics state").writer_error =
                    Some(scrub_token(&error.to_string(), 200));
            }
        }
        for (file, _, _, _) in journals.values_mut() {
            if let Err(error) = file.flush() {
                inner.state.lock().expect("diagnostics state").writer_error =
                    Some(scrub_token(&error.to_string(), 200));
            }
        }
        if wrote_batch {
            let mut state = inner.state.lock().expect("diagnostics state");
            state.writer_active = false;
            inner.wake.notify_all();
        }
        let retired = inner
            .state
            .lock()
            .map(|state| state.retired.clone())
            .unwrap_or_default();
        journals.retain(|path, _| {
            !path.components().any(|component| {
                component
                    .as_os_str()
                    .to_str()
                    .is_some_and(|name| retired.contains(name))
            })
        });
        if stopping {
            let guard = inner.state.lock().expect("diagnostics state");
            if guard.normal.is_empty() && guard.critical.is_empty() {
                break;
            }
        }
        if last_retention.elapsed() >= RETENTION_INTERVAL {
            let active_paths = journals
                .values()
                .map(|(_, _, _, path)| path.clone())
                .collect::<HashSet<_>>();
            apply_retention(&inner.root, &active_paths);
            last_retention = Instant::now();
        }
    }
}

fn append_event(
    root: &Path,
    journals: &mut HashMap<PathBuf, (File, u64, String, PathBuf)>,
    message: JournalMessage,
) -> io::Result<()> {
    let day = &message.event.timestamp[..10];
    let dir = match message.event.scope {
        DiagnosticScope::Application => root.join("application").join(day),
        DiagnosticScope::Tender => root
            .join("tenders")
            .join(
                message
                    .event
                    .correlation
                    .tender_id
                    .as_deref()
                    .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "missing tender"))?,
            )
            .join(day),
    };
    fs::create_dir_all(&dir)?;
    let path = dir.join("000001.jsonl");
    if !journals.contains_key(&path) {
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        let length = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        journals.insert(path.clone(), (file, length, day.to_string(), path.clone()));
    }
    let entry = journals
        .get_mut(&path)
        .ok_or_else(|| io::Error::other("diagnostics journal unavailable"))?;
    if entry.1 >= ROTATE_BYTES || entry.2 != day {
        let next = next_journal(&dir)?;
        *entry = (
            OpenOptions::new().create(true).append(true).open(&next)?,
            fs::metadata(&next).map(|m| m.len()).unwrap_or(0),
            day.to_string(),
            next,
        );
    }
    let line = serde_json::to_string(&message.event)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?
        + "\n";
    entry.0.write_all(line.as_bytes())?;
    entry.1 += line.len() as u64;
    Ok(())
}

fn next_journal(dir: &Path) -> io::Result<PathBuf> {
    for n in 1..=999_999 {
        let p = dir.join(format!("{n:06}.jsonl"));
        if !p.exists() {
            return Ok(p);
        }
    }
    Err(io::Error::other("journal rotation exhausted"))
}
#[cfg(test)]
fn read_events(root: &Path) -> Vec<DiagnosticEvent> {
    read_events_from_files(journal_files(root))
}
fn read_events_for_filter(root: &Path, filter: &DiagnosticTimelineFilter) -> Vec<DiagnosticEvent> {
    let files = match filter.scope {
        Some(DiagnosticScope::Application) => journal_files_under(&root.join("application")),
        Some(DiagnosticScope::Tender) => {
            let mut files = journal_files_under(&root.join("application"));
            if let Some(tender_id) = filter.tender_id.as_deref() {
                files.extend(journal_files_under(&root.join("tenders").join(tender_id)));
            }
            files
        }
        None => journal_files(root),
    };
    read_events_from_files(files)
}
fn read_events_from_files(files: Vec<PathBuf>) -> Vec<DiagnosticEvent> {
    let mut out = Vec::new();
    for path in files {
        if let Ok(file) = File::open(path) {
            for line in BufReader::new(file).lines().map_while(Result::ok) {
                if let Ok(event) = serde_json::from_str(&line) {
                    out.push(event);
                }
            }
        }
    }
    out
}
fn project_timeline_event(event: DiagnosticEvent) -> DiagnosticTimelineEvent {
    DiagnosticTimelineEvent {
        event_id: format!("{}-{}", event.timestamp, event.session_sequence),
        occurred_at: event.timestamp,
        severity: event.severity,
        component: event.component.label().into(),
        title: event.event_name,
        detail: if event.summary.is_empty() {
            None
        } else {
            Some(event.summary)
        },
        scope: event.scope,
        operation_id: event.correlation.operation_id,
        parent_operation_id: event.correlation.parent_operation_id,
        outcome: event.outcome,
        error_code: event.error_code,
    }
}
fn journal_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for base in [root.join("application"), root.join("tenders")] {
        out.extend(journal_files_under(&base));
    }
    out
}
fn journal_files_under(base: &Path) -> Vec<PathBuf> {
    if !base.exists() {
        return Vec::new();
    }
    walkdir::WalkDir::new(base)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.file_type().is_file()
                && entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension == "jsonl")
        })
        .map(walkdir::DirEntry::into_path)
        .collect()
}
fn retention_usage(root: &Path) -> (u64, u64) {
    journal_files(root)
        .into_iter()
        .filter_map(|p| fs::metadata(p).ok().map(|m| m.len()))
        .fold((0, 0), |(bytes, files), n| (bytes + n, files + 1))
}
fn apply_retention(root: &Path, active_paths: &HashSet<PathBuf>) {
    let files = journal_files(root);
    let now = SystemTime::now();
    for p in &files {
        if active_paths.contains(p) {
            continue;
        }
        if let Ok(meta) = fs::metadata(p) {
            if meta
                .modified()
                .ok()
                .and_then(|m| now.duration_since(m).ok())
                .is_some_and(|d| d > Duration::from_secs(90 * 86400))
            {
                let _ = fs::remove_file(p);
            }
        }
    }
    let mut remaining = journal_files(root)
        .into_iter()
        .filter(|path| !active_paths.contains(path))
        .filter_map(|p| {
            fs::metadata(&p)
                .ok()
                .map(|m| (p, m.len(), m.modified().ok()))
        })
        .collect::<Vec<_>>();
    let mut total = retention_usage(root).0;
    if total <= MAX_RETAIN_BYTES {
        return;
    }
    remaining.sort_by_key(|(_, _, modified)| *modified);
    for (p, len, _) in remaining {
        if total <= PURGE_TARGET_BYTES {
            break;
        }
        if fs::remove_file(p).is_ok() {
            total = total.saturating_sub(len);
        }
    }
}

fn validate_id(id: &str) -> io::Result<String> {
    if id.len() != 32
        || !id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid tender id (expected 32 lowercase hexadecimal characters)",
        ));
    }
    Ok(id.to_string())
}
fn scrub_token(value: &str, max: usize) -> String {
    let mut s = value.replace(['\n', '\r', '\t'], " ");
    for marker in [
        "/users/",
        "/home/",
        "/tmp/",
        "/var/",
        "/opt/",
        "\\users\\",
        "%userprofile%",
    ] {
        let lower = s.to_ascii_lowercase();
        if let Some(index) = lower.find(marker) {
            s.replace_range(index.., "[PATH REDACTED]");
        }
    }
    if let Some(index) = windows_absolute_path_index(&s) {
        s.replace_range(index.., "[PATH REDACTED]");
    }
    for key in [
        "password",
        "token",
        "secret",
        "authorization",
        "cookie",
        "api_key",
        "prompt",
        "response",
        "reasoning",
        "chain_of_thought",
        "chain-of-thought",
        "hidden_thought",
        "tool_arguments",
        "tool input",
        "tool result",
        "filename",
        "tender_name",
        "tender_text",
        "document_content",
        "bearer ",
        "sk-",
        "ghp_",
        "aiza",
        "xoxb-",
    ] {
        let lower = s.to_ascii_lowercase();
        if let Some(i) = lower.find(key) {
            s.replace_range(i.., "[REDACTED]");
        }
    }
    if s.len() > max {
        let mut boundary = max;
        while boundary > 0 && !s.is_char_boundary(boundary) {
            boundary -= 1;
        }
        s.truncate(boundary);
        s.push('…');
    }
    s
}
fn windows_absolute_path_index(value: &str) -> Option<usize> {
    let bytes = value.as_bytes();
    bytes.windows(3).position(|window| {
        window[0].is_ascii_alphabetic() && window[1] == b':' && matches!(window[2], b'\\' | b'/')
    })
}
fn scrub_correlation(mut c: DiagnosticCorrelation) -> DiagnosticCorrelation {
    c.operation_id = c.operation_id.map(|v| scrub_token(&v, 100));
    c.parent_operation_id = c.parent_operation_id.map(|v| scrub_token(&v, 100));
    c.run_id = c.run_id.map(|v| scrub_token(&v, 100));
    c.task_id = c.task_id.map(|v| scrub_token(&v, 100));
    c.tender_id = c.tender_id.and_then(|v| validate_id(&v).ok());
    c
}
fn normalize_error_code(code: &str) -> String {
    let upper = code.to_ascii_uppercase();
    let filtered: String = upper
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect();
    scrub_token(&filtered, 80)
}
fn now_timestamp() -> String {
    jiff::Timestamp::now().to_string()
}
fn encode_cursor(snapshot: &str, offset: usize) -> String {
    URL_SAFE_NO_PAD.encode(format!("{snapshot}\n{offset}"))
}
fn decode_cursor(cursor: &str) -> Option<(String, usize)> {
    if cursor.len() > 512 {
        return None;
    }
    let decoded = URL_SAFE_NO_PAD.decode(cursor).ok()?;
    let decoded = String::from_utf8(decoded).ok()?;
    let (snapshot, offset) = decoded.rsplit_once('\n')?;
    snapshot.parse::<jiff::Timestamp>().ok()?;
    Some((snapshot.to_string(), offset.parse().ok()?))
}
fn unix_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}
fn deep_session(
    session_id: &str,
    id: &str,
    started: SystemTime,
    bytes: u64,
    active: bool,
) -> DeepDiagnosticsSession {
    let start = started
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let expires = start + DEEP_TTL_SECONDS;
    DeepDiagnosticsSession {
        session_id: session_id.to_string(),
        tender_id: id.to_string(),
        started_at: format_unix(start),
        expires_at: format_unix(expires),
        bytes_written: bytes,
        cap_bytes: DEEP_CAP_BYTES,
        active,
    }
}
fn format_unix(seconds: u64) -> String {
    jiff::Timestamp::from_second(seconds as i64)
        .map(|t| t.to_string())
        .unwrap_or_else(|_| now_timestamp())
}
fn format_system_time(time: SystemTime) -> String {
    let seconds = time
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format_unix(seconds)
}
fn within_days(timestamp: &str, days: u32) -> bool {
    let Ok(event_time) = timestamp.parse::<jiff::Timestamp>() else {
        return false;
    };
    let age = jiff::Timestamp::now().duration_since(event_time);
    age.is_negative() || age.as_secs() <= i64::from(days) * 86_400
}
fn bundle_pseudonym(id: &str) -> String {
    format!("tender-{}", &hex_sha256(id.as_bytes())[..16])
}
fn hex_sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}
fn zip_io(error: zip::result::ZipError) -> io::Error {
    io::Error::other(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dormant_activation_is_idempotent_and_writes_typed_jsonl() {
        let temp = tempfile::tempdir().unwrap();
        let store = DiagnosticsStore::new(temp.path());
        assert!(!store.record_application(fact("before")));
        store.activate().unwrap();
        store.activate().unwrap();
        assert!(store.record_application(fact("started")));
        std::thread::sleep(Duration::from_millis(30));
        let events = read_events(&temp.path().join("logs"));
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_name, "started");
    }
    #[test]
    fn redaction_and_id_validation() {
        assert!(validate_id("../secret").is_err());
        let temp = tempfile::tempdir().unwrap();
        let store = DiagnosticsStore::new(temp.path());
        store.activate().unwrap();
        let mut f = fact("password=supersecret C:\\Users\\person\\prompt");
        f.deep = true;
        let tender_id = "0123456789abcdef0123456789abcdef";
        store.start_deep(tender_id).unwrap();
        assert!(store.record_tender(tender_id, f));
        std::thread::sleep(Duration::from_millis(30));
        let events = read_events(&temp.path().join("logs"));
        assert!(events[0].summary.contains("[REDACTED]"));
        assert!(!events[0].summary.contains("supersecret"));
    }
    #[test]
    fn redaction_covers_security_sentinels_and_unicode_boundaries() {
        for (raw, secret) in [
            ("Bearer credential-value", "credential-value"),
            ("D:\\private\\document.pdf", "private"),
            ("filename=proposal.pdf", "proposal.pdf"),
            ("tender_name=Confidential Project", "Confidential Project"),
            ("reasoning=hidden chain", "hidden chain"),
            ("tool_arguments=raw payload", "raw payload"),
            ("AIza-provider-secret", "provider-secret"),
        ] {
            let scrubbed = scrub_token(raw, 500);
            assert!(!scrubbed.contains(secret), "secret survived: {raw}");
        }
        let unicode = "آمن".repeat(300);
        let scrubbed = scrub_token(&unicode, 499);
        assert!(scrubbed.ends_with('…'));
        assert!(scrubbed.len() <= 502);
    }
    #[test]
    fn timeline_is_newest_first_and_capped() {
        let temp = tempfile::tempdir().unwrap();
        let store = DiagnosticsStore::new(temp.path());
        store.activate().unwrap();
        for i in 0..3 {
            assert!(store.record_application(fact(&format!("e{i}"))));
        }
        std::thread::sleep(Duration::from_millis(30));
        let page = store.inspect_timeline(
            DiagnosticTimelineFilter {
                scope: None,
                severity: None,
                component: None,
                tender_id: None,
            },
            None,
            2,
        );
        assert_eq!(page.events.len(), 2);
        assert!(page.next_cursor.is_some());
    }
    #[test]
    fn support_bundle_hashes_output() {
        let temp = tempfile::tempdir().unwrap();
        let store = DiagnosticsStore::new(temp.path());
        store.activate().unwrap();
        store.record_application(fact("bundle"));
        std::thread::sleep(Duration::from_millis(30));
        let result = store.export_support_bundle(None, 7, false, None).unwrap();
        assert!(Path::new(&result.path).exists());
        assert_eq!(result.sha256.len(), 64);
    }

    #[test]
    fn application_and_tender_streams_are_isolated_and_tender_view_is_merged() {
        let temp = tempfile::tempdir().unwrap();
        let store = DiagnosticsStore::new(temp.path());
        let first = "0123456789abcdef0123456789abcdef";
        let second = "fedcba9876543210fedcba9876543210";
        store.activate().unwrap();
        assert!(store.record_application(fact("application-event")));
        assert!(store.record_tender(first, fact("first-tender-event")));
        assert!(store.record_tender(second, fact("second-tender-event")));
        wait_for_events(&temp.path().join("logs"), 3);

        let application = store.inspect_timeline(
            DiagnosticTimelineFilter {
                scope: Some(DiagnosticScope::Application),
                severity: None,
                component: None,
                tender_id: None,
            },
            None,
            MAX_PAGE,
        );
        assert_eq!(application.events.len(), 1);
        assert_eq!(application.events[0].title, "application-event");

        let tender = store.inspect_timeline(
            DiagnosticTimelineFilter {
                scope: Some(DiagnosticScope::Tender),
                severity: None,
                component: None,
                tender_id: Some(first.into()),
            },
            None,
            MAX_PAGE,
        );
        let titles = tender
            .events
            .iter()
            .map(|event| event.title.as_str())
            .collect::<HashSet<_>>();
        assert_eq!(titles.len(), 2);
        assert!(titles.contains("application-event"));
        assert!(titles.contains("first-tender-event"));
        assert!(!titles.contains("second-tender-event"));
    }

    #[test]
    fn pagination_cursor_is_opaque_and_holds_a_fixed_snapshot() {
        let temp = tempfile::tempdir().unwrap();
        let store = DiagnosticsStore::new(temp.path());
        store.activate().unwrap();
        for event in ["oldest", "middle", "newest"] {
            assert!(store.record_application(fact(event)));
        }
        wait_for_events(&temp.path().join("logs"), 3);
        let filter = DiagnosticTimelineFilter {
            scope: Some(DiagnosticScope::Application),
            severity: None,
            component: None,
            tender_id: None,
        };
        let first = store.inspect_timeline(filter.clone(), None, 1);
        let cursor = first.next_cursor.as_deref().unwrap();
        assert!(!cursor.contains(&first.snapshot));
        assert!(decode_cursor(cursor).is_some());

        assert!(store.record_application(fact("after-snapshot")));
        wait_for_events(&temp.path().join("logs"), 4);
        let second = store.inspect_timeline(filter, Some(cursor), MAX_PAGE);
        assert_eq!(second.snapshot, first.snapshot);
        assert!(second
            .events
            .iter()
            .all(|event| event.title != "after-snapshot"));
    }

    #[test]
    fn deep_sessions_are_single_tender_scoped_and_retirement_closes_them() {
        let temp = tempfile::tempdir().unwrap();
        let store = DiagnosticsStore::new(temp.path());
        let first = "0123456789abcdef0123456789abcdef";
        let second = "fedcba9876543210fedcba9876543210";
        store.activate().unwrap();
        let session = store.start_deep(first).unwrap();
        assert_eq!(
            store.start_deep(second).unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
        let mut deep = fact("deep-event");
        deep.deep = true;
        assert!(!store.record_tender(second, deep.clone()));
        assert!(store.record_tender(first, deep));
        assert!(store.stop_deep(first, &session.session_id));
        assert!(store.retire_tender(first));
        assert_eq!(
            store.start_deep(first).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
    }

    #[test]
    fn malformed_crash_tail_is_ignored() {
        let temp = tempfile::tempdir().unwrap();
        let store = DiagnosticsStore::new(temp.path());
        store.activate().unwrap();
        assert!(store.record_application(fact("valid-event")));
        wait_for_events(&temp.path().join("logs"), 1);
        store.shutdown();
        let journal = journal_files(&temp.path().join("logs")).pop().unwrap();
        OpenOptions::new()
            .append(true)
            .open(journal)
            .unwrap()
            .write_all(b"{\"schema_version\":1")
            .unwrap();
        let page = store.inspect_timeline(
            DiagnosticTimelineFilter {
                scope: Some(DiagnosticScope::Application),
                severity: None,
                component: None,
                tender_id: None,
            },
            None,
            MAX_PAGE,
        );
        assert_eq!(page.events.len(), 1);
        assert_eq!(page.events[0].title, "valid-event");
    }

    #[test]
    fn support_bundle_is_complete_and_pseudonymizes_tender_identity() {
        let temp = tempfile::tempdir().unwrap();
        let store = DiagnosticsStore::new(temp.path());
        let tender_id = "0123456789abcdef0123456789abcdef";
        store.activate().unwrap();
        assert!(store.record_tender(tender_id, fact("bundle-tender-event")));
        wait_for_events(&temp.path().join("logs"), 1);
        let result = store
            .export_support_bundle(Some(tender_id), 7, false, None)
            .unwrap();
        let file = File::open(result.path).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        for required in [
            "events.jsonl",
            "manifest.json",
            "diagnostics-health.json",
            "runtime.json",
            "doctor-report.json",
        ] {
            assert!(archive.by_name(required).is_ok(), "missing {required}");
        }
        let mut events = String::new();
        archive
            .by_name("events.jsonl")
            .unwrap()
            .read_to_string(&mut events)
            .unwrap();
        assert!(!events.contains(tender_id));
        assert!(events.contains("tender-"));
    }

    #[test]
    fn permanent_delete_removes_only_the_exact_tender_journal() {
        let temp = tempfile::tempdir().unwrap();
        let store = DiagnosticsStore::new(temp.path());
        let first = "0123456789abcdef0123456789abcdef";
        let second = "fedcba9876543210fedcba9876543210";
        store.activate().unwrap();
        assert!(store.record_application(fact("application")));
        assert!(store.record_tender(first, fact("first")));
        assert!(store.record_tender(second, fact("second")));
        wait_for_events(&temp.path().join("logs"), 3);
        store.delete_tender(first).unwrap();
        assert!(!store.tender_directory(first).unwrap().exists());
        assert!(store.tender_directory(second).unwrap().exists());
        assert!(store.application_directory().exists());
    }

    #[test]
    fn normal_shutdown_is_bounded() {
        let temp = tempfile::tempdir().unwrap();
        let store = DiagnosticsStore::new(temp.path());
        store.activate().unwrap();
        assert!(store.record_application(fact("shutdown")));
        let started = Instant::now();
        store.shutdown();
        assert!(started.elapsed() < Duration::from_millis(2_500));
    }

    #[test]
    fn journal_rotates_at_the_32_mib_boundary() {
        let temp = tempfile::tempdir().unwrap();
        let day = &now_timestamp()[..10];
        let directory = temp.path().join("logs").join("application").join(day);
        fs::create_dir_all(&directory).unwrap();
        File::create(directory.join("000001.jsonl"))
            .unwrap()
            .set_len(ROTATE_BYTES)
            .unwrap();
        let store = DiagnosticsStore::new(temp.path());
        store.activate().unwrap();
        assert!(store.record_application(fact("rotated-event")));
        let rotated = directory.join("000002.jsonl");
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline && !rotated.exists() {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(rotated.exists());
        assert!(fs::metadata(rotated).unwrap().len() > 0);
    }
    #[test]
    fn age_retention_removes_only_closed_segments() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("logs");
        let directory = root.join("application").join("2026-01-01");
        fs::create_dir_all(&directory).unwrap();
        let closed = directory.join("000001.jsonl");
        let active = directory.join("000002.jsonl");
        File::create(&closed).unwrap();
        File::create(&active).unwrap();
        let old = SystemTime::now() - Duration::from_secs(91 * 86_400);
        File::options()
            .write(true)
            .open(&closed)
            .unwrap()
            .set_modified(old)
            .unwrap();
        File::options()
            .write(true)
            .open(&active)
            .unwrap()
            .set_modified(old)
            .unwrap();
        apply_retention(&root, &HashSet::from([active.clone()]));
        assert!(!closed.exists());
        assert!(active.exists());
    }

    #[test]
    fn critical_queue_remains_reserved_when_normal_queue_saturates() {
        let temp = tempfile::tempdir().unwrap();
        let store = DiagnosticsStore::new(temp.path());
        store.inner.state.lock().unwrap().activated = true;
        for _ in 0..NORMAL_CAPACITY {
            assert!(store.record_application(fact("normal")));
        }
        assert!(!store.record_application(fact("dropped-normal")));
        let mut warning = fact("critical-reservation");
        warning.severity = DiagnosticSeverity::Warning;
        assert!(store.record_application(warning));
        let health = store.inspect_health();
        assert_eq!(health.dropped_normal, 1);
        assert_eq!(health.dropped_critical, 0);
    }

    #[test]
    fn parallel_host_dispatches_do_not_cross_route_events() {
        let first_home = tempfile::tempdir().unwrap();
        let second_home = tempfile::tempdir().unwrap();
        let first = DiagnosticsStore::new(first_home.path());
        let second = DiagnosticsStore::new(second_home.path());
        first.activate().unwrap();
        second.activate().unwrap();
        let first_writer = first.clone();
        let second_writer = second.clone();
        let first_thread = thread::spawn(move || {
            assert!(first_writer.record_application(fact("first-host")));
        });
        let second_thread = thread::spawn(move || {
            assert!(second_writer.record_application(fact("second-host")));
        });
        first_thread.join().unwrap();
        second_thread.join().unwrap();
        wait_for_events(&first_home.path().join("logs"), 1);
        wait_for_events(&second_home.path().join("logs"), 1);
        let first_events = read_events(&first_home.path().join("logs"));
        let second_events = read_events(&second_home.path().join("logs"));
        assert_eq!(first_events[0].event_name, "first-host");
        assert_eq!(second_events[0].event_name, "second-host");
    }

    #[test]
    fn juhayna_like_failure_chain_is_fully_correlated_without_content_leakage() {
        let temp = tempfile::tempdir().unwrap();
        let store = DiagnosticsStore::new(temp.path());
        let tender_id = "0123456789abcdef0123456789abcdef";
        store.activate().unwrap();
        for (component, operation, parent, name, severity, code) in [
            (
                DiagnosticComponent::Parsing,
                "parse-artifact-1",
                None,
                "document_parse_started",
                DiagnosticSeverity::Info,
                None,
            ),
            (
                DiagnosticComponent::Parsing,
                "parse-artifact-1",
                None,
                "document_parse_failed",
                DiagnosticSeverity::Error,
                Some("PARSER_PROCESS_FAILED"),
            ),
            (
                DiagnosticComponent::Embedding,
                "embed-batch-1",
                Some("parse-artifact-1"),
                "embedding_batch_failed",
                DiagnosticSeverity::Error,
                Some("EMBEDDING_MODEL_FAILED"),
            ),
            (
                DiagnosticComponent::Provider,
                "provider-turn-run-1",
                Some("manager-intake-1"),
                "provider_turn_failed",
                DiagnosticSeverity::Error,
                Some("RPC_FAILED"),
            ),
            (
                DiagnosticComponent::Manager,
                "manager-intake-1",
                None,
                "manager_intake_failed",
                DiagnosticSeverity::Error,
                Some("PROVIDER_FAILED"),
            ),
        ] {
            let mut event = fact("tender_text=JUHAYNA_DOCUMENT_SENTINEL");
            event.component = component;
            event.event_name = name.into();
            event.severity = severity;
            event.correlation.operation_id = Some(operation.into());
            event.correlation.parent_operation_id = parent.map(str::to_owned);
            event.error_code = code.map(str::to_owned);
            assert!(store.record_tender(tender_id, event));
        }
        wait_for_events(&temp.path().join("logs"), 5);
        let events = read_events(&temp.path().join("logs"));
        assert_eq!(events.len(), 5);
        assert!(events
            .iter()
            .all(|event| event.correlation.operation_id.is_some()));
        assert!(events
            .iter()
            .any(|event| event.correlation.parent_operation_id.is_some()));
        let persisted = events
            .iter()
            .map(|event| serde_json::to_string(event).unwrap())
            .collect::<String>();
        assert!(!persisted.contains("JUHAYNA_DOCUMENT_SENTINEL"));
        assert!(persisted.contains("[REDACTED]"));
    }

    fn wait_for_events(root: &Path, expected: usize) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if read_events(root).len() >= expected {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("diagnostics writer did not persist {expected} events in time");
    }

    fn fact(summary: &str) -> RecordDiagnosticFact {
        RecordDiagnosticFact {
            severity: DiagnosticSeverity::Info,
            scope: DiagnosticScope::Application,
            component: DiagnosticComponent::Diagnostics,
            event_name: summary.to_string(),
            summary: summary.to_string(),
            correlation: DiagnosticCorrelation::default(),
            duration_ms: None,
            outcome: None,
            error_code: None,
            deep: false,
            request_id: None,
            size_bytes: None,
            redaction_count: 0,
            original_error_event: None,
            initiated_by: None,
            action: None,
            success: None,
        }
    }
}
