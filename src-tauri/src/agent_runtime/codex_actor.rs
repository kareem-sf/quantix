use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use serde_json::{json, Value};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use crate::process_supervisor::ProcessError;

use super::{
    codex_protocol::{
        parse_wire_message, process_failure, protocol_failure, response_result, validate_schema,
        write_rpc,
    },
    permission_failure, CodexProviderProcess, ProviderFailure,
};
use crate::application_settings::{
    codex_failure_connection_status, ProviderConnectionStatus, ProviderConnectionView,
    ProviderLoginMethod, ProviderLoginStatus, ProviderLoginView, CODEX_CONNECTION_ID,
};

const COMMAND_CAPACITY: usize = 8;
const ACTOR_OUTPUT_LIMIT: usize = super::PROVIDER_OUTPUT_LIMIT * 2;
const ARCHIVE_CONFIRMATION_PAGE_LIMIT: u32 = 100;
const LOGIN_OPERATION_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const ACCOUNT_OPERATION_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Clone)]
pub(crate) struct CodexProvider {
    sender: mpsc::Sender<ProviderCommand>,
    alive: Arc<AtomicBool>,
    connection: Arc<Mutex<ProviderConnectionView>>,
    login: Arc<Mutex<Option<ProviderLoginView>>>,
}

impl CodexProvider {
    pub(crate) async fn readiness(
        supervisor: &crate::process_supervisor::ProcessSupervisor,
        executable: PathBuf,
        process_directory: &Path,
        cancellation: CancellationToken,
    ) -> Result<Self, ProviderFailure> {
        let process = tokio::select! {
            biased;
            _ = cancellation.cancelled() => return Err(super::readiness_interruption_failure()),
            process = CodexProviderProcess::readiness(
                supervisor,
                executable,
                process_directory,
                CancellationToken::new(),
            ) => process?,
        };
        let connection = Arc::new(Mutex::new(process.connection_snapshot()));
        let login = Arc::new(Mutex::new(None));
        let (sender, receiver) = mpsc::channel(COMMAND_CAPACITY);
        let alive = Arc::new(AtomicBool::new(true));
        tokio::spawn(run_actor(
            process,
            receiver,
            Arc::clone(&alive),
            Arc::clone(&connection),
            Arc::clone(&login),
        ));
        Ok(Self {
            sender,
            alive,
            connection,
            login,
        })
    }

    pub(crate) async fn refresh_readiness(&self) -> Result<bool, ProviderFailure> {
        let (response, receiver) = oneshot::channel();
        self.sender
            .send(ProviderCommand::Refresh { response })
            .await
            .map_err(|_| process_failure(false))?;
        receiver.await.map_err(|_| process_failure(false))?
    }

