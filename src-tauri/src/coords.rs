/// A point in screen or window-local pixel coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

use crate::ports::Frame;

/// Convert a window-local point to screen coordinates by adding the window origin.
pub fn window_to_screen(window_origin: Point, local: Point) -> Point {
    Point { x: window_origin.x + local.x, y: window_origin.y + local.y }
}

/// Center of a rectangle given its top-left corner and pixel size.
pub fn rect_center(top_left: Point, size: (u32, u32)) -> Point {
    Point { x: top_left.x + (size.0 as i32) / 2, y: top_left.y + (size.1 as i32) / 2 }
}

/// Uniform scale from captured-frame pixels to window points. Screen capture on
/// a Retina display returns 2x pixels while the window geometry (and hence
/// `window.x`/`window.y` and click targets) is in points, so frame-local pixel
/// coordinates must be divided by this before being offset by the window origin.
/// Returns 1.0 when the frame is narrower than the window (defensive).
pub fn frame_to_point_scale(frame_px: u32, window_pt: u32) -> f32 {
    if frame_px == 0 || window_pt == 0 || frame_px <= window_pt {
        1.0
    } else {
        frame_px as f32 / window_pt as f32
    }
}

/// A rectangle expressed as fractions (0.0..1.0) of the frame or game picture on each axis.
/// Used to crop a small region of interest before OCR, which is far cheaper
/// than recognizing the whole window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RelativeRect {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

/// A cropped sub-frame plus the pixel offset of its top-left corner within the
/// original frame, so region-local OCR matches can be mapped back to
/// full-frame coordinates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Crop {
    pub frame: Frame,
    pub origin_x: u32,
    pub origin_y: u32,
}

/// Crop `frame` to `rect` (fractions of the frame size). Returns `None` if the
/// rect is degenerate or lies entirely outside the frame.
pub fn crop_frame(frame: &Frame, rect: RelativeRect) -> Option<Crop> {
    crop_within(frame, PixelRect { x: 0, y: 0, width: frame.width, height: frame.height }, rect)
}

/// Crop `frame` to `rect` given as fractions of the game picture (see
/// [`game_picture`]), so regions hold windowed and fullscreen alike.
pub fn crop_game(frame: &Frame, rect: RelativeRect) -> Option<Crop> {
    crop_within(frame, game_picture(frame), rect)
}

fn crop_within(frame: &Frame, area: PixelRect, rect: RelativeRect) -> Option<Crop> {
    let fx = |v: f32, len: u32, start: u32, max: u32| {
        ((start as f32 + v * len as f32).round() as i64).clamp(0, max as i64) as u32
    };
    let x0 = fx(rect.x0, area.width, area.x, frame.width);
    let x1 = fx(rect.x1, area.width, area.x, frame.width);
    let y0 = fx(rect.y0, area.height, area.y, frame.height);
    let y1 = fx(rect.y1, area.height, area.y, frame.height);
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let w = x1 - x0;
    let h = y1 - y0;
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for y in y0..y1 {
        let start = ((y * frame.width + x0) * 4) as usize;
        let len = (w * 4) as usize;
        rgba.extend_from_slice(&frame.rgba[start..start + len]);
    }
    Some(Crop { frame: Frame { width: w, height: h, rgba }, origin_x: x0, origin_y: y0 })
}

