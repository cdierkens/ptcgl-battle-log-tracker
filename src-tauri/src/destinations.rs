//! Destinations: sites that saved logs sync to, imported from a pasted destination config.

use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::Url;

use crate::delivery_queue::{DeliveryQueue, QueueError};
use crate::ports::{SecretError, SecretStore};

const CONFIG_MARKER: &str = "ptcglTrackerDestination";
const CONFIG_VERSION: u64 = 1;

/// A destination as kept in `settings.json`. Its token lives in the keychain under `id`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Destination {
    /// Generated once; survives re-pastes and renames. Keys the keychain entry and deliveries.
    pub id: String,
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub token_url: Option<String>,
    pub enabled: bool,
    /// Send the match's deck snapshot in the envelope. Opt-in, default off.
    #[serde(default)]
    pub send_deck_identity: bool,
}

/// A validated destination config, not yet saved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DestinationConfig {
    pub name: String,
    /// Normalised, so the same site pasted twice compares equal.
    pub url: String,
    pub headers: BTreeMap<String, String>,
    pub token: Option<String>,
    pub token_url: Option<String>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ConfigError {
    #[error("That isn't a destination config. Copy it again from the site.")]
    NotAConfig,
    #[error("This config needs a newer version of PTCGL Tracker.")]
    NewerVersion,
    #[error("This config is invalid: {0}")]
    Invalid(String),
}

#[derive(Debug, thiserror::Error)]
pub enum DestinationError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error("Paste a token for {0} first.")]
    TokenRequired(String),
    #[error("That destination no longer exists.")]
    NotFound,
    #[error("Sync isn't available on this OS yet.")]
    SecretsUnsupported,
    #[error("Keychain: {0}")]
    Secrets(String),
    #[error(transparent)]
    Queue(#[from] QueueError),
}