    pub(crate) fn connection_snapshot(&self) -> ProviderConnectionView {
        self.connection
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub(crate) fn login_snapshot(&self) -> Option<ProviderLoginView> {
        self.login
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub(crate) async fn start_login(
        &self,
        method: ProviderLoginMethod,
    ) -> Result<ProviderLoginView, ProviderFailure> {
        let (response, receiver) = oneshot::channel();
        self.sender
            .send(ProviderCommand::StartLogin { method, response })
            .await
            .map_err(|_| process_failure(false))?;
        receiver.await.map_err(|_| process_failure(false))?
    }

    pub(crate) async fn cancel_login(&self, login_id: String) -> Result<(), ProviderFailure> {
        let (response, receiver) = oneshot::channel();
        self.sender
            .send(ProviderCommand::CancelLogin { login_id, response })
            .await
            .map_err(|_| process_failure(false))?;
        receiver.await.map_err(|_| process_failure(false))?
    }

    pub(crate) async fn logout(&self) -> Result<ProviderConnectionView, ProviderFailure> {
        let (response, receiver) = oneshot::channel();
        self.sender
            .send(ProviderCommand::Logout { response })
            .await
            .map_err(|_| process_failure(false))?;
        receiver.await.map_err(|_| process_failure(false))?
    }

    pub(crate) async fn delete_thread(&self, thread_ref: String) -> Result<(), ProviderFailure> {
        let (response, receiver) = oneshot::channel();
        self.sender
            .send(ProviderCommand::DeleteThread {
                thread_ref,
                response,
            })
            .await
            .map_err(|_| process_failure(false))?;
        receiver.await.map_err(|_| process_failure(false))?
    }

    pub(crate) async fn shutdown(&self) -> Result<(), ProcessError> {
        let (response, receiver) = oneshot::channel();
        self.sender
            .send(ProviderCommand::Shutdown { response })
            .await
            .map_err(|_| ProcessError::ObservationFailed)?;
        receiver
            .await
            .map_err(|_| ProcessError::ObservationFailed)?
    }

    pub(crate) fn is_closed(&self) -> bool {
        !self.alive.load(Ordering::Acquire) || self.sender.is_closed()
    }

    pub(crate) fn same_instance(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.alive, &other.alive)
    }
}

enum ProviderCommand {
    Refresh {
        response: oneshot::Sender<Result<bool, ProviderFailure>>,
    },
    StartLogin {
        method: ProviderLoginMethod,
        response: oneshot::Sender<Result<ProviderLoginView, ProviderFailure>>,
    },
    CancelLogin {
        login_id: String,
        response: oneshot::Sender<Result<(), ProviderFailure>>,
    },
    Logout {
        response: oneshot::Sender<Result<ProviderConnectionView, ProviderFailure>>,
    },
    DeleteThread {
        thread_ref: String,
        response: oneshot::Sender<Result<(), ProviderFailure>>,
    },
    Shutdown {
        response: oneshot::Sender<Result<(), ProcessError>>,
    },
}

enum PendingRpc {
    LoginStart {
        method: ProviderLoginMethod,
        response: oneshot::Sender<Result<ProviderLoginView, ProviderFailure>>,
    },
    LoginCancel {
        login_id: String,
        response: oneshot::Sender<Result<(), ProviderFailure>>,
    },
    Logout {
        response: oneshot::Sender<Result<ProviderConnectionView, ProviderFailure>>,
    },
    DeleteThread {
        thread_ref: String,
        response: oneshot::Sender<Result<(), ProviderFailure>>,
    },
    DeleteConfirm {
        thread_ref: String,
        archived: bool,
        page: u32,
        delete_failure: ProviderFailure,
        response: oneshot::Sender<Result<(), ProviderFailure>>,
    },
}

async fn run_actor(
    mut process: CodexProviderProcess,
    mut commands: mpsc::Receiver<ProviderCommand>,
    alive: Arc<AtomicBool>,
    connection: Arc<Mutex<ProviderConnectionView>>,
    login: Arc<Mutex<Option<ProviderLoginView>>>,
) {
    let mut pending = HashMap::<u64, PendingRpc>::new();
    let mut active_login_id: Option<String> = None;
    let mut next_rpc_id = 10_000_u64;

    loop {
        tokio::select! {
            biased;
            command = commands.recv() => {
                let Some(command) = command else {
                    terminate_actor(
                        &mut process,
                        &mut commands,
                        &alive,
                    ).await;
                    return;
                };
                match command {
                    ProviderCommand::Refresh { response } => {
                        let result = if pending.is_empty() && active_login_id.is_none() {
                            process.refresh_readiness().await.map(|updated| {
                                *connection
                                    .lock()
                                    .unwrap_or_else(|poisoned| poisoned.into_inner()) = updated;
                                true
                            })
                        } else {
                            Ok(false)
                        };
                        let fatal = result.is_err();
                        let _ = response.send(result);
                        if fatal {
                            terminate_actor(
                                &mut process,
                                &mut commands,
                                &alive,
                            ).await;
                            return;
                        }
                    }
                    ProviderCommand::StartLogin { method, response } => {
                        if !pending.is_empty() || active_login_id.is_some() {
                            let _ = response.send(Err(permission_failure()));
                            continue;
                        }
                        if connection
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner())
                            .status
                            != ProviderConnectionStatus::AuthenticationRequired
                        {
                            let _ = response.send(Err(permission_failure()));
                            continue;
                        }
                        if let Err(failure) = process.conversation_mut().and_then(|conversation| {
                            conversation
                                .begin_operation(
                                    LOGIN_OPERATION_TIMEOUT,
                                    ACTOR_OUTPUT_LIMIT,
                                    ACTOR_OUTPUT_LIMIT,
                                )
                                .map_err(|_| process_failure(false))
                        }) {
                            let _ = response.send(Err(failure));
                            continue;
                        }
                        let id = match allocate_id(&mut next_rpc_id) {
                            Ok(id) => id,
                            Err(failure) => {
                                let _ = response.send(Err(failure));
                                continue;
                            }
                        };
                        let login_type = match method {
                            ProviderLoginMethod::Browser => "chatgpt",
                            ProviderLoginMethod::DeviceCode => "chatgptDeviceCode",
                        };
                        if write_rpc(
                            process.conversation_mut().expect("provider conversation is active"),
                            &json!({
                                "method": "account/login/start",
                                "id": id,
                                "params": { "type": login_type }
                            }),
                        )
                        .await
                        .is_err()
                        {
                            let failure = process_failure(false);
                            let _ = response.send(Err(failure.clone()));
                            terminate_actor(
                                &mut process,
                                &mut commands,
                                &alive,
                            )
                            .await;
                            return;
                        }
                        pending.insert(id, PendingRpc::LoginStart { method, response });
                    }
                    ProviderCommand::CancelLogin { login_id, response } => {
                        let awaiting_user = login
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner())
                            .as_ref()
                            .is_some_and(|current| {
                                current.login_id == login_id
                                    && current.status == ProviderLoginStatus::AwaitingUser
                            });
                        if active_login_id.as_deref() != Some(login_id.as_str()) || !awaiting_user {
                            let _ = response.send(Err(permission_failure()));
                            continue;
                        }
                        if let Err(failure) = process.conversation_mut().and_then(|conversation| {
                            conversation
                                .begin_operation(
                                    ACCOUNT_OPERATION_TIMEOUT,
                                    ACTOR_OUTPUT_LIMIT,
                                    ACTOR_OUTPUT_LIMIT,
                                )
                                .map_err(|_| process_failure(false))
                        }) {
                            let _ = response.send(Err(failure));
                            continue;
                        }
                        if let Some(current) = login
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner())
                            .as_mut()
                        {
                            current.status = ProviderLoginStatus::Cancelling;
                            current.status_summary = "Cancelling managed login…".to_owned();
                        }
                        let id = match allocate_id(&mut next_rpc_id) {
                            Ok(id) => id,
                            Err(failure) => {
                                let _ = response.send(Err(failure));
                                continue;
                            }
                        };
                        if write_rpc(
                            process.conversation_mut().expect("provider conversation is active"),
                            &json!({
                                "method": "account/login/cancel",
                                "id": id,
                                "params": { "loginId": login_id }
                            }),
                        )
                        .await
                        .is_err()
                        {
                            let failure = process_failure(false);
                            let _ = response.send(Err(failure.clone()));
                            terminate_actor(
                                &mut process,
                                &mut commands,
                                &alive,
                            )
                            .await;
                            return;
                        }
                        pending.insert(
                            id,
                            PendingRpc::LoginCancel {
                                login_id,
                                response,
                            },
                        );
                    }
                    ProviderCommand::Logout { response } => {
                        if !pending.is_empty() || active_login_id.is_some() {
                            let _ = response.send(Err(permission_failure()));
                            continue;
                        }
                        if let Err(failure) = process.conversation_mut().and_then(|conversation| {
                            conversation
                                .begin_operation(
                                    ACCOUNT_OPERATION_TIMEOUT,
                                    ACTOR_OUTPUT_LIMIT,
                                    ACTOR_OUTPUT_LIMIT,
                                )
                                .map_err(|_| process_failure(false))
                        }) {
                            let _ = response.send(Err(failure));
                            continue;
                        }
                        let id = match allocate_id(&mut next_rpc_id) {
                            Ok(id) => id,
                            Err(failure) => {
                                let _ = response.send(Err(failure));
                                continue;
                            }
                        };
                        if write_rpc(
                            process.conversation_mut().expect("provider conversation is active"),
                            &json!({
                                "method": "account/logout",
                                "id": id,
                                "params": null
                            }),
                        )
                        .await
                        .is_err()
                        {
                            let failure = process_failure(false);
                            let _ = response.send(Err(failure.clone()));
                            terminate_actor(
                                &mut process,
                                &mut commands,
                                &alive,
                            )
                            .await;
                            return;
                        }
                        pending.insert(id, PendingRpc::Logout { response });
                    }
                    ProviderCommand::DeleteThread {
                        thread_ref,
                        response,
                    } => {
                        if thread_ref.is_empty()
                            || thread_ref.len() > 1000
                            || !pending.is_empty()
                            || active_login_id.is_some()
                            || connection
                                .lock()
                                .unwrap_or_else(|poisoned| poisoned.into_inner())
                                .status
                                != ProviderConnectionStatus::Ready
                        {
                            let _ = response.send(Err(permission_failure()));
                            continue;
                        }
                        if let Err(failure) = process.conversation_mut().and_then(|conversation| {
                            conversation
                                .begin_operation(
                                    ACCOUNT_OPERATION_TIMEOUT,
                                    ACTOR_OUTPUT_LIMIT,
                                    ACTOR_OUTPUT_LIMIT,
                                )
                                .map_err(|_| process_failure(false))
                        }) {
                            let _ = response.send(Err(failure));
                            continue;
                        }
                        let id = match allocate_id(&mut next_rpc_id) {
                            Ok(id) => id,
                            Err(failure) => {
                                let _ = response.send(Err(failure));
                                continue;
                            }
                        };
                        if write_rpc(
                            process.conversation_mut().expect("provider conversation is active"),
                            &json!({
                                "method": "thread/delete",
                                "id": id,
                                "params": { "threadId": thread_ref.clone() }
                            }),
                        )
                        .await
                        .is_err()
                        {
                            let failure = process_failure(false);
                            let _ = response.send(Err(failure.clone()));
                            terminate_actor(
                                &mut process,
                                &mut commands,
                                &alive,
                            )
                            .await;
                            return;
                        }
                        pending.insert(
                            id,
                            PendingRpc::DeleteThread {
                                thread_ref,
                                response,
                            },
                        );
                    }
                    ProviderCommand::Shutdown { response } => {
                        alive.store(false, Ordering::Release);
                        commands.close();
                        let result = process.shutdown().await;
                        let _ = response.send(result);
                        return;
                    }
                }
            }
            line = process.read_line(), if !pending.is_empty() || active_login_id.is_some() => {
                let line = match line {
                    Ok(line) => line,
                    Err(_) => {
                        terminate_actor(
                            &mut process,
                            &mut commands,
                            &alive,
                        ).await;
                        return;
                    }
                };
                let message = match parse_wire_message(&line) {
                    Ok(message) => message,
                    Err(_) => {
                        terminate_actor(
                            &mut process,
                            &mut commands,
                            &alive,
                        ).await;
                        return;
                    }
                };
                let handled = handle_message(
                    CodexActorContext {
                        process: &mut process,
                        pending: &mut pending,
                        next_rpc_id: &mut next_rpc_id,
                        active_login_id: &mut active_login_id,
                        connection: &connection,
                        login: &login,
                    },
                    message,
                ).await;
                if handled.is_err() {
                    terminate_actor(
                        &mut process,
                        &mut commands,
                        &alive,
                    ).await;
                    return;
                }
            }
        }
    }
}

