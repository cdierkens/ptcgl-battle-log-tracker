//! Deck library and immutable identity snapshots.
//!
//! The webview owns decklist parsing and identity. Rust only stores the
//! serializable snapshot that the UI sends, and the delivery worker reads it
//! when building an envelope. This module never parses a decklist.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

const SCHEMA_VERSION: i32 = 1;
const FILE_NAME: &str = "decks.sqlite";

/// A confirmed player or opponent archetype. `id` is null for a hand-entered
/// opponent label; `cards` carries the sprite slugs when known.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchetypeRef {
    pub id: Option<String>,
    pub label: String,
    pub cards: Vec<String>,
}

/// One immutable version of a decklist, plus the identity computed by the webview.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeckVersion {
    pub id: String,
    pub source: String,
    pub raw_text: String,
    pub decklist_text: String,
    pub format_label: Option<String>,
    pub archetype: Option<ArchetypeRef>,
    pub deck_id_hash: Option<String>,
    pub list_id_hash: Option<String>,
    pub hash_version: Option<i64>,
    pub catalog_version: Option<String>,
    pub catalog_context_id: Option<String>,
    pub created_at: i64,
}

/// A stable local deck with its current version. Package `deckId` values are
/// metadata, not the primary key; the local id survives list edits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Deck {
    pub id: String,
    pub name: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub archived_at: Option<i64>,
    pub current_version: DeckVersion,
}

