//! Text recognition for the capture engine.
//!
//! - macOS: Vision framework (VNRecognizeTextRequest)
//! - Windows: Windows.Media.Ocr (WinRT; built in on Windows 10+)
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

#[cfg(target_os = "windows")]
use std::sync::{Mutex, OnceLock};
#[cfg(target_os = "windows")]
use tracing::warn;
#[cfg(target_os = "windows")]
use windows::core::HSTRING;
#[cfg(target_os = "windows")]
use windows::Globalization::Language;
#[cfg(target_os = "windows")]
use windows::Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap};
#[cfg(target_os = "windows")]
use windows::Media::Ocr::{OcrEngine, OcrLine};
#[cfg(target_os = "windows")]
use windows::Storage::Streams::DataWriter;
#[cfg(target_os = "windows")]
use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};

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
// Uses Windows.Media.Ocr (WinRT) via the `windows` crate 0.58.
//
// The frame is handed to OCR as a BGRA8 `SoftwareBitmap`. WinRT has no
// zero-copy window into existing bytes, so the pixels are copied through a
// `DataWriter` into a WinRT buffer. OCR returns one `OcrLine` per line of text
// and, unlike Vision, gives bounding rects only per `OcrWord`, so each line's
// words are unioned into a single line rect. That mirrors the macOS path, which
// reports one box per recognized line.
#[cfg(target_os = "windows")]
#[derive(Clone, Copy)]
struct WindowsOcrRecognizer;

/// The recognizer is free-threaded (`OcrEngine` is `Send + Sync`), so a
/// process-wide cache avoids reloading the model on every poll. `None` records
/// that no recognizer language is installed, which retrying cannot fix.
#[cfg(target_os = "windows")]
fn ocr_engine() -> Option<&'static Mutex<OcrEngine>> {
    static ENGINE: OnceLock<Option<Mutex<OcrEngine>>> = OnceLock::new();
    ENGINE.get_or_init(|| create_engine().map(Mutex::new)).as_ref()
}

#[cfg(target_os = "windows")]
fn create_engine() -> Option<OcrEngine> {
    if let Ok(engine) = OcrEngine::TryCreateFromUserProfileLanguages() {
        return Some(engine);
    }
    // The user profile may carry no OCR-capable language; try the locales the
    // game ships in, then whatever is installed, before giving up.
    for tag in ["en-US", "en-GB", "es-ES", "fr-FR", "de-DE"] {
        if let Ok(language) = Language::CreateLanguage(&HSTRING::from(tag)) {
            if let Ok(engine) = OcrEngine::TryCreateFromLanguage(&language) {
                return Some(engine);
            }
        }
    }
    if let Ok(languages) = OcrEngine::AvailableRecognizerLanguages() {
        for language in languages {
            if let Ok(engine) = OcrEngine::TryCreateFromLanguage(&language) {
                return Some(engine);
            }
        }
    }
    warn!("no Windows OCR recognizer language is installed");
    None
}

/// Windows Runtime calls require the calling thread to be initialized. Both the
/// engine's poll thread and the warm-up thread call this. `RoInitialize` is
/// idempotent per thread, so a failure just means the thread was already
/// initialized — ignore it. The flag keeps the per-thread init count from
/// growing on every poll.
#[cfg(target_os = "windows")]
fn ensure_apartment() {
    thread_local! {
        static READY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }
    READY.with(|ready| {
        if !ready.get() {
            // SAFETY: `RoInitialize` is a plain entry point taking no pointers.
            let _ = unsafe { RoInitialize(RO_INIT_MULTITHREADED) };
            ready.set(true);
        }
    });
}

