//! Text recognition for the capture engine.
//!
//! - macOS: Vision framework (VNRecognizeTextRequest)
//! - Windows: Windows.Media.Ocr (built-in, Windows 10+) — stub for CI, implement on VM
//! - Other: no-op stub

#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(target_os = "macos")]
use objc2::rc::Retained;
#[cfg(target_os = "macos")]
use objc2::runtime::AnyObject;
#[cfg(target_os = "macos")]
use objc2::AnyThread;
#[cfg(target_os = "macos")]
use objc2_foundation::{NSArray, NSData, NSDictionary, NSError};
#[cfg(target_os = "macos")]
use objc2_vision::{
    VNImageOption, VNImageRequestHandler, VNRecognizeTextRequest, VNRequest, VNRequestTextRecognitionLevel,
};
#[cfg(target_os = "macos")]
use tracing::warn;

#[cfg(target_os = "macos")]
use crate::bmp::encode_bmp;
use crate::ports::{Frame, TextMatch, TextRecognizer};

/// Platform-specific text recognizer.
/// - macOS: Vision framework
/// - Windows: Windows.Media.Ocr
/// - Other: stub
#[derive(Clone, Copy)]
pub struct PlatformTextRecognizer;

#[cfg(target_os = "macos")]
impl TextRecognizer for PlatformTextRecognizer {
    fn find_text(&self, frame: &Frame, needles: &[String]) -> Vec<TextMatch> {
        VisionTextRecognizer.scan(frame, needles, MatchKind::Exact)
    }

    fn find_text_containing(&self, frame: &Frame, needles: &[String]) -> Vec<TextMatch> {
        VisionTextRecognizer.scan(frame, needles, MatchKind::Substring)
    }

    fn warm_up(&self) {
        VisionTextRecognizer.warm_up();
    }
}

#[cfg(target_os = "windows")]
impl TextRecognizer for PlatformTextRecognizer {
    fn find_text(&self, frame: &Frame, needles: &[String]) -> Vec<TextMatch> {
        WindowsOcrRecognizer.find_text_exact(frame, needles)
    }

    fn find_text_containing(&self, frame: &Frame, needles: &[String]) -> Vec<TextMatch> {
        WindowsOcrRecognizer.find_text_containing(frame, needles)
    }

