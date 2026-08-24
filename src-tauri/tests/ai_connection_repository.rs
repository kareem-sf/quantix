#![cfg(windows)]

use std::{
    io,
    path::{Path, PathBuf},
    sync::{Arc, Barrier, Mutex},
};

use quantix_lib::{
    ai::{
        connections::{
            AiAdapterVersions, AiConnectionRepository, AiCredentialInput,
            ClearActiveAiConfigurationCommand, CreateAiConnectionCommand,
            DeleteAiConnectionCommand, DisconnectAiConnectionCommand,
            SameAccountTokenRefreshCommand, SecretNameValueInput, SetActiveAiConfigurationCommand,
            SetAiConnectionEnabledCommand, UpdateAiConnectionCommand,
        },
        contract::{
            catalogue_sha256, AiCapabilitySet, AiConnectionConfiguration, AiConnectionId,
            AiConnectionRevision, AiModelView, AiProbeEvidence, AiProviderKind, AiReasoningOption,
            AiReasoningSelection, AiStructuredOutputMode, CapabilitySupport,
            CompatibleCredentialKind, CompatibleEndpointConfiguration,
        },
        vault::AiConnectionVault,
    },
    ensure_quantix_setup, QuantixHost, SetupPlatform, SetupState, StoragePermissions,
    MINIMUM_SETUP_FREE_SPACE_BYTES,
};
use rusqlite::{Connection, OptionalExtension, Transaction};
use sha2::{Digest, Sha256};

struct RepositoryFixture {
    _root: tempfile::TempDir,
    application_home: PathBuf,
    repo: Arc<AiConnectionRepository>,
    installation: Arc<Mutex<Connection>>,
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
        let application_home = root.path().join(".quantix");
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
                 CREATE TABLE future_nonterminal_references (
                   connection_id TEXT NOT NULL,
                   terminal INTEGER NOT NULL
                 );
                 INSERT INTO application_settings (singleton, settings_json, updated_at)
                 VALUES (
                   1,
                   '{\"general_preferences\":{\"appearance\":\"system\",\"reduced_motion\":false,\"larger_text\":false,\"notify_when_attention_needed\":false},\"active_ai_configuration\":null}',
                   '2026-08-24T12:00:00Z'
                 );",
            )
            .unwrap();
        let repo = Arc::new(AiConnectionRepository::new(
            vault,
            Arc::clone(&installation),
            AiAdapterVersions {
                codex: "codex-v1".to_owned(),
                general: "general-v1".to_owned(),
            },
            Arc::new(|| "2026-08-24T12:34:56Z".to_owned()),
            Arc::new(future_nonterminal_reference_check),
        ));
        Self {
            _root: root,
            application_home,
            repo,
            installation,
        }
    }
}

fn future_nonterminal_reference_check(
    transaction: &Transaction<'_>,
    connection_id: &str,
) -> Result<bool, quantix_lib::ai::connections::AiConnectionError> {
    transaction
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
}

