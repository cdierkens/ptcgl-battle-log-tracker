//! Listing and reading captured battle-log files from the output directory.
//! Pure filesystem logic, kept separate from the Tauri command wrappers so it
//! is directly unit-testable. This module is the security boundary for
//! webview-supplied filenames, so every read path rejects path traversal and
//! symlinks.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::logname::parse_log_file_name;

/// Metadata for one captured battle-log file, sent to the UI.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogSummary {
    pub file_name: String,
    pub size_bytes: u64,
    pub created_ms: u64,
}

/// Convert a nanosecond epoch timestamp to milliseconds.
fn nanos_to_ms(nanos: u64) -> u64 {
    nanos / 1_000_000
}

/// Extract the created-time (ms) encoded in a `ptcgl-<nanos>.txt` filename.
fn nanos_to_name_created_ms(file_name: &str) -> Option<u64> {
    parse_log_file_name(file_name).map(nanos_to_ms)
}

/// List captured logs in `dir`, newest first. A missing directory yields an
/// empty list (nothing captured yet), not an error. Only regular files are
/// listed; directories and symlinks are ignored (read_log refuses symlinks).
pub fn list_logs(dir: &Path) -> Result<Vec<LogSummary>, String> {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.to_string()),
    };
    let mut logs = Vec::new();
    for entry in entries.flatten() {
        let file_name = entry.file_name().to_string_lossy().to_string();
        let Some(created_ms) = nanos_to_name_created_ms(&file_name) else {
            continue; // not a capture file
        };
        // Regular files only, using the no-follow file type so a symlink named
        // like a capture file is never listed. Entries that vanish or become
        // unreadable mid-listing are skipped, not reported as phantom 0-byte logs.
        if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            continue;
        }
        let size_bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
        logs.push(LogSummary { file_name, size_bytes, created_ms });
    }
    logs.sort_by_key(|a| std::cmp::Reverse(a.created_ms)); // newest first
    Ok(logs)
}

/// Resolve a bare log filename inside `dir`, rejecting path traversal.
///
/// The name must be a single path component (no `/`, `\`, or `..`) and the
/// canonicalized result must stay within the canonicalized output directory.
/// Because `fs::canonicalize` requires the file to exist, a missing file
/// surfaces as a clean "not found" error rather than as an escape.
pub fn resolve_log_path(dir: &Path, file_name: &str) -> Result<PathBuf, String> {
    if file_name.is_empty() || file_name.contains('/') || file_name.contains('\\') || file_name.contains("..") {
        return Err(format!("invalid log file name: {file_name}"));
    }
    let path = dir.join(file_name);
    // Canonicalize both sides so the containment check resolves symlinks and
    // redundant separators instead of comparing path components lexically
    // (which would let a symlink or `..`-containing path slip through).
    let canon_dir = fs::canonicalize(dir).map_err(|e| format!("output directory unavailable: {e}"))?;
    let canon_path = fs::canonicalize(&path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            format!("log not found: {file_name}")
        } else {
            format!("log unavailable: {e}")
        }
    })?;
    if !canon_path.starts_with(&canon_dir) {
        return Err("log path escapes the output directory".into());
    }
    Ok(path)
}

/// Read one captured log's text by bare filename.
pub fn read_log(dir: &Path, file_name: &str) -> Result<String, String> {
    let path = resolve_log_path(dir, file_name)?;
    let mut file = open_no_follow(&path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents).map_err(|e| e.to_string())?;
    Ok(contents)
}

/// Move one captured log to the Trash. Returns its text so the session dedupe can forget it.
pub fn trash_log(dir: &Path, file_name: &str) -> Result<String, String> {
    let text = read_log(dir, file_name)?;
    let path = resolve_log_path(dir, file_name)?;
    move_to_trash(&path).map_err(|e| format!("couldn't move the log to the Trash: {e}"))?;
    Ok(text)
}