struct DeleteConfirmationRequest {
    thread_ref: String,
    archived: bool,
    cursor: Option<String>,
    page: u32,
    delete_failure: ProviderFailure,
    response: oneshot::Sender<Result<(), ProviderFailure>>,
}

async fn send_delete_confirmation(
    process: &mut CodexProviderProcess,
    pending: &mut HashMap<u64, PendingRpc>,
    next_rpc_id: &mut u64,
    request: DeleteConfirmationRequest,
) -> Result<(), ProviderFailure> {
    if request.page > ARCHIVE_CONFIRMATION_PAGE_LIMIT {
        let _ = request.response.send(Err(request.delete_failure));
        return Ok(());
    }
    let id = allocate_id(next_rpc_id)?;
    write_rpc(
        process.conversation_mut()?,
        &json!({
            "method": "thread/list",
            "id": id,
            "params": {
                "archived": request.archived,
                "cursor": request.cursor,
                "limit": 100
            }
        }),
    )
    .await
    .map_err(|_| process_failure(false))?;
    pending.insert(
        id,
        PendingRpc::DeleteConfirm {
            thread_ref: request.thread_ref,
            archived: request.archived,
            page: request.page,
            delete_failure: request.delete_failure,
            response: request.response,
        },
    );
    Ok(())
}