impl From<SecretError> for DestinationError {
    fn from(e: SecretError) -> Self {
        match e {
            SecretError::Unsupported => Self::SecretsUnsupported,
            other => Self::Secrets(other.to_string()),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawConfig {
    #[serde(rename = "ptcglTrackerDestination")]
    _version: u64,
    name: String,
    url: String,
    auth: RawAuth,
    #[serde(default)]
    headers: BTreeMap<String, String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawAuth {
    #[serde(rename = "type")]
    kind: String,
    token: Option<String>,
    token_url: Option<String>,
}

/// Dev builds also accept `http://localhost` so the local stub works.
const ALLOW_LOOPBACK_HTTP: bool = cfg!(debug_assertions);

pub fn parse_config(text: &str) -> Result<DestinationConfig, ConfigError> {
    parse_config_with(text, ALLOW_LOOPBACK_HTTP)
}

fn parse_config_with(text: &str, allow_loopback_http: bool) -> Result<DestinationConfig, ConfigError> {
    let value: serde_json::Value = serde_json::from_str(text.trim()).map_err(|_| ConfigError::NotAConfig)?;
    match value.get(CONFIG_MARKER).and_then(serde_json::Value::as_u64) {
        None => return Err(ConfigError::NotAConfig),
        Some(v) if v > CONFIG_VERSION => return Err(ConfigError::NewerVersion),
        Some(v) if v < CONFIG_VERSION => return Err(ConfigError::NotAConfig),
        Some(_) => {}
    }
    let raw: RawConfig = serde_json::from_value(value).map_err(|e| ConfigError::Invalid(e.to_string()))?;

    let name = raw.name.trim().to_string();
    if name.is_empty() {
        return Err(ConfigError::Invalid("name is empty".into()));
    }
    if raw.auth.kind != "bearer" {
        return Err(ConfigError::Invalid(format!("auth type \"{}\" isn't supported", raw.auth.kind)));
    }
    let url = destination_url(&raw.url, "url", allow_loopback_http)?;
    let token_url =
        raw.auth.token_url.as_deref().map(|u| destination_url(u, "auth.tokenUrl", allow_loopback_http)).transpose()?;
    let token = raw.auth.token.as_deref().map(validate_token).transpose()?;
    if token.is_none() && token_url.is_none() {
        return Err(ConfigError::Invalid("auth needs a token or a tokenUrl".into()));
    }
    let mut seen = std::collections::BTreeSet::new();
    for (header, value) in &raw.headers {
        validate_header(header, value)?;
        if !seen.insert(header.to_ascii_lowercase()) {
            return Err(ConfigError::Invalid(format!("header \"{header}\" is listed twice")));
        }
    }
    Ok(DestinationConfig { name, url, headers: raw.headers, token, token_url })
}

fn destination_url(text: &str, field: &str, allow_loopback_http: bool) -> Result<String, ConfigError> {
    let url = Url::parse(text.trim()).map_err(|_| ConfigError::Invalid(format!("{field} isn't a URL")))?;
    let loopback = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
    let scheme_ok = url.scheme() == "https" || (allow_loopback_http && loopback && url.scheme() == "http");
    if !scheme_ok || url.host_str().is_none() {
        return Err(ConfigError::Invalid(format!("{field} must be an https URL")));
    }
    Ok(url.to_string())
}

/// Tokens go into an `Authorization` header, so only visible ASCII.
pub fn validate_token(token: &str) -> Result<String, ConfigError> {
    let token = token.trim();
    if token.is_empty() {
        return Err(ConfigError::Invalid("token is empty".into()));
    }
    if !token.bytes().all(|b| b.is_ascii_graphic()) {
        return Err(ConfigError::Invalid("token has characters a header can't carry".into()));
    }
    Ok(token.to_string())
}

/// The app sets these itself: auth, and the envelope's body and target.
const RESERVED_HEADERS: [&str; 4] = ["authorization", "content-type", "content-length", "host"];

fn validate_header(name: &str, value: &str) -> Result<(), ConfigError> {
    const TCHAR: &[u8] = b"!#$%&'*+-.^_`|~";
    if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || TCHAR.contains(&b)) {
        return Err(ConfigError::Invalid(format!("header \"{name}\" isn't a valid header name")));
    }
    if RESERVED_HEADERS.iter().any(|r| name.eq_ignore_ascii_case(r)) {
        return Err(ConfigError::Invalid(format!("header \"{name}\" is set by the app")));
    }
    if !value.bytes().all(|b| b.is_ascii_graphic() || b == b' ' || b == b'\t') {
        return Err(ConfigError::Invalid(format!("header \"{name}\" has an invalid value")));
    }
    Ok(())
}

/// Whether the confirmation step asks for a token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TokenPrompt {
    /// The config carries its own token.
    Embedded,
    Required,
    /// Updating a destination that already has a token; leave blank to keep it.
    Optional,
}

/// What the confirmation step shows before an import is saved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    pub name: String,
    pub url: String,
    pub token: TokenPrompt,
    pub token_url: Option<String>,
    /// Name of the destination this updates in place, if any.
    pub replaces: Option<String>,
}

pub fn preview(
    config: &DestinationConfig,
    existing: &[Destination],
    secrets: &dyn SecretStore,
) -> Result<ImportPreview, DestinationError> {
    let replaces = existing.iter().find(|d| d.url == config.url);
    let token = if config.token.is_some() {
        TokenPrompt::Embedded
    } else if has_token(replaces, secrets)? {
        TokenPrompt::Optional
    } else {
        TokenPrompt::Required
    };
    Ok(ImportPreview {
        name: config.name.clone(),
        url: config.url.clone(),
        token,
        token_url: config.token_url.clone(),
        replaces: replaces.map(|d| d.name.clone()),
    })
}

fn has_token(destination: Option<&Destination>, secrets: &dyn SecretStore) -> Result<bool, SecretError> {
    match destination {
        Some(d) => Ok(secrets.get(&d.id)?.is_some()),
        None => Ok(false),
    }
}

/// Saves the token, then upserts by URL: a known URL keeps its id and enabled state.
/// A new token un-pauses the destination. Returns the destination's id. The caller saves
/// settings after; if that fails a new destination's keychain entry is orphaned, which is harmless.
pub fn import(
    destinations: &mut Vec<Destination>,
    secrets: &dyn SecretStore,
    queue: &DeliveryQueue,
    config: DestinationConfig,
    pasted_token: Option<&str>,
) -> Result<String, DestinationError> {
    let pasted = pasted_token.filter(|t| !t.trim().is_empty()).map(validate_token).transpose()?;
    let token = config.token.clone().or(pasted);
    let existing = destinations.iter().position(|d| d.url == config.url);
    if token.is_none() && !has_token(existing.map(|i| &destinations[i]), secrets)? {
        return Err(DestinationError::TokenRequired(config.name));
    }
    let id = match existing {
        Some(i) => destinations[i].id.clone(),
        None => fresh_id(destinations),
    };
    if let Some(token) = &token {
        secrets.set(&id, token)?;
        queue.clear_attention(&id)?;
    }
    let enabled = existing.map(|i| destinations[i].enabled).unwrap_or(true);
    let send_deck_identity = existing.map(|i| destinations[i].send_deck_identity).unwrap_or(false);
    let next = Destination {
        id: id.clone(),
        name: config.name,
        url: config.url,
        headers: config.headers,
        token_url: config.token_url,
        enabled,
        send_deck_identity,
    };
    match existing {
        Some(i) => destinations[i] = next,
        None => destinations.push(next),
    }
    Ok(id)
}

pub fn find<'a>(destinations: &'a [Destination], id: &str) -> Result<&'a Destination, DestinationError> {
    destinations.iter().find(|d| d.id == id).ok_or(DestinationError::NotFound)
}

