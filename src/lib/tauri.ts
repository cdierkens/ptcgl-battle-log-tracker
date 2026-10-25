import { invoke } from "@tauri-apps/api/core";

export interface ArchetypeRef {
  cards: string[];
  id: null | string;
  label: string;
}

/** A destination paused because it rejected its token. */
export interface Attention {
  destination: string;
  reason: string;
}

export interface CaptureStatusEvent {
  detail: null | string;
  message: string;
  state: string;
  ts: number;
}

export interface Deck {
  archivedAt: null | number;
  createdAt: number;
  currentVersion: DeckVersion;
  id: string;
  name: string;
  updatedAt: number;
}

export interface DeckVersion {
  archetype: ArchetypeRef | null;
  catalogContextId: null | string;
  catalogVersion: null | string;
  createdAt: number;
  deckIdHash: null | string;
  decklistText: string;
  formatLabel: null | string;
  hashVersion: null | number;
  id: string;
  listIdHash: null | string;
  rawText: string;
  source: string;
}

/** One delivery of a saved log to a destination. Held deliveries read as pending. */
export interface DeliveryStatus {
  destination: string;
  lastError: null | string;
  /** The saved log's file name. */
  log: string;
  state: SyncState;
}

export interface Destination {
  enabled: boolean;
  headers: Record<string, string>;
  id: string;
  name: string;
  token_url: null | string;
  url: string;
}
/** The confirmation step for a pasted destination config. */
export interface ImportPreview {
  name: string;
  /** Name of the destination this updates in place. */
  replaces: null | string;
  token: TokenPrompt;
  tokenUrl: null | string;
  url: string;
}

export interface LogSummary {
  createdMs: number;
  fileName: string;
  sizeBytes: number;
}

/** The immutable deck snapshot attached to one saved log. */
export interface MatchDeck {
  archetype: ArchetypeRef | null;
  catalogContextId: null | string;
  catalogVersion: null | string;
  createdAt: number;
  deckId: string;
  deckIdHash: null | string;
  decklistText: string;
  deckName: string;
  deckVersionId: string;
  formatLabel: null | string;
  hashVersion: null | number;
  listIdHash: null | string;
  logFileName: string;
  opponentArchetype: ArchetypeRef | null;
  updatedAt: number;
}

export interface OverlayStateEvent {
  autosaved: boolean;
  path: null | string;
  state: string;
}

export interface SaveDeckInput {
  archetype: ArchetypeRef | null;
  catalogContextId: null | string;
  catalogVersion: null | string;
  deckIdHash: null | string;
  decklistText: string;
  formatLabel: null | string;
  hashVersion: null | number;
  id?: string;
  listIdHash: null | string;
  name: string;
  rawText: string;
  source: string;
}

export interface Settings {
  clipboard_autosave: boolean;
  /** Read-only here: `save_settings` keeps the saved list; use the destination commands. */
  destinations: Destination[];
  output_dir: string;
  player_name: string;
  prompt_after_match: boolean;
}

export type SubmitOutcome = { kind: "alreadySaved" } | { kind: "saved"; path: string };

export type SyncState = "failed" | "pending" | "synced";

export interface SyncStatus {
  attention: Attention[];
  deliveries: DeliveryStatus[];
}

/** `embedded`: the config has a token. `optional`: blank keeps the current token. */
export type TokenPrompt = "embedded" | "optional" | "required";

export const DECKS_CHANGED_EVENT = "decks://changed";
export const OVERLAY_STATE_EVENT = "overlay://state";
export const SETTINGS_CHANGED_EVENT = "settings://changed";
export const SYNC_CHANGED_EVENT = "sync://changed";

export const getSettings = () => invoke<Settings>("get_settings");
export const saveSettings = (s: Settings) => invoke<Settings>("save_settings", { s });
export const listLogs = () => invoke<LogSummary[]>("list_logs");
export const readLog = (fileName: string) => invoke<string>("read_log", { fileName });
export const listDecks = () => invoke<Deck[]>("list_decks");
export const saveDeck = (input: SaveDeckInput) => invoke<Deck>("save_deck", { input });
export const archiveDeck = (id: string) => invoke<void>("archive_deck", { id });
export const getActiveDeck = () => invoke<Deck | null>("get_active_deck");
export const setActiveDeck = (id: null | string) => invoke<void>("set_active_deck", { id });
export const listMatchDecks = () => invoke<MatchDeck[]>("list_match_decks");
export const getMatchDeck = (fileName: string) => invoke<MatchDeck | null>("get_match_deck", { fileName });
export const assignMatchDeck = (fileName: string, deckId: string) =>
  invoke<MatchDeck>("assign_match_deck", { deckId, fileName });
export const clearMatchDeck = (fileName: string) => invoke<void>("clear_match_deck", { fileName });

export const getCaptureState = () => invoke<string>("get_capture_state");
export const submitBattleLog = (text: string) => invoke<SubmitOutcome>("submit_battle_log", { text });
export type ClipboardAccess = "allowed" | "ask" | "default" | "denied";

export const dismissShield = () => invoke<void>("dismiss_shield");
export const getClipboardAccess = () => invoke<ClipboardAccess>("get_clipboard_access");
export const requestClipboardAccess = () => invoke<ClipboardAccess>("request_clipboard_access");
export const deleteLog = (fileName: string) => invoke<void>("delete_log", { fileName });
export const undoLastSave = (path: string) => invoke<void>("undo_last_save", { path });
export const revealLog = (fileName: string) => invoke<void>("reveal_log", { fileName });
export const pickOutputDir = () => invoke<null | string>("pick_output_dir");
export const getDiagnostics = () => invoke<string>("get_diagnostics");
export const isFirstRun = () => invoke<boolean>("is_first_run");
export const setDrawerOpen = (open: boolean) => invoke<void>("set_drawer_open", { open });
export const openLogsFolder = () => invoke<void>("open_logs_folder");
export const openScreenRecordingSettings = () => invoke<void>("open_screen_recording_settings");
export const relaunchApp = () => invoke<void>("relaunch_app");
export const previewDestination = (config: string) => invoke<ImportPreview>("preview_destination", { config });
export const importDestination = (config: string, token: null | string) =>
  invoke<void>("import_destination", { config, token });
export const setDestinationEnabled = (id: string, enabled: boolean) =>
  invoke<void>("set_destination_enabled", { enabled, id });
export const removeDestination = (id: string) => invoke<void>("remove_destination", { id });
export const openTokenPage = (config: string) => invoke<void>("open_token_page", { config });
export const openDestinationTokenPage = (id: string) => invoke<void>("open_destination_token_page", { id });
export const setDestinationToken = (id: string, token: string) =>
  invoke<void>("set_destination_token", { id, token });
export const retryDestination = (id: string) => invoke<void>("retry_destination", { id });
export const getSyncStatus = () => invoke<SyncStatus>("get_sync_status");
export const syncLog = (fileName: string, destinationId: string) =>
  invoke<void>("sync_log", { destinationId, fileName });
export const CAPTURE_STATUS_EVENT = "capture-status";