/// Convert an RGBA frame to a BGRA8 `SoftwareBitmap` for OCR.
///
/// Alpha is forced opaque because a screen capture can carry `alpha == 0`, which
/// OCR may read as a fully transparent image. A frame longer than
/// `OcrEngine::MaxImageDimension` is scaled down; the returned factor maps OCR
/// pixel coordinates back to the original frame.
#[cfg(target_os = "windows")]
fn to_software_bitmap(frame: &Frame) -> Option<(SoftwareBitmap, f32)> {
    if frame.width == 0 || frame.height == 0 {
        return None;
    }
    let needed = frame.width as usize * frame.height as usize * 4;
    if frame.rgba.len() < needed {
        return None;
    }

    let max = OcrEngine::MaxImageDimension().unwrap_or(u32::MAX).max(1);
    let longest = frame.width.max(frame.height);
    let (src, scale) = if longest > max {
        use xcap::image::{imageops, RgbaImage};
        let factor = max as f32 / longest as f32;
        let w = ((frame.width as f32 * factor).round() as u32).max(1);
        let h = ((frame.height as f32 * factor).round() as u32).max(1);
        let image = RgbaImage::from_raw(frame.width, frame.height, frame.rgba[..needed].to_vec())?;
        let resized = imageops::resize(&image, w, h, imageops::FilterType::Triangle);
        (Frame { width: w, height: h, rgba: resized.into_raw() }, factor)
    } else {
        (Frame { width: frame.width, height: frame.height, rgba: frame.rgba[..needed].to_vec() }, 1.0)
    };

    let mut bgra = Vec::with_capacity(src.rgba.len());
    for px in src.rgba.as_chunks::<4>().0 {
        bgra.push(px[2]);
        bgra.push(px[1]);
        bgra.push(px[0]);
        bgra.push(0xFF);
    }

    let writer = DataWriter::new().ok()?;
    writer.WriteBytes(&bgra).ok()?;
    let buffer = writer.DetachBuffer().ok()?;
    let bitmap =
        SoftwareBitmap::CreateCopyFromBuffer(&buffer, BitmapPixelFormat::Bgra8, src.width as i32, src.height as i32)
            .ok()?;
    Some((bitmap, scale))
}

/// Union of a line's word rects as `(x, y, width, height)` in frame-local
/// pixels. OCR reports pixels in the bitmap it was given, so `scale` maps them
/// back to the original frame.
#[cfg(target_os = "windows")]
fn line_bounds(line: &OcrLine, scale: f32) -> Option<(u32, u32, u32, u32)> {
    let words = line.Words().ok()?;
    let (mut min_x, mut min_y) = (f32::MAX, f32::MAX);
    let (mut max_x, mut max_y) = (f32::MIN, f32::MIN);
    for word in words {
        let Ok(rect) = word.BoundingRect() else {
            continue;
        };
        min_x = min_x.min(rect.X);
        min_y = min_y.min(rect.Y);
        max_x = max_x.max(rect.X + rect.Width);
        max_y = max_y.max(rect.Y + rect.Height);
    }
    if min_x > max_x || min_y > max_y {
        return None;
    }
    let inv = 1.0 / scale;
    let x = (min_x * inv).max(0.0).round() as u32;
    let y = (min_y * inv).max(0.0).round() as u32;
    let w = ((max_x - min_x) * inv).max(0.0).round() as u32;
    let h = ((max_y - min_y) * inv).max(0.0).round() as u32;
    (w != 0 && h != 0).then_some((x, y, w, h))
}

#[cfg(target_os = "windows")]
impl WindowsOcrRecognizer {
    fn scan(&self, frame: &Frame, needles: &[String], kind: MatchKind) -> Vec<TextMatch> {
        ensure_apartment();
        let Some(engine) = ocr_engine() else {
            return Vec::new();
        };
        let Some((bitmap, scale)) = to_software_bitmap(frame) else {
            return Vec::new();
        };
        let engine = engine.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let Ok(results) = engine.RecognizeAsync(&bitmap).and_then(|op| op.join()) else {
            return Vec::new();
        };
        let Ok(lines) = results.Lines() else {
            return Vec::new();
        };

        let normalized_needles: Vec<String> = needles.iter().map(|n| normalize(n)).collect();
        let mut matches = Vec::new();
        for line in lines {
            let text = line.Text().map(|t| t.to_string()).unwrap_or_default();
            let hit = match kind {
                MatchKind::Exact => matches_needle(&text, &normalized_needles),
                MatchKind::Substring => matches_substring(&text, &normalized_needles),
            };
            let Some((x, y, width, height)) = line_bounds(&line, scale).filter(|_| hit) else {
                continue;
            };
            matches.push(TextMatch { text, x, y, width, height });
        }
        matches
    }

    fn find_text_exact(&self, frame: &Frame, needles: &[String]) -> Vec<TextMatch> {
        self.scan(frame, needles, MatchKind::Exact)
    }

    fn find_text_containing(&self, frame: &Frame, needles: &[String]) -> Vec<TextMatch> {
        self.scan(frame, needles, MatchKind::Substring)
    }

    /// Force engine creation and one recognition so the first real poll isn't
    /// the one that pays the model-load cost.
    fn warm_up(&self) {
        let blank = Frame { width: 64, height: 64, rgba: vec![255; 64 * 64 * 4] };
        self.scan(&blank, &[], MatchKind::Exact);
    }
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