/// What the webview sends to create or edit a deck. `None` id creates a deck.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveDeckInput {
    pub id: Option<String>,
    pub name: String,
    pub source: String,
    pub raw_text: String,
    pub decklist_text: String,
    pub format_label: Option<String>,
    pub archetype: Option<ArchetypeRef>,
    pub deck_id_hash: Option<String>,
    pub list_id_hash: Option<String>,
    pub hash_version: Option<i64>,
    pub catalog_version: Option<String>,
    pub catalog_context_id: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum DeckStoreError {
    #[error("deck store: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("deck store folder: {0}")]
    Io(#[from] std::io::Error),
    #[error("deck store: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Invalid(String),
    #[error("deck not found")]
    NotFound,
}

/// Shared by commands and the delivery worker; `Connection` isn't `Sync`.
pub type SharedDeckStore = Arc<Mutex<DeckStore>>;

/// A poisoned lock still holds a usable connection.
pub fn lock(store: &SharedDeckStore) -> MutexGuard<'_, DeckStore> {
    store.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// SQLite-backed deck library, in the app-data folder (ADR 0002).
pub struct DeckStore {
    conn: Connection,
}

impl DeckStore {
    /// `<app data>/decks.sqlite`. App state stays out of the log folder.
    pub fn resolve_path(app: &tauri::AppHandle) -> tauri::Result<PathBuf> {
        use tauri::Manager;
        Ok(app.path().app_data_dir()?.join(FILE_NAME))
    }

    pub fn open(path: &Path) -> Result<Self, DeckStoreError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        Self::init(Connection::open(path)?)
    }

    /// Fallback when the file can't be opened. Log capture never depends on this.
    pub fn open_in_memory() -> Result<Self, DeckStoreError> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self, DeckStoreError> {
        conn.execute_batch(&format!(
            "CREATE TABLE IF NOT EXISTS decks (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                current_version_id TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                archived_at INTEGER
            );
            CREATE TABLE IF NOT EXISTS deck_versions (
                id TEXT PRIMARY KEY,
                deck_id TEXT NOT NULL REFERENCES decks(id),
                source TEXT NOT NULL,
                raw_text TEXT NOT NULL,
                decklist_text TEXT NOT NULL,
                format_label TEXT,
                archetype_json TEXT,
                deck_id_hash TEXT,
                list_id_hash TEXT,
                hash_version INTEGER,
                catalog_version TEXT,
                catalog_context_id TEXT,
                created_at INTEGER NOT NULL
            );
            PRAGMA user_version = {SCHEMA_VERSION};"
        ))?;
        Ok(Self { conn })
    }

    /// List active (non-archived) decks, most recently updated first.
    pub fn list(&self) -> Result<Vec<Deck>, DeckStoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT d.id, d.name, d.created_at, d.updated_at, d.archived_at,
                    v.id, v.source, v.raw_text, v.decklist_text, v.format_label,
                    v.archetype_json, v.deck_id_hash, v.list_id_hash, v.hash_version,
                    v.catalog_version, v.catalog_context_id, v.created_at
             FROM decks d
             JOIN deck_versions v ON v.id = d.current_version_id
             WHERE d.archived_at IS NULL
             ORDER BY d.updated_at DESC, d.id DESC",
        )?;
        let rows = stmt.query_map([], row_to_deck)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn get(&self, id: &str) -> Result<Option<Deck>, DeckStoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT d.id, d.name, d.created_at, d.updated_at, d.archived_at,
                    v.id, v.source, v.raw_text, v.decklist_text, v.format_label,
                    v.archetype_json, v.deck_id_hash, v.list_id_hash, v.hash_version,
                    v.catalog_version, v.catalog_context_id, v.created_at
             FROM decks d
             JOIN deck_versions v ON v.id = d.current_version_id
             WHERE d.id = ?1 AND d.archived_at IS NULL",
        )?;
        stmt.query_row([id], row_to_deck).optional().map_err(Into::into)
    }

    /// Create a deck or add an immutable version to an existing deck.
    pub fn save(&mut self, input: SaveDeckInput) -> Result<Deck, DeckStoreError> {
        let name = input.name.trim();
        if name.is_empty() {
            return Err(DeckStoreError::Invalid("deck name is required".into()));
        }
        if !matches!(input.source.as_str(), "ptcgl" | "limitless" | "typed") {
            return Err(DeckStoreError::Invalid(format!("unsupported deck source: {}", input.source)));
        }
        if input.raw_text.trim().is_empty() || input.decklist_text.trim().is_empty() {
            return Err(DeckStoreError::Invalid("decklist is required".into()));
        }

        let format_label = input.format_label.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned);
        let archetype_json = input.archetype.as_ref().map(serde_json::to_string).transpose()?;
        let now = now_ms();
        let version_id = new_id("ver");
        let existing = input.id.clone();
        let deck_id = existing.clone().unwrap_or_else(|| new_id("deck"));

        let tx = self.conn.transaction()?;
        if existing.is_some() {
            let archived: Option<Option<i64>> =
                tx.query_row("SELECT archived_at FROM decks WHERE id = ?1", [&deck_id], |row| row.get(0)).optional()?;
            match archived {
                None => return Err(DeckStoreError::NotFound),
                Some(Some(_)) => return Err(DeckStoreError::Invalid("deck is archived".into())),
                Some(None) => {}
            }
        } else {
            tx.execute(
                "INSERT INTO decks (id, name, current_version_id, created_at, updated_at)
                 VALUES (?1, ?2, NULL, ?3, ?3)",
                params![deck_id, name, now],
            )?;
        }

        tx.execute(
            "INSERT INTO deck_versions (
                id, deck_id, source, raw_text, decklist_text, format_label, archetype_json,
                deck_id_hash, list_id_hash, hash_version, catalog_version, catalog_context_id,
                created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                version_id,
                deck_id,
                input.source,
                input.raw_text,
                input.decklist_text,
                format_label,
                archetype_json,
                input.deck_id_hash,
                input.list_id_hash,
                input.hash_version,
                input.catalog_version,
                input.catalog_context_id,
                now,
            ],
        )?;
        tx.execute(
            "UPDATE decks SET name = ?2, current_version_id = ?3, updated_at = ?4 WHERE id = ?1",
            params![deck_id, name, version_id, now],
        )?;
        tx.commit()?;

        self.get(&deck_id)?.ok_or(DeckStoreError::NotFound)
    }

    /// Hide a deck from the picker without breaking match snapshots.
    pub fn archive(&self, id: &str) -> Result<(), DeckStoreError> {
        let changed = self.conn.execute(
            "UPDATE decks SET archived_at = ?2, updated_at = ?2 WHERE id = ?1 AND archived_at IS NULL",
            params![id, now_ms()],
        )?;
        if changed == 0 {
            return Err(DeckStoreError::NotFound);
        }
        Ok(())
    }
}