    fn warm_up(&self) {
        WindowsOcrRecognizer.warm_up();
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
impl TextRecognizer for PlatformTextRecognizer {
    fn find_text(&self, _frame: &Frame, _needles: &[String]) -> Vec<TextMatch> {
        Vec::new()
    }

    fn find_text_containing(&self, _frame: &Frame, _needles: &[String]) -> Vec<TextMatch> {
        Vec::new()
    }
}

// ===== macOS Vision Implementation =====

#[cfg(target_os = "macos")]
#[derive(Clone, Copy)]
pub struct VisionTextRecognizer;

#[cfg(target_os = "macos")]
static ACCURATE_FAILED: AtomicBool = AtomicBool::new(false);

#[cfg(target_os = "macos")]
fn recognize(
    frame: &Frame,
    level: VNRequestTextRecognitionLevel,
) -> Result<Retained<VNRecognizeTextRequest>, Option<Retained<NSError>>> {
    let bmp = encode_bmp(frame).ok_or(None)?;
    let data = NSData::with_bytes(&bmp);
    let options = NSDictionary::<VNImageOption, AnyObject>::new();
    let handler = VNImageRequestHandler::initWithData_options(VNImageRequestHandler::alloc(), &data, &options);
    let request = VNRecognizeTextRequest::new();
    request.setRecognitionLevel(level);
    request.setUsesLanguageCorrection(false);
    let requests = NSArray::<VNRequest>::from_slice(&[&*request]);
    handler.performRequests_error(&requests).map(|()| request).map_err(Some)
}

/// Fast recognition drops small labels at native capture size; a smooth 2x
/// upscale makes it reliable. Boxes are normalized, so callers are unaffected.
#[cfg(target_os = "macos")]
fn recognize_fast(frame: &Frame) -> Option<Retained<VNRecognizeTextRequest>> {
    use xcap::image::{imageops, RgbaImage};
    let img = RgbaImage::from_raw(frame.width, frame.height, frame.rgba.clone())?;
    let big = imageops::resize(&img, frame.width * 2, frame.height * 2, imageops::FilterType::Triangle);
    let big = Frame { width: big.width(), height: big.height(), rgba: big.into_raw() };
    recognize(&big, VNRequestTextRecognitionLevel::Fast).ok()
}

#[cfg(target_os = "macos")]
impl VisionTextRecognizer {
    fn scan(&self, frame: &Frame, needles: &[String], kind: MatchKind) -> Vec<TextMatch> {
        let request = if ACCURATE_FAILED.load(Ordering::Relaxed) {
            recognize_fast(frame)
        } else {
            match recognize(frame, VNRequestTextRecognitionLevel::Accurate) {
                Ok(request) => Some(request),
                Err(None) => None,
                Err(Some(e)) => {
                    // Accurate can fail system-wide (Neural Engine model compile timeouts) and
                    // each attempt stalls ~60s, so stay on fast for the rest of the session.
                    ACCURATE_FAILED.store(true, Ordering::Relaxed);
                    warn!("accurate text recognition failed, using fast from now on: {}", e.localizedDescription());
                    recognize_fast(frame)
                }
            }
        };
        let Some(request) = request else {
            return Vec::new();
        };
        let Some(results) = request.results() else {
            return Vec::new();
        };

        let normalized_needles: Vec<String> = needles.iter().map(|n| normalize(n)).collect();
        let fw = frame.width as f32;
        let fh = frame.height as f32;
        let mut matches = Vec::new();

        for observation in results.iter() {
            let Some(top) = observation.topCandidates(1).iter().next() else {
                continue;
            };
            let text = top.string().to_string();
            let hit = match kind {
                MatchKind::Exact => matches_needle(&text, &normalized_needles),
                MatchKind::Substring => matches_substring(&text, &normalized_needles),
            };
            if !hit {
                continue;
            }
            let rect = unsafe { observation.boundingBox() };
            let w = (rect.size.width as f32 * fw).round().max(0.0) as u32;
            let h = (rect.size.height as f32 * fh).round().max(0.0) as u32;
            if w == 0 || h == 0 {
                continue;
            }
            let x = (rect.origin.x as f32 * fw).round().max(0.0) as u32;
            let y = ((1.0 - rect.origin.y as f32 - rect.size.height as f32) * fh).round().max(0.0) as u32;
            matches.push(TextMatch { text, x, y, width: w, height: h });
        }

        matches
    }

    fn warm_up(&self) {
        let blank = Frame { width: 64, height: 64, rgba: vec![255; 64 * 64 * 4] };
        self.scan(&blank, &[], MatchKind::Exact);
    }
}

#[cfg(target_os = "macos")]
impl TextRecognizer for VisionTextRecognizer {
    fn find_text(&self, frame: &Frame, needles: &[String]) -> Vec<TextMatch> {
        self.scan(frame, needles, MatchKind::Exact)
    }

    fn find_text_containing(&self, frame: &Frame, needles: &[String]) -> Vec<TextMatch> {
        self.scan(frame, needles, MatchKind::Substring)
    }

    fn warm_up(&self) {
        self.warm_up();
    }
}

// ===== Windows OCR Implementation =====
//
// Currently a compile-only stub. Full implementation requires testing on Windows VM
// due to Windows.Media.Ocr API differences from documentation.
#[cfg(target_os = "windows")]
#[derive(Clone, Copy)]
struct WindowsOcrRecognizer;

#[cfg(target_os = "windows")]
impl WindowsOcrRecognizer {
    fn find_text_exact(&self, _frame: &Frame, _needles: &[String]) -> Vec<TextMatch> {
        Vec::new()
    }

    fn find_text_containing(&self, _frame: &Frame, _needles: &[String]) -> Vec<TextMatch> {
        Vec::new()
    }

    fn warm_up(&self) {}
}

// ===== Shared Matching Logic =====

#[cfg(any(target_os = "macos", target_os = "windows", test))]
#[derive(Clone, Copy, PartialEq)]
enum MatchKind {
    Exact,
    Substring,
}

/// Trim surrounding whitespace and lowercase, so matching is case-insensitive
/// (PTCG Live uses mixed casing across locales).
#[cfg(any(target_os = "macos", target_os = "windows", test))]
fn normalize(s: &str) -> String {
    s.trim().to_lowercase()
}

/// True when `text` is one of the needles. Matching is whole-string
/// (case-insensitive) so a stray fragment that happens to appear inside a
/// needle — e.g. "LO" inside "Battle Log" — cannot be mistaken for the button.
/// Fast recognition reads label icons as a short leading token ("o BATTLE LOG")
/// and garbles single letters, so that token is dropped and needles of
/// [`FUZZY_MIN_CHARS`]+ characters tolerate one edit.
#[cfg(any(target_os = "macos", target_os = "windows", test))]
fn matches_needle(text: &str, needles: &[String]) -> bool {
    let norm = normalize(text);
    let without_icon = norm.split_once(' ').filter(|(head, _)| head.chars().count() <= 2).map(|(_, rest)| rest);
    let matches = |candidate: &str| {
        needles
            .iter()
            .any(|n| n == candidate || (n.chars().count() >= FUZZY_MIN_CHARS && edit_distance(n, candidate) <= 1))
    };
    matches(&norm) || without_icon.is_some_and(matches)
}

#[cfg(any(target_os = "macos", target_os = "windows", test))]
const FUZZY_MIN_CHARS: usize = 8;

/// Levenshtein distance over chars.
#[cfg(any(target_os = "macos", target_os = "windows", test))]
fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut diag = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let next = (diag + usize::from(ca != *cb)).min(row[j] + 1).min(row[j + 1] + 1);
            diag = row[j + 1];
            row[j + 1] = next;
        }
    }
    row[b.len()]
}

