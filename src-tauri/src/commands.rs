use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

use crate::clipboard::{ClipboardAccess, ClipboardProbe};
use crate::deck_store::{self, Deck, MatchDeck, SaveDeckInput, SharedDeckStore};
use crate::delivery_queue::{self, DeliveryQueue, Release, SharedQueue};
use crate::destinations::{self, Destination, DestinationError, ImportPreview};
use crate::diagnostics::Diagnostics;
use crate::logs::{self, LogSummary};
use crate::overlay_state::{OverlayEvent, OverlayState};
use crate::platform::clipboard::SystemClipboard;
use crate::platform::permission::ScreenRecordingAccess;
use crate::ports::{OverlayPresenter, ScreenCaptureAccess, SecretStore};
use crate::settings::{self, Settings, SettingsPaths, SharedSettings};
use crate::sink::{LogSink, SubmitError, SubmitOutcome};
use crate::sync_status::{self, SyncStatus, SYNC_CHANGED_EVENT};

#[tauri::command]
pub fn is_first_run(paths: State<'_, SettingsPaths>) -> bool {
    !paths.exists()
}

/// Broadcast so every window reflects the latest settings.
pub const SETTINGS_CHANGED_EVENT: &str = "settings://changed";
/// Deck library, active deck, or match-snapshot changes. Both windows refresh.
pub const DECKS_CHANGED_EVENT: &str = "decks://changed";
/// The drawer asks the main window to show its Decks tab.
pub const OPEN_DECKS_EVENT: &str = "decks://open";

#[tauri::command]
pub fn get_settings(settings: State<'_, SharedSettings>) -> Settings {
    settings::current(&settings)
}

/// Held across every settings read-modify-write so concurrent commands can't drop each other's changes.
static SETTINGS_WRITE: Mutex<()> = Mutex::new(());

fn settings_write() -> std::sync::MutexGuard<'static, ()> {
    SETTINGS_WRITE.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Destinations are left as they are; only the destination commands change them.
/// Returns what was saved.
#[tauri::command]
pub fn save_settings(s: Settings, app: AppHandle) -> Result<Settings, String> {
    let _write = settings_write();
    let destinations = settings::current(&app.state::<SharedSettings>()).destinations;
    let next = Settings { destinations, ..s };
    apply_settings(&app, next.clone())?;
    Ok(next)
}

/// The one write path for settings (Settings tab, setup flow): persist, share, broadcast.
pub fn apply_settings(app: &AppHandle, next: Settings) -> Result<(), String> {
    app.state::<SettingsPaths>().save(&next)?;
    settings::replace(&app.state::<SharedSettings>(), next.clone());
    if let Err(e) = app.emit(SETTINGS_CHANGED_EVENT, &next) {
        tracing::warn!("failed to emit settings changed: {e}");
    }
    Ok(())
}

/// Runs `change` on the destination list with the keychain and queue, then saves settings.
fn change_destinations<T>(
    app: &AppHandle,
    change: impl FnOnce(&mut Vec<Destination>, &dyn SecretStore, &DeliveryQueue) -> Result<T, DestinationError>,
) -> Result<T, String> {
    let _write = settings_write();
    let mut next = settings::current(&app.state::<SharedSettings>());
    let secrets = app.state::<Arc<dyn SecretStore>>();
    let shared_queue = app.state::<SharedQueue>();
    let out = {
        let queue = delivery_queue::lock(&shared_queue);
        change(&mut next.destinations, secrets.inner().as_ref(), &queue).map_err(|e| e.to_string())?
    };
    apply_settings(app, next)?;
    sync_changed(app);
    Ok(out)
}

/// Validates a pasted destination config for the confirmation step. Saves nothing.
#[tauri::command]
pub fn preview_destination(
    config: String,
    settings: State<'_, SharedSettings>,
    secrets: State<'_, Arc<dyn SecretStore>>,
) -> Result<ImportPreview, String> {
    let parsed = destinations::parse_config(&config).map_err(|e| e.to_string())?;
    destinations::preview(&parsed, &settings::current(&settings).destinations, secrets.inner().as_ref())
        .map_err(|e| e.to_string())
}

/// Saves a confirmed config; a URL already configured is updated in place.
#[tauri::command]
pub fn import_destination(config: String, token: Option<String>, app: AppHandle) -> Result<(), String> {
    let parsed = destinations::parse_config(&config).map_err(|e| e.to_string())?;
    change_destinations(&app, |list, secrets, queue| {
        destinations::import(list, secrets, queue, parsed, token.as_deref()).map(|_| ())
    })
}