/// A rectangle in frame pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Where PTCG Live's 16:9 picture sits in a captured window frame. The game
/// letterboxes (fullscreen on 16:10) or pillarboxes (ultrawide) with black bars,
/// and a windowed capture adds the title bar on top, so the picture is anchored
/// to the bottom edge above any bottom bar and centred between any side bars.
pub fn game_picture(frame: &Frame) -> PixelRect {
    let (w, h) = (frame.width, frame.height);
    let whole = PixelRect { x: 0, y: 0, width: w, height: h };
    if w == 0 || h == 0 || frame.rgba.len() < (w as usize * h as usize * 4) {
        return whole;
    }
    let ideal_h = (w as u64 * 9 / 16) as u32;
    if ideal_h <= h {
        // Bars are at most half the spare height; the cap keeps a dark scene edge
        // from being mistaken for a bar.
        let bottom_bar = black_run(frame, h, (h - ideal_h) / 2, |i| h - 1 - i, |row| row_samples(w, row));
        PixelRect { x: 0, y: h - bottom_bar - ideal_h, width: w, height: ideal_h }
    } else {
        let side_bar = black_run(frame, w, (w - h * 16 / 9) / 2, |i| w - 1 - i, |col| col_samples(h, col));
        let width = w - 2 * side_bar;
        let height = ((width as u64 * 9 / 16) as u32).min(h);
        PixelRect { x: side_bar, y: h - height, width, height }
    }
}

/// Count of consecutive black lines from an edge, up to `cap`.
fn black_run(
    frame: &Frame,
    lines: u32,
    cap: u32,
    line_at: impl Fn(u32) -> u32,
    samples: impl Fn(u32) -> [(u32, u32); 3],
) -> u32 {
    (0..cap.min(lines)).take_while(|&i| samples(line_at(i)).iter().all(|&(x, y)| is_black(frame, x, y))).count() as u32
}

fn row_samples(w: u32, row: u32) -> [(u32, u32); 3] {
    [(w / 4, row), (w / 2, row), (w * 3 / 4, row)]
}

fn col_samples(h: u32, col: u32) -> [(u32, u32); 3] {
    [(col, h / 4), (col, h / 2), (col, h * 3 / 4)]
}