fn move_to_trash(path: &Path) -> Result<(), trash::Error> {
    #[allow(unused_mut)]
    let mut trash = trash::TrashContext::default();
    // The default Finder method needs Automation permission and plays a sound.
    #[cfg(target_os = "macos")]
    {
        use trash::macos::{DeleteMethod, TrashContextExtMacos};
        trash.set_delete_method(DeleteMethod::NsFileManager);
    }
    trash.delete(path)
}

/// The bare file name of a saved path (e.g. from `overlay://state`), for `resolve_log_path`.
pub fn file_name_of(path: &str) -> Result<String, String> {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_string)
        .ok_or_else(|| format!("invalid log path: {path}"))
}

/// Open a file without following symlinks. On Unix this is atomic via
/// `O_NOFOLLOW`, closing the TOCTOU window where a symlink is planted between
/// the containment check and the open.
#[cfg(unix)]
fn open_no_follow(path: &Path) -> Result<fs::File, String> {
    use std::os::unix::fs::OpenOptionsExt;
    fs::OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW).open(path).map_err(|e| open_err(path, e))
}

/// Non-Unix fallback: no `O_NOFOLLOW`, so pre-check via a no-follow stat that
/// the path is a regular file (not a symlink) before opening it.
#[cfg(not(unix))]
fn open_no_follow(path: &Path) -> Result<fs::File, String> {
    let is_regular = fs::symlink_metadata(path).map(|m| m.file_type().is_file()).unwrap_or(false);
    if !is_regular {
        return Err(format!("log not available: {}", path.display()));
    }
    fs::File::open(path).map_err(|e| open_err(path, e))
}