#[tauri::command]
pub fn set_destination_enabled(id: String, enabled: bool, app: AppHandle) -> Result<(), String> {
    change_destinations(&app, |list, _, _| destinations::set_enabled(list, &id, enabled))
}

#[tauri::command]
pub fn remove_destination(
    id: String,
    app: AppHandle,
    queue: State<'_, SharedQueue>,
    secrets: State<'_, Arc<dyn SecretStore>>,
) -> Result<(), String> {
    change_destinations(&app, |list, _, _| destinations::remove(list, &id))?;
    destinations::forget(secrets.inner().as_ref(), &delivery_queue::lock(&queue), &id).map_err(|e| e.to_string())
}

/// Opens the token page named by a pasted config; the webview can't pick an arbitrary URL.
#[tauri::command]
pub fn open_token_page(config: String, app: AppHandle) -> Result<(), String> {
    let parsed = destinations::parse_config(&config).map_err(|e| e.to_string())?;
    let url = parsed.token_url.ok_or("This config has no token page.")?;
    app.opener().open_url(url, None::<&str>).map_err(|e| e.to_string())
}

/// Opens a configured destination's token page, for pasting a replacement token.
#[tauri::command]
pub fn open_destination_token_page(id: String, app: AppHandle) -> Result<(), String> {
    let settings = settings::current(&app.state::<SharedSettings>());
    let destination = destinations::find(&settings.destinations, &id).map_err(|e| e.to_string())?;
    let url = destination.token_url.clone().ok_or("This destination has no token page.")?;
    app.opener().open_url(url, None::<&str>).map_err(|e| e.to_string())
}

/// Saves a replacement token for a destination that needs attention, which resumes it.
#[tauri::command]
pub fn set_destination_token(id: String, token: String, app: AppHandle) -> Result<(), String> {
    {
        // Under the settings lock so a concurrent remove can't orphan the keychain entry.
        let _write = settings_write();
        let settings = settings::current(&app.state::<SharedSettings>());
        let secrets = app.state::<Arc<dyn SecretStore>>();
        let shared_queue = app.state::<SharedQueue>();
        let queue = delivery_queue::lock(&shared_queue);
        destinations::replace_token(&settings.destinations, secrets.inner().as_ref(), &queue, &id, &token)
            .map_err(|e| e.to_string())?;
    }
    sync_changed(&app);
    Ok(())
}

/// Resumes a paused destination with its current token.
#[tauri::command]
pub fn retry_destination(id: String, app: AppHandle) -> Result<(), String> {
    delivery_queue::lock(&app.state::<SharedQueue>()).clear_attention(&id).map_err(|e| e.to_string())?;
    sync_changed(&app);
    Ok(())
}

#[tauri::command]
pub fn get_sync_status(queue: State<'_, SharedQueue>) -> Result<SyncStatus, String> {
    sync_status::read(&delivery_queue::lock(&queue)).map_err(|e| e.to_string())
}

fn emit_decks_changed(app: &AppHandle) {
    if let Err(e) = app.emit(DECKS_CHANGED_EVENT, ()) {
        tracing::warn!("failed to emit decks changed: {e}");
    }
}

