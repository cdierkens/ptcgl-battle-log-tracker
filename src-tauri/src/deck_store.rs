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

const SCHEMA_VERSION: i32 = 2;
const FILE_NAME: &str = "decks.sqlite";
const ACTIVE_DECK_KEY: &str = "active_deck_id";
const MATCH_DECK_COLUMNS: &str = "m.log_file_name, m.deck_id, m.deck_version_id, m.deck_name,
    m.decklist_text, m.format_label, m.archetype_id, m.archetype_label, m.archetype_cards,
    m.opponent_archetype_label, m.opponent_archetype_cards, m.deck_id_hash, m.list_id_hash,
    m.hash_version, m.catalog_version, m.catalog_context_id, m.created_at, m.updated_at";

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

/// An immutable deck snapshot attached to one saved log. Everything, including
/// the name, is copied at assignment time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchDeck {
    pub log_file_name: String,
    pub deck_id: String,
    pub deck_version_id: String,
    pub deck_name: String,
    pub decklist_text: String,
    pub format_label: Option<String>,
    pub archetype: Option<ArchetypeRef>,
    pub opponent_archetype: Option<ArchetypeRef>,
    pub deck_id_hash: Option<String>,
    pub list_id_hash: Option<String>,
    pub hash_version: Option<i64>,
    pub catalog_version: Option<String>,
    pub catalog_context_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
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
    #[error("deck library is unavailable")]
    Unavailable,
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
    persistent: bool,
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
        Self::init(Connection::open(path)?, true)
    }

    /// Fallback when the file can't be opened. Commands report the library as
    /// unavailable; log capture never depends on it.
    pub fn open_in_memory() -> Result<Self, DeckStoreError> {
        Self::init(Connection::open_in_memory()?, false)
    }

    fn init(conn: Connection, persistent: bool) -> Result<Self, DeckStoreError> {
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
            CREATE TABLE IF NOT EXISTS match_decks (
                log_file_name TEXT PRIMARY KEY,
                deck_id TEXT NOT NULL REFERENCES decks(id),
                deck_version_id TEXT NOT NULL REFERENCES deck_versions(id),
                deck_name TEXT NOT NULL,
                decklist_text TEXT NOT NULL,
                format_label TEXT,
                archetype_id TEXT,
                archetype_label TEXT,
                archetype_cards TEXT,
                opponent_archetype_label TEXT,
                opponent_archetype_cards TEXT,
                deck_id_hash TEXT,
                list_id_hash TEXT,
                hash_version INTEGER,
                catalog_version TEXT,
                catalog_context_id TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS meta (
                key TEXT PRIMARY KEY,
                value TEXT
            );
            PRAGMA user_version = {SCHEMA_VERSION};"
        ))?;
        Ok(Self { conn, persistent })
    }

    fn require_persistent(&self) -> Result<(), DeckStoreError> {
        if self.persistent {
            Ok(())
        } else {
            Err(DeckStoreError::Unavailable)
        }
    }

    /// List active (non-archived) decks, most recently updated first.
    pub fn list(&self) -> Result<Vec<Deck>, DeckStoreError> {
        self.require_persistent()?;
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
        self.require_persistent()?;
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

    fn active_deck_id(&self) -> Result<Option<String>, DeckStoreError> {
        let value: Option<Option<String>> = self
            .conn
            .query_row("SELECT value FROM meta WHERE key = ?1", [ACTIVE_DECK_KEY], |row| row.get(0))
            .optional()?;
        Ok(value.flatten())
    }

    /// The one active deck, if it still exists and has not been archived.
    pub fn active_deck(&self) -> Result<Option<Deck>, DeckStoreError> {
        self.require_persistent()?;
        let Some(id) = self.active_deck_id()? else { return Ok(None) };
        let deck = self.get(&id)?;
        if deck.is_none() {
            // The deck was archived or removed outside this code path.
            self.clear_active_deck()?;
        }
        Ok(deck)
    }

    /// `None` means **No deck**: saves continue without a deck snapshot.
    pub fn set_active_deck(&self, id: Option<&str>) -> Result<(), DeckStoreError> {
        self.require_persistent()?;
        match id {
            None => {
                self.clear_active_deck()?;
                Ok(())
            }
            Some(id) if id.trim().is_empty() => Err(DeckStoreError::Invalid("deck id is required".into())),
            Some(id) => {
                self.get(id)?.ok_or(DeckStoreError::NotFound)?;
                self.conn.execute(
                    "INSERT INTO meta (key, value) VALUES (?1, ?2)
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                    params![ACTIVE_DECK_KEY, id],
                )?;
                Ok(())
            }
        }
    }

    fn clear_active_deck(&self) -> Result<(), DeckStoreError> {
        self.conn.execute("DELETE FROM meta WHERE key = ?1", [ACTIVE_DECK_KEY])?;
        Ok(())
    }

    /// All match snapshots, newest first. Archived decks keep naming their old
    /// matches because the name is part of the snapshot.
    pub fn list_match_decks(&self) -> Result<Vec<MatchDeck>, DeckStoreError> {
        self.require_persistent()?;
        let sql = format!(
            "SELECT {MATCH_DECK_COLUMNS}
             FROM match_decks m
             ORDER BY m.created_at DESC, m.log_file_name DESC"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], row_to_match_deck)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn match_deck(&self, file_name: &str) -> Result<Option<MatchDeck>, DeckStoreError> {
        self.require_persistent()?;
        let sql = format!(
            "SELECT {MATCH_DECK_COLUMNS}
             FROM match_decks m
             WHERE m.log_file_name = ?1"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        stmt.query_row([file_name], row_to_match_deck).optional().map_err(Into::into)
    }

    /// Snapshot the deck's current version onto a saved log. Re-assigning
    /// replaces the snapshot with the current version; old version rows remain.
    pub fn assign_match_deck(&mut self, file_name: &str, deck_id: &str) -> Result<MatchDeck, DeckStoreError> {
        self.require_persistent()?;
        let deck = self.get(deck_id)?.ok_or(DeckStoreError::NotFound)?;
        self.write_match_deck(file_name, &deck)
    }

    /// Associate the active deck after a save. No active deck means no row.
    /// A database failure is the caller's to log; it must not fail the save.
    pub fn assign_active_match(&mut self, file_name: &str) -> Result<Option<MatchDeck>, DeckStoreError> {
        self.require_persistent()?;
        let Some(deck_id) = self.active_deck_id()? else { return Ok(None) };
        match self.get(&deck_id)? {
            Some(deck) => self.write_match_deck(file_name, &deck).map(Some),
            None => {
                self.clear_active_deck()?;
                Ok(None)
            }
        }
    }

    fn write_match_deck(&self, file_name: &str, deck: &Deck) -> Result<MatchDeck, DeckStoreError> {
        let version = &deck.current_version;
        let archetype = version.archetype.as_ref();
        let archetype_cards = archetype.map(|a| serde_json::to_string(&a.cards)).transpose()?;
        let now = now_ms();
        self.conn.execute(
            "INSERT INTO match_decks (
                log_file_name, deck_id, deck_version_id, deck_name, decklist_text, format_label,
                archetype_id, archetype_label, archetype_cards,
                opponent_archetype_label, opponent_archetype_cards,
                deck_id_hash, list_id_hash, hash_version, catalog_version,
                catalog_context_id, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL, NULL, ?10, ?11, ?12, ?13, ?14, ?15, ?15)
             ON CONFLICT(log_file_name) DO UPDATE SET
                deck_id = excluded.deck_id,
                deck_version_id = excluded.deck_version_id,
                deck_name = excluded.deck_name,
                decklist_text = excluded.decklist_text,
                format_label = excluded.format_label,
                archetype_id = excluded.archetype_id,
                archetype_label = excluded.archetype_label,
                archetype_cards = excluded.archetype_cards,
                opponent_archetype_label = COALESCE(excluded.opponent_archetype_label, match_decks.opponent_archetype_label),
                opponent_archetype_cards = COALESCE(excluded.opponent_archetype_cards, match_decks.opponent_archetype_cards),
                deck_id_hash = excluded.deck_id_hash,
                list_id_hash = excluded.list_id_hash,
                hash_version = excluded.hash_version,
                catalog_version = excluded.catalog_version,
                catalog_context_id = excluded.catalog_context_id,
                updated_at = excluded.updated_at",
            params![
                file_name,
                &deck.id,
                &version.id,
                &deck.name,
                &version.decklist_text,
                version.format_label.as_deref(),
                archetype.and_then(|a| a.id.as_deref()),
                archetype.map(|a| a.label.as_str()),
                archetype_cards,
                version.deck_id_hash.as_deref(),
                version.list_id_hash.as_deref(),
                version.hash_version,
                version.catalog_version.as_deref(),
                version.catalog_context_id.as_deref(),
                now,
            ],
        )?;
        self.match_deck(file_name)?.ok_or(DeckStoreError::NotFound)
    }

    /// Undo and Trash drop the match's snapshot; a later save creates a new one.
    pub fn clear_match_deck(&self, file_name: &str) -> Result<(), DeckStoreError> {
        self.require_persistent()?;
        self.conn.execute("DELETE FROM match_decks WHERE log_file_name = ?1", [file_name])?;
        Ok(())
    }

    /// Create a deck or add an immutable version to an existing deck.
    pub fn save(&mut self, input: SaveDeckInput) -> Result<Deck, DeckStoreError> {
        self.require_persistent()?;
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
        self.require_persistent()?;
        let changed = self.conn.execute(
            "UPDATE decks SET archived_at = ?2, updated_at = ?2 WHERE id = ?1 AND archived_at IS NULL",
            params![id, now_ms()],
        )?;
        if changed == 0 {
            return Err(DeckStoreError::NotFound);
        }
        if self.active_deck_id()?.as_deref() == Some(id) {
            self.clear_active_deck()?;
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

fn row_to_match_deck(row: &rusqlite::Row<'_>) -> rusqlite::Result<MatchDeck> {
    Ok(MatchDeck {
        log_file_name: row.get(0)?,
        deck_id: row.get(1)?,
        deck_version_id: row.get(2)?,
        deck_name: row.get(3)?,
        decklist_text: row.get(4)?,
        format_label: row.get(5)?,
        archetype: archetype_from_columns(row, Some(6), 7, 8)?,
        opponent_archetype: opponent_from_columns(row, 9, 10)?,
        deck_id_hash: row.get(11)?,
        list_id_hash: row.get(12)?,
        hash_version: row.get(13)?,
        catalog_version: row.get(14)?,
        catalog_context_id: row.get(15)?,
        created_at: row.get(16)?,
        updated_at: row.get(17)?,
    })
}

fn archetype_from_columns(
    row: &rusqlite::Row<'_>,
    id_column: Option<usize>,
    label_column: usize,
    cards_column: usize,
) -> rusqlite::Result<Option<ArchetypeRef>> {
    let Some(label) = row.get::<_, Option<String>>(label_column)? else { return Ok(None) };
    let id = match id_column {
        Some(column) => row.get::<_, Option<String>>(column)?,
        None => None,
    };
    Ok(Some(ArchetypeRef { id, label, cards: cards_from_column(row, cards_column)? }))
}

fn opponent_from_columns(
    row: &rusqlite::Row<'_>,
    label_column: usize,
    cards_column: usize,
) -> rusqlite::Result<Option<ArchetypeRef>> {
    let Some(label) = row.get::<_, Option<String>>(label_column)? else { return Ok(None) };
    Ok(Some(ArchetypeRef { id: None, label, cards: cards_from_column(row, cards_column)? }))
}

fn cards_from_column(row: &rusqlite::Row<'_>, column: usize) -> rusqlite::Result<Vec<String>> {
    let cards_json: Option<String> = row.get(column)?;
    cards_json.map(|json| serde_json::from_str(&json)).transpose().map(|cards| cards.unwrap_or_default()).map_err(
        |error| rusqlite::Error::FromSqlConversionFailure(column, rusqlite::types::Type::Text, Box::new(error)),
    )
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
    fn in_memory_fallback_reports_the_library_unavailable() {
        let mut store = DeckStore::open_in_memory().unwrap();
        assert!(matches!(store.list(), Err(DeckStoreError::Unavailable)));
        assert!(matches!(store.save(input(None, "x")), Err(DeckStoreError::Unavailable)));
    }

    #[test]
    fn empty_format_is_stored_as_null() {
        let mut temp = TempStore::new("format");
        let deck = temp.store.save(SaveDeckInput { format_label: Some("   ".into()), ..input(None, "x") }).unwrap();
        assert_eq!(deck.current_version.format_label, None);
    }

    #[test]
    fn active_deck_round_trips_and_archiving_clears_it() {
        let mut temp = TempStore::new("active");
        let deck = temp.store.save(input(None, "League deck")).unwrap();

        assert!(temp.store.active_deck().unwrap().is_none());
        assert!(matches!(temp.store.set_active_deck(Some("missing")), Err(DeckStoreError::NotFound)));
        temp.store.set_active_deck(Some(&deck.id)).unwrap();
        assert_eq!(temp.store.active_deck().unwrap().map(|active| active.id), Some(deck.id.clone()));

        temp.store.archive(&deck.id).unwrap();
        assert!(temp.store.active_deck().unwrap().is_none());
    }

    #[test]
    fn a_match_snapshot_stays_put_when_the_deck_changes() {
        let mut temp = TempStore::new("snapshot");
        let first = temp.store.save(input(None, "League deck")).unwrap();
        temp.store.set_active_deck(Some(&first.id)).unwrap();

        let snapshot = temp.store.assign_active_match("ptcgl-1.txt").unwrap().unwrap();
        assert_eq!(snapshot.deck_name, "League deck");
        assert_eq!(snapshot.deck_version_id, first.current_version.id);
        assert_eq!(snapshot.decklist_text, first.current_version.decklist_text);

        let edited = temp
            .store
            .save(SaveDeckInput {
                id: Some(first.id.clone()),
                decklist_text: "Trainer: 4\n4 Ultra Ball MEG 131\n\nTotal Cards: 4\n".into(),
                ..input(Some(first.id.clone()), "League deck")
            })
            .unwrap();
        let old = temp.store.match_deck("ptcgl-1.txt").unwrap().unwrap();
        assert_eq!(old.deck_version_id, first.current_version.id);
        assert_eq!(old.decklist_text, first.current_version.decklist_text);

        let reassigned = temp.store.assign_match_deck("ptcgl-1.txt", &edited.id).unwrap();
        assert_eq!(reassigned.deck_version_id, edited.current_version.id);
        assert_eq!(reassigned.decklist_text, edited.current_version.decklist_text);
    }

    #[test]
    fn a_save_without_an_active_deck_has_no_snapshot() {
        let mut temp = TempStore::new("no-active");
        temp.store.save(input(None, "League deck")).unwrap();

        assert!(temp.store.assign_active_match("ptcgl-2.txt").unwrap().is_none());
        assert!(temp.store.match_deck("ptcgl-2.txt").unwrap().is_none());
    }

    #[test]
    fn clearing_a_match_snapshot_does_not_touch_the_deck() {
        let mut temp = TempStore::new("clear-match");
        let deck = temp.store.save(input(None, "League deck")).unwrap();
        temp.store.set_active_deck(Some(&deck.id)).unwrap();
        temp.store.assign_active_match("ptcgl-3.txt").unwrap().unwrap();

        temp.store.clear_match_deck("ptcgl-3.txt").unwrap();
        assert!(temp.store.match_deck("ptcgl-3.txt").unwrap().is_none());
        assert_eq!(temp.store.list_match_decks().unwrap().len(), 0);
        assert_eq!(temp.store.list().unwrap().len(), 1);
    }

    #[test]
    fn a_snapshot_keeps_the_confirmed_archetype_cards() {
        let mut temp = TempStore::new("archetype-snapshot");
        let archetype = ArchetypeRef {
            id: Some("limitless:284/3".into()),
            label: "Dragapult Dusknoir".into(),
            cards: vec!["dragapult".into(), "dusknoir".into()],
        };
        let deck = temp
            .store
            .save(SaveDeckInput { archetype: Some(archetype.clone()), ..input(None, "League deck") })
            .unwrap();

        let snapshot = temp.store.assign_match_deck("ptcgl-5.txt", &deck.id).unwrap();
        assert_eq!(snapshot.archetype, Some(archetype));
        assert_eq!(snapshot.opponent_archetype, None);
    }

    #[test]
    fn match_snapshots_keep_the_name_they_were_saved_with() {
        let mut temp = TempStore::new("snapshot-name");
        let deck = temp.store.save(input(None, "Old league deck")).unwrap();
        temp.store.assign_match_deck("ptcgl-4.txt", &deck.id).unwrap();
        temp.store
            .save(SaveDeckInput { id: Some(deck.id.clone()), ..input(Some(deck.id.clone()), "Renamed deck") })
            .unwrap();

        let snapshot = temp.store.match_deck("ptcgl-4.txt").unwrap().unwrap();
        assert_eq!(snapshot.deck_name, "Old league deck");

        temp.store.archive(&deck.id).unwrap();
        let snapshots = temp.store.list_match_decks().unwrap();
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].deck_name, "Old league deck");
    }

    #[test]
    fn adds_slice_two_tables_to_an_existing_version_one_database() {
        let dir = std::env::temp_dir().join(format!("ptcgl-decks-v1-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("decks.sqlite");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "CREATE TABLE decks (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    current_version_id TEXT,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL,
                    archived_at INTEGER
                 );
                 CREATE TABLE deck_versions (
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
                 PRAGMA user_version = 1;",
            )
            .unwrap();
        }

        let mut store = DeckStore::open(&path).unwrap();
        let deck = store.save(input(None, "League deck")).unwrap();
        store.set_active_deck(Some(&deck.id)).unwrap();
        assert_eq!(store.active_deck().unwrap().map(|active| active.id), Some(deck.id));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn in_memory_fallback_cannot_store_active_or_match_state() {
        let mut store = DeckStore::open_in_memory().unwrap();
        assert!(matches!(store.set_active_deck(None), Err(DeckStoreError::Unavailable)));
        assert!(matches!(store.assign_active_match("x.txt"), Err(DeckStoreError::Unavailable)));
        assert!(matches!(store.clear_match_deck("x.txt"), Err(DeckStoreError::Unavailable)));
    }
}