fn openai_key_command(secret: &str) -> CreateAiConnectionCommand {
    CreateAiConnectionCommand {
        display_name: "Engineering OpenAI".to_owned(),
        configuration: AiConnectionConfiguration::DirectProviderKey {
            provider: AiProviderKind::OpenAi,
        },
        credential: AiCredentialInput::ApiKey {
            api_key: secret.to_owned(),
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
            access_token: access_token.to_owned(),
            refresh_token: Some(refresh_token.to_owned()),
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
                    access_token: "wrong-access".to_owned(),
                    refresh_token: Some("wrong-refresh".to_owned()),
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
                access_token: "access-B".to_owned(),
                refresh_token: Some("refresh-B".to_owned()),
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
            configuration: AiConnectionConfiguration::AccountLogin {
                provider: AiProviderKind::Codex,
                account_id: "account-456".to_owned(),
            },
            replacement_credential: Some(AiCredentialInput::Account {
                access_token: "reauth-access".to_owned(),
                refresh_token: Some("reauth-refresh".to_owned()),
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
            configuration: AiConnectionConfiguration::DirectProviderKey {
                provider: AiProviderKind::OpenAi,
            },
            replacement_credential: Some(AiCredentialInput::ApiKey {
                api_key: "sk-after".to_owned(),
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
            configuration: AiConnectionConfiguration::DirectProviderKey {
                provider: AiProviderKind::OpenAi,
            },
            replacement_credential: Some(AiCredentialInput::ApiKey {
                api_key: "sk-replaced".to_owned(),
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
            configuration: AiConnectionConfiguration::DirectProviderKey {
                provider: AiProviderKind::OpenAi,
            },
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
            .delete_connection(delete_command())
            .unwrap_err(),
        quantix_lib::ai::connections::AiConnectionError::ActiveConnection
    );

    fixture
        .repo
        .clear_active(ClearActiveAiConfigurationCommand {})
        .unwrap();
    fixture
        .installation
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
        .installation
        .lock()
        .unwrap()
        .execute("UPDATE future_nonterminal_references SET terminal = 1", [])
        .unwrap();
    fixture.repo.delete_connection(delete_command()).unwrap();
    assert!(fixture.repo.inspect().unwrap().connections.is_empty());
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
        AiAdapterVersions {
            codex: "codex-v1".to_owned(),
            general: "general-v2".to_owned(),
        },
        Arc::new(|| "2026-08-24T12:34:56Z".to_owned()),
        Arc::new(future_nonterminal_reference_check),
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
    fixture.repo.record_probe(evidence.clone()).unwrap();
    fixture
        .repo
        .activate(SetActiveAiConfigurationCommand {
            connection_id: connection.connection_id.clone(),
            expected_execution_revision: connection.execution_revision,
            provider: AiProviderKind::OpenAi,
            endpoint_fingerprint: hex_sha256("https://api.openai.com"),
            model_id: format!("gpt-{suffix}"),
            reasoning: AiReasoningSelection::Effort {
                id: "low".to_owned(),
            },
            adapter_version: "general-v1".to_owned(),
            catalogue_sha256: catalogue_sha256(&evidence).unwrap(),
            confirmed_data_destination: "https://api.openai.com".to_owned(),
        })
        .unwrap();
    (connection, evidence)
}

#[test]
fn all_seven_routes_are_saved_without_secret_projection() {
    let fixture = RepositoryFixture::new();
    let mut commands = vec![codex_account_command("route-access", "route-refresh")];
    for (name, provider, secret) in [
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
                api_key: secret.to_owned(),
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
                api_key: "compatible-route-secret".to_owned(),
                custom_header_values: vec![SecretNameValueInput {
                    name: if name.starts_with("OpenAI") {
                        "x-tenant".to_owned()
                    } else {
                        "x-workspace".to_owned()
                    },
                    value: "header-route-secret".to_owned(),
                }],
                custom_query_values: vec![SecretNameValueInput {
                    name: if name.starts_with("OpenAI") {
                        "version".to_owned()
                    } else {
                        "revision".to_owned()
                    },
                    value: "query-route-secret".to_owned(),
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
                    api_key: format!("secret-{index}"),
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
                    access_token: "access".to_owned(),
                    refresh_token: Some("refresh".to_owned()),
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
                    api_key: "key".to_owned(),
                    custom_header_values: vec![SecretNameValueInput {
                        name: "x-tenant".to_owned(),
                        value: "v".repeat(4_097),
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
    fixture.repo.record_probe(rerouted.clone()).unwrap();
    let rerouted_command = SetActiveAiConfigurationCommand {
        connection_id: connection.connection_id.clone(),
        expected_execution_revision: connection.execution_revision,
        provider: AiProviderKind::OpenAi,
        endpoint_fingerprint: hex_sha256("https://api.openai.com"),
        model_id: "gpt-exact".to_owned(),
        reasoning: AiReasoningSelection::Effort {
            id: "low".to_owned(),
        },
        adapter_version: "general-v1".to_owned(),
        catalogue_sha256: catalogue_sha256(&rerouted).unwrap(),
        confirmed_data_destination: "https://api.openai.com".to_owned(),
    };
    assert_eq!(
        fixture.repo.activate(rerouted_command).unwrap_err(),
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
        configuration: AiConnectionConfiguration::DirectProviderKey {
            provider: AiProviderKind::Anthropic,
        },
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
                configuration: AiConnectionConfiguration::DirectProviderKey {
                    provider: AiProviderKind::OpenAi,
                },
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
                    access_token: "rotated-access".to_owned(),
                    refresh_token: Some("rotated-refresh".to_owned()),
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
            access_token: "access-secret-sentinel".to_owned(),
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
            access_token: "access-secret-sentinel".to_owned(),
            refresh_token: Some(String::new()),
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
        AiAdapterVersions {
            codex: "codex-v1".to_owned(),
            general: "general-v1".to_owned(),
        },
        Arc::new(|| "2026-08-24T12:34:56Z".to_owned()),
        Arc::new(future_nonterminal_reference_check),
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
                configuration: AiConnectionConfiguration::DirectProviderKey { provider },
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
                    api_key: format!("secret-{name}"),
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
    let renamed_fields = CompatibleEndpointConfiguration::parse(
        &base.base_url,
        CompatibleCredentialKind::Bearer,
        vec!["x-workspace".to_owned()],
        vec!["version".to_owned()],
        "material-model",
    )
    .unwrap();
    assert_material_update(
        compatible_create_command("Names", base.clone(), "header-A", "query-A"),
        AiProviderKind::OpenAiCompatible,
        &base.base_url,
        AiConnectionConfiguration::OpenAiCompatible {
            provider: AiProviderKind::OpenAiCompatible,
            endpoint: renamed_fields,
        },
        Some(AiCredentialInput::ApiKey {
            api_key: "replacement-key".to_owned(),
            custom_header_values: vec![SecretNameValueInput {
                name: "x-workspace".to_owned(),
                value: "header-B".to_owned(),
            }],
            custom_query_values: vec![SecretNameValueInput {
                name: "version".to_owned(),
                value: "query-B".to_owned(),
            }],
        }),
        1,
    );
    assert_material_update(
        compatible_create_command("Values", base.clone(), "header-A", "query-A"),
        AiProviderKind::OpenAiCompatible,
        &base.base_url,
        AiConnectionConfiguration::OpenAiCompatible {
            provider: AiProviderKind::OpenAiCompatible,
            endpoint: base.clone(),
        },
        Some(AiCredentialInput::ApiKey {
            api_key: "replacement-key".to_owned(),
            custom_header_values: vec![SecretNameValueInput {
                name: "x-tenant".to_owned(),
                value: "header-value-B".to_owned(),
            }],
            custom_query_values: vec![SecretNameValueInput {
                name: "revision".to_owned(),
                value: "query-value-B".to_owned(),
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
        .installation
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
            api_key: "compatible-key".to_owned(),
            custom_header_values: vec![SecretNameValueInput {
                name: "x-tenant".to_owned(),
                value: header_value.to_owned(),
            }],
            custom_query_values: vec![SecretNameValueInput {
                name: "revision".to_owned(),
                value: query_value.to_owned(),
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
            configuration: new_configuration,
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