fn row_to_deck(row: &rusqlite::Row<'_>) -> rusqlite::Result<Deck> {
    let archetype_json: Option<String> = row.get(10)?;
    let archetype = archetype_json
        .map(|json| serde_json::from_str(&json))
        .transpose()
        .map_err(|error| rusqlite::Error::FromSqlConversionFailure(10, rusqlite::types::Type::Text, Box::new(error)))?;

    Ok(Deck {
        id: row.get(0)?,
        name: row.get(1)?,
        created_at: row.get(2)?,
        updated_at: row.get(3)?,
        archived_at: row.get(4)?,
        current_version: DeckVersion {
            id: row.get(5)?,
            source: row.get(6)?,
            raw_text: row.get(7)?,
            decklist_text: row.get(8)?,
            format_label: row.get(9)?,
            archetype,
            deck_id_hash: row.get(11)?,
            list_id_hash: row.get(12)?,
            hash_version: row.get(13)?,
            catalog_version: row.get(14)?,
            catalog_context_id: row.get(15)?,
            created_at: row.get(16)?,
        },
    })
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|duration| duration.as_millis() as i64).unwrap_or_default()
}

fn new_id(prefix: &str) -> String {
    format!("{prefix}-{:032x}", rand::random::<u128>())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempStore {
        path: PathBuf,
        store: DeckStore,
    }

    impl TempStore {
        fn new(tag: &str) -> Self {
            let path = std::env::temp_dir().join(format!("ptcgl-decks-{tag}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&path);
            let store = DeckStore::open(&path.join("decks.sqlite")).unwrap();
            Self { path, store }
        }
    }

    impl Drop for TempStore {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn input(id: Option<String>, name: &str) -> SaveDeckInput {
        SaveDeckInput {
            id,
            name: name.into(),
            source: "ptcgl".into(),
            raw_text: "4 Ultra Ball SVI 196".into(),
            decklist_text: "Trainer: 4\n4 Ultra Ball SVI 196\n\nTotal Cards: 4\n".into(),
            format_label: Some("TEF-30C".into()),
            archetype: None,
            deck_id_hash: Some("sha256:deck".into()),
            list_id_hash: Some("sha256:list".into()),
            hash_version: Some(1),
            catalog_version: Some("2026-10-23".into()),
            catalog_context_id: Some("catalog:test".into()),
        }
    }

    #[test]
    fn creates_a_deck_with_an_immutable_version() {
        let mut temp = TempStore::new("create");
        let deck = temp.store.save(input(None, "League deck")).unwrap();
        assert_eq!(deck.name, "League deck");
        assert_eq!(deck.current_version.format_label.as_deref(), Some("TEF-30C"));
        assert_eq!(deck.current_version.deck_id_hash.as_deref(), Some("sha256:deck"));
        assert_eq!(temp.store.list().unwrap().len(), 1);
    }

    #[test]
    fn edit_adds_a_version_and_moves_the_current_pointer() {
        let mut temp = TempStore::new("edit");
        let first = temp.store.save(input(None, "League deck")).unwrap();
        let second = temp
            .store
            .save(SaveDeckInput {
                id: Some(first.id.clone()),
                decklist_text: "Trainer: 4\n4 Ultra Ball MEG 131\n\nTotal Cards: 4\n".into(),
                ..input(Some(first.id.clone()), "League deck")
            })
            .unwrap();
        assert_eq!(second.id, first.id);
        assert_ne!(second.current_version.id, first.current_version.id);
        assert_eq!(temp.store.list().unwrap().len(), 1);
    }

    #[test]
    fn archived_decks_leave_the_list() {
        let mut temp = TempStore::new("archive");
        let deck = temp.store.save(input(None, "League deck")).unwrap();
        temp.store.archive(&deck.id).unwrap();
        assert!(temp.store.list().unwrap().is_empty());
        assert!(temp.store.get(&deck.id).unwrap().is_none());
    }

    #[test]
    fn rejects_an_unknown_source() {
        let mut temp = TempStore::new("source");
        let error = temp.store.save(SaveDeckInput { source: "ocr".into(), ..input(None, "x") }).unwrap_err();
        assert!(matches!(error, DeckStoreError::Invalid(_)));
    }

    #[test]
    fn empty_format_is_stored_as_null() {
        let mut temp = TempStore::new("format");
        let deck = temp.store.save(SaveDeckInput { format_label: Some("   ".into()), ..input(None, "x") }).unwrap();
        assert_eq!(deck.current_version.format_label, None);
    }
}