fn open_err(path: &Path, e: std::io::Error) -> String {
    if e.kind() == std::io::ErrorKind::NotFound {
        format!("log not found: {}", path.display())
    } else {
        format!("log unavailable: {}", path.display())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// A temp directory that removes itself (and its contents) on drop, so
    /// tests do not leak files into the system temp dir.
    struct TempDir {
        path: std::path::PathBuf,
    }

    impl TempDir {
        fn new(tag: &str) -> Self {
            let nanos =
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
            let path = std::env::temp_dir().join(format!("ptcgl-logs-{tag}-{nanos}"));
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }

        fn path(&self) -> &std::path::Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn file_name_of_keeps_only_the_last_component() {
        assert_eq!(file_name_of("/Users/me/logs/ptcgl-1.txt"), Ok("ptcgl-1.txt".to_string()));
        assert!(file_name_of("/").is_err());
        assert!(file_name_of("").is_err());
    }

    // Successful trashing touches the real Trash, so it's covered by the manual check instead.
    #[test]
    fn trash_log_refuses_before_touching_anything() {
        let dir = TempDir::new("trash");
        assert!(trash_log(dir.path(), "../secret.txt").is_err());
        assert!(trash_log(dir.path(), "ptcgl-1700000000000000000.txt").is_err(), "missing file");
    }

    #[test]
    fn parses_created_ms_from_ptcgl_name() {
        assert_eq!(nanos_to_ms(1_700_000_000_000_000_000), 1_700_000_000_000);
        assert_eq!(nanos_to_name_created_ms("ptcgl-1700000000000000000.txt"), Some(1_700_000_000_000));
        assert_eq!(nanos_to_name_created_ms("other-123.txt"), None);
        assert_eq!(nanos_to_name_created_ms("ptcgl-abc.txt"), None);
    }

    #[test]
    fn list_logs_empty_when_dir_missing() {
        let dir = TempDir::new("missing");
        let missing = dir.path().join("nope");
        assert!(list_logs(&missing).unwrap().is_empty());
    }

    #[test]
    fn list_logs_sorted_newest_first() {
        let dir = TempDir::new("sort");
        fs::write(dir.path().join("ptcgl-1000000000000000000.txt"), "old").unwrap();
        fs::write(dir.path().join("ptcgl-1700000000000000000.txt"), "new").unwrap();
        fs::write(dir.path().join("notalog.txt"), "ignore").unwrap();
        let logs = list_logs(dir.path()).unwrap();
        assert_eq!(logs.len(), 2, "non-ptcgl files ignored");
        assert_eq!(logs[0].file_name, "ptcgl-1700000000000000000.txt");
        assert_eq!(logs[1].file_name, "ptcgl-1000000000000000000.txt");
    }

    #[test]
    fn list_logs_ignores_directories_named_like_capture_files() {
        let dir = TempDir::new("dir-skip");
        fs::create_dir(dir.path().join("ptcgl-1700000000000000000.txt")).unwrap();
        fs::write(dir.path().join("ptcgl-1000000000000000000.txt"), "real").unwrap();
        let logs = list_logs(dir.path()).unwrap();
        assert_eq!(logs.len(), 1, "directories must not be listed");
        assert_eq!(logs[0].file_name, "ptcgl-1000000000000000000.txt");
    }

    #[test]
    fn resolve_rejects_path_traversal() {
        let dir = TempDir::new("guard");
        fs::write(dir.path().join("ptcgl-1.txt"), "x").unwrap();
        assert!(resolve_log_path(dir.path(), "../evil.txt").is_err());
        assert!(resolve_log_path(dir.path(), "a/b.txt").is_err());
        assert!(resolve_log_path(dir.path(), "..\\evil.txt").is_err());
        assert!(resolve_log_path(dir.path(), "/etc/passwd").is_err());
        assert!(resolve_log_path(dir.path(), "ptcgl-1.txt").is_ok());
    }

    #[test]
    fn read_log_missing_file_is_not_found() {
        let dir = TempDir::new("missing-file");
        let err = read_log(dir.path(), "ptcgl-123.txt").unwrap_err();
        assert!(err.contains("not found"), "got: {err}");
        assert!(!err.contains("escapes"), "got: {err}");
    }

    #[cfg(unix)]
    #[test]
    fn read_log_does_not_follow_symlink_pointing_outside() {
        let dir = TempDir::new("symlink-out");
        let link = dir.path().join("ptcgl-1700000000000000000.txt");
        std::os::unix::fs::symlink("/etc/passwd", &link).unwrap();
        // The canonicalized link target is outside the dir, so the containment
        // check itself must reject it.
        assert!(resolve_log_path(dir.path(), "ptcgl-1700000000000000000.txt").is_err());
        let err = read_log(dir.path(), "ptcgl-1700000000000000000.txt").unwrap_err();
        assert!(err.contains("escapes") || err.contains("unavailable"), "got: {err}");
    }

    #[cfg(unix)]
    #[test]
    fn read_log_rejects_symlink_even_within_dir() {
        let dir = TempDir::new("symlink-in");
        fs::write(dir.path().join("ptcgl-2.txt"), "real").unwrap();
        // The link target is inside the dir, so containment passes; the
        // O_NOFOLLOW open must still refuse to follow the link.
        std::os::unix::fs::symlink(dir.path().join("ptcgl-2.txt"), dir.path().join("ptcgl-1700000000000000000.txt"))
            .unwrap();
        assert!(resolve_log_path(dir.path(), "ptcgl-1700000000000000000.txt").is_ok());
        let err = read_log(dir.path(), "ptcgl-1700000000000000000.txt").unwrap_err();
        assert!(err.contains("unavailable"), "got: {err}");
    }

    #[test]
    fn read_log_rejects_traversal_and_reads_valid() {
        let dir = TempDir::new("read");
        fs::write(dir.path().join("ptcgl-1.txt"), "hello log").unwrap();
        assert!(read_log(dir.path(), "../secret.txt").is_err());
        assert_eq!(read_log(dir.path(), "ptcgl-1.txt").unwrap(), "hello log");
    }
}