#[tauri::command]
pub fn list_decks(store: State<'_, SharedDeckStore>) -> Result<Vec<Deck>, String> {
    deck_store::lock(&store).list().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_deck(input: SaveDeckInput, store: State<'_, SharedDeckStore>, app: AppHandle) -> Result<Deck, String> {
    let deck = deck_store::lock(&store).save(input).map_err(|e| e.to_string())?;
    emit_decks_changed(&app);
    Ok(deck)
}

#[tauri::command]
pub fn archive_deck(id: String, store: State<'_, SharedDeckStore>, app: AppHandle) -> Result<(), String> {
    deck_store::lock(&store).archive(&id).map_err(|e| e.to_string())?;
    emit_decks_changed(&app);
    Ok(())
}

#[tauri::command]
pub fn get_active_deck(store: State<'_, SharedDeckStore>) -> Result<Option<Deck>, String> {
    deck_store::lock(&store).active_deck().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_active_deck(id: Option<String>, store: State<'_, SharedDeckStore>, app: AppHandle) -> Result<(), String> {
    deck_store::lock(&store).set_active_deck(id.as_deref()).map_err(|e| e.to_string())?;
    emit_decks_changed(&app);
    Ok(())
}

#[tauri::command]
pub fn list_match_decks(store: State<'_, SharedDeckStore>) -> Result<Vec<MatchDeck>, String> {
    deck_store::lock(&store).list_match_decks().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_match_deck(file_name: String, store: State<'_, SharedDeckStore>) -> Result<Option<MatchDeck>, String> {
    deck_store::lock(&store).match_deck(&file_name).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn assign_match_deck(
    file_name: String,
    deck_id: String,
    store: State<'_, SharedDeckStore>,
    app: AppHandle,
) -> Result<MatchDeck, String> {
    let snapshot = deck_store::lock(&store).assign_match_deck(&file_name, &deck_id).map_err(|e| e.to_string())?;
    emit_decks_changed(&app);
    Ok(snapshot)
}

#[tauri::command]
pub fn clear_match_deck(file_name: String, store: State<'_, SharedDeckStore>, app: AppHandle) -> Result<(), String> {
    deck_store::lock(&store).clear_match_deck(&file_name).map_err(|e| e.to_string())?;
    emit_decks_changed(&app);
    Ok(())
}

/// Bring the main window forward on its Decks tab. The drawer cannot show the
/// full library, so its match-detail edit link delegates here.
#[tauri::command]
pub fn open_decks(app: AppHandle) {
    crate::show_main(&app);
    if let Err(e) = app.emit(OPEN_DECKS_EVENT, ()) {
        tracing::warn!("failed to emit open decks: {e}");
    }
}

/// Sends one saved log to a destination now: a first sync for an older match, or a retry.
#[tauri::command]
pub fn sync_log(file_name: String, destination_id: String, app: AppHandle) -> Result<(), String> {
    let settings = settings::current(&app.state::<SharedSettings>());
    let destination = destinations::find(&settings.destinations, &destination_id).map_err(|e| e.to_string())?;
    if !destination.enabled {
        return Err(format!("{} is turned off in Settings.", destination.name));
    }
    logs::resolve_log_path(&PathBuf::from(&settings.output_dir), &file_name)?;
    delivery_queue::lock(&app.state::<SharedQueue>())
        .enqueue(&file_name, &destination_id, Release::Now)
        .map_err(|e| e.to_string())?;
    sync_changed(&app);
    Ok(())
}

pub fn sync_changed(app: &AppHandle) {
    if let Err(e) = app.emit(SYNC_CHANGED_EVENT, ()) {
        tracing::warn!("failed to emit sync changed: {e}");
    }
}

fn output_dir(settings: &SharedSettings) -> PathBuf {
    PathBuf::from(settings::current(settings).output_dir)
}

#[tauri::command]
pub fn list_logs(settings: State<'_, SharedSettings>) -> Result<Vec<LogSummary>, String> {
    logs::list_logs(&output_dir(&settings))
}

#[tauri::command]
pub fn read_log(file_name: String, settings: State<'_, SharedSettings>) -> Result<String, String> {
    logs::read_log(&output_dir(&settings), &file_name)
}

#[tauri::command]
pub fn get_capture_state(state: State<'_, Arc<Mutex<OverlayState>>>) -> String {
    match state.lock() {
        Ok(s) => s.name().to_string(),
        Err(_) => OverlayState::NoWindow.name().to_string(),
    }
}

#[tauri::command]
pub fn submit_battle_log(
    text: String,
    sink: State<'_, Arc<dyn LogSink>>,
    events: State<'_, Sender<OverlayEvent>>,
) -> Result<SubmitOutcome, SubmitError> {
    let outcome = sink.submit(&text)?;
    if let Some(event) = shield_event(&outcome) {
        // A closed channel means the engine thread has exited; the log is saved regardless.
        let _ = events.send(event);
    }
    Ok(outcome)
}

/// Which overlay event a submit outcome should raise.
fn shield_event(outcome: &SubmitOutcome) -> Option<OverlayEvent> {
    match outcome {
        SubmitOutcome::Saved { .. } => Some(OverlayEvent::LogSaved),
        SubmitOutcome::AlreadySaved => None,
    }
}

#[tauri::command]
pub fn get_clipboard_access() -> ClipboardAccess {
    SystemClipboard.access()
}

/// Reads once, discarding the text, so macOS shows its paste alert now — while the player is
/// turning the setting on — instead of mid-match. Async: off the main thread while it waits.
#[tauri::command]
pub async fn request_clipboard_access() -> ClipboardAccess {
    let _ = SystemClipboard.read_text();
    SystemClipboard.access()
}

#[tauri::command]
pub fn dismiss_shield(events: State<'_, Sender<OverlayEvent>>) {
    let _ = events.send(OverlayEvent::Dismissed);
}

/// Also drops held deliveries: a log trashed before it could be undone is never sent.
fn trash_and_forget(
    settings: &SharedSettings,
    sink: &dyn LogSink,
    queue: &SharedQueue,
    decks: &SharedDeckStore,
    file_name: &str,
) -> Result<(), String> {
    let text = logs::trash_log(&output_dir(settings), file_name)?;
    sink.forget(&text);
    if let Err(e) = delivery_queue::lock(queue).drop_held(file_name) {
        tracing::warn!("sync: couldn't drop held deliveries for {file_name}: {e}");
    }
    if let Err(e) = deck_store::lock(decks).clear_match_deck(file_name) {
        tracing::warn!("deck: couldn't clear match snapshot for {file_name}: {e}");
    }
    Ok(())
}

#[tauri::command]
pub fn delete_log(
    file_name: String,
    settings: State<'_, SharedSettings>,
    sink: State<'_, Arc<dyn LogSink>>,
    queue: State<'_, SharedQueue>,
    decks: State<'_, SharedDeckStore>,
    app: AppHandle,
) -> Result<(), String> {
    trash_and_forget(settings.inner(), sink.inner().as_ref(), queue.inner(), decks.inner(), &file_name)?;
    emit_decks_changed(&app);
    Ok(())
}

/// Undo the latest save: trash it, forget it, and put the shield back up.
#[tauri::command]
pub fn undo_last_save(
    path: String,
    settings: State<'_, SharedSettings>,
    sink: State<'_, Arc<dyn LogSink>>,
    queue: State<'_, SharedQueue>,
    decks: State<'_, SharedDeckStore>,
    events: State<'_, Sender<OverlayEvent>>,
    app: AppHandle,
) -> Result<(), String> {
    let file_name = logs::file_name_of(&path)?;
    trash_and_forget(settings.inner(), sink.inner().as_ref(), queue.inner(), decks.inner(), &file_name)?;
    emit_decks_changed(&app);
    let _ = events.send(OverlayEvent::SaveUndone);
    Ok(())
}

#[tauri::command]
pub fn reveal_log(file_name: String, settings: State<'_, SharedSettings>, app: AppHandle) -> Result<(), String> {
    let path = logs::resolve_log_path(&output_dir(&settings), &file_name)?;
    app.opener().reveal_item_in_dir(path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn open_logs_folder(settings: State<'_, SharedSettings>, app: AppHandle) -> Result<(), String> {
    let dir = output_dir(&settings);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    app.opener().open_path(dir.to_string_lossy(), None::<&str>).map_err(|e| e.to_string())
}

const SCREEN_RECORDING_SETTINGS_URL: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture";

#[tauri::command]
pub fn open_screen_recording_settings(app: AppHandle) -> Result<(), String> {
    app.opener().open_url(SCREEN_RECORDING_SETTINGS_URL, None::<&str>).map_err(|e| e.to_string())
}

/// macOS applies a new Screen Recording grant only after a relaunch.
#[tauri::command]
pub fn relaunch_app(app: AppHandle) {
    app.restart();
}

/// Native folder picker starting at the current folder; `None` when cancelled. The UI saves the
/// choice through `save_settings`. Async so the blocking dialog waits off the main thread.
#[tauri::command]
pub async fn pick_output_dir(app: AppHandle) -> Option<String> {
    let current = settings::current(&app.state::<SharedSettings>()).output_dir;
    app.dialog()
        .file()
        .set_directory(current)
        .blocking_pick_folder()
        .and_then(|picked| picked.into_path().ok())
        .map(|path| path.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn get_diagnostics(
    app: AppHandle,
    state: State<'_, Arc<Mutex<OverlayState>>>,
    settings: State<'_, SharedSettings>,
) -> String {
    let current = settings::current(&settings);
    Diagnostics {
        version: app.package_info().version.to_string(),
        os: format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
        state: state.lock().map(|s| s.name().to_string()).unwrap_or_else(|_| "unknown".into()),
        screen_recording: ScreenRecordingAccess.is_authorized(),
        clipboard_access: SystemClipboard.access(),
        prompt_after_match: current.prompt_after_match,
        clipboard_autosave: current.clipboard_autosave,
        saved_logs: logs::list_logs(&PathBuf::from(&current.output_dir)).map(|logs| logs.len()),
    }
    .report()
}

#[tauri::command]
pub fn set_drawer_open(open: bool, presenter: State<'_, Arc<dyn OverlayPresenter>>) -> Result<(), String> {
    presenter.set_drawer_open(open).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_new_save_lowers_the_shield() {
        assert_eq!(shield_event(&SubmitOutcome::Saved { path: "/x".into() }), Some(OverlayEvent::LogSaved));
        // A re-paste (e.g. the previous match's log still on the clipboard) must not.
        assert_eq!(shield_event(&SubmitOutcome::AlreadySaved), None);
    }
}
