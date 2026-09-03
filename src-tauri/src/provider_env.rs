//! Provider connections held in a hand-editable `.env` file in the Application Home.
//!
//! Quantix deliberately does not use an operating-system credential vault here: the
//! Engineer asked for one plain file they can read, edit and copy between machines.
//! That trade is explicit — **API keys are stored in clear text**, so the file is only
//! as private as the Application Home, whose permissions Setup already verifies are
//! restricted. Nothing in this module may log a key, and no key is ever handed to the
//! renderer; [`AiProviderView`] exists so the interface can show a connection
//! without its secret.
//!
//! Layout — one block per connection, keyed by an Engineer-visible id:
//!
//! ```text
//! QUANTIX_ACTIVE_PROVIDER=OPENROUTER
//! QUANTIX_PROVIDER_OPENROUTER_NAME=OpenRouter
//! QUANTIX_PROVIDER_OPENROUTER_ROUTE=openai_compatible
//! QUANTIX_PROVIDER_OPENROUTER_BASE_URL=https://openrouter.ai/api/v1
//! QUANTIX_PROVIDER_OPENROUTER_MODEL=anthropic/claude-sonnet-4.5
//! QUANTIX_PROVIDER_OPENROUTER_API_KEY=sk-or-...
//! ```
//!
//! Lines Quantix does not own are preserved across writes, so an Engineer's own
//! variables and comments survive edits made through the interface.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::setup::PROVIDER_ENVIRONMENT_FILE;
use crate::tender_store::{TenderCommandError, TenderErrorCode};

const KEY_PREFIX: &str = "QUANTIX_PROVIDER_";
const ACTIVE_KEY: &str = "QUANTIX_ACTIVE_PROVIDER";
const STAGED_FILE: &str = ".env.staging";
const MAX_CONNECTIONS: usize = 32;
const MAX_VALUE_BYTES: usize = 8 * 1024;
const MAX_FILE_BYTES: u64 = 1024 * 1024;

const HEADER: &str = "\
# Quantix AI providers.
#
# This file is PLAIN TEXT. Anything that can read it can use your API keys, so keep it
# as private as the rest of your Quantix data, and never commit it anywhere.
#
# Each connection is one block of QUANTIX_PROVIDER_<ID>_* entries. You may edit them by
# hand. Lines that are not QUANTIX_PROVIDER_* or QUANTIX_ACTIVE_PROVIDER are left alone.
";

/// Which client speaks to the endpoint. These match the AI worker's `route` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum AiProviderRoute {
    // Spelled exactly as the worker's `route` values, so the name the interface sends
    // is the name that reaches the model client. `rename_all` would give "open_ai".
    #[serde(rename = "openai")]
    OpenAi,
    #[serde(rename = "openai_compatible")]
    OpenAiCompatible,
    #[serde(rename = "anthropic")]
    Anthropic,
    #[serde(rename = "anthropic_compatible")]
    AnthropicCompatible,
    #[serde(rename = "google")]
    Google,
    #[serde(rename = "xai")]
    XAi,
}

impl AiProviderRoute {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OpenAi => "openai",
            Self::OpenAiCompatible => "openai_compatible",
            Self::Anthropic => "anthropic",
            Self::AnthropicCompatible => "anthropic_compatible",
            Self::Google => "google",
            Self::XAi => "xai",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "openai" => Some(Self::OpenAi),
            "openai_compatible" => Some(Self::OpenAiCompatible),
            "anthropic" => Some(Self::Anthropic),
            "anthropic_compatible" => Some(Self::AnthropicCompatible),
            "google" => Some(Self::Google),
            "xai" => Some(Self::XAi),
            _ => None,
        }
    }

    /// Routes carrying no built-in destination, so a base URL is mandatory. The worker
    /// rejects these without one rather than silently using a vendor default.
    pub fn requires_base_url(self) -> bool {
        matches!(self, Self::OpenAiCompatible | Self::AnthropicCompatible)
    }
}

/// A connection together with its secret. This never crosses into the renderer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiProviderConnection {
    pub id: String,
    pub display_name: String,
    pub route: AiProviderRoute,
    pub base_url: Option<String>,
    pub model_id: String,
    pub api_key: String,
}

