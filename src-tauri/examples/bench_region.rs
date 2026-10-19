// Compare: full-frame OCR vs OCR on just the region where the Battle Log tab
// sits on the results screen (fixed-size window => fixed position).
// macOS only: benchmarks Vision framework performance.
#[cfg(target_os = "macos")]
use ptcgl_battle_log_capture_lib::bmp::encode_bmp;
#[cfg(target_os = "macos")]
use ptcgl_battle_log_capture_lib::buttons::battle_log_needles;
#[cfg(target_os = "macos")]
use ptcgl_battle_log_capture_lib::platform::frame::XcapFrameCapturer;
#[cfg(target_os = "macos")]
use ptcgl_battle_log_capture_lib::platform::ocr::VisionTextRecognizer;
#[cfg(target_os = "macos")]
use ptcgl_battle_log_capture_lib::platform::window::XcapWindowProvider;
#[cfg(target_os = "macos")]
use ptcgl_battle_log_capture_lib::ports::{Frame, FrameCapturer, TextRecognizer, WindowProvider};
#[cfg(target_os = "macos")]
use std::time::Instant;

#[cfg(not(target_os = "macos"))]
fn main() {
    println!("This benchmark runs on macOS only (Vision framework).");
}

#[cfg(target_os = "macos")]
fn crop(f: &Frame, x0: f32, y0: f32, x1: f32, y1: f32) -> Frame {
    let fx = |v: f32, max: u32| ((v * max as f32).round() as i32).clamp(0, max as i32) as u32;
    let x0 = fx(x0, f.width);
    let x1 = fx(x1, f.width);
    let y0 = fx(y0, f.height);
    let y1 = fx(y1, f.height);
    let w = x1 - x0;
    let h = y1 - y0;
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for y in y0..y1 {
        let start = ((y * f.width + x0) * 4) as usize;
        let len = (w * 4) as usize;
        rgba.extend_from_slice(&f.rgba[start..start + len]);
    }
    Frame { width: w, height: h, rgba }
}

#[cfg(target_os = "macos")]
/// Median of 5 OCR calls, in milliseconds.
fn median_ms(ocr: &VisionTextRecognizer, frame: &Frame, needles: &[String]) -> f64 {
    let mut t = vec![];
    for _ in 0..5 {
        let s = Instant::now();
        let _ = ocr.find_text(frame, needles);
        t.push(s.elapsed().as_secs_f64() * 1000.0);
    }
    t.sort_by(|a, b| a.partial_cmp(b).unwrap());
    t[2]
}

#[cfg(target_os = "macos")]
/// Median of 5 encodes, in milliseconds. Isolates our own cost from Vision's,
/// which varies with on-screen content and so is not comparable across runs.
fn median_encode_ms(frame: &Frame) -> f64 {
    let mut t = vec![];
    for _ in 0..5 {
        let s = Instant::now();
        let out = encode_bmp(frame).expect("non-degenerate frame");
        std::hint::black_box(&out);
        t.push(s.elapsed().as_secs_f64() * 1000.0);
    }
    t.sort_by(|a, b| a.partial_cmp(b).unwrap());
    t[2]
}

#[cfg(target_os = "macos")]
fn main() {
    let wp = XcapWindowProvider::new("pokemon");
    let Some(window) = wp.find_game_window() else {
        println!("no window");
        return;
    };
    let fc = XcapFrameCapturer;
    let Some(frame) = fc.capture(&window) else {
        println!("no frame");
        return;
    };
    let ocr = VisionTextRecognizer;
    let needles = battle_log_needles();

    // region around where BATTLE LOG appears on the results screen (measured)
    let region = crop(&frame, 0.40, 0.76, 0.60, 0.86);
    println!("region size: {}x{}", region.width, region.height);

    // Positive control. The needle counts below are ambiguous on their own:
    // "0 matches" looks the same whether Vision decoded our BMP correctly and
    // simply found no "BATTLE LOG", or whether the BMP is garbage and Vision
    // sees nothing at all. This asks for any recognized string containing an
    // "e", which any real screen will have, so a non-zero count proves the
    // encoder produces something Vision can actually read.
    let control = vec!["e".to_string()];
    let control_hits = ocr.find_text_containing(&frame, &control).len();
    println!("vision control (any text with an 'e'): {} hits", control_hits);
    if control_hits == 0 {
        println!("  !! Vision read NOTHING — the BMP encoder is not producing a decodable image");
    }

    // Our own encode cost, isolated from Vision's content-dependent variance.
    println!("encode full  ({}x{}): median {:.2} ms", frame.width, frame.height, median_encode_ms(&frame));
    println!("encode region({}x{}): median {:.2} ms", region.width, region.height, median_encode_ms(&region));

    println!("full-frame ocr:    median {:.1} ms", median_ms(&ocr, &frame, &needles));
    println!("region-only ocr:   median {:.1} ms", median_ms(&ocr, &region, &needles));
    println!("region match: {:?}", ocr.find_text(&region, &needles).len());
    println!("full   match: {:?}", ocr.find_text(&frame, &needles).len());

    // Candidate sweep bands: the safety net the engine uses instead of a
    // whole-frame OCR. All three cover the measured tab box (x 0.43-0.58,
    // y 0.76-0.86); pick the cheapest that keeps the margin you want.
    for (name, r) in [
        ("band-tight  x0.30-0.70 y0.70-0.92", (0.30, 0.70, 0.70, 0.92)),
        ("band-medium x0.20-0.80 y0.65-0.95", (0.20, 0.65, 0.80, 0.95)),
        ("band-wide   x0.00-1.00 y0.60-1.00", (0.00, 0.60, 1.00, 1.00)),
    ] {
        let c = crop(&frame, r.0, r.1, r.2, r.3);
        println!("{}: {}x{} ocr median {:.1} ms", name, c.width, c.height, median_ms(&ocr, &c, &needles));
    }
}