/// True when `text` *contains* one of the needles as a substring, ignoring
/// punctuation and whitespace. Used for the transient toast, which OCR often
/// merges with neighbouring text (e.g. "cdierkens Export Copiedlondro") and
/// misreads the trailing "!" as a letter. Safe here because the needle is a
/// distinctive multi-word phrase.
#[cfg(any(target_os = "macos", target_os = "windows", test))]
fn matches_substring(text: &str, needles: &[String]) -> bool {
    let hay = alphanumeric(text);
    needles.iter().any(|n| hay.contains(&alphanumeric(n)))
}

/// Lowercase and keep only alphanumeric characters, dropping punctuation and
/// whitespace so OCR noise (e.g. "!" read as "l") does not break a match.
#[cfg(any(target_os = "macos", target_os = "windows", test))]
fn alphanumeric(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(|c| c.to_lowercase()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn needles() -> Vec<String> {
        vec![normalize("Battle Log"), normalize("Export"), normalize("Exportar")]
    }

    #[test]
    fn matches_exact_whole_string() {
        assert!(matches_needle("BATTLE LOG", &needles()));
        assert!(matches_needle("Export", &needles()));
        assert!(matches_needle("Exportar", &needles()));
    }

    #[test]
    fn matches_with_surrounding_whitespace() {
        assert!(matches_needle("  Battle Log  ", &needles()));
    }

    #[test]
    fn rejects_fragment_inside_a_needle() {
        assert!(!matches_needle("LO", &needles()));
        assert!(!matches_needle("Log", &needles()));
        assert!(!matches_needle("Expor", &needles()));
    }

    #[test]
    fn matches_fast_mode_icon_prefix_and_typo() {
        assert!(matches_needle("o BATTLE LOG", &needles()));
        assert!(matches_needle("Ql BATTLE LOG", &needles()));
        assert!(matches_needle("o BAThLE LOG", &needles()));
        assert!(!matches_needle("BArrLE LOG", &needles()));
        assert!(!matches_needle("o Expor", &needles()));
        assert!(!matches_needle("o LO", &needles()));
        assert!(!matches_needle("Battle", &needles()));
    }

    #[test]
    fn edit_distance_counts_single_edits() {
        assert_eq!(edit_distance("battle log", "battle log"), 0);
        assert_eq!(edit_distance("battle log", "bathle log"), 1);
        assert_eq!(edit_distance("battle log", "battle lo"), 1);
        assert_eq!(edit_distance("battle log", "barrle log"), 2);
        assert_eq!(edit_distance("", "abc"), 3);
    }

    #[test]
    fn rejects_unrelated_text() {
        assert!(!matches_needle("INSPECT BOARD", &needles()));
        assert!(!matches_needle("Pokemon TCG Live", &needles()));
        assert!(!matches_needle("", &needles()));
    }

    #[test]
    fn substring_matches_merged_toast() {
        let toast = vec![normalize("Export Copied!")];
        assert!(matches_substring("cdierkens Export Copiedlondro", &toast));
        assert!(matches_substring("Export Copiedlondro", &toast));
        assert!(matches_substring("Export Copied!", &toast));
    }

    #[test]
    fn substring_rejects_when_phrase_absent() {
        let toast = vec![normalize("Export Copied!")];
        assert!(!matches_substring("BATTLE LOG", &toast));
        assert!(!matches_substring("cdierkens CONTINUE", &toast));
    }
}