/// What the interface sees: everything except the key, plus whether one is stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct AiProviderView {
    pub id: String,
    pub display_name: String,
    pub route: AiProviderRoute,
    pub base_url: Option<String>,
    pub model_id: String,
    pub has_api_key: bool,
    pub is_active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct AiProviderSettingsView {
    pub connections: Vec<AiProviderView>,
    pub active_id: Option<String>,
    /// Surfaced in the interface so the storage trade-off is never a surprise.
    pub file_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[ts(export)]
pub struct SaveAiProviderCommand {
    pub id: String,
    pub display_name: String,
    pub route: AiProviderRoute,
    pub base_url: Option<String>,
    pub model_id: String,
    /// Absent means "keep the stored key", so editing a connection never requires the
    /// interface to hold a secret it should not have.
    pub api_key: Option<String>,
}

/// What a probe tells the Engineer. Deliberately not the raw driver error: the
/// message is shown in the interface, so it must never carry request material.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct AiProviderProbeResult {
    pub reached: bool,
    pub summary: String,
}

impl AiProviderProbeResult {
    pub(crate) fn from_outcome(
        outcome: Result<
            crate::agent_runtime::worker_lane::WorkerOutcome,
            crate::agent_runtime::worker_lane::WorkerDriverError,
        >,
    ) -> Self {
        use crate::agent_runtime::worker_lane::WorkerFailureCategory as Category;
        match outcome {
            Ok(_) => Self {
                reached: true,
                summary: "The provider answered. This connection is ready to use."
                    .to_owned(),
            },
            Err(failure) => Self {
                reached: false,
                summary: match failure.category {
                    Category::Auth => {
                        "The provider rejected the API key. Check the key and try again."
                    }
                    Category::RateLimited => {
                        "The provider is rate limiting this key right now. Try again shortly."
                    }
                    Category::Network => {
                        "Quantix could not reach the endpoint. Check the base URL and your connection."
                    }
                    Category::Budget => "The probe ran out of its time or size budget.",
                    Category::Cancelled => "The check was cancelled.",
                    Category::InvalidOutput | Category::Protocol => {
                        "The endpoint answered in a form Quantix could not read. Check the API style and model."
                    }
                    Category::Provider | Category::Process => {
                        "The provider reported an error. Check the model name for this endpoint."
                    }
                }
                .to_owned(),
            },
        }
    }
}

fn invalid() -> TenderCommandError {
    TenderCommandError::new(TenderErrorCode::InvalidCommand)
}

fn unavailable() -> TenderCommandError {
    TenderCommandError::new(TenderErrorCode::StoreUnavailable)
}

fn provider_required() -> TenderCommandError {
    TenderCommandError::new(TenderErrorCode::AiProviderRequired)
}

pub fn environment_path(application_home: &Path) -> PathBuf {
    application_home.join(PROVIDER_ENVIRONMENT_FILE)
}

/// Ids become part of an environment variable name, so they are deliberately narrow.
fn normalize_id(raw: &str) -> Result<String, TenderCommandError> {
    let id = raw.trim().to_ascii_uppercase();
    if id.is_empty() || id.len() > 48 {
        return Err(invalid());
    }
    if !id
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        return Err(invalid());
    }
    if id.starts_with(|first: char| first.is_ascii_digit()) {
        return Err(invalid());
    }
    Ok(id)
}

/// A value has to survive a round trip through a single `KEY=value` line.
fn valid_value(value: &str) -> bool {
    value.len() <= MAX_VALUE_BYTES
        && !value.contains('\n')
        && !value.contains('\r')
        && !value.contains('\0')
}

fn parse_lines(contents: &str) -> (BTreeMap<String, String>, Vec<String>) {
    let mut recognised = BTreeMap::new();
    let mut preserved = Vec::new();
    for line in contents.lines() {
        let trimmed = line.trim_start();
        let owned_key = trimmed.starts_with(KEY_PREFIX) || trimmed.starts_with(ACTIVE_KEY);
        match trimmed.split_once('=') {
            Some((key, value)) if owned_key => {
                recognised.insert(key.trim().to_owned(), value.trim().to_owned());
            }
            // Anything else belongs to the Engineer, including their comments. The
            // generated header is re-emitted on write, so it is not preserved here.
            _ if trimmed.starts_with('#') => {}
            _ if trimmed.is_empty() => {}
            _ => preserved.push(line.to_owned()),
        }
    }
    (recognised, preserved)
}

fn read_contents(application_home: &Path) -> Result<String, TenderCommandError> {
    let path = environment_path(application_home);
    match fs::metadata(&path) {
        Ok(metadata) if metadata.len() > MAX_FILE_BYTES => return Err(unavailable()),
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(String::new()),
        Err(_) => return Err(unavailable()),
    }
    fs::read_to_string(&path).map_err(|_| unavailable())
}