/// A pasted replacement token: saved to the keychain, and the destination resumes.
pub fn replace_token(
    destinations: &[Destination],
    secrets: &dyn SecretStore,
    queue: &DeliveryQueue,
    id: &str,
    token: &str,
) -> Result<(), DestinationError> {
    find(destinations, id)?;
    secrets.set(id, &validate_token(token)?)?;
    queue.clear_attention(id)?;
    Ok(())
}

pub fn set_enabled(destinations: &mut [Destination], id: &str, enabled: bool) -> Result<(), DestinationError> {
    let destination = destinations.iter_mut().find(|d| d.id == id).ok_or(DestinationError::NotFound)?;
    destination.enabled = enabled;
    Ok(())
}

pub fn set_send_deck_identity(
    destinations: &mut [Destination],
    id: &str,
    enabled: bool,
) -> Result<(), DestinationError> {
    let destination = destinations.iter_mut().find(|d| d.id == id).ok_or(DestinationError::NotFound)?;
    destination.send_deck_identity = enabled;
    Ok(())
}

pub fn remove(destinations: &mut Vec<Destination>, id: &str) -> Result<(), DestinationError> {
    let index = destinations.iter().position(|d| d.id == id).ok_or(DestinationError::NotFound)?;
    destinations.remove(index);
    Ok(())
}

/// Deletes a removed destination's keychain entry and deliveries. Runs after settings are saved,
/// so a failure leaves only orphans, never a listed destination without its token.
pub fn forget(secrets: &dyn SecretStore, queue: &DeliveryQueue, id: &str) -> Result<(), DestinationError> {
    secrets.delete(id)?;
    queue.remove_destination(id)?;
    Ok(())
}