fn is_black(frame: &Frame, x: u32, y: u32) -> bool {
    let i = ((y * frame.width + x) * 4) as usize;
    frame.rgba[i..i + 3].iter().all(|&c| c <= 8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_to_screen_adds_origin() {
        let origin = Point { x: 100, y: 50 };
        let local = Point { x: 20, y: 30 };
        assert_eq!(window_to_screen(origin, local), Point { x: 120, y: 80 });
    }

    #[test]
    fn rect_center_of_even_size() {
        let top_left = Point { x: 0, y: 0 };
        assert_eq!(rect_center(top_left, (100, 50)), Point { x: 50, y: 25 });
    }

    #[test]
    fn rect_center_with_offset_top_left() {
        let top_left = Point { x: 10, y: 20 };
        assert_eq!(rect_center(top_left, (10, 10)), Point { x: 15, y: 25 });
    }

    #[test]
    fn scale_is_one_on_non_retina() {
        let s = frame_to_point_scale(1280, 1280);
        assert!((s - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn scale_is_two_on_retina_capture() {
        let s = frame_to_point_scale(2560, 1280);
        assert!((s - 2.0).abs() < f32::EPSILON);
    }

    #[test]
    fn scale_defensive_defaults_to_one() {
        assert!((frame_to_point_scale(0, 1280) - 1.0).abs() < f32::EPSILON);
        assert!((frame_to_point_scale(2560, 0) - 1.0).abs() < f32::EPSILON);
        assert!((frame_to_point_scale(100, 1280) - 1.0).abs() < f32::EPSILON);
    }

    fn solid_frame(w: u32, h: u32) -> Frame {
        Frame { width: w, height: h, rgba: vec![100u8; (w * h * 4) as usize] }
    }

    #[test]
    fn crop_extracts_sub_rectangle() {
        // 2x1 pixels: left pixel red, right pixel blue.
        let f = Frame { width: 2, height: 1, rgba: vec![255, 0, 0, 255, 0, 0, 255, 255] };
        let crop = crop_frame(&f, RelativeRect { x0: 0.5, y0: 0.0, x1: 1.0, y1: 1.0 }).unwrap();
        assert_eq!(crop.frame.width, 1);
        assert_eq!(crop.frame.height, 1);
        assert_eq!(crop.origin_x, 1);
        assert_eq!(crop.origin_y, 0);
        assert_eq!(crop.frame.rgba, vec![0, 0, 255, 255]);
    }

    #[test]
    fn crop_rejects_degenerate_rect() {
        let f = solid_frame(10, 10);
        assert!(crop_frame(&f, RelativeRect { x0: 0.5, y0: 0.0, x1: 0.5, y1: 1.0 }).is_none());
        assert!(crop_frame(&f, RelativeRect { x0: 0.0, y0: 0.5, x1: 1.0, y1: 0.5 }).is_none());
    }

    /// Paints `rows` black, mimicking the game's letterbox bars.
    fn with_black_rows(mut f: Frame, rows: impl IntoIterator<Item = u32>) -> Frame {
        for y in rows {
            let start = (y * f.width * 4) as usize;
            f.rgba[start..start + (f.width * 4) as usize].fill(0);
        }
        f
    }

    fn assert_contains(crop: &Crop, xs: std::ops::RangeInclusive<u32>, ys: std::ops::RangeInclusive<u32>) {
        let (x0, y0) = (crop.origin_x, crop.origin_y);
        let (x1, y1) = (x0 + crop.frame.width, y0 + crop.frame.height);
        assert!(x0 <= *xs.start() && *xs.end() <= x1, "x {x0}..{x1} must contain {xs:?}");
        assert!(y0 <= *ys.start() && *ys.end() <= y1, "y {y0}..{y1} must contain {ys:?}");
    }

    #[test]
    fn windowed_picture_sits_below_the_title_bar() {
        assert_eq!(game_picture(&solid_frame(2560, 1496)), PixelRect { x: 0, y: 56, width: 2560, height: 1440 });
    }

    #[test]
    fn fullscreen_picture_skips_the_letterbox_bars() {
        let f = with_black_rows(solid_frame(3456, 2168), (0..112).chain(2056..2168));
        assert_eq!(game_picture(&f), PixelRect { x: 0, y: 112, width: 3456, height: 1944 });
    }

    #[test]
    fn a_dark_scene_edge_is_not_mistaken_for_a_bar() {
        // Only 28px of spare height could be bar; the rest is scene.
        let f = with_black_rows(solid_frame(2560, 1496), 1396..1496);
        assert_eq!(game_picture(&f).y, 56 - 28);
    }

    #[test]
    fn pillarbox_picture_skips_the_side_bars() {
        let mut f = solid_frame(400, 150);
        for y in 0..150 {
            for x in (0..67).chain(333..400) {
                let i = ((y * 400 + x) * 4) as usize;
                f.rgba[i..i + 4].fill(0);
            }
        }
        assert_eq!(game_picture(&f), PixelRect { x: 67, y: 1, width: 266, height: 149 });
    }

    #[test]
    fn frames_without_pixels_use_the_whole_frame() {
        let f = Frame { width: 2560, height: 1496, rgba: Vec::new() };
        assert_eq!(game_picture(&f), PixelRect { x: 0, y: 0, width: 2560, height: 1496 });
    }

    #[test]
    fn regions_contain_the_measured_windowed_labels() {
        // Measured live in 2560x1496 captures.
        let f = solid_frame(2560, 1496);
        assert_contains(&crop_game(&f, crate::buttons::battle_log_region()).unwrap(), 1220..=1347, 1213..=1232);
        assert_contains(&crop_game(&f, crate::buttons::continue_region()).unwrap(), 1203..=1359, 1405..=1438);
    }

    #[test]
    fn regions_contain_the_measured_fullscreen_labels() {
        // Measured live fullscreen on a 16:10 display: 3456x2168 with 112px bars.
        let f = with_black_rows(solid_frame(3456, 2168), (0..112).chain(2056..2168));
        assert_contains(&crop_game(&f, crate::buttons::battle_log_region()).unwrap(), 1586..=1804, 1676..=1708);
        assert_contains(&crop_game(&f, crate::buttons::continue_region()).unwrap(), 1628..=1828, 1939..=1970);
    }
}