fn connections_from(entries: &BTreeMap<String, String>) -> Vec<AiProviderConnection> {
    let mut ids: Vec<String> = entries
        .keys()
        .filter_map(|key| key.strip_prefix(KEY_PREFIX)?.strip_suffix("_NAME"))
        .map(str::to_owned)
        .collect();
    ids.sort();
    ids.into_iter()
        .filter_map(|id| {
            let field = |name: &str| entries.get(&format!("{KEY_PREFIX}{id}_{name}")).cloned();
            let route = AiProviderRoute::parse(field("ROUTE")?.as_str())?;
            let base_url = field("BASE_URL").filter(|value| !value.is_empty());
            // A compatible route without a destination cannot be executed, so it is
            // not offered at all rather than silently reaching a vendor default.
            if route.requires_base_url() && base_url.is_none() {
                return None;
            }
            Some(AiProviderConnection {
                display_name: field("NAME").filter(|value| !value.is_empty())?,
                route,
                base_url,
                model_id: field("MODEL").filter(|value| !value.is_empty())?,
                api_key: field("API_KEY").unwrap_or_default(),
                id,
            })
        })
        .collect()
}

pub fn load_connections(
    application_home: &Path,
) -> Result<(Vec<AiProviderConnection>, Option<String>), TenderCommandError> {
    let contents = read_contents(application_home)?;
    let (entries, _) = parse_lines(&contents);
    let connections = connections_from(&entries);
    let active = entries
        .get(ACTIVE_KEY)
        .filter(|value| !value.is_empty())
        .filter(|value| connections.iter().any(|entry| &&entry.id == value))
        .cloned();
    Ok((connections, active))
}

pub fn inspect(application_home: &Path) -> Result<AiProviderSettingsView, TenderCommandError> {
    let (connections, active_id) = load_connections(application_home)?;
    Ok(AiProviderSettingsView {
        connections: connections
            .into_iter()
            .map(|entry| AiProviderView {
                is_active: active_id.as_deref() == Some(entry.id.as_str()),
                has_api_key: !entry.api_key.is_empty(),
                id: entry.id,
                display_name: entry.display_name,
                route: entry.route,
                base_url: entry.base_url,
                model_id: entry.model_id,
            })
            .collect(),
        active_id,
        file_path: environment_path(application_home)
            .to_string_lossy()
            .into_owned(),
    })
}

/// One named connection ready to execute. Used to check a provider before it is
/// made the default, so it deliberately does not consult the active id.
pub fn resolve_connection(
    application_home: &Path,
    id: &str,
) -> Result<AiProviderConnection, TenderCommandError> {
    let id = normalize_id(id)?;
    let (connections, _) = load_connections(application_home)?;
    let connection = connections
        .into_iter()
        .find(|entry| entry.id == id)
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::NotFound))?;
    if connection.api_key.is_empty() {
        return Err(provider_required());
    }
    Ok(connection)
}

/// The one connection ready to execute: it must exist and carry a key.
///
/// The only caller today is this module's tests. It becomes live when the worker lane
/// gains a production caller — `WorkerRunRequest.api_key` has no other producer — and
/// it is kept here so the "configured but unusable" rule stays covered until then.
#[allow(dead_code, reason = "consumed by the worker-lane execution wiring")]
pub fn resolve_active(application_home: &Path) -> Result<AiProviderConnection, TenderCommandError> {
    let (connections, active) = load_connections(application_home)?;
    let active = active.ok_or_else(provider_required)?;
    let connection = connections
        .into_iter()
        .find(|entry| entry.id == active)
        .ok_or_else(provider_required)?;
    if connection.api_key.is_empty() {
        return Err(provider_required());
    }
    Ok(connection)
}

fn write_entries(
    application_home: &Path,
    entries: &BTreeMap<String, String>,
    preserved: &[String],
) -> Result<(), TenderCommandError> {
    let mut rendered = String::from(HEADER);
    for (key, value) in entries {
        rendered.push_str(key);
        rendered.push('=');
        rendered.push_str(value);
        rendered.push('\n');
    }
    if !preserved.is_empty() {
        rendered.push('\n');
        for line in preserved {
            rendered.push_str(line);
            rendered.push('\n');
        }
    }
    // Write beside the target and rename, so an interrupted save can never leave a
    // half-written file where the keys used to be.
    let staged = application_home.join(STAGED_FILE);
    fs::write(&staged, rendered).map_err(|_| unavailable())?;
    restrict_to_owner(&staged);
    if let Err(error) = fs::rename(&staged, environment_path(application_home)) {
        let _ = fs::remove_file(&staged);
        let _ = error;
        return Err(unavailable());
    }
    Ok(())
}

