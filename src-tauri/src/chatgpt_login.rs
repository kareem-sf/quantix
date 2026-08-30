use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::agent_runtime::{AgentProvider, ChatGptLoginType, LoginOutcome};
use crate::application_settings::{
    load_application_settings, ProviderConnectionStatus, ProviderConnectionView,
};
use crate::host::QuantixHost;
use crate::tender_store::{TenderCommandError, TenderErrorCode};

const CHATGPT_DEVICE_LOGIN_URL: &str = "https://auth.openai.com/codex/device";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ChatGptConnectionState {
    Absent,
    Connected,
    Unusable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct ChatGptConnectionStatus {
    pub state: ChatGptConnectionState,
    pub account_id: Option<String>,
    pub plan_type: Option<String>,
    pub expires_at_ms: Option<u64>,
    pub login_phase: ChatGptLoginPhase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum StartChatGptLoginStatus {
    AwaitingBrowser,
    Connected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct StartChatGptLoginResult {
    pub status: StartChatGptLoginStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct StartChatGptDeviceLoginResult {
    pub verification_url: String,
    pub user_code: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct StartChatGptLoginError {
    pub code: TenderErrorCode,
}

impl StartChatGptLoginError {
    pub(crate) fn new(code: TenderErrorCode) -> Self {
        Self { code }
    }
}

impl std::fmt::Display for StartChatGptLoginError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "ChatGPT login failed: {:?}", self.code)
    }
}

impl std::error::Error for StartChatGptLoginError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ChatGptLoginPhase {
    Idle,
    AwaitingBrowser,
    AwaitingDevice,
    Completed,
    Failed,
    Cancelled,
}

pub(crate) struct ActiveLogin {
    verification_url: Option<String>,
    cancelled: AtomicBool,
}

impl ActiveLogin {
    fn was_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

pub(crate) struct ChatGptLoginFlowState {
    phase: ChatGptLoginPhase,
    active: Option<Arc<ActiveLogin>>,
    disconnecting: usize,
}

impl Default for ChatGptLoginFlowState {
    fn default() -> Self {
        Self {
            phase: ChatGptLoginPhase::Idle,
            active: None,
            disconnecting: 0,
        }
    }
}

enum LoginStartOutcome {
    AlreadyConnected,
    Started {
        verification_url: Option<String>,
        user_code: Option<String>,
    },
}

pub(crate) fn chatgpt_connection_status_with_phase(
    home: &Path,
    login_phase: ChatGptLoginPhase,
) -> ChatGptConnectionStatus {
    chatgpt_connection_status_from_view(
        &crate::application_settings::load_codex_connection_view(home),
        login_phase,
    )
}

pub(crate) fn chatgpt_connection_status_from_view(
    connection: &Option<ProviderConnectionView>,
    login_phase: ChatGptLoginPhase,
) -> ChatGptConnectionStatus {
    let state = match connection.as_ref().map(|connection| connection.status) {
        Some(ProviderConnectionStatus::Ready) => ChatGptConnectionState::Connected,
        Some(
            ProviderConnectionStatus::TemporarilyUnavailable
            | ProviderConnectionStatus::Incompatible,
        ) => ChatGptConnectionState::Unusable,
        Some(ProviderConnectionStatus::AuthenticationRequired)
        | Some(ProviderConnectionStatus::SubscriptionRequired)
        | None => ChatGptConnectionState::Absent,
    };
    ChatGptConnectionStatus {
        state,
        account_id: connection
            .as_ref()
            .and_then(|connection| connection.account_label.clone()),
        plan_type: connection
            .as_ref()
            .and_then(|connection| connection.account_plan.clone()),
        expires_at_ms: None,
        login_phase,
    }
}

fn open_in_system_browser(url: &str) {
    let _ = webbrowser::open(url);
}

impl QuantixHost {
    pub(crate) fn chatgpt_login_phase(&self) -> ChatGptLoginPhase {
        self.chatgpt_login_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .phase
    }

    pub(crate) fn open_chatgpt_device_login_page(&self) -> Result<(), TenderCommandError> {
        let verification_url = self
            .chatgpt_login_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .active
            .as_ref()
            .and_then(|active| active.verification_url.clone())
            .unwrap_or_else(|| CHATGPT_DEVICE_LOGIN_URL.to_owned());
        webbrowser::open(&verification_url)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))
    }

    pub async fn start_chatgpt_login(
        &self,
    ) -> Result<StartChatGptLoginResult, StartChatGptLoginError> {
        match self
            .begin_chatgpt_login(ChatGptLoginType::Browser)
            .await
            .map_err(StartChatGptLoginError::new)?
        {
            LoginStartOutcome::AlreadyConnected => Ok(StartChatGptLoginResult {
                status: StartChatGptLoginStatus::Connected,
            }),
            LoginStartOutcome::Started { .. } => Ok(StartChatGptLoginResult {
                status: StartChatGptLoginStatus::AwaitingBrowser,
            }),
        }
    }

    pub async fn start_chatgpt_device_login(
        &self,
    ) -> Result<StartChatGptDeviceLoginResult, StartChatGptLoginError> {
        match self
            .begin_chatgpt_login(ChatGptLoginType::Device)
            .await
            .map_err(StartChatGptLoginError::new)?
        {
            LoginStartOutcome::AlreadyConnected => Err(StartChatGptLoginError::new(
                TenderErrorCode::OauthAlreadyRunning,
            )),
            LoginStartOutcome::Started {
                verification_url,
                user_code,
            } => {
                let verification_url = verification_url
                    .or_else(|| Some(CHATGPT_DEVICE_LOGIN_URL.to_owned()))
                    .expect("device login returns a verification URL");
                let user_code = user_code.expect("device login returns a one-time code");
                Ok(StartChatGptDeviceLoginResult {
                    verification_url,
                    user_code,
                })
            }
        }
    }

    async fn begin_chatgpt_login(
        &self,
        login_type: ChatGptLoginType,
    ) -> Result<LoginStartOutcome, TenderErrorCode> {
        {
            let state = self
                .chatgpt_login_state()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if state.active.is_some() || state.disconnecting > 0 {
                return Err(TenderErrorCode::OauthAlreadyRunning);
            }
        }
        let provider = self
            .ensure_codex_provider_for_login()
            .await
            .map_err(|_| TenderErrorCode::StoreUnavailable)?;
        if provider.connection_snapshot().status == ProviderConnectionStatus::Ready {
            let mut state = self
                .chatgpt_login_state()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            state.phase = ChatGptLoginPhase::Completed;
            return Ok(LoginStartOutcome::AlreadyConnected);
        }
        let info = provider
            .login_start(login_type)
            .await
            .map_err(|_| TenderErrorCode::StoreUnavailable)?;
        {
            let mut state = self
                .chatgpt_login_state()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            state.active = Some(Arc::new(ActiveLogin {
                verification_url: info.verification_url.clone(),
                cancelled: AtomicBool::new(false),
            }));
            state.phase = match login_type {
                ChatGptLoginType::Browser => ChatGptLoginPhase::AwaitingBrowser,
                ChatGptLoginType::Device => ChatGptLoginPhase::AwaitingDevice,
            };
        }
        if login_type == ChatGptLoginType::Browser {
            if let Some(auth_url) = info.auth_url.as_deref() {
                let auth_url = auth_url.to_owned();
                tokio::task::spawn_blocking(move || open_in_system_browser(&auth_url));
            }
        }
        let watcher_host = self.clone();
        let watcher_provider = provider;
        tokio::spawn(async move {
            let outcome = watcher_provider
                .login_wait(tokio_util::sync::CancellationToken::new())
                .await;
            watcher_host.finish_chatgpt_login_flow(outcome);
        });
        Ok(LoginStartOutcome::Started {
            verification_url: info.verification_url,
            user_code: info.user_code,
        })
    }

    async fn ensure_codex_provider_for_login(
        &self,
    ) -> Result<AgentProvider, crate::agent_runtime::ProviderFailure> {
        self.ensure_codex_provider(tokio_util::sync::CancellationToken::new())
            .await
    }

    fn finish_chatgpt_login_flow(&self, outcome: LoginOutcome) {
        let mut state = self
            .chatgpt_login_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let cancelled = state
            .active
            .as_ref()
            .is_some_and(|active| active.was_cancelled());
        state.active = None;
        if state.disconnecting > 0 {
            state.phase = ChatGptLoginPhase::Idle;
        } else if cancelled {
            state.phase = ChatGptLoginPhase::Cancelled;
        } else {
            state.phase = match outcome {
                LoginOutcome::Completed => ChatGptLoginPhase::Completed,
                LoginOutcome::Cancelled => ChatGptLoginPhase::Cancelled,
                LoginOutcome::Failed => ChatGptLoginPhase::Failed,
            };
        }
    }

    pub async fn cancel_chatgpt_login(&self) {
        let active = self
            .chatgpt_login_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .active
            .clone();
        let Some(active) = active else {
            let mut state = self
                .chatgpt_login_state()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if state.phase != ChatGptLoginPhase::Cancelled {
                state.phase = ChatGptLoginPhase::Idle;
            }
            return;
        };
        active.cancelled.store(true, Ordering::Release);
        let provider = self.agent_provider().lock().await.clone();
        if let Some(provider) = provider {
            let _ = provider.login_cancel().await;
        }
        let mut state = self
            .chatgpt_login_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state
            .active
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, &active))
        {
            state.phase = ChatGptLoginPhase::Cancelled;
        }
    }

    pub async fn disconnect_chatgpt(
        &self,
    ) -> Result<crate::application_settings::ApplicationSettingsView, TenderCommandError> {
        {
            let mut state = self
                .chatgpt_login_state()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            state.disconnecting = state.disconnecting.saturating_add(1);
            if let Some(active) = state.active.as_ref() {
                active.cancelled.store(true, Ordering::Release);
            }
        }
        let provider = self.agent_provider().lock().await.clone();
        if let Some(provider) = provider {
            let _ = provider.logout().await;
        }
        let _ = std::fs::remove_file(self.application_home().join("codex").join("auth.json"));
        let result = (|| {
            crate::application_settings::save_codex_disconnected(self.application_home())?;
            load_application_settings(self.application_home())
        })();
        let mut state = self
            .chatgpt_login_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.active = None;
        state.phase = ChatGptLoginPhase::Idle;
        state.disconnecting = state.disconnecting.saturating_sub(1);
        result
    }
}
