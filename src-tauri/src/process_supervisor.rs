use std::{
    ffi::OsString,
    path::PathBuf,
    process::{ExitStatus, Stdio},
    time::Duration,
};

use processkit::ProcessGroup;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    process::{Child, ChildStdin, ChildStdout, Command},
    sync::mpsc,
    task::JoinHandle,
    time::{sleep, sleep_until, timeout, Instant},
};
use tokio_util::sync::CancellationToken;

const MAX_STDIN_BYTES: usize = 1024 * 1024;
const REAP_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone)]
pub(crate) struct ProcessSpec {
    pub executable: PathBuf,
    pub arguments: Vec<OsString>,
    pub current_directory: Option<PathBuf>,
    pub environment: Vec<(OsString, OsString)>,
    pub inherit_environment: bool,
    pub stdin: Vec<u8>,
    pub timeout: Duration,
    pub stdout_limit: usize,
    pub stderr_limit: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProcessTermination {
    Exited,
    TimedOut,
    Cancelled,
    OutputLimitExceeded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProcessOutput {
    pub termination: ProcessTermination,
    pub exit_code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ProcessError {
    #[error("the supervised process request is invalid")]
    InvalidRequest,
    #[error("the supervised process could not be started")]
    SpawnFailed,
    #[error("the supervised process could not be observed")]
    ObservationFailed,
    #[error("the supervised process exited")]
    Exited,
    #[error("the supervised process conversation timed out")]
    TimedOut,
    #[error("the supervised process conversation was cancelled")]
    Cancelled,
    #[error("the supervised process conversation exceeded its output limit")]
    OutputLimitExceeded,
}

impl ProcessError {
    fn termination(&self) -> Option<ProcessTermination> {
        match self {
            Self::Exited => Some(ProcessTermination::Exited),
            Self::TimedOut => Some(ProcessTermination::TimedOut),
            Self::Cancelled => Some(ProcessTermination::Cancelled),
            Self::OutputLimitExceeded => Some(ProcessTermination::OutputLimitExceeded),
            Self::InvalidRequest | Self::SpawnFailed | Self::ObservationFailed => None,
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct ProcessSupervisor;

impl ProcessSupervisor {
    pub async fn run(
        &self,
        spec: ProcessSpec,
        cancellation: CancellationToken,
    ) -> Result<ProcessOutput, ProcessError> {
        validate_spec(&spec)?;
        let mut child = spawn_supervised(&spec)?;
        let mut stdin = child
            .child
            .stdin
            .take()
            .ok_or(ProcessError::ObservationFailed)?;
        let stdout = child
            .child
            .stdout
            .take()
            .ok_or(ProcessError::ObservationFailed)?;
        let stderr = child
            .child
            .stderr
            .take()
            .ok_or(ProcessError::ObservationFailed)?;

        let stdin_bytes = spec.stdin;
        let stdin_task = tokio::spawn(async move {
            if !stdin_bytes.is_empty() {
                stdin.write_all(&stdin_bytes).await?;
            }
            stdin.shutdown().await
        });
        let (limit_sender, mut limit_receiver) = mpsc::channel(2);
        let stdout_task = tokio::spawn(read_bounded(
            stdout,
            spec.stdout_limit,
            limit_sender.clone(),
        ));
        let stderr_task = tokio::spawn(read_bounded(stderr, spec.stderr_limit, limit_sender));

        let timeout = sleep(spec.timeout);
        tokio::pin!(timeout);
        let (mut termination, exit_status) = tokio::select! {
            status = child.child.wait() => {
                let status = status.map_err(|_| ProcessError::ObservationFailed)?;
                terminate_descendants(&child)?;
                (ProcessTermination::Exited, status)
            }
            _ = &mut timeout => {
                let status = terminate_and_reap(&mut child).await?;
                (ProcessTermination::TimedOut, status)
            }
            _ = cancellation.cancelled() => {
                let status = terminate_and_reap(&mut child).await?;
                (ProcessTermination::Cancelled, status)
            }
            Some(()) = limit_receiver.recv() => {
                let status = terminate_and_reap(&mut child).await?;
                (ProcessTermination::OutputLimitExceeded, status)
            }
        };

        let _stdin_result = stdin_task
            .await
            .map_err(|_| ProcessError::ObservationFailed)?;
        let stdout = stdout_task
            .await
            .map_err(|_| ProcessError::ObservationFailed)?
            .map_err(|_| ProcessError::ObservationFailed)?;
        let stderr = stderr_task
            .await
            .map_err(|_| ProcessError::ObservationFailed)?
            .map_err(|_| ProcessError::ObservationFailed)?;
        if stdout.exceeded || stderr.exceeded {
            termination = ProcessTermination::OutputLimitExceeded;
        }

        Ok(ProcessOutput {
            termination,
            exit_code: exit_status.code(),
            stdout: stdout.bytes,
            stderr: stderr.bytes,
        })
    }

    pub async fn start_conversation(
        &self,
        spec: ProcessSpec,
        cancellation: CancellationToken,
    ) -> Result<SupervisedConversation, ProcessError> {
        validate_spec(&spec)?;
        if !spec.stdin.is_empty() {
            return Err(ProcessError::InvalidRequest);
        }
        let mut child = spawn_supervised(&spec)?;
        let stdin = child
            .child
            .stdin
            .take()
            .ok_or(ProcessError::ObservationFailed)?;
        let stdout = child
            .child
            .stdout
            .take()
            .ok_or(ProcessError::ObservationFailed)?;
        let stderr = child
            .child
            .stderr
            .take()
            .ok_or(ProcessError::ObservationFailed)?;
        let (limit_sender, limit_receiver) = mpsc::channel(1);
        let stderr_task = tokio::spawn(read_bounded(stderr, spec.stderr_limit, limit_sender));
        Ok(SupervisedConversation {
            child,
            stdin: Some(stdin),
            stdout: Some(stdout),
            stdout_buffer: Vec::new(),
            stdin_bytes: 0,
            stdout_bytes: 0,
            stdout_limit: spec.stdout_limit,
            deadline: Instant::now() + spec.timeout,
            cancellation,
            limit_receiver,
            stderr_task,
            failure_termination: None,
            observed_exit_status: None,
        })
    }
}

fn validate_spec(spec: &ProcessSpec) -> Result<(), ProcessError> {
    if !spec.executable.is_absolute()
        || spec.timeout.is_zero()
        || spec.stdout_limit == 0
        || spec.stderr_limit == 0
        || spec.stdin.len() > MAX_STDIN_BYTES
    {
        Err(ProcessError::InvalidRequest)
    } else {
        Ok(())
    }
}

fn spawn_supervised(spec: &ProcessSpec) -> Result<SupervisedChild, ProcessError> {
    let group = ProcessGroup::new().map_err(|_| ProcessError::SpawnFailed)?;
    let mut command = Command::new(&spec.executable);
    if !spec.inherit_environment {
        command.env_clear();
    }
    command
        .args(&spec.arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(current_directory) = spec.current_directory.as_ref() {
        command.current_dir(current_directory);
    }
    for (name, value) in &spec.environment {
        command.env(name, value);
    }
    #[cfg(windows)]
    command.creation_flags(windows::Win32::System::Threading::CREATE_NO_WINDOW.0);
    let child = group
        .spawn(command)
        .map_err(|_| ProcessError::SpawnFailed)?;
    Ok(SupervisedChild { group, child })
}

struct SupervisedChild {
    group: ProcessGroup,
    child: Child,
}

pub(crate) struct SupervisedConversation {
    child: SupervisedChild,
    stdin: Option<ChildStdin>,
    stdout: Option<ChildStdout>,
    stdout_buffer: Vec<u8>,
    stdin_bytes: usize,
    stdout_bytes: usize,
    stdout_limit: usize,
    deadline: Instant,
    cancellation: CancellationToken,
    limit_receiver: mpsc::Receiver<()>,
    stderr_task: JoinHandle<std::io::Result<BoundedOutput>>,
    failure_termination: Option<ProcessTermination>,
    observed_exit_status: Option<ExitStatus>,
}

impl SupervisedConversation {
    pub async fn write(&mut self, bytes: &[u8]) -> Result<(), ProcessError> {
        self.stdin_bytes = self
            .stdin_bytes
            .checked_add(bytes.len())
            .filter(|total| *total <= MAX_STDIN_BYTES)
            .ok_or(ProcessError::InvalidRequest)?;
        let stdin = self.stdin.as_mut().ok_or(ProcessError::ObservationFailed)?;
        let result = tokio::select! {
            result = stdin.write_all(bytes) => result.map_err(|_| ProcessError::ObservationFailed),
            _ = sleep_until(self.deadline) => Err(ProcessError::TimedOut),
            _ = self.cancellation.cancelled() => Err(ProcessError::Cancelled),
            Some(()) = self.limit_receiver.recv() => Err(ProcessError::OutputLimitExceeded),
        };
        self.record_failure(&result);
        result
    }

    pub async fn read_line(&mut self) -> Result<Vec<u8>, ProcessError> {
        loop {
            if let Some(newline) = self.stdout_buffer.iter().position(|byte| *byte == b'\n') {
                let mut line = self.stdout_buffer.drain(..=newline).collect::<Vec<_>>();
                line.pop();
                if line.last() == Some(&b'\r') {
                    line.pop();
                }
                return Ok(line);
            }
            let mut buffer = [0_u8; 8192];
            let read = {
                let stdout = self
                    .stdout
                    .as_mut()
                    .ok_or(ProcessError::ObservationFailed)?;
                tokio::select! {
                    result = stdout.read(&mut buffer) => result.map_err(|_| ProcessError::ObservationFailed),
                    _ = sleep_until(self.deadline) => Err(ProcessError::TimedOut),
                    _ = self.cancellation.cancelled() => Err(ProcessError::Cancelled),
                    Some(()) = self.limit_receiver.recv() => Err(ProcessError::OutputLimitExceeded),
                }
            };
            self.record_failure(&read);
            let read = read?;
            if read == 0 {
                let status = tokio::select! {
                    result = self.child.child.wait() => result.map_err(|_| ProcessError::ObservationFailed),
                    _ = sleep_until(self.deadline) => Err(ProcessError::TimedOut),
                    _ = self.cancellation.cancelled() => Err(ProcessError::Cancelled),
                    Some(()) = self.limit_receiver.recv() => Err(ProcessError::OutputLimitExceeded),
                };
                self.record_failure(&status);
                let status = status?;
                terminate_descendants(&self.child)?;
                self.observed_exit_status = Some(status);
                self.failure_termination = Some(ProcessTermination::Exited);
                return Err(ProcessError::Exited);
            }
            self.stdout_bytes = self
                .stdout_bytes
                .checked_add(read)
                .filter(|total| *total <= self.stdout_limit)
                .ok_or_else(|| {
                    self.failure_termination = Some(ProcessTermination::OutputLimitExceeded);
                    ProcessError::OutputLimitExceeded
                })?;
            self.stdout_buffer.extend_from_slice(&buffer[..read]);
        }
    }

    pub fn failure_termination(&self) -> Option<ProcessTermination> {
        self.failure_termination
    }

    fn record_failure<T>(&mut self, result: &Result<T, ProcessError>) {
        if let Err(error) = result {
            self.failure_termination = error.termination();
        }
    }

    pub async fn finish(
        mut self,
        abort_reason: Option<ProcessTermination>,
    ) -> Result<ProcessOutput, ProcessError> {
        let mut stdout_task = None;
        let (mut termination, exit_status) = if let Some(reason) = abort_reason {
            let status = if reason == ProcessTermination::Exited {
                self.observed_exit_status
                    .take()
                    .ok_or(ProcessError::ObservationFailed)?
            } else {
                terminate_and_reap(&mut self.child).await?
            };
            (reason, status)
        } else {
            if let Some(mut stdin) = self.stdin.take() {
                let _ = stdin.shutdown().await;
            }
            let stdout = self.stdout.take().ok_or(ProcessError::ObservationFailed)?;
            let remaining_stdout = self.stdout_limit.saturating_sub(self.stdout_bytes);
            let (stdout_limit_sender, mut stdout_limit_receiver) = mpsc::channel(1);
            stdout_task = Some(tokio::spawn(read_bounded(
                stdout,
                remaining_stdout,
                stdout_limit_sender,
            )));
            tokio::select! {
                status = self.child.child.wait() => {
                    let status = status.map_err(|_| ProcessError::ObservationFailed)?;
                    terminate_descendants(&self.child)?;
                    (ProcessTermination::Exited, status)
                }
                _ = sleep_until(self.deadline) => {
                    let status = terminate_and_reap(&mut self.child).await?;
                    (ProcessTermination::TimedOut, status)
                }
                _ = self.cancellation.cancelled() => {
                    let status = terminate_and_reap(&mut self.child).await?;
                    (ProcessTermination::Cancelled, status)
                }
                Some(()) = self.limit_receiver.recv() => {
                    let status = terminate_and_reap(&mut self.child).await?;
                    (ProcessTermination::OutputLimitExceeded, status)
                }
                Some(()) = stdout_limit_receiver.recv() => {
                    let status = terminate_and_reap(&mut self.child).await?;
                    (ProcessTermination::OutputLimitExceeded, status)
                }
            }
        };
        if let Some(stdout_task) = stdout_task {
            let stdout = stdout_task
                .await
                .map_err(|_| ProcessError::ObservationFailed)?
                .map_err(|_| ProcessError::ObservationFailed)?;
            if stdout.exceeded {
                termination = ProcessTermination::OutputLimitExceeded;
            }
        }
        let stderr = self
            .stderr_task
            .await
            .map_err(|_| ProcessError::ObservationFailed)?
            .map_err(|_| ProcessError::ObservationFailed)?;
        if stderr.exceeded {
            termination = ProcessTermination::OutputLimitExceeded;
        }
        Ok(ProcessOutput {
            termination,
            exit_code: exit_status.code(),
            stdout: Vec::new(),
            stderr: stderr.bytes,
        })
    }
}

struct BoundedOutput {
    bytes: Vec<u8>,
    exceeded: bool,
}

async fn read_bounded(
    mut reader: impl AsyncRead + Unpin,
    limit: usize,
    limit_sender: mpsc::Sender<()>,
) -> std::io::Result<BoundedOutput> {
    let mut bytes = Vec::with_capacity(limit.min(8192));
    let mut buffer = [0_u8; 8192];
    let mut exceeded = false;
    loop {
        let read = reader.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        let remaining = limit.saturating_sub(bytes.len());
        bytes.extend_from_slice(&buffer[..read.min(remaining)]);
        if read > remaining && !exceeded {
            exceeded = true;
            let _ = limit_sender.send(()).await;
        }
    }
    Ok(BoundedOutput { bytes, exceeded })
}

fn terminate_descendants(child: &SupervisedChild) -> Result<(), ProcessError> {
    child
        .group
        .kill_all()
        .map_err(|_| ProcessError::ObservationFailed)
}

async fn terminate_and_reap(child: &mut SupervisedChild) -> Result<ExitStatus, ProcessError> {
    terminate_descendants(child)?;
    timeout(REAP_TIMEOUT, child.child.wait())
        .await
        .map_err(|_| ProcessError::ObservationFailed)?
        .map_err(|_| ProcessError::ObservationFailed)
}

#[cfg(test)]
mod tests {
    use std::{
        env,
        ffi::OsString,
        fs::{self, OpenOptions},
        io::{BufRead, BufReader, Read, Write},
        process::Command,
        time::Duration,
    };

    use super::*;

    const FIXTURE_TEST: &str = "process_supervisor::tests::supervised_process_fixture";

    fn fixture_spec(mode: &str) -> ProcessSpec {
        ProcessSpec {
            executable: env::current_exe().expect("current test executable"),
            arguments: vec![
                OsString::from("--ignored"),
                OsString::from("--exact"),
                OsString::from(FIXTURE_TEST),
                OsString::from("--nocapture"),
            ],
            current_directory: None,
            environment: vec![(OsString::from("QUANTIX_FIXTURE_MODE"), OsString::from(mode))],
            inherit_environment: true,
            stdin: b"bounded request".to_vec(),
            timeout: Duration::from_secs(5),
            stdout_limit: 4096,
            stderr_limit: 4096,
        }
    }

    #[tokio::test]
    async fn supervisor_captures_success_and_crash_as_terminal_facts() {
        let supervisor = ProcessSupervisor;
        let success = supervisor
            .run(fixture_spec("success"), CancellationToken::new())
            .await
            .expect("successful supervised process");
        assert_eq!(success.termination, ProcessTermination::Exited);
        assert_eq!(success.exit_code, Some(0));
        assert!(String::from_utf8(success.stdout)
            .expect("UTF-8 fixture output")
            .contains("fixture:bounded request"));
        assert!(String::from_utf8(success.stderr)
            .expect("UTF-8 fixture diagnostic")
            .contains("safe diagnostic"));

        let crash = supervisor
            .run(fixture_spec("crash"), CancellationToken::new())
            .await
            .expect("crashed process still has terminal facts");
        assert_eq!(crash.termination, ProcessTermination::Exited);
        assert_eq!(crash.exit_code, Some(23));
    }

    #[tokio::test]
    async fn supervisor_sequences_a_bounded_conversation() {
        let supervisor = ProcessSupervisor;
        let mut spec = fixture_spec("conversation");
        spec.stdin.clear();
        let mut conversation = supervisor
            .start_conversation(spec, CancellationToken::new())
            .await
            .expect("start supervised conversation");
        conversation
            .write(b"initialize\n")
            .await
            .expect("write initialization");
        assert_eq!(
            read_fixture_line(&mut conversation, b"initialized").await,
            b"initialized"
        );
        conversation
            .write(b"account/read\n")
            .await
            .expect("write account request");
        assert_eq!(
            read_fixture_line(&mut conversation, b"chatgpt").await,
            b"chatgpt"
        );
        let terminal = conversation
            .finish(None)
            .await
            .expect("finish supervised conversation");
        assert_eq!(terminal.termination, ProcessTermination::Exited);
        assert_eq!(terminal.exit_code, Some(0));
    }

    #[tokio::test]
    async fn supervisor_can_start_a_child_with_only_the_explicit_environment() {
        let supervisor = ProcessSupervisor;
        let mut spec = fixture_spec("cleared_environment");
        spec.inherit_environment = false;
        let output = supervisor
            .run(spec, CancellationToken::new())
            .await
            .expect("isolated child has terminal facts");
        assert_eq!(output.termination, ProcessTermination::Exited);
        assert_eq!(output.exit_code, Some(0));
    }

    #[tokio::test]
    async fn interactive_failures_keep_their_exact_terminal_facts() {
        let supervisor = ProcessSupervisor;
        let mut timeout_spec = fixture_spec("sleep");
        timeout_spec.stdin.clear();
        timeout_spec.timeout = Duration::from_millis(100);
        let mut timed_out = supervisor
            .start_conversation(timeout_spec, CancellationToken::new())
            .await
            .expect("start timeout conversation");
        let timeout_error = read_until_fixture_failure(&mut timed_out).await;
        assert!(matches!(timeout_error, ProcessError::TimedOut));
        let reason = timed_out
            .failure_termination()
            .expect("timeout terminal reason");
        let terminal = timed_out
            .finish(Some(reason))
            .await
            .expect("finish timeout conversation");
        assert_eq!(terminal.termination, ProcessTermination::TimedOut);

        let mut flood_spec = fixture_spec("flood");
        flood_spec.stdin.clear();
        flood_spec.stdout_limit = 1024;
        let mut flooded = supervisor
            .start_conversation(flood_spec, CancellationToken::new())
            .await
            .expect("start flooded conversation");
        let flood_error = read_until_fixture_failure(&mut flooded).await;
        assert!(matches!(flood_error, ProcessError::OutputLimitExceeded));
        let reason = flooded
            .failure_termination()
            .expect("output-limit terminal reason");
        let terminal = flooded
            .finish(Some(reason))
            .await
            .expect("finish flooded conversation");
        assert_eq!(
            terminal.termination,
            ProcessTermination::OutputLimitExceeded
        );

        let mut crash_spec = fixture_spec("conversation_crash");
        crash_spec.stdin.clear();
        let mut crashed = supervisor
            .start_conversation(crash_spec, CancellationToken::new())
            .await
            .expect("start crashing conversation");
        let crash_error = read_until_fixture_failure(&mut crashed).await;
        assert!(matches!(crash_error, ProcessError::Exited));
        let reason = crashed.failure_termination().expect("exit terminal reason");
        let terminal = crashed
            .finish(Some(reason))
            .await
            .expect("finish crashing conversation");
        assert_eq!(terminal.termination, ProcessTermination::Exited);
        assert_eq!(terminal.exit_code, Some(41));
    }

    #[tokio::test]
    async fn supervisor_enforces_timeout_cancellation_and_output_limits() {
        let supervisor = ProcessSupervisor;
        let mut timeout_spec = fixture_spec("sleep");
        timeout_spec.timeout = Duration::from_millis(100);
        let timed_out = supervisor
            .run(timeout_spec, CancellationToken::new())
            .await
            .expect("timed-out process has terminal facts");
        assert_eq!(timed_out.termination, ProcessTermination::TimedOut);

        let cancellation = CancellationToken::new();
        let cancel_after_start = cancellation.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            cancel_after_start.cancel();
        });
        let cancelled = supervisor
            .run(fixture_spec("sleep"), cancellation)
            .await
            .expect("cancelled process has terminal facts");
        assert_eq!(cancelled.termination, ProcessTermination::Cancelled);

        let mut flood_spec = fixture_spec("flood");
        flood_spec.stdout_limit = 1024;
        let flooded = supervisor
            .run(flood_spec, CancellationToken::new())
            .await
            .expect("output flood has terminal facts");
        assert_eq!(flooded.termination, ProcessTermination::OutputLimitExceeded);
        assert_eq!(flooded.stdout.len(), 1024);

        let mut stderr_flood_spec = fixture_spec("stderr_flood");
        stderr_flood_spec.stderr_limit = 1024;
        let stderr_flooded = supervisor
            .run(stderr_flood_spec, CancellationToken::new())
            .await
            .expect("diagnostic flood has terminal facts");
        assert_eq!(
            stderr_flooded.termination,
            ProcessTermination::OutputLimitExceeded
        );
        assert_eq!(stderr_flooded.stderr.len(), 1024);

        let mut oversized_input = fixture_spec("success");
        oversized_input.stdin = vec![0; MAX_STDIN_BYTES + 1];
        assert!(matches!(
            supervisor
                .run(oversized_input, CancellationToken::new())
                .await,
            Err(ProcessError::InvalidRequest)
        ));
    }

    #[tokio::test]
    async fn exited_child_cannot_leave_descendants_or_inherited_pipes() {
        let supervisor = ProcessSupervisor;
        let directory = tempfile::tempdir().expect("fixture directory");
        let lock_path = directory.path().join("descendant.lock");
        let ready_path = directory.path().join("descendant.ready");
        let mut spec = fixture_spec("descendant");
        spec.timeout = Duration::from_secs(2);
        spec.environment.extend([
            (
                OsString::from("QUANTIX_FIXTURE_LOCK"),
                lock_path.as_os_str().to_owned(),
            ),
            (
                OsString::from("QUANTIX_FIXTURE_READY"),
                ready_path.as_os_str().to_owned(),
            ),
        ]);

        let result = supervisor
            .run(spec, CancellationToken::new())
            .await
            .expect("exited process tree has terminal facts");
        assert_eq!(result.termination, ProcessTermination::Exited);
        assert!(ready_path.exists(), "descendant acquired its lock");
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&lock_path)
            .expect("open descendant lock after termination");
        lock.try_lock()
            .expect("descendant released lock when process tree terminated");
    }

    async fn read_fixture_line(
        conversation: &mut SupervisedConversation,
        expected: &[u8],
    ) -> Vec<u8> {
        for _ in 0..8 {
            let line = conversation.read_line().await.expect("fixture response");
            if line == expected {
                return line;
            }
        }
        panic!("fixture did not return the expected line");
    }

    async fn read_until_fixture_failure(conversation: &mut SupervisedConversation) -> ProcessError {
        loop {
            if let Err(error) = conversation.read_line().await {
                return error;
            }
        }
    }

    #[cfg(windows)]
    #[test]
    fn abrupt_host_death_closes_the_job_and_terminates_descendants() {
        let directory = tempfile::tempdir().expect("fixture directory");
        let lock_path = directory.path().join("abrupt-descendant.lock");
        let ready_path = directory.path().join("abrupt-descendant.ready");
        let executable = env::current_exe().expect("fixture executable");
        let mut host = Command::new(executable)
            .args(["--ignored", "--exact", FIXTURE_TEST, "--nocapture"])
            .env("QUANTIX_FIXTURE_MODE", "abrupt_host")
            .env("QUANTIX_FIXTURE_LOCK", &lock_path)
            .env("QUANTIX_FIXTURE_READY", &ready_path)
            .spawn()
            .expect("spawn abrupt Host fixture");
        for _ in 0..200 {
            if ready_path.exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(ready_path.exists(), "descendant acquired its lock");
        host.kill().expect("terminate Host fixture abruptly");
        host.wait().expect("reap Host fixture");

        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&lock_path)
            .expect("open descendant lock after abrupt Host death");
        for _ in 0..200 {
            if lock.try_lock().is_ok() {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("Windows Job Object did not release the descendant lock");
    }

    #[test]
    #[ignore]
    #[allow(clippy::zombie_processes)]
    fn supervised_process_fixture() {
        match env::var("QUANTIX_FIXTURE_MODE").as_deref() {
            Ok("success") => {
                let mut input = String::new();
                std::io::stdin()
                    .read_to_string(&mut input)
                    .expect("read fixture stdin");
                println!("fixture:{input}");
                eprintln!("safe diagnostic");
            }
            Ok("crash") => std::process::exit(23),
            Ok("conversation_crash") => std::process::exit(41),
            Ok("cleared_environment") => {
                assert!(env::var_os("PATH").is_none());
                assert!(env::var_os("PYTHONPATH").is_none());
                assert!(env::var_os("PYTHONHOME").is_none());
            }
            Ok("sleep") => std::thread::sleep(Duration::from_secs(30)),
            Ok("flood") => {
                std::io::stdout()
                    .write_all(&vec![b'x'; 64 * 1024])
                    .expect("write output flood");
            }
            Ok("stderr_flood") => {
                std::io::stderr()
                    .write_all(&vec![b'x'; 64 * 1024])
                    .expect("write diagnostic flood");
            }
            Ok("conversation") => {
                let mut lines = BufReader::new(std::io::stdin()).lines();
                assert_eq!(
                    lines
                        .next()
                        .transpose()
                        .expect("read initialization")
                        .as_deref(),
                    Some("initialize")
                );
                println!("initialized");
                std::io::stdout().flush().expect("flush initialization");
                assert_eq!(
                    lines
                        .next()
                        .transpose()
                        .expect("read account request")
                        .as_deref(),
                    Some("account/read")
                );
                println!("chatgpt");
            }
            Ok("abrupt_host") => {
                let runtime = tokio::runtime::Runtime::new().expect("fixture Tokio runtime");
                runtime.block_on(async {
                    let _ = ProcessSupervisor
                        .run(fixture_spec("abrupt_descendant"), CancellationToken::new())
                        .await;
                });
            }
            Ok("descendant") => {
                let executable = env::current_exe().expect("fixture executable");
                let descendant = Command::new(executable)
                    .args(["--ignored", "--exact", FIXTURE_TEST, "--nocapture"])
                    .env("QUANTIX_FIXTURE_MODE", "hold_lock")
                    .env(
                        "QUANTIX_FIXTURE_LOCK",
                        env::var_os("QUANTIX_FIXTURE_LOCK").expect("fixture lock path"),
                    )
                    .env(
                        "QUANTIX_FIXTURE_READY",
                        env::var_os("QUANTIX_FIXTURE_READY").expect("fixture ready path"),
                    )
                    .spawn()
                    .expect("spawn fixture descendant");
                let ready_path = env::var_os("QUANTIX_FIXTURE_READY")
                    .map(std::path::PathBuf::from)
                    .expect("fixture ready path");
                for _ in 0..100 {
                    if ready_path.exists() {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                println!("descendant-ready:{}", descendant.id());
                drop(descendant);
            }
            Ok("abrupt_descendant") => {
                let executable = env::current_exe().expect("fixture executable");
                let mut descendant = Command::new(executable)
                    .args(["--ignored", "--exact", FIXTURE_TEST, "--nocapture"])
                    .env("QUANTIX_FIXTURE_MODE", "hold_lock")
                    .env(
                        "QUANTIX_FIXTURE_LOCK",
                        env::var_os("QUANTIX_FIXTURE_LOCK").expect("fixture lock path"),
                    )
                    .env(
                        "QUANTIX_FIXTURE_READY",
                        env::var_os("QUANTIX_FIXTURE_READY").expect("fixture ready path"),
                    )
                    .spawn()
                    .expect("spawn fixture descendant");
                let ready_path = env::var_os("QUANTIX_FIXTURE_READY")
                    .map(std::path::PathBuf::from)
                    .expect("fixture ready path");
                for _ in 0..100 {
                    if ready_path.exists() {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                println!("descendant-ready:{}", descendant.id());
                std::thread::sleep(Duration::from_secs(30));
                let _ = descendant.kill();
                let _ = descendant.wait();
            }
            Ok("hold_lock") => {
                let lock_path = env::var_os("QUANTIX_FIXTURE_LOCK")
                    .map(std::path::PathBuf::from)
                    .expect("fixture lock path");
                let ready_path = env::var_os("QUANTIX_FIXTURE_READY")
                    .map(std::path::PathBuf::from)
                    .expect("fixture ready path");
                let lock = OpenOptions::new()
                    .create(true)
                    .truncate(false)
                    .read(true)
                    .write(true)
                    .open(lock_path)
                    .expect("create fixture lock");
                lock.lock().expect("hold fixture lock");
                fs::write(ready_path, b"ready").expect("publish fixture readiness");
                std::thread::sleep(Duration::from_secs(30));
            }
            _ => panic!("unknown fixture mode"),
        }
    }
}