struct CodexActorContext<'a> {
    process: &'a mut CodexProviderProcess,
    pending: &'a mut HashMap<u64, PendingRpc>,
    next_rpc_id: &'a mut u64,
    active_login_id: &'a mut Option<String>,
    connection: &'a Arc<Mutex<ProviderConnectionView>>,
    login: &'a Arc<Mutex<Option<ProviderLoginView>>>,
}

async fn handle_message(
    context: CodexActorContext<'_>,
    message: Value,
) -> Result<(), ProviderFailure> {
    let CodexActorContext {
        process,
        pending,
        next_rpc_id,
        active_login_id,
        connection,
        login,
    } = context;
    if message.get("id").is_some() && message.get("method").is_none() {
        let id = message
            .get("id")
            .and_then(Value::as_u64)
            .ok_or_else(|| protocol_failure(true))?;
        let Some(operation) = pending.remove(&id) else {
            return Err(protocol_failure(true));
        };
        return handle_response(
            CodexActorContext {
                process,
                pending,
                next_rpc_id,
                active_login_id,
                connection,
                login,
            },
            message,
            operation,
        )
        .await;
    }

    let method = message
        .get("method")
        .and_then(Value::as_str)
        .ok_or_else(|| protocol_failure(true))?;
    if message.get("id").is_some() {
        return Err(protocol_failure(true));
    }

    validate_schema("ServerNotification", &message)?;
    if method != "account/login/completed" {
        return Ok(());
    }

    let params = message.get("params").unwrap_or(&Value::Null);
    let Some(expected_login_id) = active_login_id.as_deref() else {
        return Ok(());
    };
    if params.get("loginId").and_then(Value::as_str) != Some(expected_login_id) {
        return Err(protocol_failure(false));
    }
    let success = params
        .get("success")
        .and_then(Value::as_bool)
        .ok_or_else(|| protocol_failure(false))?;
    if success {
        let cancel_response_pending = pending
            .values()
            .any(|operation| matches!(operation, PendingRpc::LoginCancel { .. }));
        if !cancel_response_pending {
            refresh_connection_after_login(process, connection, login).await?;
        }
        if let Some(current) = login
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_mut()
        {
            current.status = ProviderLoginStatus::Completed;
            current.status_summary = "OpenAI account connected through Codex.".to_owned();
        }
    } else if let Some(current) = login
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .as_mut()
    {
        let cancelled = current.status == ProviderLoginStatus::Cancelling;
        current.status = if cancelled {
            ProviderLoginStatus::Cancelled
        } else {
            ProviderLoginStatus::Failed
        };
        current.status_summary = if cancelled {
            "Managed login cancelled.".to_owned()
        } else {
            "Managed login did not complete. No credentials were stored by Quantix.".to_owned()
        };
    }
    *active_login_id = None;
    Ok(())
}