fn fresh_id(existing: &[Destination]) -> String {
    let mut n = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or_default();
    loop {
        let id = format!("dest-{n:x}");
        if existing.iter().all(|d| d.id != id) {
            return id;
        }
        n += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::delivery_queue::Release;
    use crate::ports::MemSecretStore;

    const POKEDOJO: &str = r#"{
        "ptcglTrackerDestination": 1,
        "name": "PokeDojo",
        "url": "https://pokedojo.pro/api/battle-logs",
        "auth": { "type": "bearer", "token": "pdj_abc" }
    }"#;

    const ASKS_FOR_TOKEN: &str = r#"{
        "ptcglTrackerDestination": 1,
        "name": "PokeDojo",
        "url": "https://pokedojo.pro/api/battle-logs",
        "auth": { "type": "bearer", "tokenUrl": "https://pokedojo.pro/settings/tracker" },
        "headers": { "X-Client": "tracker" }
    }"#;

    fn invalid(text: &str) -> String {
        match parse_config(text) {
            Err(ConfigError::Invalid(msg)) => msg,
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    #[test]
    fn parses_a_config_with_an_embedded_token() {
        let config = parse_config(POKEDOJO).unwrap();
        assert_eq!(config.name, "PokeDojo");
        assert_eq!(config.url, "https://pokedojo.pro/api/battle-logs");
        assert_eq!(config.token.as_deref(), Some("pdj_abc"));
        assert_eq!(config.token_url, None);
    }

    #[test]
    fn parses_a_config_that_asks_for_a_token() {
        let config = parse_config(ASKS_FOR_TOKEN).unwrap();
        assert_eq!(config.token, None);
        assert_eq!(config.token_url.as_deref(), Some("https://pokedojo.pro/settings/tracker"));
        assert_eq!(config.headers.get("X-Client").map(String::as_str), Some("tracker"));
    }

    #[test]
    fn rejects_things_that_arent_configs() {
        assert_eq!(parse_config("not json"), Err(ConfigError::NotAConfig));
        assert_eq!(parse_config(r#"{"name":"x"}"#), Err(ConfigError::NotAConfig));
        assert_eq!(parse_config(r#"{"ptcglTrackerDestination":0}"#), Err(ConfigError::NotAConfig));
    }

    #[test]
    fn a_newer_config_version_asks_for_an_update() {
        assert_eq!(parse_config(r#"{"ptcglTrackerDestination":2}"#), Err(ConfigError::NewerVersion));
    }

    #[test]
    fn rejects_insecure_urls() {
        assert!(invalid(&POKEDOJO.replace("https://pokedojo", "http://pokedojo")).contains("https"));
        let bad_token_url = ASKS_FOR_TOKEN.replace("https://pokedojo.pro/settings", "http://pokedojo.pro/settings");
        assert!(invalid(&bad_token_url).contains("tokenUrl"));
    }

    #[test]
    fn localhost_http_is_allowed_only_in_dev_builds() {
        let local = POKEDOJO.replace("https://pokedojo.pro", "http://localhost:8787");
        assert_eq!(parse_config_with(&local, true).unwrap().url, "http://localhost:8787/api/battle-logs");
        assert!(matches!(parse_config_with(&local, false), Err(ConfigError::Invalid(_))));
        let remote = POKEDOJO.replace("https://pokedojo.pro", "http://pokedojo.pro");
        assert!(matches!(parse_config_with(&remote, true), Err(ConfigError::Invalid(_))));
    }

    #[test]
    fn rejects_other_auth_types() {
        assert!(invalid(&POKEDOJO.replace("bearer", "basic")).contains("basic"));
    }

    #[test]
    fn authorization_is_reserved_for_auth() {
        let config = ASKS_FOR_TOKEN.replace("X-Client", "authorization");
        assert!(invalid(&config).contains("set by the app"));
    }

    #[test]
    fn rejects_headers_that_could_inject() {
        assert!(invalid(&ASKS_FOR_TOKEN.replace("\"tracker\"", "\"a\\r\\nX-Evil: 1\"")).contains("value"));
        assert!(invalid(&ASKS_FOR_TOKEN.replace("X-Client", "X Client")).contains("name"));
    }

    #[test]
    fn the_app_owns_body_and_target_headers() {
        for reserved in ["Content-Type", "content-length", "HOST"] {
            assert!(invalid(&ASKS_FOR_TOKEN.replace("X-Client", reserved)).contains("set by the app"));
        }
    }

    #[test]
    fn rejects_headers_listed_twice_in_different_case() {
        let twice = ASKS_FOR_TOKEN.replace(r#""X-Client": "tracker""#, r#""X-Client": "a", "x-client": "b""#);
        assert!(invalid(&twice).contains("twice"));
    }

    #[test]
    fn rejects_non_ascii_header_values() {
        assert!(invalid(&ASKS_FOR_TOKEN.replace("\"tracker\"", "\"tráckér\"")).contains("value"));
    }

    #[test]
    fn a_config_without_a_token_must_say_where_to_get_one() {
        let neither = POKEDOJO.replace(r#", "token": "pdj_abc""#, "");
        assert!(invalid(&neither).contains("tokenUrl"));
    }

    #[test]
    fn rejects_tokens_a_header_cant_carry() {
        assert!(invalid(&POKEDOJO.replace("pdj_abc", "pdj abc")).contains("token"));
        assert!(invalid(&POKEDOJO.replace("pdj_abc", "  ")).contains("empty"));
    }

    #[test]
    fn unknown_fields_are_rejected_so_typos_surface() {
        assert!(invalid(&POKEDOJO.replace("\"name\"", "\"nmae\"")).contains("nmae"));
    }

    fn setup() -> (Vec<Destination>, MemSecretStore, DeliveryQueue) {
        (Vec::new(), MemSecretStore::default(), DeliveryQueue::open_in_memory().unwrap())
    }

    #[test]
    fn importing_stores_the_token_in_the_keychain_not_settings() {
        let (mut dests, secrets, queue) = setup();
        let id = import(&mut dests, &secrets, &queue, parse_config(POKEDOJO).unwrap(), None).unwrap();
        assert_eq!(secrets.get(&id).unwrap().as_deref(), Some("pdj_abc"));
        assert_eq!(dests.len(), 1);
        assert!(dests[0].enabled);
        assert!(!dests[0].send_deck_identity);
        assert!(!serde_json::to_string(&dests).unwrap().contains("pdj_abc"));
    }

    #[test]
    fn a_config_without_a_token_needs_one_pasted() {
        let (mut dests, secrets, queue) = setup();
        let config = parse_config(ASKS_FOR_TOKEN).unwrap();
        assert_eq!(preview(&config, &dests, &secrets).unwrap().token, TokenPrompt::Required);
        assert!(matches!(
            import(&mut dests, &secrets, &queue, config.clone(), Some("  ")),
            Err(DestinationError::TokenRequired(_))
        ));
        assert!(dests.is_empty());
        let id = import(&mut dests, &secrets, &queue, config, Some(" pasted ")).unwrap();
        assert_eq!(secrets.get(&id).unwrap().as_deref(), Some("pasted"));
    }

    #[test]
    fn repasting_the_same_url_updates_in_place() {
        let (mut dests, secrets, queue) = setup();
        let id = import(&mut dests, &secrets, &queue, parse_config(POKEDOJO).unwrap(), None).unwrap();
        set_enabled(&mut dests, &id, false).unwrap();
        queue.enqueue("a.txt", &id, Release::Now).unwrap();
        queue.flag_attention(&id, "401").unwrap();

        let renamed =
            parse_config(&POKEDOJO.replace("\"PokeDojo\"", "\"Dojo\"").replace("pdj_abc", "pdj_new")).unwrap();
        let preview = preview(&renamed, &dests, &secrets).unwrap();
        assert_eq!(preview.replaces.as_deref(), Some("PokeDojo"));
        assert_eq!(preview.token, TokenPrompt::Embedded);
        assert_eq!(import(&mut dests, &secrets, &queue, renamed, None).unwrap(), id);

        assert_eq!(dests.len(), 1);
        assert_eq!(dests[0].name, "Dojo");
        assert!(!dests[0].enabled, "keeps the player's enabled choice");
        assert_eq!(secrets.get(&id).unwrap().as_deref(), Some("pdj_new"));
        assert!(queue.needing_attention().unwrap().is_empty(), "a new token un-pauses");
        assert_eq!(queue.for_log("a.txt").unwrap().len(), 1, "keeps its deliveries");
    }

    #[test]
    fn repasting_keeps_the_deck_identity_choice() {
        let (mut dests, secrets, queue) = setup();
        let id = import(&mut dests, &secrets, &queue, parse_config(POKEDOJO).unwrap(), None).unwrap();
        set_send_deck_identity(&mut dests, &id, true).unwrap();
        import(&mut dests, &secrets, &queue, parse_config(POKEDOJO).unwrap(), None).unwrap();
        assert!(dests[0].send_deck_identity);
    }

    #[test]
    fn repasting_without_a_token_keeps_the_old_one_and_the_pause() {
        let (mut dests, secrets, queue) = setup();
        let config = parse_config(ASKS_FOR_TOKEN).unwrap();
        let id = import(&mut dests, &secrets, &queue, config.clone(), Some("first")).unwrap();
        queue.flag_attention(&id, "401").unwrap();
        assert_eq!(preview(&config, &dests, &secrets).unwrap().token, TokenPrompt::Optional);
        import(&mut dests, &secrets, &queue, config, None).unwrap();
        assert_eq!(secrets.get(&id).unwrap().as_deref(), Some("first"));
        assert_eq!(queue.needing_attention().unwrap().len(), 1);
    }

    #[test]
    fn repasting_requires_a_token_when_the_keychain_lost_it() {
        let (mut dests, secrets, queue) = setup();
        let config = parse_config(ASKS_FOR_TOKEN).unwrap();
        let id = import(&mut dests, &secrets, &queue, config.clone(), Some("first")).unwrap();
        secrets.delete(&id).unwrap();
        assert_eq!(preview(&config, &dests, &secrets).unwrap().token, TokenPrompt::Required);
        assert!(matches!(import(&mut dests, &secrets, &queue, config, None), Err(DestinationError::TokenRequired(_))));
    }

    #[test]
    fn different_urls_are_different_destinations() {
        let (mut dests, secrets, queue) = setup();
        let a = import(&mut dests, &secrets, &queue, parse_config(POKEDOJO).unwrap(), None).unwrap();
        let other = POKEDOJO.replace("pokedojo.pro", "example.com");
        let b = import(&mut dests, &secrets, &queue, parse_config(&other).unwrap(), None).unwrap();
        assert_ne!(a, b);
        assert_eq!(dests.len(), 2);
        assert_eq!(dests[1].url, "https://example.com/api/battle-logs");
    }

    #[test]
    fn removing_deletes_the_token_and_deliveries() {
        let (mut dests, secrets, queue) = setup();
        let id = import(&mut dests, &secrets, &queue, parse_config(POKEDOJO).unwrap(), None).unwrap();
        queue.enqueue("a.txt", &id, Release::Now).unwrap();
        remove(&mut dests, &id).unwrap();
        assert!(dests.is_empty());
        forget(&secrets, &queue, &id).unwrap();
        assert_eq!(secrets.get(&id).unwrap(), None);
        assert!(queue.all().unwrap().is_empty());
        assert!(matches!(remove(&mut dests, &id), Err(DestinationError::NotFound)));
    }

    #[test]
    fn a_replacement_token_is_saved_and_resumes_the_destination() {
        let (mut dests, secrets, queue) = setup();
        let id = import(&mut dests, &secrets, &queue, parse_config(ASKS_FOR_TOKEN).unwrap(), Some("old")).unwrap();
        queue.flag_attention(&id, "rejected").unwrap();
        assert!(matches!(replace_token(&dests, &secrets, &queue, &id, " "), Err(DestinationError::Config(_))));
        assert_eq!(queue.needing_attention().unwrap().len(), 1, "a bad token changes nothing");
        replace_token(&dests, &secrets, &queue, &id, " new ").unwrap();
        assert_eq!(secrets.get(&id).unwrap().as_deref(), Some("new"));
        assert!(queue.needing_attention().unwrap().is_empty());
        assert!(matches!(replace_token(&dests, &secrets, &queue, "gone", "t"), Err(DestinationError::NotFound)));
        assert_eq!(secrets.get("gone").unwrap(), None);
    }

    #[test]
    fn old_destination_json_defaults_deck_identity_off() {
        let d: Destination = serde_json::from_str(
            r#"{"id":"dest-1","name":"Dojo","url":"https://dojo.example/","headers":{},"token_url":null,"enabled":true}"#,
        )
        .unwrap();
        assert!(!d.send_deck_identity);
    }

    #[test]
    fn destinations_round_trip_through_settings_json() {
        let (mut dests, secrets, queue) = setup();
        import(&mut dests, &secrets, &queue, parse_config(ASKS_FOR_TOKEN).unwrap(), Some("t")).unwrap();
        let back: Vec<Destination> = serde_json::from_str(&serde_json::to_string(&dests).unwrap()).unwrap();
        assert_eq!(back, dests);
    }
}
