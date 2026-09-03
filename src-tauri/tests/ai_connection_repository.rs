#![cfg(windows)]

use std::{
    io,
    path::{Path, PathBuf},
    sync::{Arc, Barrier, Mutex, TryLockError},
};

use quantix_lib::{
    ai::{
        connections::{
            fixture_reset_secret_drop_observations, fixture_secret_drop_observations,
            AiAdapterVersions, AiConnectionRepository, AiCredentialInput,
            ClearActiveAiConfigurationCommand, CreateAiConnectionCommand,
            DeleteAiConnectionCommand, DisconnectAiConnectionCommand, FixtureSettingsCommitOutcome,
            SameAccountTokenRefreshCommand, SecretInput, SecretNameValueInput,
            SetActiveAiConfigurationCommand, SetAiConnectionEnabledCommand,
            UpdateAiConnectionCommand,
        },
        contract::{
            catalogue_sha256, AiCapabilitySet, AiConnectionConfiguration, AiConnectionId,
            AiConnectionRevision, AiModelView, AiNetworkDestinationClass, AiProbeEvidence,
            AiProviderKind, AiReasoningOption, AiReasoningSelection, AiStructuredOutputMode,
            CapabilitySupport, CompatibleCredentialKind, CompatibleEndpointConfiguration,
        },
        vault::AiConnectionVault,
    },
    ensure_quantix_setup, QuantixHost, SetupPlatform, SetupState, StoragePermissions,
    MINIMUM_SETUP_FREE_SPACE_BYTES,
};
use rusqlite::hooks::{AuthAction, Authorization, TransactionOperation};
use rusqlite::{Connection, OptionalExtension};
use sha2::{Digest, Sha256};

fn secret(value: impl Into<String>) -> SecretInput {
    SecretInput::new(value)
}

struct RepositoryFixture {
    _root: tempfile::TempDir,
    application_home: PathBuf,
    repo: Arc<AiConnectionRepository>,
    installation: Arc<Mutex<Connection>>,
    tender: Arc<Mutex<Connection>>,
}

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

impl RepositoryFixture {
    fn new() -> Self {
        let root = tempfile::Builder::new()
            .prefix("ai-connection-repository")
            .tempdir()
            .unwrap();
        // Canonicalize the parent before building the home path. The vault requires an
        // Application Home spelled the way the operating system spells it, and a build
        // machine's temporary directory is often the 8.3 short form
        // (C:\Users\RUNNER~1\...), which canonicalizes to something else and is rejected.
        let application_home = std::fs::canonicalize(root.path())
            .expect("canonical temporary parent")
            .join(".quantix");
        let host =
            QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
        assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
        let vault = AiConnectionVault::new(&application_home).unwrap();
        let installation = Arc::new(Mutex::new(Connection::open_in_memory().unwrap()));
        installation
            .lock()
            .unwrap()
            .execute_batch(
                "CREATE TABLE application_settings (
                   singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
                   settings_json TEXT NOT NULL CHECK (json_valid(settings_json)),
                   updated_at TEXT NOT NULL
                 );
                 INSERT INTO application_settings (singleton, settings_json, updated_at)
                 VALUES (
                   1,
                   '{\"general_preferences\":{\"appearance\":\"system\",\"reduced_motion\":false,\"larger_text\":false,\"notify_when_attention_needed\":false},\"active_ai_configuration\":null}',
                   '2026-08-24T12:00:00Z'
                 );",
            )
            .unwrap();
        let tender = Arc::new(Mutex::new(Connection::open_in_memory().unwrap()));
        tender
            .lock()
            .unwrap()
            .execute_batch(
                "CREATE TABLE future_nonterminal_references (
                   connection_id TEXT NOT NULL,
                   terminal INTEGER NOT NULL
                 );",
            )
            .unwrap();
        let checker_installation = Arc::clone(&installation);
        let checker_tender = Arc::clone(&tender);
        let repo = Arc::new(AiConnectionRepository::new(
            vault,
            Arc::clone(&installation),
            AiAdapterVersions::new("codex-v1", "grok-v1", "general-v1").unwrap(),
            Arc::new(|| "2026-08-24T12:34:56Z".to_owned()),
            Arc::new(move |connection_id: &str| {
                if !matches!(
                    checker_installation.try_lock(),
                    Err(TryLockError::WouldBlock)
                ) {
                    return Err(quantix_lib::ai::connections::AiConnectionError::StoreUnavailable);
                }
                checker_tender
                    .lock()
                    .map_err(|_| quantix_lib::ai::connections::AiConnectionError::StoreUnavailable)?
                    .query_row(
                        "SELECT 1 FROM future_nonterminal_references
                         WHERE connection_id = ?1 AND terminal = 0
                         LIMIT 1",
                        [connection_id],
                        |_| Ok(()),
                    )
                    .optional()
                    .map(|row| row.is_some())
                    .map_err(|_| quantix_lib::ai::connections::AiConnectionError::StoreUnavailable)
            }),
        ));
        Self {
            _root: root,
            application_home,
            repo,
            installation,
            tender,
        }
    }
}

fn no_nonterminal_reference(
    _connection_id: &str,
) -> Result<bool, quantix_lib::ai::connections::AiConnectionError> {
    Ok(false)
}

fn deny_next_select_after_settings_update(installation: &Arc<Mutex<Connection>>) {
    let mut saw_settings_update = false;
    installation
        .lock()
        .unwrap()
        .authorizer(Some(
            move |context: rusqlite::hooks::AuthContext<'_>| match context.action {
                AuthAction::Update { table_name, .. }
                    if context.accessor.is_none() && table_name == "application_settings" =>
                {
                    saw_settings_update = true;
                    Authorization::Allow
                }
                AuthAction::Select if context.accessor.is_none() && saw_settings_update => {
                    saw_settings_update = false;
                    Authorization::Deny
                }
                _ => Authorization::Allow,
            },
        ))
        .unwrap();
}