async fn handle_response(
    context: CodexActorContext<'_>,
    message: Value,
    operation: PendingRpc,
) -> Result<(), ProviderFailure> {
    let CodexActorContext {
        process,
        pending,
        next_rpc_id,
        active_login_id,
        connection,
        login,
    } = context;
    match operation {
        PendingRpc::LoginStart { method, response } => {
            let result = parse_login_start_response(&message, method);
            match result {
                Ok(view) => {
                    *active_login_id = Some(view.login_id.clone());
                    *login
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(view.clone());
                    let _ = response.send(Ok(view));
                    Ok(())
                }
                Err(failure) => {
                    let _ = response.send(Err(failure.clone()));
                    Err(failure)
                }
            }
        }
        PendingRpc::LoginCancel { login_id, response } => {
            let result = response_result(&message, "v2/CancelLoginAccountResponse", false)?;
            let completion_won = login
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .as_ref()
                .is_some_and(|current| {
                    current.login_id == login_id && current.status == ProviderLoginStatus::Completed
                });
            if completion_won {
                refresh_connection_after_login(process, connection, login).await?;
            } else if result.get("status").and_then(Value::as_str) != Some("canceled")
                && active_login_id.as_deref() == Some(login_id.as_str())
            {
                if let Some(current) = login
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .as_mut()
                {
                    current.status = ProviderLoginStatus::Cancelling;
                    current.status_summary =
                        "Waiting for Codex to report the final login state.".to_owned();
                }
            }
            let _ = response.send(Ok(()));
            Ok(())
        }
        PendingRpc::Logout { response } => {
            response_result(&message, "v2/LogoutAccountResponse", false)?;
            let mut updated = process.connection_snapshot();
            updated.status = ProviderConnectionStatus::AuthenticationRequired;
            updated.account_label = None;
            updated.account_plan = None;
            updated.models.clear();
            updated.catalogue_fetched_at = None;
            updated.status_summary =
                "Connect an OpenAI account to use Codex intelligence.".to_owned();
            process.replace_connection(updated.clone());
            *connection
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = updated.clone();
            *login
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
            let _ = response.send(Ok(updated));
            Ok(())
        }
        PendingRpc::DeleteThread {
            thread_ref,
            response,
        } => match response_result(&message, "v2/ThreadDeleteResponse", false) {
            Ok(_) => {
                let _ = response.send(Ok(()));
                Ok(())
            }
            Err(delete_failure) => {
                send_delete_confirmation(
                    process,
                    pending,
                    next_rpc_id,
                    DeleteConfirmationRequest {
                        thread_ref,
                        archived: false,
                        cursor: None,
                        page: 1,
                        delete_failure,
                        response,
                    },
                )
                .await
            }
        },
        PendingRpc::DeleteConfirm {
            thread_ref,
            archived,
            page,
            delete_failure,
            response,
        } => {
            let result = response_result(&message, "v2/ThreadListResponse", false)?;
            let present = result
                .get("data")
                .and_then(Value::as_array)
                .ok_or_else(|| protocol_failure(false))?
                .iter()
                .any(|thread| {
                    thread.get("id").and_then(Value::as_str) == Some(thread_ref.as_str())
                });
            if present {
                let _ = response.send(Err(delete_failure));
                return Ok(());
            }
            if let Some(cursor) = result.get("nextCursor").and_then(Value::as_str) {
                return send_delete_confirmation(
                    process,
                    pending,
                    next_rpc_id,
                    DeleteConfirmationRequest {
                        thread_ref,
                        archived,
                        cursor: Some(cursor.to_owned()),
                        page: page.saturating_add(1),
                        delete_failure,
                        response,
                    },
                )
                .await;
            }
            if !archived {
                return send_delete_confirmation(
                    process,
                    pending,
                    next_rpc_id,
                    DeleteConfirmationRequest {
                        thread_ref,
                        archived: true,
                        cursor: None,
                        page: 1,
                        delete_failure,
                        response,
                    },
                )
                .await;
            }
            let _ = response.send(Ok(()));
            Ok(())
        }
    }
}