#[cfg(unix)]
fn restrict_to_owner(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn restrict_to_owner(_path: &Path) {
    // Windows inherits the Application Home's access control, which Setup verifies is
    // restricted to the current user before the home is considered usable.
}

pub fn save_connection(
    application_home: &Path,
    command: SaveAiProviderCommand,
) -> Result<AiProviderSettingsView, TenderCommandError> {
    let id = normalize_id(&command.id)?;
    let display_name = command.display_name.trim().to_owned();
    let model_id = command.model_id.trim().to_owned();
    let base_url = command
        .base_url
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());

    if display_name.is_empty() || model_id.is_empty() {
        return Err(invalid());
    }
    if !valid_value(&display_name) || !valid_value(&model_id) {
        return Err(invalid());
    }
    if let Some(url) = &base_url {
        if !valid_value(url) || reqwest::Url::parse(url).is_err() {
            return Err(invalid());
        }
    }
    if command.route.requires_base_url() && base_url.is_none() {
        return Err(invalid());
    }

    let contents = read_contents(application_home)?;
    let (mut entries, preserved) = parse_lines(&contents);
    let existing = connections_from(&entries);
    if !existing.iter().any(|entry| entry.id == id) && existing.len() >= MAX_CONNECTIONS {
        return Err(invalid());
    }

    let key_field = format!("{KEY_PREFIX}{id}_API_KEY");
    let api_key = match command.api_key {
        Some(key) => {
            let key = key.trim().to_owned();
            if !valid_value(&key) {
                return Err(invalid());
            }
            key
        }
        // Absent means unchanged, so an edit that does not retype the key keeps it.
        None => entries.get(&key_field).cloned().unwrap_or_default(),
    };

    entries.insert(format!("{KEY_PREFIX}{id}_NAME"), display_name);
    entries.insert(
        format!("{KEY_PREFIX}{id}_ROUTE"),
        command.route.as_str().to_owned(),
    );
    entries.insert(
        format!("{KEY_PREFIX}{id}_BASE_URL"),
        base_url.unwrap_or_default(),
    );
    entries.insert(format!("{KEY_PREFIX}{id}_MODEL"), model_id);
    entries.insert(key_field, api_key);
    // The first connection becomes the default; later ones do not steal it.
    if entries
        .get(ACTIVE_KEY)
        .is_none_or(|active| active.is_empty())
    {
        entries.insert(ACTIVE_KEY.to_owned(), id.clone());
    }

    write_entries(application_home, &entries, &preserved)?;
    inspect(application_home)
}

pub fn remove_connection(
    application_home: &Path,
    id: &str,
) -> Result<AiProviderSettingsView, TenderCommandError> {
    let id = normalize_id(id)?;
    let contents = read_contents(application_home)?;
    let (mut entries, preserved) = parse_lines(&contents);
    let prefix = format!("{KEY_PREFIX}{id}_");
    let removed: Vec<String> = entries
        .keys()
        .filter(|key| key.starts_with(&prefix))
        .cloned()
        .collect();
    if removed.is_empty() {
        return Err(TenderCommandError::new(TenderErrorCode::NotFound));
    }
    for key in removed {
        entries.remove(&key);
    }
    if entries.get(ACTIVE_KEY) == Some(&id) {
        entries.remove(ACTIVE_KEY);
    }
    write_entries(application_home, &entries, &preserved)?;
    inspect(application_home)
}