fn remove_authorizer(installation: &Arc<Mutex<Connection>>) {
    installation
        .lock()
        .unwrap()
        .authorizer(None::<fn(rusqlite::hooks::AuthContext<'_>) -> Authorization>)
        .unwrap();
}

fn raw_settings_row(installation: &Arc<Mutex<Connection>>) -> (String, String) {
    installation
        .lock()
        .unwrap()
        .query_row(
            "SELECT settings_json, updated_at FROM application_settings WHERE singleton = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap()
}

fn openai_key_command(secret_value: &str) -> CreateAiConnectionCommand {
    CreateAiConnectionCommand {
        display_name: "Engineering OpenAI".to_owned(),
        configuration: AiConnectionConfiguration::DirectProviderKey {
            provider: AiProviderKind::OpenAi,
        },
        credential: AiCredentialInput::ApiKey {
            api_key: secret(secret_value),
            custom_header_values: Vec::new(),
            custom_query_values: Vec::new(),
        },
    }
}

fn current_probe(
    connection_id: &str,
    execution_revision: u64,
    model_id: &str,
    reasoning_id: &str,
) -> AiProbeEvidence {
    AiProbeEvidence {
        connection_id: AiConnectionId::parse(connection_id).unwrap(),
        execution_revision: AiConnectionRevision::new(execution_revision).unwrap(),
        provider: AiProviderKind::OpenAi,
        endpoint_fingerprint: hex_sha256("https://api.openai.com"),
        adapter_version: "general-v1".to_owned(),
        destination_class: AiNetworkDestinationClass::Public,
        models: vec![AiModelView {
            model_id: model_id.to_owned(),
            reported_model_id: Some(model_id.to_owned()),
            display_name: "GPT test".to_owned(),
            capabilities: AiCapabilitySet {
                streaming: CapabilitySupport::Supported,
                tools: CapabilitySupport::Supported,
                images: CapabilitySupport::Unsupported,
                reasoning: CapabilitySupport::Supported,
                reroute_detection: CapabilitySupport::Supported,
                structured_output: AiStructuredOutputMode::NativeJsonSchema,
                context_window_tokens: Some(128_000),
            },
            reasoning_options: vec![AiReasoningOption {
                selection: AiReasoningSelection::Effort {
                    id: reasoning_id.to_owned(),
                },
                label: "Low".to_owned(),
                description: "Bounded low reasoning".to_owned(),
            }],
        }],
        tested_model_id: model_id.to_owned(),
        tested_reasoning: AiReasoningSelection::Effort {
            id: reasoning_id.to_owned(),
        },
        observed_at: "2026-08-24T12:10:00Z".to_owned(),
    }
}

fn codex_account_command(access_token: &str, refresh_token: &str) -> CreateAiConnectionCommand {
    CreateAiConnectionCommand {
        display_name: "Quantix Codex".to_owned(),
        configuration: AiConnectionConfiguration::AccountLogin {
            provider: AiProviderKind::Codex,
            account_id: "account-123".to_owned(),
        },
        credential: AiCredentialInput::Account {
            access_token: secret(access_token),
            refresh_token: Some(secret(refresh_token)),
            expires_at: "2026-08-24T13:00:00Z".to_owned(),
            verified_account_id: "account-123".to_owned(),
        },
    }
}

fn codex_probe(connection_id: &str, execution_revision: u64) -> AiProbeEvidence {
    let mut evidence = current_probe(connection_id, execution_revision, "gpt-codex", "medium");
    evidence.provider = AiProviderKind::Codex;
    evidence.endpoint_fingerprint = hex_sha256("https://chatgpt.com");
    evidence.adapter_version = "codex-v1".to_owned();
    evidence
}

fn hex_sha256(value: &str) -> String {
    Sha256::digest(value.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[test]
fn fresh_repository_has_no_default_and_test_never_activates() {
    let fixture = RepositoryFixture::new();
    assert!(fixture
        .repo
        .inspect()
        .unwrap()
        .active_configuration
        .is_none());
    let connection = fixture
        .repo
        .create_connection(openai_key_command("sk-test"))
        .unwrap();
    fixture
        .repo
        .record_probe(current_probe(
            &connection.connection_id,
            connection.execution_revision,
            "gpt-test",
            "low",
        ))
        .unwrap();
    let view = fixture.repo.inspect().unwrap();
    assert!(view.active_configuration.is_none());
    assert_eq!(view.connections[0].models[0].model_id, "gpt-test");
}

#[test]
fn semantic_revision_and_credential_generation_are_independent() {
    let fixture = RepositoryFixture::new();
    let codex = fixture
        .repo
        .create_connection(codex_account_command("access-A", "refresh-A"))
        .unwrap();
    let codex_evidence = codex_probe(&codex.connection_id, codex.execution_revision);
    fixture.repo.record_probe(codex_evidence.clone()).unwrap();
    fixture
        .repo
        .activate(SetActiveAiConfigurationCommand {
            connection_id: codex.connection_id.clone(),
            expected_execution_revision: codex.execution_revision,
            provider: AiProviderKind::Codex,
            endpoint_fingerprint: hex_sha256("https://chatgpt.com"),
            model_id: "gpt-codex".to_owned(),
            reasoning: AiReasoningSelection::Effort {
                id: "medium".to_owned(),
            },
            adapter_version: "codex-v1".to_owned(),
            catalogue_sha256: catalogue_sha256(&codex_evidence).unwrap(),
            destination_class: AiNetworkDestinationClass::Public,
            confirmed_data_destination: "https://chatgpt.com".to_owned(),
        })
        .unwrap();
    let before = fixture
        .repo
        .fixture_connection_state(&codex.connection_id)
        .unwrap();

    assert_eq!(
        fixture
            .repo
            .rotate_same_account_tokens(
                SameAccountTokenRefreshCommand {
                    connection_id: codex.connection_id.clone(),
                    expected_execution_revision: before.execution_revision,
                    expected_credential_generation: before.credential_generation,
                    verified_account_id: "different-account".to_owned(),
                },
                AiCredentialInput::Account {
                    access_token: secret("wrong-access"),
                    refresh_token: Some(secret("wrong-refresh")),
                    expires_at: "2026-08-24T14:00:00Z".to_owned(),
                    verified_account_id: "different-account".to_owned(),
                },
            )
            .unwrap_err(),
        quantix_lib::ai::connections::AiConnectionError::InvalidCommand
    );
    assert_eq!(
        fixture
            .repo
            .fixture_connection_state(&codex.connection_id)
            .unwrap(),
        before
    );

    fixture
        .repo
        .rotate_same_account_tokens(
            SameAccountTokenRefreshCommand {
                connection_id: codex.connection_id.clone(),
                expected_execution_revision: before.execution_revision,
                expected_credential_generation: before.credential_generation,
                verified_account_id: "account-123".to_owned(),
            },
            AiCredentialInput::Account {
                access_token: secret("access-B"),
                refresh_token: Some(secret("refresh-B")),
                expires_at: "2026-08-24T14:00:00Z".to_owned(),
                verified_account_id: "account-123".to_owned(),
            },
        )
        .unwrap();
    let rotated = fixture
        .repo
        .fixture_connection_state(&codex.connection_id)
        .unwrap();
    assert_eq!(rotated.execution_revision, before.execution_revision);
    assert_eq!(
        rotated.credential_generation,
        before.credential_generation + 1
    );
    assert!(rotated.has_probe_evidence);
    let refreshed_view = fixture.repo.inspect().unwrap();
    assert!(refreshed_view.active_configuration.is_some());
    assert_eq!(
        refreshed_view.readiness,
        quantix_lib::ai::contract::ActiveAiReadiness::Ready
    );

    fixture
        .repo
        .replace_connection_configuration(UpdateAiConnectionCommand {
            connection_id: codex.connection_id.clone(),
            expected_execution_revision: rotated.execution_revision,
            expected_credential_generation: rotated.credential_generation,
            display_name: "Quantix Codex".to_owned(),
            configuration: Some(AiConnectionConfiguration::AccountLogin {
                provider: AiProviderKind::Codex,
                account_id: "account-456".to_owned(),
            }),
            replacement_credential: Some(AiCredentialInput::Account {
                access_token: secret("reauth-access"),
                refresh_token: Some(secret("reauth-refresh")),
                expires_at: "2026-08-24T15:00:00Z".to_owned(),
                verified_account_id: "account-456".to_owned(),
            }),
        })
        .unwrap();
    let reauthenticated = fixture
        .repo
        .fixture_connection_state(&codex.connection_id)
        .unwrap();
    assert_eq!(
        reauthenticated.execution_revision,
        rotated.execution_revision + 1
    );
    assert_eq!(
        reauthenticated.credential_generation,
        rotated.credential_generation + 1
    );
    assert!(!reauthenticated.has_probe_evidence);
    assert_eq!(
        fixture.repo.inspect().unwrap().readiness,
        quantix_lib::ai::contract::ActiveAiReadiness::StaleRevision
    );

    let openai = fixture
        .repo
        .create_connection(openai_key_command("sk-before"))
        .unwrap();
    fixture
        .repo
        .record_probe(current_probe(
            &openai.connection_id,
            openai.execution_revision,
            "gpt-test",
            "low",
        ))
        .unwrap();
    let openai_before = fixture
        .repo
        .fixture_connection_state(&openai.connection_id)
        .unwrap();
    fixture
        .repo
        .replace_connection_configuration(UpdateAiConnectionCommand {
            connection_id: openai.connection_id.clone(),
            expected_execution_revision: openai_before.execution_revision,
            expected_credential_generation: openai_before.credential_generation,
            display_name: "Engineering OpenAI".to_owned(),
            configuration: Some(AiConnectionConfiguration::DirectProviderKey {
                provider: AiProviderKind::OpenAi,
            }),
            replacement_credential: Some(AiCredentialInput::ApiKey {
                api_key: secret("sk-after"),
                custom_header_values: Vec::new(),
                custom_query_values: Vec::new(),
            }),
        })
        .unwrap();
    let replaced = fixture
        .repo
        .fixture_connection_state(&openai.connection_id)
        .unwrap();
    assert_eq!(
        replaced.execution_revision,
        openai_before.execution_revision + 1
    );
    assert_eq!(
        replaced.credential_generation,
        openai_before.credential_generation + 1
    );
    assert!(!replaced.has_probe_evidence);
}

#[test]
fn activation_persists_only_an_exact_explicit_configuration() {
    let fixture = RepositoryFixture::new();
    let connection = fixture
        .repo
        .create_connection(openai_key_command("sk-activation-sentinel"))
        .unwrap();
    let evidence = current_probe(
        &connection.connection_id,
        connection.execution_revision,
        "gpt-test",
        "low",
    );
    fixture.repo.record_probe(evidence.clone()).unwrap();
    let catalogue_sha256 = catalogue_sha256(&evidence).unwrap();

    let view = fixture
        .repo
        .activate(SetActiveAiConfigurationCommand {
            connection_id: connection.connection_id.clone(),
            expected_execution_revision: connection.execution_revision,
            provider: AiProviderKind::OpenAi,
            endpoint_fingerprint: hex_sha256("https://api.openai.com"),
            model_id: "gpt-test".to_owned(),
            reasoning: AiReasoningSelection::Effort {
                id: "low".to_owned(),
            },
            adapter_version: "general-v1".to_owned(),
            catalogue_sha256,
            destination_class: AiNetworkDestinationClass::Public,
            confirmed_data_destination: "https://api.openai.com".to_owned(),
        })
        .unwrap();
    assert_eq!(
        view.readiness,
        quantix_lib::ai::contract::ActiveAiReadiness::Ready
    );
    assert_eq!(
        view.active_configuration.unwrap().activated_at,
        "2026-08-24T12:34:56Z"
    );

    let raw: String = fixture
        .installation
        .lock()
        .unwrap()
        .query_row(
            "SELECT settings_json FROM application_settings WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let json: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(
        json.as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>(),
        vec!["active_ai_configuration", "general_preferences"]
    );
    assert!(!raw.contains("sk-activation-sentinel"));

    let state = fixture
        .repo
        .fixture_connection_state(&connection.connection_id)
        .unwrap();
    fixture
        .repo
        .replace_connection_configuration(UpdateAiConnectionCommand {
            connection_id: connection.connection_id.clone(),
            expected_execution_revision: state.execution_revision,
            expected_credential_generation: state.credential_generation,
            display_name: "Engineering OpenAI".to_owned(),
            configuration: Some(AiConnectionConfiguration::DirectProviderKey {
                provider: AiProviderKind::OpenAi,
            }),
            replacement_credential: Some(AiCredentialInput::ApiKey {
                api_key: secret("sk-replaced"),
                custom_header_values: Vec::new(),
                custom_query_values: Vec::new(),
            }),
        })
        .unwrap();
    assert_eq!(
        fixture.repo.inspect().unwrap().readiness,
        quantix_lib::ai::contract::ActiveAiReadiness::StaleRevision
    );
}

#[test]
fn name_enable_disconnect_and_clear_preserve_their_exact_dimensions() {
    let fixture = RepositoryFixture::new();
    let connection = fixture
        .repo
        .create_connection(openai_key_command("sk-state"))
        .unwrap();
    let evidence = current_probe(
        &connection.connection_id,
        connection.execution_revision,
        "gpt-state",
        "low",
    );
    fixture.repo.record_probe(evidence.clone()).unwrap();
    fixture
        .repo
        .activate(SetActiveAiConfigurationCommand {
            connection_id: connection.connection_id.clone(),
            expected_execution_revision: 1,
            provider: AiProviderKind::OpenAi,
            endpoint_fingerprint: hex_sha256("https://api.openai.com"),
            model_id: "gpt-state".to_owned(),
            reasoning: AiReasoningSelection::Effort {
                id: "low".to_owned(),
            },
            adapter_version: "general-v1".to_owned(),
            catalogue_sha256: catalogue_sha256(&evidence).unwrap(),
            destination_class: AiNetworkDestinationClass::Public,
            confirmed_data_destination: "https://api.openai.com".to_owned(),
        })
        .unwrap();
    let initial = fixture
        .repo
        .fixture_connection_state(&connection.connection_id)
        .unwrap();

    fixture
        .repo
        .rename_connection(UpdateAiConnectionCommand {
            connection_id: connection.connection_id.clone(),
            expected_execution_revision: initial.execution_revision,
            expected_credential_generation: initial.credential_generation,
            display_name: "Renamed OpenAI".to_owned(),
            configuration: None,
            replacement_credential: None,
        })
        .unwrap();
    let renamed = fixture
        .repo
        .fixture_connection_state(&connection.connection_id)
        .unwrap();
    assert_eq!(renamed, initial);
    assert_eq!(
        fixture.repo.inspect().unwrap().readiness,
        quantix_lib::ai::contract::ActiveAiReadiness::Ready
    );

    fixture
        .repo
        .set_enabled(SetAiConnectionEnabledCommand {
            connection_id: connection.connection_id.clone(),
            expected_execution_revision: renamed.execution_revision,
            expected_credential_generation: renamed.credential_generation,
            enabled: false,
        })
        .unwrap();
    assert_eq!(
        fixture.repo.inspect().unwrap().readiness,
        quantix_lib::ai::contract::ActiveAiReadiness::Disabled
    );
    fixture
        .repo
        .set_enabled(SetAiConnectionEnabledCommand {
            connection_id: connection.connection_id.clone(),
            expected_execution_revision: renamed.execution_revision,
            expected_credential_generation: renamed.credential_generation,
            enabled: true,
        })
        .unwrap();
    assert_eq!(
        fixture.repo.inspect().unwrap().readiness,
        quantix_lib::ai::contract::ActiveAiReadiness::Ready
    );

    fixture
        .repo
        .disconnect(DisconnectAiConnectionCommand {
            connection_id: connection.connection_id.clone(),
            expected_execution_revision: renamed.execution_revision,
            expected_credential_generation: renamed.credential_generation,
        })
        .unwrap();
    let disconnected = fixture
        .repo
        .fixture_connection_state(&connection.connection_id)
        .unwrap();
    assert_eq!(disconnected.execution_revision, renamed.execution_revision);
    assert_eq!(
        disconnected.credential_generation,
        renamed.credential_generation + 1
    );
    assert!(disconnected.has_probe_evidence);
    let unavailable = fixture.repo.inspect().unwrap();
    assert!(unavailable.active_configuration.is_some());
    assert_eq!(
        unavailable.readiness,
        quantix_lib::ai::contract::ActiveAiReadiness::AuthenticationRequired
    );

    let cleared = fixture
        .repo
        .clear_active(ClearActiveAiConfigurationCommand {})
        .unwrap();
    assert!(cleared.active_configuration.is_none());
    assert_eq!(
        cleared.readiness,
        quantix_lib::ai::contract::ActiveAiReadiness::NotConfigured
    );
}

#[test]
fn delete_rejects_active_and_nonterminal_references() {
    let fixture = RepositoryFixture::new();
    let connection = fixture
        .repo
        .create_connection(openai_key_command("sk-delete"))
        .unwrap();
    let evidence = current_probe(
        &connection.connection_id,
        connection.execution_revision,
        "gpt-delete",
        "low",
    );
    fixture.repo.record_probe(evidence.clone()).unwrap();
    fixture
        .repo
        .activate(SetActiveAiConfigurationCommand {
            connection_id: connection.connection_id.clone(),
            expected_execution_revision: 1,
            provider: AiProviderKind::OpenAi,
            endpoint_fingerprint: hex_sha256("https://api.openai.com"),
            model_id: "gpt-delete".to_owned(),
            reasoning: AiReasoningSelection::Effort {
                id: "low".to_owned(),
            },
            adapter_version: "general-v1".to_owned(),
            catalogue_sha256: catalogue_sha256(&evidence).unwrap(),
            destination_class: AiNetworkDestinationClass::Public,
            confirmed_data_destination: "https://api.openai.com".to_owned(),
        })
        .unwrap();
    let state = fixture
        .repo
        .fixture_connection_state(&connection.connection_id)
        .unwrap();
    let delete_command = || DeleteAiConnectionCommand {
        connection_id: connection.connection_id.clone(),
        expected_execution_revision: state.execution_revision,
        expected_credential_generation: state.credential_generation,
    };
    assert_eq!(
        fixture
            .repo
            .delete_connection(DeleteAiConnectionCommand {
                connection_id: connection.connection_id.clone(),
                expected_execution_revision: state.execution_revision + 1,
                expected_credential_generation: state.credential_generation,
            })
            .unwrap_err(),
        quantix_lib::ai::connections::AiConnectionError::Conflict
    );
    assert_eq!(
        fixture
            .repo
            .delete_connection(delete_command())
            .unwrap_err(),
        quantix_lib::ai::connections::AiConnectionError::ActiveConnection
    );

    fixture
        .repo
        .clear_active(ClearActiveAiConfigurationCommand {})
        .unwrap();
    fixture
        .tender
        .lock()
        .unwrap()
        .execute(
            "INSERT INTO future_nonterminal_references (connection_id, terminal)
             VALUES (?1, 0)",
            [&connection.connection_id],
        )
        .unwrap();
    assert_eq!(
        fixture
            .repo
            .delete_connection(delete_command())
            .unwrap_err(),
        quantix_lib::ai::connections::AiConnectionError::ReferencedByNonterminalRun
    );

    fixture
        .tender
        .lock()
        .unwrap()
        .execute("UPDATE future_nonterminal_references SET terminal = 1", [])
        .unwrap();
    fixture.repo.delete_connection(delete_command()).unwrap();
    assert!(fixture.repo.inspect().unwrap().connections.is_empty());
}

#[test]
fn missing_targets_and_checker_failures_are_precise_and_nonpublishing() {
    let fixture = RepositoryFixture::new();
    let missing_id = "ffffffffffffffffffffffffffffffff";
    let missing_probe = current_probe(missing_id, 1, "missing-model", "low");
    assert_eq!(
        fixture.repo.record_probe(missing_probe).unwrap_err(),
        quantix_lib::ai::connections::AiConnectionError::NotFound
    );
    assert_eq!(
        fixture
            .repo
            .delete_connection(DeleteAiConnectionCommand {
                connection_id: missing_id.to_owned(),
                expected_execution_revision: 1,
                expected_credential_generation: 1,
            })
            .unwrap_err(),
        quantix_lib::ai::connections::AiConnectionError::NotFound
    );

    let connection = fixture
        .repo
        .create_connection(openai_key_command("checker-error"))
        .unwrap();
    let failing_repo = AiConnectionRepository::new(
        AiConnectionVault::new(&fixture.application_home).unwrap(),
        Arc::clone(&fixture.installation),
        AiAdapterVersions::new("codex-v1", "grok-v1", "general-v1").unwrap(),
        Arc::new(|| "2026-08-24T12:34:56Z".to_owned()),
        Arc::new(|_| Err(quantix_lib::ai::connections::AiConnectionError::StoreUnavailable)),
    );
    assert_eq!(
        failing_repo
            .delete_connection(DeleteAiConnectionCommand {
                connection_id: connection.connection_id.clone(),
                expected_execution_revision: connection.execution_revision,
                expected_credential_generation: connection.credential_generation,
            })
            .unwrap_err(),
        quantix_lib::ai::connections::AiConnectionError::StoreUnavailable
    );
    assert_eq!(fixture.repo.inspect().unwrap().connections.len(), 1);
}

#[test]
fn direct_tender_insertion_is_detected_but_is_not_the_conforming_race_proof() {
    let fixture = RepositoryFixture::new();
    let connection = fixture
        .repo
        .create_connection(openai_key_command("ordered-delete"))
        .unwrap();
    let barrier = Arc::new(Barrier::new(2));
    let insertion_barrier = Arc::clone(&barrier);
    let insertion_tender = Arc::clone(&fixture.tender);
    let inserted_connection_id = connection.connection_id.clone();
    let inserter = std::thread::spawn(move || {
        insertion_barrier.wait();
        insertion_tender
            .lock()
            .unwrap()
            .execute(
                "INSERT INTO future_nonterminal_references (connection_id, terminal)
                 VALUES (?1, 0)",
                [&inserted_connection_id],
            )
            .unwrap();
        insertion_barrier.wait();
    });

    let checker_installation = Arc::clone(&fixture.installation);
    let checker_tender = Arc::clone(&fixture.tender);
    let checker_barrier = Arc::clone(&barrier);
    let ordered_repo = AiConnectionRepository::new(
        AiConnectionVault::new(&fixture.application_home).unwrap(),
        Arc::clone(&fixture.installation),
        AiAdapterVersions::new("codex-v1", "grok-v1", "general-v1").unwrap(),
        Arc::new(|| "2026-08-24T12:34:56Z".to_owned()),
        Arc::new(move |connection_id: &str| {
            if !matches!(
                checker_installation.try_lock(),
                Err(TryLockError::WouldBlock)
            ) {
                return Err(quantix_lib::ai::connections::AiConnectionError::StoreUnavailable);
            }
            checker_barrier.wait();
            checker_barrier.wait();
            checker_tender
                .lock()
                .map_err(|_| quantix_lib::ai::connections::AiConnectionError::StoreUnavailable)?
                .query_row(
                    "SELECT 1 FROM future_nonterminal_references
                     WHERE connection_id = ?1 AND terminal = 0 LIMIT 1",
                    [connection_id],
                    |_| Ok(()),
                )
                .optional()
                .map(|row| row.is_some())
                .map_err(|_| quantix_lib::ai::connections::AiConnectionError::StoreUnavailable)
        }),
    );
    assert_eq!(
        ordered_repo
            .delete_connection(DeleteAiConnectionCommand {
                connection_id: connection.connection_id.clone(),
                expected_execution_revision: connection.execution_revision,
                expected_credential_generation: connection.credential_generation,
            })
            .unwrap_err(),
        quantix_lib::ai::connections::AiConnectionError::ReferencedByNonterminalRun
    );
    inserter.join().unwrap();
    assert_eq!(fixture.repo.inspect().unwrap().connections.len(), 1);
}

#[test]
fn conforming_future_run_and_delete_race_has_one_linearized_winner() {
    let fixture = RepositoryFixture::new();
    let connection = fixture
        .repo
        .create_connection(openai_key_command("conforming-race"))
        .unwrap();
    let barrier = Arc::new(Barrier::new(3));

    let run_repo = Arc::clone(&fixture.repo);
    let run_barrier = Arc::clone(&barrier);
    let run_tender = Arc::clone(&fixture.tender);
    let run_connection_id = connection.connection_id.clone();
    let run = std::thread::spawn(move || {
        run_barrier.wait();
        run_repo.fixture_create_future_run(&run_connection_id, || {
            run_tender
                .lock()
                .map_err(|_| quantix_lib::ai::connections::AiConnectionError::StoreUnavailable)?
                .execute(
                    "INSERT INTO future_nonterminal_references (connection_id, terminal)
                     VALUES (?1, 0)",
                    [&run_connection_id],
                )
                .map_err(|_| quantix_lib::ai::connections::AiConnectionError::StoreUnavailable)?;
            Ok(())
        })
    });

    let delete_repo = Arc::clone(&fixture.repo);
    let delete_barrier = Arc::clone(&barrier);
    let delete_command = DeleteAiConnectionCommand {
        connection_id: connection.connection_id.clone(),
        expected_execution_revision: connection.execution_revision,
        expected_credential_generation: connection.credential_generation,
    };
    let delete = std::thread::spawn(move || {
        delete_barrier.wait();
        delete_repo.delete_connection(delete_command)
    });

    barrier.wait();
    let run_result = run.join().unwrap();
    let delete_result = delete.join().unwrap();
    let tender_count: i64 = fixture
        .tender
        .lock()
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM future_nonterminal_references WHERE terminal = 0",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let connection_count = fixture.repo.inspect().unwrap().connections.len();

    match (run_result, delete_result) {
        (
            Ok(()),
            Err(quantix_lib::ai::connections::AiConnectionError::ReferencedByNonterminalRun),
        ) => {
            assert_eq!(tender_count, 1);
            assert_eq!(connection_count, 1);
        }
        (Err(quantix_lib::ai::connections::AiConnectionError::NotFound), Ok(())) => {
            assert_eq!(tender_count, 0);
            assert_eq!(connection_count, 0);
        }
        outcomes => panic!("nonlinear future-run/delete outcome: {outcomes:?}"),
    }
}

#[test]
fn corrupt_vault_projects_vault_unavailable_readiness() {
    let fixture = RepositoryFixture::new();
    std::fs::write(
        fixture.application_home.join("ai-connections.vault"),
        [0xa5; 64],
    )
    .unwrap();

    let view = fixture.repo.inspect().unwrap();
    assert!(view.connections.is_empty());
    assert!(view.active_configuration.is_none());
    assert_eq!(
        view.readiness,
        quantix_lib::ai::contract::ActiveAiReadiness::VaultUnavailable
    );
}

#[test]
fn readiness_fails_closed_for_destination_adapter_and_catalogue_drift() {
    let destination_fixture = RepositoryFixture::new();
    let (destination_connection, _) = ready_active_openai(&destination_fixture, "destination");
    let raw: String = destination_fixture
        .installation
        .lock()
        .unwrap()
        .query_row(
            "SELECT settings_json FROM application_settings WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let mut json: serde_json::Value = serde_json::from_str(&raw).unwrap();
    json["active_ai_configuration"]["data_destination"] =
        serde_json::Value::String("https://wrong.example".to_owned());
    destination_fixture
        .installation
        .lock()
        .unwrap()
        .execute(
            "UPDATE application_settings SET settings_json = ?1 WHERE singleton = 1",
            [serde_json::to_string(&json).unwrap()],
        )
        .unwrap();
    assert_eq!(
        destination_fixture.repo.inspect().unwrap().readiness,
        quantix_lib::ai::contract::ActiveAiReadiness::StaleRevision,
        "destination drift for {} must fail closed",
        destination_connection.connection_id
    );

    let adapter_fixture = RepositoryFixture::new();
    ready_active_openai(&adapter_fixture, "adapter");
    let drifted_adapter_repo = AiConnectionRepository::new(
        AiConnectionVault::new(&adapter_fixture.application_home).unwrap(),
        Arc::clone(&adapter_fixture.installation),
        AiAdapterVersions::new("codex-v1", "grok-v1", "general-v2").unwrap(),
        Arc::new(|| "2026-08-24T12:34:56Z".to_owned()),
        Arc::new(no_nonterminal_reference),
    );
    assert_eq!(
        drifted_adapter_repo.inspect().unwrap().readiness,
        quantix_lib::ai::contract::ActiveAiReadiness::WorkerUnavailable
    );

    let catalogue_fixture = RepositoryFixture::new();
    let (catalogue_connection, _) = ready_active_openai(&catalogue_fixture, "catalogue");
    catalogue_fixture
        .repo
        .record_probe(current_probe(
            &catalogue_connection.connection_id,
            catalogue_connection.execution_revision,
            "replacement-model",
            "high",
        ))
        .unwrap();
    assert_eq!(
        catalogue_fixture.repo.inspect().unwrap().readiness,
        quantix_lib::ai::contract::ActiveAiReadiness::CapabilityChanged
    );
}

fn ready_active_openai(
    fixture: &RepositoryFixture,
    suffix: &str,
) -> (quantix_lib::ai::contract::AiConnectionView, AiProbeEvidence) {
    let (connection, evidence) = ready_unactivated_openai(fixture, suffix);
    fixture
        .repo
        .activate(SetActiveAiConfigurationCommand {
            connection_id: connection.connection_id.clone(),
            expected_execution_revision: connection.execution_revision,
            provider: connection.provider,
            endpoint_fingerprint: connection.endpoint_fingerprint.clone(),
            model_id: connection.tested_model_id.clone().unwrap(),
            reasoning: connection.tested_reasoning.clone().unwrap(),
            adapter_version: connection.adapter_version.clone().unwrap(),
            catalogue_sha256: connection.catalogue_sha256.clone().unwrap(),
            destination_class: connection.destination_class.unwrap(),
            confirmed_data_destination: connection.data_destination.clone(),
        })
        .unwrap();
    (connection, evidence)
}

fn ready_unactivated_openai(
    fixture: &RepositoryFixture,
    suffix: &str,
) -> (quantix_lib::ai::contract::AiConnectionView, AiProbeEvidence) {
    let connection = fixture
        .repo
        .create_connection(openai_key_command(&format!("sk-{suffix}")))
        .unwrap();
    let evidence = current_probe(
        &connection.connection_id,
        connection.execution_revision,
        &format!("gpt-{suffix}"),
        "low",
    );
    let tested = fixture.repo.record_probe(evidence.clone()).unwrap();
    (tested, evidence)
}

#[test]
fn all_seven_routes_are_saved_without_secret_projection() {
    let fixture = RepositoryFixture::new();
    let mut commands = vec![codex_account_command("route-access", "route-refresh")];
    for (name, provider, secret_value) in [
        ("OpenAI", AiProviderKind::OpenAi, "openai-route-secret"),
        (
            "Anthropic",
            AiProviderKind::Anthropic,
            "anthropic-route-secret",
        ),
        (
            "Gemini",
            AiProviderKind::GoogleGemini,
            "gemini-route-secret",
        ),
        ("xAI", AiProviderKind::XAi, "xai-route-secret"),
    ] {
        commands.push(CreateAiConnectionCommand {
            display_name: name.to_owned(),
            configuration: AiConnectionConfiguration::DirectProviderKey { provider },
            credential: AiCredentialInput::ApiKey {
                api_key: secret(secret_value),
                custom_header_values: Vec::new(),
                custom_query_values: Vec::new(),
            },
        });
    }
    for (name, configuration) in [
        (
            "OpenAI compatible",
            AiConnectionConfiguration::OpenAiCompatible {
                provider: AiProviderKind::OpenAiCompatible,
                endpoint: CompatibleEndpointConfiguration::parse(
                    "https://openai-compatible.example/v1",
                    CompatibleCredentialKind::Bearer,
                    vec!["x-tenant".to_owned()],
                    vec!["version".to_owned()],
                    "compatible-model",
                )
                .unwrap(),
            },
        ),
        (
            "Anthropic compatible",
            AiConnectionConfiguration::AnthropicCompatible {
                provider: AiProviderKind::AnthropicCompatible,
                endpoint: CompatibleEndpointConfiguration::parse(
                    "https://anthropic-compatible.example/messages",
                    CompatibleCredentialKind::ApiKeyHeader {
                        name: "x-api-key".to_owned(),
                    },
                    vec!["x-workspace".to_owned()],
                    vec!["revision".to_owned()],
                    "compatible-model",
                )
                .unwrap(),
            },
        ),
    ] {
        commands.push(CreateAiConnectionCommand {
            display_name: name.to_owned(),
            configuration,
            credential: AiCredentialInput::ApiKey {
                api_key: secret("compatible-route-secret"),
                custom_header_values: vec![SecretNameValueInput {
                    name: if name.starts_with("OpenAI") {
                        "x-tenant".to_owned()
                    } else {
                        "x-workspace".to_owned()
                    },
                    value: secret("header-route-secret"),
                }],
                custom_query_values: vec![SecretNameValueInput {
                    name: if name.starts_with("OpenAI") {
                        "version".to_owned()
                    } else {
                        "revision".to_owned()
                    },
                    value: secret("query-route-secret"),
                }],
            },
        });
    }

    for command in commands {
        fixture.repo.create_connection(command).unwrap();
    }
    let view = fixture.repo.inspect().unwrap();
    assert_eq!(view.connections.len(), 7);
    let mut providers = view
        .connections
        .iter()
        .map(|connection| connection.provider)
        .collect::<Vec<_>>();
    providers.sort_by_key(|provider| format!("{provider:?}"));
    providers.dedup();
    assert_eq!(providers.len(), 7);
    assert!(view.connections.iter().all(|connection| {
        connection.connection_id.len() == 32
            && connection
                .connection_id
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    }));
    let json = serde_json::to_string(&view).unwrap();
    for sentinel in [
        "route-access",
        "route-refresh",
        "route-secret",
        "header-route-secret",
        "query-route-secret",
    ] {
        assert!(!json.contains(sentinel), "leaked sentinel: {sentinel}");
    }
}

#[test]
fn fixed_connection_and_secret_bounds_are_enforced() {
    let fixture = RepositoryFixture::new();
    for index in 0..32 {
        fixture
            .repo
            .create_connection(CreateAiConnectionCommand {
                display_name: format!("Connection {index:02}"),
                configuration: AiConnectionConfiguration::DirectProviderKey {
                    provider: AiProviderKind::OpenAi,
                },
                credential: AiCredentialInput::ApiKey {
                    api_key: secret(format!("secret-{index}")),
                    custom_header_values: Vec::new(),
                    custom_query_values: Vec::new(),
                },
            })
            .unwrap();
    }
    assert_eq!(
        fixture
            .repo
            .create_connection(openai_key_command("connection-33"))
            .unwrap_err(),
        quantix_lib::ai::connections::AiConnectionError::InvalidCommand
    );

    let account_fixture = RepositoryFixture::new();
    let oversized_account = "a".repeat(4_097);
    assert_eq!(
        account_fixture
            .repo
            .create_connection(CreateAiConnectionCommand {
                display_name: "Oversized account".to_owned(),
                configuration: AiConnectionConfiguration::AccountLogin {
                    provider: AiProviderKind::Codex,
                    account_id: oversized_account.clone(),
                },
                credential: AiCredentialInput::Account {
                    access_token: secret("access"),
                    refresh_token: Some(secret("refresh")),
                    expires_at: "2026-08-24T13:00:00Z".to_owned(),
                    verified_account_id: oversized_account,
                },
            })
            .unwrap_err(),
        quantix_lib::ai::connections::AiConnectionError::InvalidCommand
    );

    let custom_fixture = RepositoryFixture::new();
    assert_eq!(
        custom_fixture
            .repo
            .create_connection(CreateAiConnectionCommand {
                display_name: "Oversized custom value".to_owned(),
                configuration: AiConnectionConfiguration::OpenAiCompatible {
                    provider: AiProviderKind::OpenAiCompatible,
                    endpoint: CompatibleEndpointConfiguration::parse(
                        "https://bounds.example/v1",
                        CompatibleCredentialKind::Bearer,
                        vec!["x-tenant".to_owned()],
                        Vec::new(),
                        "model",
                    )
                    .unwrap(),
                },
                credential: AiCredentialInput::ApiKey {
                    api_key: secret("key"),
                    custom_header_values: vec![SecretNameValueInput {
                        name: "x-tenant".to_owned(),
                        value: secret("v".repeat(4_097)),
                    }],
                    custom_query_values: Vec::new(),
                },
            })
            .unwrap_err(),
        quantix_lib::ai::connections::AiConnectionError::InvalidCommand
    );
}

#[test]
fn credential_aggregate_bound_counts_only_secret_value_bytes() {
    let direct_fixture = RepositoryFixture::new();
    let exact = direct_fixture
        .repo
        .create_connection(openai_key_command(&"k".repeat(16_384)))
        .unwrap();
    assert!(exact.secret_configured);

    let oversized_fixture = RepositoryFixture::new();
    assert_eq!(
        oversized_fixture
            .repo
            .create_connection(openai_key_command(&"k".repeat(16_385)))
            .unwrap_err(),
        quantix_lib::ai::connections::AiConnectionError::InvalidCommand
    );
    assert!(oversized_fixture
        .repo
        .inspect()
        .unwrap()
        .connections
        .is_empty());

    let endpoint = CompatibleEndpointConfiguration::parse(
        "https://credential-bound.example/v1",
        CompatibleCredentialKind::Bearer,
        vec!["x-bound".to_owned()],
        Vec::new(),
        "model",
    )
    .unwrap();
    let compatible_fixture = RepositoryFixture::new();
    assert!(compatible_fixture
        .repo
        .create_connection(CreateAiConnectionCommand {
            display_name: "Exact compatible credential".to_owned(),
            configuration: AiConnectionConfiguration::OpenAiCompatible {
                provider: AiProviderKind::OpenAiCompatible,
                endpoint: endpoint.clone(),
            },
            credential: AiCredentialInput::ApiKey {
                api_key: secret("a".repeat(12_288)),
                custom_header_values: vec![SecretNameValueInput {
                    name: "x-bound".to_owned(),
                    value: secret("h".repeat(4_096)),
                }],
                custom_query_values: Vec::new(),
            },
        })
        .is_ok());
    let compatible_oversized = RepositoryFixture::new();
    assert_eq!(
        compatible_oversized
            .repo
            .create_connection(CreateAiConnectionCommand {
                display_name: "Oversized compatible credential".to_owned(),
                configuration: AiConnectionConfiguration::OpenAiCompatible {
                    provider: AiProviderKind::OpenAiCompatible,
                    endpoint,
                },
                credential: AiCredentialInput::ApiKey {
                    api_key: secret("a".repeat(12_289)),
                    custom_header_values: vec![SecretNameValueInput {
                        name: "x-bound".to_owned(),
                        value: secret("h".repeat(4_096)),
                    }],
                    custom_query_values: Vec::new(),
                },
            })
            .unwrap_err(),
        quantix_lib::ai::connections::AiConnectionError::InvalidCommand
    );
}

#[test]
fn probe_and_activation_require_exact_nonrerouted_evidence() {
    let fixture = RepositoryFixture::new();
    let connection = fixture
        .repo
        .create_connection(openai_key_command("sk-exact"))
        .unwrap();
    let exact = current_probe(
        &connection.connection_id,
        connection.execution_revision,
        "gpt-exact",
        "low",
    );
    for mut mismatch in [
        {
            let mut evidence = exact.clone();
            evidence.provider = AiProviderKind::Anthropic;
            evidence
        },
        {
            let mut evidence = exact.clone();
            evidence.endpoint_fingerprint = hex_sha256("https://wrong.example");
            evidence
        },
        {
            let mut evidence = exact.clone();
            evidence.adapter_version = "general-v2".to_owned();
            evidence
        },
    ] {
        assert_eq!(
            fixture.repo.record_probe(mismatch.clone()).unwrap_err(),
            quantix_lib::ai::connections::AiConnectionError::InvalidCommand
        );
        mismatch.models.clear();
    }
    assert!(fixture.repo.inspect().unwrap().connections[0]
        .models
        .is_empty());

    let mut rerouted = exact.clone();
    rerouted.models[0].reported_model_id = Some("provider-reroute".to_owned());
    assert_eq!(
        fixture.repo.record_probe(rerouted).unwrap_err(),
        quantix_lib::ai::connections::AiConnectionError::InvalidCommand
    );

    let mut with_sibling = exact.clone();
    let mut sibling = with_sibling.models[0].clone();
    sibling.model_id = "gpt-sibling".to_owned();
    sibling.reported_model_id = Some("gpt-sibling".to_owned());
    sibling.reasoning_options[0].selection = AiReasoningSelection::Effort {
        id: "high".to_owned(),
    };
    with_sibling.models.push(sibling);
    fixture.repo.record_probe(with_sibling.clone()).unwrap();
    let sibling_command = SetActiveAiConfigurationCommand {
        connection_id: connection.connection_id.clone(),
        expected_execution_revision: connection.execution_revision,
        provider: AiProviderKind::OpenAi,
        endpoint_fingerprint: hex_sha256("https://api.openai.com"),
        model_id: "gpt-sibling".to_owned(),
        reasoning: AiReasoningSelection::Effort {
            id: "high".to_owned(),
        },
        adapter_version: "general-v1".to_owned(),
        catalogue_sha256: catalogue_sha256(&with_sibling).unwrap(),
        destination_class: AiNetworkDestinationClass::Public,
        confirmed_data_destination: "https://api.openai.com".to_owned(),
    };
    assert_eq!(
        fixture.repo.activate(sibling_command).unwrap_err(),
        quantix_lib::ai::connections::AiConnectionError::CapabilityChanged
    );

    fixture.repo.record_probe(exact.clone()).unwrap();
    let exact_command = SetActiveAiConfigurationCommand {
        connection_id: connection.connection_id.clone(),
        expected_execution_revision: connection.execution_revision,
        provider: AiProviderKind::OpenAi,
        endpoint_fingerprint: hex_sha256("https://api.openai.com"),
        model_id: "gpt-exact".to_owned(),
        reasoning: AiReasoningSelection::Effort {
            id: "low".to_owned(),
        },
        adapter_version: "general-v1".to_owned(),
        catalogue_sha256: catalogue_sha256(&exact).unwrap(),
        destination_class: AiNetworkDestinationClass::Public,
        confirmed_data_destination: "https://api.openai.com".to_owned(),
    };
    let mut wrong_model = exact_command.clone();
    wrong_model.model_id = "not-tested".to_owned();
    assert_eq!(
        fixture.repo.activate(wrong_model).unwrap_err(),
        quantix_lib::ai::connections::AiConnectionError::CapabilityChanged
    );
    let mut wrong_reasoning = exact_command.clone();
    wrong_reasoning.reasoning = AiReasoningSelection::Unsupported;
    assert_eq!(
        fixture.repo.activate(wrong_reasoning).unwrap_err(),
        quantix_lib::ai::connections::AiConnectionError::CapabilityChanged
    );
    let mut wrong_hash = exact_command.clone();
    wrong_hash.catalogue_sha256 = "0".repeat(64);
    assert_eq!(
        fixture.repo.activate(wrong_hash).unwrap_err(),
        quantix_lib::ai::connections::AiConnectionError::CapabilityChanged
    );
    let mut wrong_adapter = exact_command.clone();
    wrong_adapter.adapter_version = "general-v2".to_owned();
    assert_eq!(
        fixture.repo.activate(wrong_adapter).unwrap_err(),
        quantix_lib::ai::connections::AiConnectionError::WorkerUnavailable
    );
    let mut wrong_destination = exact_command;
    wrong_destination.confirmed_data_destination = "https://wrong.example".to_owned();
    assert_eq!(
        fixture.repo.activate(wrong_destination).unwrap_err(),
        quantix_lib::ai::connections::AiConnectionError::InvalidCommand
    );
}

#[test]
fn untested_sibling_reasoning_effort_is_not_activatable() {
    let fixture = RepositoryFixture::new();
    let connection = fixture
        .repo
        .create_connection(openai_key_command("sibling-effort"))
        .unwrap();
    let mut evidence = current_probe(
        &connection.connection_id,
        connection.execution_revision,
        "gpt-efforts",
        "low",
    );
    evidence.models[0]
        .reasoning_options
        .push(AiReasoningOption {
            selection: AiReasoningSelection::Effort {
                id: "high".to_owned(),
            },
            label: "High".to_owned(),
            description: "High reasoning effort".to_owned(),
        });
    let tested = fixture.repo.record_probe(evidence).unwrap();
    let command = |reasoning| SetActiveAiConfigurationCommand {
        connection_id: tested.connection_id.clone(),
        expected_execution_revision: tested.execution_revision,
        provider: tested.provider,
        endpoint_fingerprint: tested.endpoint_fingerprint.clone(),
        model_id: tested.tested_model_id.clone().unwrap(),
        reasoning,
        adapter_version: tested.adapter_version.clone().unwrap(),
        catalogue_sha256: tested.catalogue_sha256.clone().unwrap(),
        destination_class: tested.destination_class.unwrap(),
        confirmed_data_destination: tested.data_destination.clone(),
    };
    assert_eq!(
        fixture
            .repo
            .activate(command(AiReasoningSelection::Effort {
                id: "high".to_owned(),
            }))
            .unwrap_err(),
        quantix_lib::ai::connections::AiConnectionError::CapabilityChanged
    );
    assert_eq!(
        fixture
            .repo
            .activate(command(AiReasoningSelection::Effort {
                id: "low".to_owned(),
            }))
            .unwrap()
            .readiness,
        quantix_lib::ai::contract::ActiveAiReadiness::Ready
    );
}

#[test]
fn compatible_probe_must_test_the_configured_explicit_model() {
    let fixture = RepositoryFixture::new();
    let endpoint = CompatibleEndpointConfiguration::parse(
        "https://compatible-model.example/v1",
        CompatibleCredentialKind::Bearer,
        Vec::new(),
        Vec::new(),
        "configured-model",
    )
    .unwrap();
    let connection = fixture
        .repo
        .create_connection(CreateAiConnectionCommand {
            display_name: "Compatible model".to_owned(),
            configuration: AiConnectionConfiguration::OpenAiCompatible {
                provider: AiProviderKind::OpenAiCompatible,
                endpoint: endpoint.clone(),
            },
            credential: AiCredentialInput::ApiKey {
                api_key: secret("compatible-secret"),
                custom_header_values: Vec::new(),
                custom_query_values: Vec::new(),
            },
        })
        .unwrap();
    let mut wrong = current_probe(
        &connection.connection_id,
        connection.execution_revision,
        "different-model",
        "low",
    );
    wrong.provider = AiProviderKind::OpenAiCompatible;
    wrong.endpoint_fingerprint = hex_sha256(&endpoint.base_url);
    assert_eq!(
        fixture.repo.record_probe(wrong).unwrap_err(),
        quantix_lib::ai::connections::AiConnectionError::InvalidCommand
    );
}

#[test]
fn network_destination_class_is_pinned_through_probe_activation_and_readiness() {
    let fixture = RepositoryFixture::new();
    let endpoint = CompatibleEndpointConfiguration::parse(
        "https://destination-class.example/v1",
        CompatibleCredentialKind::Bearer,
        Vec::new(),
        Vec::new(),
        "class-model",
    )
    .unwrap();
    let connection = fixture
        .repo
        .create_connection(CreateAiConnectionCommand {
            display_name: "Private destination".to_owned(),
            configuration: AiConnectionConfiguration::OpenAiCompatible {
                provider: AiProviderKind::OpenAiCompatible,
                endpoint: endpoint.clone(),
            },
            credential: AiCredentialInput::ApiKey {
                api_key: secret("class-secret"),
                custom_header_values: Vec::new(),
                custom_query_values: Vec::new(),
            },
        })
        .unwrap();
    let mut private_evidence = current_probe(
        &connection.connection_id,
        connection.execution_revision,
        "class-model",
        "low",
    );
    private_evidence.provider = AiProviderKind::OpenAiCompatible;
    private_evidence.endpoint_fingerprint = hex_sha256(&endpoint.base_url);
    private_evidence.destination_class = AiNetworkDestinationClass::Private;
    let tested = fixture.repo.record_probe(private_evidence.clone()).unwrap();
    assert_eq!(
        tested.destination_class,
        Some(AiNetworkDestinationClass::Private)
    );
    let command = |destination_class| SetActiveAiConfigurationCommand {
        connection_id: tested.connection_id.clone(),
        expected_execution_revision: tested.execution_revision,
        provider: tested.provider,
        endpoint_fingerprint: tested.endpoint_fingerprint.clone(),
        model_id: tested.tested_model_id.clone().unwrap(),
        reasoning: tested.tested_reasoning.clone().unwrap(),
        adapter_version: tested.adapter_version.clone().unwrap(),
        catalogue_sha256: tested.catalogue_sha256.clone().unwrap(),
        destination_class,
        confirmed_data_destination: tested.data_destination.clone(),
    };
    assert_eq!(
        fixture
            .repo
            .activate(command(AiNetworkDestinationClass::Public))
            .unwrap_err(),
        quantix_lib::ai::connections::AiConnectionError::CapabilityChanged
    );
    assert_eq!(
        fixture
            .repo
            .activate(command(AiNetworkDestinationClass::Private))
            .unwrap()
            .readiness,
        quantix_lib::ai::contract::ActiveAiReadiness::Ready
    );
    let mut public_evidence = private_evidence;
    public_evidence.destination_class = AiNetworkDestinationClass::Public;
    fixture.repo.record_probe(public_evidence).unwrap();
    assert_eq!(
        fixture.repo.inspect().unwrap().readiness,
        quantix_lib::ai::contract::ActiveAiReadiness::CapabilityChanged
    );

    let loopback_fixture = RepositoryFixture::new();
    let loopback_endpoint = CompatibleEndpointConfiguration::parse(
        "http://127.0.0.1:11434/v1",
        CompatibleCredentialKind::Bearer,
        Vec::new(),
        Vec::new(),
        "loopback-model",
    )
    .unwrap();
    let loopback = loopback_fixture
        .repo
        .create_connection(CreateAiConnectionCommand {
            display_name: "Loopback".to_owned(),
            configuration: AiConnectionConfiguration::OpenAiCompatible {
                provider: AiProviderKind::OpenAiCompatible,
                endpoint: loopback_endpoint.clone(),
            },
            credential: AiCredentialInput::ApiKey {
                api_key: secret("loopback-secret"),
                custom_header_values: Vec::new(),
                custom_query_values: Vec::new(),
            },
        })
        .unwrap();
    let mut loopback_evidence = current_probe(
        &loopback.connection_id,
        loopback.execution_revision,
        "loopback-model",
        "low",
    );
    loopback_evidence.provider = AiProviderKind::OpenAiCompatible;
    loopback_evidence.endpoint_fingerprint = hex_sha256(&loopback_endpoint.base_url);
    assert_eq!(
        loopback_fixture
            .repo
            .record_probe(loopback_evidence.clone())
            .unwrap_err(),
        quantix_lib::ai::connections::AiConnectionError::InvalidCommand
    );
    loopback_evidence.destination_class = AiNetworkDestinationClass::Loopback;
    assert_eq!(
        loopback_fixture
            .repo
            .record_probe(loopback_evidence)
            .unwrap()
            .destination_class,
        Some(AiNetworkDestinationClass::Loopback)
    );
}

#[test]
fn stale_cas_and_counter_overflow_never_publish() {
    let fixture = RepositoryFixture::new();
    let connection = fixture
        .repo
        .create_connection(openai_key_command("sk-cas"))
        .unwrap();
    let state = fixture
        .repo
        .fixture_connection_state(&connection.connection_id)
        .unwrap();
    let material_update = |name: &str| UpdateAiConnectionCommand {
        connection_id: connection.connection_id.clone(),
        expected_execution_revision: state.execution_revision,
        expected_credential_generation: state.credential_generation,
        display_name: name.to_owned(),
        configuration: Some(AiConnectionConfiguration::DirectProviderKey {
            provider: AiProviderKind::Anthropic,
        }),
        replacement_credential: None,
    };
    fixture
        .repo
        .replace_connection_configuration(material_update("First material update"))
        .unwrap();
    assert_eq!(
        fixture
            .repo
            .replace_connection_configuration(material_update("Stale update"))
            .unwrap_err(),
        quantix_lib::ai::connections::AiConnectionError::Conflict
    );
    assert_eq!(
        fixture.repo.inspect().unwrap().connections[0].display_name,
        "First material update"
    );
    let mut stale_probe = current_probe(
        &connection.connection_id,
        state.execution_revision,
        "stale-model",
        "low",
    );
    stale_probe.provider = AiProviderKind::Anthropic;
    stale_probe.endpoint_fingerprint = hex_sha256("https://api.anthropic.com");
    assert_eq!(
        fixture.repo.record_probe(stale_probe).unwrap_err(),
        quantix_lib::ai::connections::AiConnectionError::Conflict
    );

    let vault = AiConnectionVault::new(&fixture.application_home).unwrap();
    vault
        .fixture_set_connection_counters(&connection.connection_id, u64::MAX, 1)
        .unwrap();
    assert_eq!(
        fixture
            .repo
            .replace_connection_configuration(UpdateAiConnectionCommand {
                connection_id: connection.connection_id.clone(),
                expected_execution_revision: u64::MAX,
                expected_credential_generation: 1,
                display_name: "Overflow rejected".to_owned(),
                configuration: Some(AiConnectionConfiguration::DirectProviderKey {
                    provider: AiProviderKind::OpenAi,
                }),
                replacement_credential: None,
            })
            .unwrap_err(),
        quantix_lib::ai::connections::AiConnectionError::RevisionOverflow
    );
    let after = fixture
        .repo
        .fixture_connection_state(&connection.connection_id)
        .unwrap();
    assert_eq!(after.execution_revision, u64::MAX);
    assert_eq!(after.credential_generation, 1);

    let account_fixture = RepositoryFixture::new();
    let account = account_fixture
        .repo
        .create_connection(codex_account_command("access-overflow", "refresh-overflow"))
        .unwrap();
    AiConnectionVault::new(&account_fixture.application_home)
        .unwrap()
        .fixture_set_connection_counters(&account.connection_id, 1, u64::MAX)
        .unwrap();
    assert_eq!(
        account_fixture
            .repo
            .rotate_same_account_tokens(
                SameAccountTokenRefreshCommand {
                    connection_id: account.connection_id.clone(),
                    expected_execution_revision: 1,
                    expected_credential_generation: u64::MAX,
                    verified_account_id: "account-123".to_owned(),
                },
                AiCredentialInput::Account {
                    access_token: secret("rotated-access"),
                    refresh_token: Some(secret("rotated-refresh")),
                    expires_at: "2026-08-24T15:00:00Z".to_owned(),
                    verified_account_id: "account-123".to_owned(),
                }
            )
            .unwrap_err(),
        quantix_lib::ai::connections::AiConnectionError::RevisionOverflow
    );
    let account_after = account_fixture
        .repo
        .fixture_connection_state(&account.connection_id)
        .unwrap();
    assert_eq!(account_after.execution_revision, 1);
    assert_eq!(account_after.credential_generation, u64::MAX);
}

#[test]
fn account_credentials_require_refresh_and_are_always_redacted() {
    let fixture = RepositoryFixture::new();
    let missing_refresh = CreateAiConnectionCommand {
        display_name: "Missing refresh".to_owned(),
        configuration: AiConnectionConfiguration::AccountLogin {
            provider: AiProviderKind::Codex,
            account_id: "account-refresh".to_owned(),
        },
        credential: AiCredentialInput::Account {
            access_token: secret("access-secret-sentinel"),
            refresh_token: None,
            expires_at: "2026-08-24T13:00:00Z".to_owned(),
            verified_account_id: "account-refresh".to_owned(),
        },
    };
    let debug = format!("{missing_refresh:?}");
    assert_eq!(debug, "CreateAiConnectionCommand([REDACTED])");
    assert!(!debug.contains("access-secret-sentinel"));
    assert_eq!(
        fixture.repo.create_connection(missing_refresh).unwrap_err(),
        quantix_lib::ai::connections::AiConnectionError::InvalidCommand
    );
    let empty_refresh = CreateAiConnectionCommand {
        display_name: "Empty refresh".to_owned(),
        configuration: AiConnectionConfiguration::AccountLogin {
            provider: AiProviderKind::Codex,
            account_id: "account-refresh".to_owned(),
        },
        credential: AiCredentialInput::Account {
            access_token: secret("access-secret-sentinel"),
            refresh_token: Some(secret(String::new())),
            expires_at: "2026-08-24T13:00:00Z".to_owned(),
            verified_account_id: "account-refresh".to_owned(),
        },
    };
    assert_eq!(
        fixture.repo.create_connection(empty_refresh).unwrap_err(),
        quantix_lib::ai::connections::AiConnectionError::InvalidCommand
    );
    assert!(fixture.repo.inspect().unwrap().connections.is_empty());
}

#[test]
fn concurrent_repository_handles_serialize_material_cas() {
    let fixture = RepositoryFixture::new();
    let connection = fixture
        .repo
        .create_connection(openai_key_command("sk-concurrent"))
        .unwrap();
    let state = fixture
        .repo
        .fixture_connection_state(&connection.connection_id)
        .unwrap();
    let second_repo = Arc::new(AiConnectionRepository::new(
        AiConnectionVault::new(&fixture.application_home).unwrap(),
        Arc::clone(&fixture.installation),
        AiAdapterVersions::new("codex-v1", "grok-v1", "general-v1").unwrap(),
        Arc::new(|| "2026-08-24T12:34:56Z".to_owned()),
        Arc::new(no_nonterminal_reference),
    ));
    let barrier = Arc::new(Barrier::new(3));
    let mut workers = Vec::new();
    for (repo, provider, name) in [
        (
            Arc::clone(&fixture.repo),
            AiProviderKind::Anthropic,
            "Anthropic winner",
        ),
        (second_repo, AiProviderKind::GoogleGemini, "Gemini winner"),
    ] {
        let barrier = Arc::clone(&barrier);
        let connection_id = connection.connection_id.clone();
        workers.push(std::thread::spawn(move || {
            barrier.wait();
            repo.replace_connection_configuration(UpdateAiConnectionCommand {
                connection_id,
                expected_execution_revision: state.execution_revision,
                expected_credential_generation: state.credential_generation,
                display_name: name.to_owned(),
                configuration: Some(AiConnectionConfiguration::DirectProviderKey { provider }),
                replacement_credential: None,
            })
        }));
    }
    barrier.wait();
    let results = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(
                result,
                Err(quantix_lib::ai::connections::AiConnectionError::Conflict)
            ))
            .count(),
        1
    );
    let final_state = fixture
        .repo
        .fixture_connection_state(&connection.connection_id)
        .unwrap();
    assert_eq!(final_state.execution_revision, 2);
    assert_eq!(final_state.credential_generation, 1);
}

#[test]
fn sqlite_immediate_failure_preserves_the_prior_active_reference() {
    let fixture = RepositoryFixture::new();
    let connection = fixture
        .repo
        .create_connection(openai_key_command("sk-sqlite"))
        .unwrap();
    let evidence = current_probe(
        &connection.connection_id,
        connection.execution_revision,
        "gpt-sqlite",
        "low",
    );
    fixture.repo.record_probe(evidence.clone()).unwrap();
    fixture
        .installation
        .lock()
        .unwrap()
        .execute_batch("BEGIN IMMEDIATE")
        .unwrap();
    let result = fixture.repo.activate(SetActiveAiConfigurationCommand {
        connection_id: connection.connection_id.clone(),
        expected_execution_revision: connection.execution_revision,
        provider: AiProviderKind::OpenAi,
        endpoint_fingerprint: hex_sha256("https://api.openai.com"),
        model_id: "gpt-sqlite".to_owned(),
        reasoning: AiReasoningSelection::Effort {
            id: "low".to_owned(),
        },
        adapter_version: "general-v1".to_owned(),
        catalogue_sha256: catalogue_sha256(&evidence).unwrap(),
        destination_class: AiNetworkDestinationClass::Public,
        confirmed_data_destination: "https://api.openai.com".to_owned(),
    });
    assert_eq!(
        result.unwrap_err(),
        quantix_lib::ai::connections::AiConnectionError::StoreUnavailable
    );
    fixture
        .installation
        .lock()
        .unwrap()
        .execute_batch("ROLLBACK")
        .unwrap();
    assert!(fixture
        .repo
        .inspect()
        .unwrap()
        .active_configuration
        .is_none());
}

#[test]
fn committed_settings_errors_reconcile_activation_and_clear_to_success() {
    let activation_fixture = RepositoryFixture::new();
    let (tested, _) = ready_unactivated_openai(&activation_fixture, "commit-after");
    activation_fixture
        .repo
        .fixture_set_next_settings_commit_outcome(FixtureSettingsCommitOutcome::ErrorAfterCommit);
    let activated = activation_fixture
        .repo
        .activate(SetActiveAiConfigurationCommand {
            connection_id: tested.connection_id,
            expected_execution_revision: tested.execution_revision,
            provider: tested.provider,
            endpoint_fingerprint: tested.endpoint_fingerprint,
            model_id: tested.tested_model_id.unwrap(),
            reasoning: tested.tested_reasoning.unwrap(),
            adapter_version: tested.adapter_version.unwrap(),
            catalogue_sha256: tested.catalogue_sha256.unwrap(),
            destination_class: tested.destination_class.unwrap(),
            confirmed_data_destination: tested.data_destination,
        })
        .unwrap();
    let (activation_json, activation_updated_at) =
        raw_settings_row(&activation_fixture.installation);
    let activation_stored: serde_json::Value = serde_json::from_str(&activation_json).unwrap();
    assert_eq!(activation_updated_at, "2026-08-24T12:34:56Z");
    assert_eq!(
        activation_stored["active_ai_configuration"],
        serde_json::to_value(activated.active_configuration.unwrap()).unwrap()
    );

    let clear_fixture = RepositoryFixture::new();
    ready_active_openai(&clear_fixture, "clear-after-commit");
    clear_fixture
        .repo
        .fixture_set_next_settings_commit_outcome(FixtureSettingsCommitOutcome::ErrorAfterCommit);
    let cleared = clear_fixture
        .repo
        .clear_active(ClearActiveAiConfigurationCommand {})
        .unwrap();
    let (clear_json, clear_updated_at) = raw_settings_row(&clear_fixture.installation);
    let clear_stored: serde_json::Value = serde_json::from_str(&clear_json).unwrap();
    assert_eq!(clear_updated_at, "2026-08-24T12:34:56Z");
    assert!(clear_stored["active_ai_configuration"].is_null());
    assert!(cleared.active_configuration.is_none());
    assert_eq!(
        cleared.readiness,
        quantix_lib::ai::contract::ActiveAiReadiness::NotConfigured
    );
}

#[test]
fn rolled_back_settings_errors_preserve_exact_prior_rows() {
    let activation_fixture = RepositoryFixture::new();
    let (tested, _) = ready_unactivated_openai(&activation_fixture, "commit-before");
    let activation_prior = raw_settings_row(&activation_fixture.installation);
    activation_fixture
        .repo
        .fixture_set_next_settings_commit_outcome(FixtureSettingsCommitOutcome::ErrorBeforeCommit);
    assert_eq!(
        activation_fixture
            .repo
            .activate(SetActiveAiConfigurationCommand {
                connection_id: tested.connection_id,
                expected_execution_revision: tested.execution_revision,
                provider: tested.provider,
                endpoint_fingerprint: tested.endpoint_fingerprint,
                model_id: tested.tested_model_id.unwrap(),
                reasoning: tested.tested_reasoning.unwrap(),
                adapter_version: tested.adapter_version.unwrap(),
                catalogue_sha256: tested.catalogue_sha256.unwrap(),
                destination_class: tested.destination_class.unwrap(),
                confirmed_data_destination: tested.data_destination,
            })
            .unwrap_err(),
        quantix_lib::ai::connections::AiConnectionError::StoreUnavailable
    );
    assert_eq!(
        raw_settings_row(&activation_fixture.installation),
        activation_prior
    );

    let clear_fixture = RepositoryFixture::new();
    ready_active_openai(&clear_fixture, "clear-before-commit");
    let clear_prior = raw_settings_row(&clear_fixture.installation);
    clear_fixture
        .repo
        .fixture_set_next_settings_commit_outcome(FixtureSettingsCommitOutcome::ErrorBeforeCommit);
    assert_eq!(
        clear_fixture
            .repo
            .clear_active(ClearActiveAiConfigurationCommand {})
            .unwrap_err(),
        quantix_lib::ai::connections::AiConnectionError::StoreUnavailable
    );
    assert_eq!(raw_settings_row(&clear_fixture.installation), clear_prior);
}

#[test]
fn indeterminate_settings_latch_blocks_current_repository_until_restart() {
    let fixture = RepositoryFixture::new();
    let (tested, _) = ready_unactivated_openai(&fixture, "indeterminate");
    fixture.repo.fixture_set_next_settings_commit_outcome(
        FixtureSettingsCommitOutcome::IndeterminateReread,
    );
    let connection_id = tested.connection_id.clone();
    let credential_generation = tested.credential_generation;
    let activation_command = SetActiveAiConfigurationCommand {
        connection_id: connection_id.clone(),
        expected_execution_revision: tested.execution_revision,
        provider: tested.provider,
        endpoint_fingerprint: tested.endpoint_fingerprint,
        model_id: tested.tested_model_id.unwrap(),
        reasoning: tested.tested_reasoning.unwrap(),
        adapter_version: tested.adapter_version.unwrap(),
        catalogue_sha256: tested.catalogue_sha256.unwrap(),
        destination_class: tested.destination_class.unwrap(),
        confirmed_data_destination: tested.data_destination,
    };
    assert_eq!(
        fixture
            .repo
            .activate(activation_command.clone())
            .unwrap_err(),
        quantix_lib::ai::connections::AiConnectionError::StoreIndeterminate
    );
    assert_eq!(
        fixture.repo.inspect().unwrap_err(),
        quantix_lib::ai::connections::AiConnectionError::StoreIndeterminate
    );
    assert_eq!(
        fixture.repo.activate(activation_command).unwrap_err(),
        quantix_lib::ai::connections::AiConnectionError::StoreIndeterminate
    );
    assert_eq!(
        fixture
            .repo
            .clear_active(ClearActiveAiConfigurationCommand {})
            .unwrap_err(),
        quantix_lib::ai::connections::AiConnectionError::StoreIndeterminate
    );
    assert_eq!(
        fixture
            .repo
            .delete_connection(DeleteAiConnectionCommand {
                connection_id: connection_id.clone(),
                expected_execution_revision: tested.execution_revision,
                expected_credential_generation: credential_generation,
            })
            .unwrap_err(),
        quantix_lib::ai::connections::AiConnectionError::StoreIndeterminate
    );
    assert_eq!(
        fixture
            .repo
            .fixture_create_future_run(&connection_id, || {
                panic!("latched repository must not resolve a future run")
            })
            .unwrap_err(),
        quantix_lib::ai::connections::AiConnectionError::StoreIndeterminate
    );

    let restarted = AiConnectionRepository::new(
        AiConnectionVault::new(&fixture.application_home).unwrap(),
        Arc::clone(&fixture.installation),
        AiAdapterVersions::new("codex-v1", "grok-v1", "general-v1").unwrap(),
        Arc::new(|| "2026-08-24T12:34:56Z".to_owned()),
        Arc::new(no_nonterminal_reference),
    );
    assert_eq!(
        restarted.inspect().unwrap().readiness,
        quantix_lib::ai::contract::ActiveAiReadiness::Ready
    );
}

#[test]
fn reconciliation_never_reads_until_commit_failure_leaves_autocommit() {
    let fixture = RepositoryFixture::new();
    let (tested, _) = ready_unactivated_openai(&fixture, "commit-and-rollback-denied");
    let prior = raw_settings_row(&fixture.installation);
    fixture
        .installation
        .lock()
        .unwrap()
        .authorizer(Some(
            |context: rusqlite::hooks::AuthContext<'_>| match context.action {
                AuthAction::Transaction {
                    operation: TransactionOperation::Begin,
                } => Authorization::Allow,
                AuthAction::Transaction { .. } => Authorization::Deny,
                _ => Authorization::Allow,
            },
        ))
        .unwrap();

    let activation = fixture.repo.activate(SetActiveAiConfigurationCommand {
        connection_id: tested.connection_id.clone(),
        expected_execution_revision: tested.execution_revision,
        provider: tested.provider,
        endpoint_fingerprint: tested.endpoint_fingerprint,
        model_id: tested.tested_model_id.unwrap(),
        reasoning: tested.tested_reasoning.unwrap(),
        adapter_version: tested.adapter_version.unwrap(),
        catalogue_sha256: tested.catalogue_sha256.unwrap(),
        destination_class: tested.destination_class.unwrap(),
        confirmed_data_destination: tested.data_destination,
    });
    assert!(!fixture.installation.lock().unwrap().is_autocommit());
    let uncommitted_visible_row = raw_settings_row(&fixture.installation);
    assert_ne!(uncommitted_visible_row, prior);
    let uncommitted_settings: serde_json::Value =
        serde_json::from_str(&uncommitted_visible_row.0).unwrap();
    assert!(uncommitted_settings["active_ai_configuration"].is_object());
    let inspect = fixture.repo.inspect();
    let clear = fixture
        .repo
        .clear_active(ClearActiveAiConfigurationCommand {});
    let future_run = fixture
        .repo
        .fixture_create_future_run(&tested.connection_id, || {
            panic!("latched repository must not invoke the Tender closure")
        });

    remove_authorizer(&fixture.installation);
    fixture
        .installation
        .lock()
        .unwrap()
        .execute_batch("ROLLBACK")
        .unwrap();
    assert!(fixture.installation.lock().unwrap().is_autocommit());
    assert_eq!(raw_settings_row(&fixture.installation), prior);

    assert_eq!(
        activation.unwrap_err(),
        quantix_lib::ai::connections::AiConnectionError::StoreIndeterminate
    );
    assert_eq!(
        inspect.unwrap_err(),
        quantix_lib::ai::connections::AiConnectionError::StoreIndeterminate
    );
    assert_eq!(
        clear.unwrap_err(),
        quantix_lib::ai::connections::AiConnectionError::StoreIndeterminate
    );
    assert_eq!(
        future_run.unwrap_err(),
        quantix_lib::ai::connections::AiConnectionError::StoreIndeterminate
    );

    let restarted = AiConnectionRepository::new(
        AiConnectionVault::new(&fixture.application_home).unwrap(),
        Arc::clone(&fixture.installation),
        AiAdapterVersions::new("codex-v1", "grok-v1", "general-v1").unwrap(),
        Arc::new(|| "2026-08-24T12:34:56Z".to_owned()),
        Arc::new(no_nonterminal_reference),
    );
    assert_eq!(
        restarted.inspect().unwrap().readiness,
        quantix_lib::ai::contract::ActiveAiReadiness::NotConfigured
    );
}

#[test]
fn prior_active_clear_delete_and_disconnect_failures_preserve_state() {
    let fixture = RepositoryFixture::new();
    let (prior, _) = ready_active_openai(&fixture, "prior-active");
    let (replacement, replacement_evidence) = ready_unactivated_openai(&fixture, "replacement");
    fixture
        .installation
        .lock()
        .unwrap()
        .execute_batch("BEGIN IMMEDIATE")
        .unwrap();
    assert_eq!(
        fixture
            .repo
            .activate(SetActiveAiConfigurationCommand {
                connection_id: replacement.connection_id,
                expected_execution_revision: replacement.execution_revision,
                provider: replacement.provider,
                endpoint_fingerprint: replacement.endpoint_fingerprint,
                model_id: replacement.tested_model_id.unwrap(),
                reasoning: replacement.tested_reasoning.unwrap(),
                adapter_version: replacement.adapter_version.unwrap(),
                catalogue_sha256: catalogue_sha256(&replacement_evidence).unwrap(),
                destination_class: replacement.destination_class.unwrap(),
                confirmed_data_destination: replacement.data_destination,
            })
            .unwrap_err(),
        quantix_lib::ai::connections::AiConnectionError::StoreUnavailable
    );
    fixture
        .installation
        .lock()
        .unwrap()
        .execute_batch("ROLLBACK")
        .unwrap();
    assert_eq!(
        fixture
            .repo
            .inspect()
            .unwrap()
            .active_configuration
            .unwrap()
            .connection_id,
        prior.connection_id
    );

    fixture
        .installation
        .lock()
        .unwrap()
        .execute_batch("BEGIN IMMEDIATE")
        .unwrap();
    assert_eq!(
        fixture
            .repo
            .clear_active(ClearActiveAiConfigurationCommand {})
            .unwrap_err(),
        quantix_lib::ai::connections::AiConnectionError::StoreUnavailable
    );
    fixture
        .installation
        .lock()
        .unwrap()
        .execute_batch("ROLLBACK")
        .unwrap();
    assert!(fixture
        .repo
        .inspect()
        .unwrap()
        .active_configuration
        .is_some());

    let delete_fixture = RepositoryFixture::new();
    let deleting = delete_fixture
        .repo
        .create_connection(openai_key_command("delete-rollback"))
        .unwrap();
    delete_fixture
        .installation
        .lock()
        .unwrap()
        .execute_batch("BEGIN IMMEDIATE")
        .unwrap();
    assert_eq!(
        delete_fixture
            .repo
            .delete_connection(DeleteAiConnectionCommand {
                connection_id: deleting.connection_id.clone(),
                expected_execution_revision: deleting.execution_revision,
                expected_credential_generation: deleting.credential_generation,
            })
            .unwrap_err(),
        quantix_lib::ai::connections::AiConnectionError::StoreUnavailable
    );
    delete_fixture
        .installation
        .lock()
        .unwrap()
        .execute_batch("ROLLBACK")
        .unwrap();
    assert_eq!(delete_fixture.repo.inspect().unwrap().connections.len(), 1);

    AiConnectionVault::new(&delete_fixture.application_home)
        .unwrap()
        .fixture_set_connection_counters(&deleting.connection_id, 1, u64::MAX)
        .unwrap();
    assert_eq!(
        delete_fixture
            .repo
            .disconnect(DisconnectAiConnectionCommand {
                connection_id: deleting.connection_id.clone(),
                expected_execution_revision: 1,
                expected_credential_generation: u64::MAX,
            })
            .unwrap_err(),
        quantix_lib::ai::connections::AiConnectionError::RevisionOverflow
    );
    assert!(delete_fixture.repo.inspect().unwrap().connections[0].secret_configured);
}

#[test]
fn settings_writes_preserve_preferences_and_return_exact_snapshots() {
    let fixture = RepositoryFixture::new();
    fixture
        .installation
        .lock()
        .unwrap()
        .execute(
            "UPDATE application_settings
             SET settings_json = '{\"general_preferences\":{\"appearance\":\"dark\",\"reduced_motion\":true,\"larger_text\":true,\"notify_when_attention_needed\":true},\"active_ai_configuration\":null}'
             WHERE singleton = 1",
            [],
        )
        .unwrap();
    ready_active_openai(&fixture, "preferences");
    let after_activation: serde_json::Value = serde_json::from_str(
        &fixture
            .installation
            .lock()
            .unwrap()
            .query_row(
                "SELECT settings_json FROM application_settings WHERE singleton = 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        after_activation["general_preferences"]["appearance"],
        "dark"
    );
    assert_eq!(
        after_activation["general_preferences"]["reduced_motion"],
        true
    );
    fixture
        .repo
        .clear_active(ClearActiveAiConfigurationCommand {})
        .unwrap();
    let after_clear: serde_json::Value = serde_json::from_str(
        &fixture
            .installation
            .lock()
            .unwrap()
            .query_row(
                "SELECT settings_json FROM application_settings WHERE singleton = 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        after_clear["general_preferences"],
        after_activation["general_preferences"]
    );

    let response_fixture = RepositoryFixture::new();
    let created = response_fixture
        .repo
        .create_connection(openai_key_command("exact-response"))
        .unwrap();
    let competing_repo = AiConnectionRepository::new(
        AiConnectionVault::new(&response_fixture.application_home).unwrap(),
        Arc::clone(&response_fixture.installation),
        AiAdapterVersions::new("codex-v1", "grok-v1", "general-v1").unwrap(),
        Arc::new(|| "2026-08-24T12:34:56Z".to_owned()),
        Arc::new(no_nonterminal_reference),
    );
    competing_repo
        .replace_connection_configuration(UpdateAiConnectionCommand {
            connection_id: created.connection_id.clone(),
            expected_execution_revision: created.execution_revision,
            expected_credential_generation: created.credential_generation,
            display_name: "Competing mutation".to_owned(),
            configuration: Some(AiConnectionConfiguration::DirectProviderKey {
                provider: AiProviderKind::Anthropic,
            }),
            replacement_credential: None,
        })
        .unwrap();
    assert_eq!(created.provider, AiProviderKind::OpenAi);
    assert_eq!(
        response_fixture.repo.inspect().unwrap().connections[0].provider,
        AiProviderKind::Anthropic
    );

    let clear_fixture = RepositoryFixture::new();
    ready_active_openai(&clear_fixture, "clear-snapshot");
    deny_next_select_after_settings_update(&clear_fixture.installation);
    let cleared = clear_fixture
        .repo
        .clear_active(ClearActiveAiConfigurationCommand {})
        .unwrap();
    assert!(cleared.active_configuration.is_none());
    assert_eq!(
        cleared.readiness,
        quantix_lib::ai::contract::ActiveAiReadiness::NotConfigured
    );
    assert_eq!(
        clear_fixture.repo.inspect().unwrap_err(),
        quantix_lib::ai::connections::AiConnectionError::StoreUnavailable
    );
    remove_authorizer(&clear_fixture.installation);
    let stored: serde_json::Value = serde_json::from_str(
        &clear_fixture
            .installation
            .lock()
            .unwrap()
            .query_row(
                "SELECT settings_json FROM application_settings WHERE singleton = 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
    )
    .unwrap();
    assert!(stored["active_ai_configuration"].is_null());
    assert_eq!(stored["general_preferences"]["appearance"], "system");
}

#[test]
fn views_sort_by_normalized_name_then_connection_id() {
    let fixture = RepositoryFixture::new();
    for name in ["é", "Alpha", "e\u{301}"] {
        fixture
            .repo
            .create_connection(CreateAiConnectionCommand {
                display_name: name.to_owned(),
                configuration: AiConnectionConfiguration::DirectProviderKey {
                    provider: AiProviderKind::OpenAi,
                },
                credential: AiCredentialInput::ApiKey {
                    api_key: secret(format!("secret-{name}")),
                    custom_header_values: Vec::new(),
                    custom_query_values: Vec::new(),
                },
            })
            .unwrap();
    }
    let connections = fixture.repo.inspect().unwrap().connections;
    assert_eq!(connections[0].display_name, "Alpha");
    assert_eq!(connections[1].display_name, "é");
    assert_eq!(connections[2].display_name, "é");
    assert!(connections[1].connection_id < connections[2].connection_id);
}

#[test]
fn public_results_errors_and_settings_have_no_secret_or_default_surface() {
    let fixture = RepositoryFixture::new();
    fixture
        .repo
        .create_connection(openai_key_command("sentinel-public-secret"))
        .unwrap();
    let view_json = serde_json::to_string(&fixture.repo.inspect().unwrap()).unwrap();
    let error_json =
        serde_json::to_string(&quantix_lib::ai::connections::AiConnectionError::InvalidCommand)
            .unwrap();
    let settings_json: String = fixture
        .installation
        .lock()
        .unwrap()
        .query_row(
            "SELECT settings_json FROM application_settings WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    for rendered in [view_json, error_json, settings_json] {
        for forbidden in [
            "sentinel-public-secret",
            "api_key",
            "access_token",
            "refresh_token",
            "id_token",
            "header_value",
            "query_value",
            "is_default",
            "recommended",
            "provider_default",
            "fallback",
        ] {
            assert!(
                !rendered.contains(forbidden),
                "forbidden surface: {forbidden}"
            );
        }
    }
}

#[test]
fn public_view_round_trip_reaches_every_final_command() {
    let fixture = RepositoryFixture::new();
    ready_active_openai(&fixture, "public-command");
    let serialized = serde_json::to_string(&fixture.repo.inspect().unwrap()).unwrap();
    let round_trip: quantix_lib::ai::connections::ApplicationAiSettingsView =
        serde_json::from_str(&serialized).unwrap();
    let connection = &round_trip.connections[0];
    assert_eq!(connection.credential_generation, 1);
    assert_eq!(connection.data_destination, "https://api.openai.com");
    assert_eq!(
        connection.endpoint_fingerprint,
        hex_sha256("https://api.openai.com")
    );
    assert!(connection.adapter_version.is_some());
    assert!(connection.catalogue_sha256.is_some());
    assert!(connection.tested_model_id.is_some());
    assert!(connection.tested_reasoning.is_some());

    let rename: UpdateAiConnectionCommand = serde_json::from_value(serde_json::json!({
        "connection_id": connection.connection_id,
        "expected_execution_revision": connection.execution_revision,
        "expected_credential_generation": connection.credential_generation,
        "display_name": "Renamed from public view",
        "configuration": null,
        "replacement_credential": null
    }))
    .unwrap();
    let update: UpdateAiConnectionCommand = serde_json::from_value(serde_json::json!({
        "connection_id": connection.connection_id,
        "expected_execution_revision": connection.execution_revision,
        "expected_credential_generation": connection.credential_generation,
        "display_name": connection.display_name,
        "configuration": connection.configuration,
        "replacement_credential": {
            "kind": "api_key",
            "api_key": "newly-typed-secret",
            "custom_header_values": [],
            "custom_query_values": []
        }
    }))
    .unwrap();
    let enabled: SetAiConnectionEnabledCommand = serde_json::from_value(serde_json::json!({
        "connection_id": connection.connection_id,
        "expected_execution_revision": connection.execution_revision,
        "expected_credential_generation": connection.credential_generation,
        "enabled": false
    }))
    .unwrap();
    let disconnect: DisconnectAiConnectionCommand = serde_json::from_value(serde_json::json!({
        "connection_id": connection.connection_id,
        "expected_execution_revision": connection.execution_revision,
        "expected_credential_generation": connection.credential_generation
    }))
    .unwrap();
    let delete: DeleteAiConnectionCommand = serde_json::from_value(serde_json::json!({
        "connection_id": connection.connection_id,
        "expected_execution_revision": connection.execution_revision,
        "expected_credential_generation": connection.credential_generation
    }))
    .unwrap();
    let activate: SetActiveAiConfigurationCommand = serde_json::from_value(serde_json::json!({
        "connection_id": connection.connection_id,
        "expected_execution_revision": connection.execution_revision,
        "provider": connection.provider,
        "endpoint_fingerprint": connection.endpoint_fingerprint,
        "model_id": connection.tested_model_id,
        "reasoning": connection.tested_reasoning,
        "adapter_version": connection.adapter_version,
        "catalogue_sha256": connection.catalogue_sha256,
        "destination_class": connection.destination_class,
        "confirmed_data_destination": connection.data_destination
    }))
    .unwrap();
    drop((rename, update, enabled, disconnect, delete, activate));
}

#[test]
fn vault_only_mutations_ignore_unavailable_installation_projection() {
    let fixture = RepositoryFixture::new();
    fixture
        .installation
        .lock()
        .unwrap()
        .execute(
            "UPDATE application_settings SET settings_json = '{}' WHERE singleton = 1",
            [],
        )
        .unwrap();

    let created = fixture
        .repo
        .create_connection(openai_key_command("projection-secret"))
        .unwrap();
    assert_eq!(created.execution_revision, 1);
    assert_eq!(created.credential_generation, 1);
    let renamed = fixture
        .repo
        .rename_connection(UpdateAiConnectionCommand {
            connection_id: created.connection_id.clone(),
            expected_execution_revision: created.execution_revision,
            expected_credential_generation: created.credential_generation,
            display_name: "Projection-safe rename".to_owned(),
            configuration: None,
            replacement_credential: None,
        })
        .unwrap();
    assert_eq!(renamed.display_name, "Projection-safe rename");

    fixture
        .installation
        .lock()
        .unwrap()
        .execute(
            "UPDATE application_settings
             SET settings_json = '{\"general_preferences\":{\"appearance\":\"system\",\"reduced_motion\":false,\"larger_text\":false,\"notify_when_attention_needed\":false},\"active_ai_configuration\":null}'
             WHERE singleton = 1",
            [],
        )
        .unwrap();
    assert_eq!(fixture.repo.inspect().unwrap().connections.len(), 1);
}

#[test]
fn activation_returns_the_committed_in_memory_snapshot_without_reread() {
    let fixture = RepositoryFixture::new();
    let connection = fixture
        .repo
        .create_connection(openai_key_command("activation-projection"))
        .unwrap();
    let evidence = current_probe(
        &connection.connection_id,
        connection.execution_revision,
        "gpt-projection",
        "low",
    );
    let tested = fixture.repo.record_probe(evidence.clone()).unwrap();
    deny_next_select_after_settings_update(&fixture.installation);

    let result = fixture
        .repo
        .activate(SetActiveAiConfigurationCommand {
            connection_id: tested.connection_id,
            expected_execution_revision: tested.execution_revision,
            provider: tested.provider,
            endpoint_fingerprint: tested.endpoint_fingerprint,
            model_id: tested.tested_model_id.unwrap(),
            reasoning: tested.tested_reasoning.unwrap(),
            adapter_version: tested.adapter_version.unwrap(),
            catalogue_sha256: tested.catalogue_sha256.unwrap(),
            destination_class: tested.destination_class.unwrap(),
            confirmed_data_destination: tested.data_destination,
        })
        .unwrap();
    assert!(result.active_configuration.is_some());
    assert_eq!(
        result.readiness,
        quantix_lib::ai::contract::ActiveAiReadiness::Ready
    );
    assert_eq!(
        fixture.repo.inspect().unwrap_err(),
        quantix_lib::ai::connections::AiConnectionError::StoreUnavailable
    );
    remove_authorizer(&fixture.installation);
    let stored: serde_json::Value = serde_json::from_str(
        &fixture
            .installation
            .lock()
            .unwrap()
            .query_row(
                "SELECT settings_json FROM application_settings WHERE singleton = 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        stored["active_ai_configuration"],
        serde_json::to_value(result.active_configuration.unwrap()).unwrap()
    );
    assert_eq!(stored["general_preferences"]["appearance"], "system");
    assert_eq!(
        fixture.repo.inspect().unwrap().readiness,
        quantix_lib::ai::contract::ActiveAiReadiness::Ready
    );
}

#[test]
fn material_change_matrix_advances_only_the_required_counters() {
    assert_material_update(
        openai_key_command("provider-secret"),
        AiProviderKind::OpenAi,
        "https://api.openai.com",
        AiConnectionConfiguration::DirectProviderKey {
            provider: AiProviderKind::Anthropic,
        },
        None,
        0,
    );

    let base = CompatibleEndpointConfiguration::parse(
        "https://material.example/v1",
        CompatibleCredentialKind::Bearer,
        vec!["x-tenant".to_owned()],
        vec!["revision".to_owned()],
        "material-model",
    )
    .unwrap();
    assert_material_update(
        compatible_create_command("Endpoint", base.clone(), "header-A", "query-A"),
        AiProviderKind::OpenAiCompatible,
        &base.base_url,
        AiConnectionConfiguration::OpenAiCompatible {
            provider: AiProviderKind::OpenAiCompatible,
            endpoint: CompatibleEndpointConfiguration::parse(
                "https://changed.example/v1",
                CompatibleCredentialKind::Bearer,
                vec!["x-tenant".to_owned()],
                vec!["revision".to_owned()],
                "material-model",
            )
            .unwrap(),
        },
        None,
        0,
    );
    assert_material_update(
        compatible_create_command("Placement", base.clone(), "header-A", "query-A"),
        AiProviderKind::OpenAiCompatible,
        &base.base_url,
        AiConnectionConfiguration::OpenAiCompatible {
            provider: AiProviderKind::OpenAiCompatible,
            endpoint: CompatibleEndpointConfiguration::parse(
                &base.base_url,
                CompatibleCredentialKind::ApiKeyHeader {
                    name: "x-api-key".to_owned(),
                },
                vec!["x-tenant".to_owned()],
                vec!["revision".to_owned()],
                "material-model",
            )
            .unwrap(),
        },
        None,
        0,
    );
    assert_material_update(
        compatible_create_command("Header name", base.clone(), "header-A", "query-A"),
        AiProviderKind::OpenAiCompatible,
        &base.base_url,
        AiConnectionConfiguration::OpenAiCompatible {
            provider: AiProviderKind::OpenAiCompatible,
            endpoint: CompatibleEndpointConfiguration::parse(
                &base.base_url,
                CompatibleCredentialKind::Bearer,
                vec!["x-workspace".to_owned()],
                vec!["revision".to_owned()],
                "material-model",
            )
            .unwrap(),
        },
        Some(AiCredentialInput::ApiKey {
            api_key: secret("compatible-key"),
            custom_header_values: vec![SecretNameValueInput {
                name: "x-workspace".to_owned(),
                value: secret("header-A"),
            }],
            custom_query_values: vec![SecretNameValueInput {
                name: "revision".to_owned(),
                value: secret("query-A"),
            }],
        }),
        1,
    );
    assert_material_update(
        compatible_create_command("Query name", base.clone(), "header-A", "query-A"),
        AiProviderKind::OpenAiCompatible,
        &base.base_url,
        AiConnectionConfiguration::OpenAiCompatible {
            provider: AiProviderKind::OpenAiCompatible,
            endpoint: CompatibleEndpointConfiguration::parse(
                &base.base_url,
                CompatibleCredentialKind::Bearer,
                vec!["x-tenant".to_owned()],
                vec!["version".to_owned()],
                "material-model",
            )
            .unwrap(),
        },
        Some(AiCredentialInput::ApiKey {
            api_key: secret("compatible-key"),
            custom_header_values: vec![SecretNameValueInput {
                name: "x-tenant".to_owned(),
                value: secret("header-A"),
            }],
            custom_query_values: vec![SecretNameValueInput {
                name: "version".to_owned(),
                value: secret("query-A"),
            }],
        }),
        1,
    );
    assert_material_update(
        compatible_create_command("Header value", base.clone(), "header-A", "query-A"),
        AiProviderKind::OpenAiCompatible,
        &base.base_url,
        AiConnectionConfiguration::OpenAiCompatible {
            provider: AiProviderKind::OpenAiCompatible,
            endpoint: base.clone(),
        },
        Some(AiCredentialInput::ApiKey {
            api_key: secret("compatible-key"),
            custom_header_values: vec![SecretNameValueInput {
                name: "x-tenant".to_owned(),
                value: secret("header-B"),
            }],
            custom_query_values: vec![SecretNameValueInput {
                name: "revision".to_owned(),
                value: secret("query-A"),
            }],
        }),
        1,
    );
    assert_material_update(
        compatible_create_command("Query value", base.clone(), "header-A", "query-A"),
        AiProviderKind::OpenAiCompatible,
        &base.base_url,
        AiConnectionConfiguration::OpenAiCompatible {
            provider: AiProviderKind::OpenAiCompatible,
            endpoint: base.clone(),
        },
        Some(AiCredentialInput::ApiKey {
            api_key: secret("compatible-key"),
            custom_header_values: vec![SecretNameValueInput {
                name: "x-tenant".to_owned(),
                value: secret("header-A"),
            }],
            custom_query_values: vec![SecretNameValueInput {
                name: "revision".to_owned(),
                value: secret("query-B"),
            }],
        }),
        1,
    );
}

#[test]
fn activation_requires_enabled_credentials_and_explicit_unsupported_reasoning() {
    let fixture = RepositoryFixture::new();
    let connection = fixture
        .repo
        .create_connection(openai_key_command("sk-unsupported"))
        .unwrap();
    let mut evidence = current_probe(
        &connection.connection_id,
        connection.execution_revision,
        "gpt-unsupported",
        "unused",
    );
    evidence.models[0].capabilities.reasoning = CapabilitySupport::Unsupported;
    evidence.models[0].reasoning_options.clear();
    evidence.tested_reasoning = AiReasoningSelection::Unsupported;
    fixture.repo.record_probe(evidence.clone()).unwrap();
    let state = fixture
        .repo
        .fixture_connection_state(&connection.connection_id)
        .unwrap();
    let command = |reasoning| SetActiveAiConfigurationCommand {
        connection_id: connection.connection_id.clone(),
        expected_execution_revision: state.execution_revision,
        provider: AiProviderKind::OpenAi,
        endpoint_fingerprint: hex_sha256("https://api.openai.com"),
        model_id: "gpt-unsupported".to_owned(),
        reasoning,
        adapter_version: "general-v1".to_owned(),
        catalogue_sha256: catalogue_sha256(&evidence).unwrap(),
        destination_class: AiNetworkDestinationClass::Public,
        confirmed_data_destination: "https://api.openai.com".to_owned(),
    };
    fixture
        .repo
        .set_enabled(SetAiConnectionEnabledCommand {
            connection_id: connection.connection_id.clone(),
            expected_execution_revision: state.execution_revision,
            expected_credential_generation: state.credential_generation,
            enabled: false,
        })
        .unwrap();
    assert_eq!(
        fixture
            .repo
            .activate(command(AiReasoningSelection::Unsupported))
            .unwrap_err(),
        quantix_lib::ai::connections::AiConnectionError::Disabled
    );
    fixture
        .repo
        .set_enabled(SetAiConnectionEnabledCommand {
            connection_id: connection.connection_id.clone(),
            expected_execution_revision: state.execution_revision,
            expected_credential_generation: state.credential_generation,
            enabled: true,
        })
        .unwrap();
    assert_eq!(
        fixture
            .repo
            .activate(command(AiReasoningSelection::Effort {
                id: "low".to_owned(),
            }))
            .unwrap_err(),
        quantix_lib::ai::connections::AiConnectionError::CapabilityChanged
    );
    assert_eq!(
        fixture
            .repo
            .activate(command(AiReasoningSelection::Unsupported))
            .unwrap()
            .readiness,
        quantix_lib::ai::contract::ActiveAiReadiness::Ready
    );
    fixture
        .repo
        .disconnect(DisconnectAiConnectionCommand {
            connection_id: connection.connection_id.clone(),
            expected_execution_revision: state.execution_revision,
            expected_credential_generation: state.credential_generation,
        })
        .unwrap();
    assert_eq!(
        fixture
            .repo
            .activate(command(AiReasoningSelection::Unsupported))
            .unwrap_err(),
        quantix_lib::ai::connections::AiConnectionError::AuthenticationRequired
    );
}

#[test]
fn invalid_identifiers_and_zero_cas_fail_before_reference_checks() {
    let fixture = RepositoryFixture::new();
    fixture
        .tender
        .lock()
        .unwrap()
        .execute(
            "INSERT INTO future_nonterminal_references (connection_id, terminal)
             VALUES ('not-a-connection-id', 0)",
            [],
        )
        .unwrap();
    assert_eq!(
        fixture
            .repo
            .delete_connection(DeleteAiConnectionCommand {
                connection_id: "not-a-connection-id".to_owned(),
                expected_execution_revision: 1,
                expected_credential_generation: 1,
            })
            .unwrap_err(),
        quantix_lib::ai::connections::AiConnectionError::InvalidCommand
    );

    let connection = fixture
        .repo
        .create_connection(openai_key_command("sk-cas-validation"))
        .unwrap();
    assert_eq!(
        fixture
            .repo
            .set_enabled(SetAiConnectionEnabledCommand {
                connection_id: connection.connection_id,
                expected_execution_revision: 0,
                expected_credential_generation: 1,
                enabled: false,
            })
            .unwrap_err(),
        quantix_lib::ai::connections::AiConnectionError::InvalidCommand
    );
}

#[test]
fn adapter_activation_and_account_expiry_inputs_are_strictly_bounded() {
    assert!(AiAdapterVersions::new("", "grok-v1", "general-v1").is_err());
    assert!(AiAdapterVersions::new("codex-v1", "grok-v1", "g".repeat(129)).is_err());

    let base = serde_json::json!({
        "connection_id":"0123456789abcdef0123456789abcdef",
        "expected_execution_revision":1,
        "provider":"open_ai",
        "endpoint_fingerprint":"a".repeat(64),
        "model_id":"model",
        "reasoning":{"kind":"unsupported"},
        "adapter_version":"general-v1",
        "catalogue_sha256":"b".repeat(64),
        "destination_class":"public",
        "confirmed_data_destination":"https://api.openai.com"
    });
    assert!(serde_json::from_value::<SetActiveAiConfigurationCommand>(base.clone()).is_ok());
    for (field, bad) in [
        ("endpoint_fingerprint", serde_json::json!("a".repeat(63))),
        ("model_id", serde_json::json!("m".repeat(257))),
        ("adapter_version", serde_json::json!("v".repeat(129))),
        ("catalogue_sha256", serde_json::json!("z".repeat(64))),
        (
            "confirmed_data_destination",
            serde_json::json!("d".repeat(2_049)),
        ),
    ] {
        let mut invalid = base.clone();
        invalid[field] = bad;
        assert!(serde_json::from_value::<SetActiveAiConfigurationCommand>(invalid).is_err());
    }

    let fixture = RepositoryFixture::new();
    let mut account = codex_account_command("access", "refresh");
    let AiCredentialInput::Account { expires_at, .. } = &mut account.credential else {
        unreachable!()
    };
    *expires_at = "t".repeat(129);
    assert_eq!(
        fixture.repo.create_connection(account).unwrap_err(),
        quantix_lib::ai::connections::AiConnectionError::InvalidCommand
    );
}

#[test]
fn exact_bound_compatible_destination_activates_and_bad_clock_cannot_write() {
    let fixture = RepositoryFixture::new();
    let prefix = "https://long-destination.example/";
    let destination = format!("{prefix}{}", "p".repeat(2_048 - prefix.len()));
    assert_eq!(destination.len(), 2_048);
    let endpoint = CompatibleEndpointConfiguration::parse(
        &destination,
        CompatibleCredentialKind::Bearer,
        Vec::new(),
        Vec::new(),
        "long-model",
    )
    .unwrap();
    let connection = fixture
        .repo
        .create_connection(CreateAiConnectionCommand {
            display_name: "Long destination".to_owned(),
            configuration: AiConnectionConfiguration::OpenAiCompatible {
                provider: AiProviderKind::OpenAiCompatible,
                endpoint: endpoint.clone(),
            },
            credential: AiCredentialInput::ApiKey {
                api_key: secret("long-secret"),
                custom_header_values: Vec::new(),
                custom_query_values: Vec::new(),
            },
        })
        .unwrap();
    let mut evidence = current_probe(
        &connection.connection_id,
        connection.execution_revision,
        "long-model",
        "low",
    );
    evidence.provider = AiProviderKind::OpenAiCompatible;
    evidence.endpoint_fingerprint = hex_sha256(&endpoint.base_url);
    let tested = fixture.repo.record_probe(evidence).unwrap();
    let view = fixture
        .repo
        .activate(SetActiveAiConfigurationCommand {
            connection_id: tested.connection_id,
            expected_execution_revision: tested.execution_revision,
            provider: tested.provider,
            endpoint_fingerprint: tested.endpoint_fingerprint,
            model_id: tested.tested_model_id.unwrap(),
            reasoning: tested.tested_reasoning.unwrap(),
            adapter_version: tested.adapter_version.unwrap(),
            catalogue_sha256: tested.catalogue_sha256.unwrap(),
            destination_class: tested.destination_class.unwrap(),
            confirmed_data_destination: tested.data_destination,
        })
        .unwrap();
    assert_eq!(
        view.active_configuration.unwrap().data_destination.len(),
        2_048
    );

    let invalid_clock_fixture = RepositoryFixture::new();
    let (invalid_connection, invalid_evidence) =
        ready_unactivated_openai(&invalid_clock_fixture, "clock");
    let invalid_clock_repo = AiConnectionRepository::new(
        AiConnectionVault::new(&invalid_clock_fixture.application_home).unwrap(),
        Arc::clone(&invalid_clock_fixture.installation),
        AiAdapterVersions::new("codex-v1", "grok-v1", "general-v1").unwrap(),
        Arc::new(String::new),
        Arc::new(no_nonterminal_reference),
    );
    assert_eq!(
        invalid_clock_repo
            .activate(SetActiveAiConfigurationCommand {
                connection_id: invalid_connection.connection_id,
                expected_execution_revision: invalid_connection.execution_revision,
                provider: invalid_connection.provider,
                endpoint_fingerprint: invalid_connection.endpoint_fingerprint,
                model_id: invalid_connection.tested_model_id.unwrap(),
                reasoning: invalid_connection.tested_reasoning.unwrap(),
                adapter_version: invalid_connection.adapter_version.unwrap(),
                catalogue_sha256: catalogue_sha256(&invalid_evidence).unwrap(),
                destination_class: invalid_connection.destination_class.unwrap(),
                confirmed_data_destination: invalid_connection.data_destination,
            })
            .unwrap_err(),
        quantix_lib::ai::connections::AiConnectionError::InvalidCommand
    );
    assert!(invalid_clock_fixture
        .repo
        .inspect()
        .unwrap()
        .active_configuration
        .is_none());
}

#[test]
fn secret_owners_drop_on_partial_deserialization_and_staged_transfer_failure() {
    fixture_reset_secret_drop_observations();
    let partial = r#"{
      "display_name":"Partial secret",
      "configuration":{"method":"direct_provider_key","provider":"open_ai"},
      "credential":{
        "kind":"api_key",
        "api_key":"partial-secret",
        "custom_header_values":[],
        "custom_query_values":[]
      },
      "unexpected":true
    }"#;
    assert!(serde_json::from_str::<CreateAiConnectionCommand>(partial).is_err());
    assert!(fixture_secret_drop_observations() >= 1);

    fixture_reset_secret_drop_observations();
    let fixture = RepositoryFixture::new();
    assert_eq!(
        fixture
            .repo
            .fixture_reject_after_secret_transfer(openai_key_command("staged-secret"))
            .unwrap_err(),
        quantix_lib::ai::connections::AiConnectionError::InvalidCommand
    );
    assert!(fixture_secret_drop_observations() >= 1);
    assert!(fixture.repo.inspect().unwrap().connections.is_empty());
}

fn compatible_create_command(
    display_name: &str,
    endpoint: CompatibleEndpointConfiguration,
    header_value: &str,
    query_value: &str,
) -> CreateAiConnectionCommand {
    CreateAiConnectionCommand {
        display_name: display_name.to_owned(),
        configuration: AiConnectionConfiguration::OpenAiCompatible {
            provider: AiProviderKind::OpenAiCompatible,
            endpoint,
        },
        credential: AiCredentialInput::ApiKey {
            api_key: secret("compatible-key"),
            custom_header_values: vec![SecretNameValueInput {
                name: "x-tenant".to_owned(),
                value: secret(header_value),
            }],
            custom_query_values: vec![SecretNameValueInput {
                name: "revision".to_owned(),
                value: secret(query_value),
            }],
        },
    }
}

fn assert_material_update(
    create: CreateAiConnectionCommand,
    original_provider: AiProviderKind,
    original_destination: &str,
    new_configuration: AiConnectionConfiguration,
    replacement_credential: Option<AiCredentialInput>,
    expected_credential_delta: u64,
) {
    let fixture = RepositoryFixture::new();
    let connection = fixture.repo.create_connection(create).unwrap();
    let mut evidence = current_probe(
        &connection.connection_id,
        connection.execution_revision,
        "material-model",
        "low",
    );
    evidence.provider = original_provider;
    evidence.endpoint_fingerprint = hex_sha256(original_destination);
    fixture.repo.record_probe(evidence).unwrap();
    let before = fixture
        .repo
        .fixture_connection_state(&connection.connection_id)
        .unwrap();
    fixture
        .repo
        .replace_connection_configuration(UpdateAiConnectionCommand {
            connection_id: connection.connection_id.clone(),
            expected_execution_revision: before.execution_revision,
            expected_credential_generation: before.credential_generation,
            display_name: "Material update".to_owned(),
            configuration: Some(new_configuration),
            replacement_credential,
        })
        .unwrap();
    let after = fixture
        .repo
        .fixture_connection_state(&connection.connection_id)
        .unwrap();
    assert_eq!(after.execution_revision, before.execution_revision + 1);
    assert_eq!(
        after.credential_generation,
        before.credential_generation + expected_credential_delta
    );
    assert!(!after.has_probe_evidence);
}