async fn refresh_connection_after_login(
    process: &mut CodexProviderProcess,
    connection: &Arc<Mutex<ProviderConnectionView>>,
    login: &Arc<Mutex<Option<ProviderLoginView>>>,
) -> Result<(), ProviderFailure> {
    let updated = match process.refresh_readiness().await {
        Ok(updated) => updated,
        Err(failure) => {
            let (status, summary) = codex_failure_connection_status(failure.category);
            let mut updated = process.connection_snapshot();
            updated.status = status;
            updated.models.clear();
            updated.catalogue_fetched_at = None;
            updated.status_summary = summary.to_owned();
            process.replace_connection(updated.clone());
            *connection
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = updated;
            if let Some(current) = login
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .as_mut()
            {
                current.status = ProviderLoginStatus::Failed;
                current.status_summary = summary.to_owned();
            }
            return Err(failure);
        }
    };
    *connection
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = updated;
    Ok(())
}

fn parse_login_start_response(
    message: &Value,
    method: ProviderLoginMethod,
) -> Result<ProviderLoginView, ProviderFailure> {
    let result = response_result(message, "v2/LoginAccountResponse", false)?;
    let expected_type = match method {
        ProviderLoginMethod::Browser => "chatgpt",
        ProviderLoginMethod::DeviceCode => "chatgptDeviceCode",
    };
    if result.get("type").and_then(Value::as_str) != Some(expected_type) {
        return Err(protocol_failure(false));
    }
    let login_id = result
        .get("loginId")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && value.len() <= 200)
        .ok_or_else(|| protocol_failure(false))?
        .to_owned();
    let authorization_url = match method {
        ProviderLoginMethod::Browser => result.get("authUrl"),
        ProviderLoginMethod::DeviceCode => result.get("verificationUrl"),
    }
    .and_then(Value::as_str)
    .filter(|value| valid_login_url(method, value))
    .ok_or_else(|| protocol_failure(false))?
    .to_owned();
    let user_code = match method {
        ProviderLoginMethod::Browser => None,
        ProviderLoginMethod::DeviceCode => Some(
            result
                .get("userCode")
                .and_then(Value::as_str)
                .filter(|value| {
                    !value.is_empty() && value.len() <= 100 && !value.chars().any(char::is_control)
                })
                .ok_or_else(|| protocol_failure(false))?
                .to_owned(),
        ),
    };
    Ok(ProviderLoginView {
        connection_id: CODEX_CONNECTION_ID.to_owned(),
        login_id,
        method,
        status: ProviderLoginStatus::AwaitingUser,
        authorization_url,
        user_code,
        status_summary: match method {
            ProviderLoginMethod::Browser => {
                "Continue the Codex-managed login in your browser.".to_owned()
            }
            ProviderLoginMethod::DeviceCode => {
                "Enter the one-time code on the OpenAI verification page.".to_owned()
            }
        },
    })
}