pub fn set_active_connection(
    application_home: &Path,
    id: &str,
) -> Result<AiProviderSettingsView, TenderCommandError> {
    let id = normalize_id(id)?;
    let contents = read_contents(application_home)?;
    let (mut entries, preserved) = parse_lines(&contents);
    if !connections_from(&entries)
        .iter()
        .any(|entry| entry.id == id)
    {
        return Err(TenderCommandError::new(TenderErrorCode::NotFound));
    }
    entries.insert(ACTIVE_KEY.to_owned(), id);
    write_entries(application_home, &entries, &preserved)?;
    inspect(application_home)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home() -> tempfile::TempDir {
        tempfile::tempdir().expect("temporary application home")
    }

    fn command(id: &str) -> SaveAiProviderCommand {
        SaveAiProviderCommand {
            id: id.to_owned(),
            display_name: "OpenRouter".to_owned(),
            route: AiProviderRoute::OpenAiCompatible,
            base_url: Some("https://openrouter.ai/api/v1".to_owned()),
            model_id: "anthropic/claude-sonnet-4.5".to_owned(),
            api_key: Some("sk-or-secret".to_owned()),
        }
    }

    #[test]
    fn a_saved_connection_round_trips_without_exposing_its_key() {
        let home = home();
        let view = save_connection(home.path(), command("openrouter")).expect("saved");
        assert_eq!(view.connections.len(), 1);
        let stored = &view.connections[0];
        assert_eq!(stored.id, "OPENROUTER");
        assert!(stored.has_api_key);
        assert!(stored.is_active, "the first connection becomes the default");

        // The view is what reaches the renderer, so the secret must not be in it.
        let rendered = serde_json::to_string(&view).expect("view serializes");
        assert!(!rendered.contains("sk-or-secret"));

        let resolved = resolve_active(home.path()).expect("active connection");
        assert_eq!(resolved.api_key, "sk-or-secret");
        assert_eq!(resolved.route.as_str(), "openai_compatible");
    }

    #[test]
    fn editing_without_a_key_keeps_the_stored_one() {
        let home = home();
        save_connection(home.path(), command("openrouter")).expect("saved");
        let mut edit = command("openrouter");
        edit.display_name = "Renamed".to_owned();
        edit.api_key = None;
        save_connection(home.path(), edit).expect("edited");

        let resolved = resolve_active(home.path()).expect("active connection");
        assert_eq!(resolved.display_name, "Renamed");
        assert_eq!(resolved.api_key, "sk-or-secret");
    }

    #[test]
    fn unrelated_lines_survive_a_write() {
        let home = home();
        fs::write(environment_path(home.path()), "MY_OWN_TOOL_TOKEN=keep-me\n").expect("seed");
        save_connection(home.path(), command("openrouter")).expect("saved");
        let contents = fs::read_to_string(environment_path(home.path())).expect("read back");
        assert!(contents.contains("MY_OWN_TOOL_TOKEN=keep-me"));
        assert!(contents.contains("QUANTIX_PROVIDER_OPENROUTER_MODEL="));
    }

    #[test]
    fn a_compatible_route_without_a_destination_is_rejected() {
        let home = home();
        let mut without_url = command("openrouter");
        without_url.base_url = None;
        assert_eq!(
            save_connection(home.path(), without_url).unwrap_err().code,
            TenderErrorCode::InvalidCommand
        );
    }

    #[test]
    fn ids_that_would_not_survive_an_environment_name_are_rejected() {
        let home = home();
        for candidate in ["", "has space", "has-dash", "9leading", "sym$bol"] {
            let mut invalid_id = command("openrouter");
            invalid_id.id = candidate.to_owned();
            assert!(
                save_connection(home.path(), invalid_id).is_err(),
                "{candidate} should be rejected"
            );
        }
    }

    #[test]
    fn a_value_carrying_a_newline_cannot_break_out_of_its_line() {
        let home = home();
        let mut smuggled = command("openrouter");
        smuggled.api_key = Some("real\nQUANTIX_ACTIVE_PROVIDER=OTHER".to_owned());
        assert_eq!(
            save_connection(home.path(), smuggled).unwrap_err().code,
            TenderErrorCode::InvalidCommand
        );
    }

    #[test]
    fn removing_the_active_connection_clears_the_default() {
        let home = home();
        save_connection(home.path(), command("openrouter")).expect("saved");
        let view = remove_connection(home.path(), "openrouter").expect("removed");
        assert!(view.connections.is_empty());
        assert!(view.active_id.is_none());
        assert_eq!(
            resolve_active(home.path()).unwrap_err().code,
            TenderErrorCode::AiProviderRequired
        );
    }

    #[test]
    fn the_default_can_be_moved_between_connections() {
        let home = home();
        save_connection(home.path(), command("openrouter")).expect("saved");
        let mut second = command("groq");
        second.display_name = "Groq".to_owned();
        second.base_url = Some("https://api.groq.com/openai/v1".to_owned());
        save_connection(home.path(), second).expect("saved");

        let view = inspect(home.path()).expect("inspected");
        assert_eq!(view.active_id.as_deref(), Some("OPENROUTER"));

        let moved = set_active_connection(home.path(), "groq").expect("activated");
        assert_eq!(moved.active_id.as_deref(), Some("GROQ"));
        assert_eq!(resolve_active(home.path()).expect("active").id, "GROQ");
    }

    #[test]
    fn a_connection_without_a_key_is_not_executable() {
        let home = home();
        let mut keyless = command("openrouter");
        keyless.api_key = Some(String::new());
        let view = save_connection(home.path(), keyless).expect("saved");
        assert!(!view.connections[0].has_api_key);
        assert_eq!(
            resolve_active(home.path()).unwrap_err().code,
            TenderErrorCode::AiProviderRequired
        );
    }

    #[test]
    fn a_missing_file_reads_as_no_connections() {
        let home = home();
        let view = inspect(home.path()).expect("inspected");
        assert!(view.connections.is_empty());
        assert!(view.active_id.is_none());
    }
}