pub(crate) fn valid_login_url(method: ProviderLoginMethod, value: &str) -> bool {
    let Ok(url) = tauri::Url::parse(value) else {
        return false;
    };
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port_or_known_default() != Some(443)
        || url.fragment().is_some()
    {
        return false;
    }
    let origin_and_path_are_expected = match method {
        ProviderLoginMethod::Browser => url.host_str() == Some("chatgpt.com"),
        ProviderLoginMethod::DeviceCode => {
            url.host_str() == Some("auth.openai.com")
                && matches!(url.path(), "/codex/device" | "/codex/device/")
                && url.query().is_none()
        }
    };
    origin_and_path_are_expected
        && !url.query_pairs().any(|(key, _)| {
            matches!(
                key.to_ascii_lowercase().as_str(),
                "access_token" | "refresh_token" | "id_token" | "client_secret" | "api_key"
            )
        })
}

async fn terminate_actor(
    process: &mut CodexProviderProcess,
    commands: &mut mpsc::Receiver<ProviderCommand>,
    alive: &AtomicBool,
) {
    alive.store(false, Ordering::Release);
    commands.close();
    let _ = process.shutdown().await;
}

fn allocate_id(next_rpc_id: &mut u64) -> Result<u64, ProviderFailure> {
    let id = *next_rpc_id;
    *next_rpc_id = next_rpc_id
        .checked_add(1)
        .ok_or_else(|| protocol_failure(false))?;
    Ok(id)
}

impl CodexProviderProcess {
    async fn read_line(&mut self) -> Result<Vec<u8>, ProcessError> {
        self.conversation_mut()
            .map_err(|_| ProcessError::ObservationFailed)?
            .read_line()
            .await
    }
}

#[cfg(test)]
mod login_tests {
    use super::*;

    #[test]
    fn device_login_projection_contains_only_the_provider_code_and_url() {
        let view = parse_login_start_response(
            &json!({
                "id": 1,
                "result": {
                    "type": "chatgptDeviceCode",
                    "loginId": "login-1",
                    "userCode": "ABCD-EFGH",
                    "verificationUrl": "https://auth.openai.com/codex/device"
                }
            }),
            ProviderLoginMethod::DeviceCode,
        )
        .expect("valid managed device login response");

        assert_eq!(view.user_code.as_deref(), Some("ABCD-EFGH"));
        assert_eq!(view.status, ProviderLoginStatus::AwaitingUser);
        let projected = serde_json::to_string(&view).expect("serialize login projection");
        assert!(!projected.contains("accessToken"));
        assert!(!projected.contains("auth.openai.com"));
    }

    #[test]
    fn managed_login_rejects_non_https_authorization_urls() {
        let result = parse_login_start_response(
            &json!({
                "id": 1,
                "result": {
                    "type": "chatgpt",
                    "loginId": "login-1",
                    "authUrl": "file:///tmp/credential"
                }
            }),
            ProviderLoginMethod::Browser,
        );

        assert!(result.is_err());
    }

    #[test]
    fn managed_login_rejects_untrusted_https_origins_and_sensitive_fragments() {
        for auth_url in [
            "https://example.com/oauth/authorize",
            "https://chatgpt.com/oauth/authorize#access_token=secret",
        ] {
            let result = parse_login_start_response(
                &json!({
                    "id": 1,
                    "result": {
                        "type": "chatgpt",
                        "loginId": "login-1",
                        "authUrl": auth_url
                    }
                }),
                ProviderLoginMethod::Browser,
            );
            assert!(result.is_err());
        }
    }
}
