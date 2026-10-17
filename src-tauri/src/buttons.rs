/// Localized results-screen labels, extracted from PTCG Live's translation
/// cache (`localization-cache/{monthly}/{locale}`). Matching is
/// case-insensitive, so the mixed casing across locales is fine.
use crate::coords::RelativeRect;

pub struct ButtonLabels {
    pub battle_log: &'static str,
    pub continue_label: &'static str,
}

pub const BUTTON_LABELS: &[(&str, ButtonLabels)] = &[
    ("en", ButtonLabels { battle_log: "Battle Log", continue_label: "Continue" }),
    ("de", ButtonLabels { battle_log: "Kampf-Protokoll", continue_label: "Fortfahren" }),
    ("es", ButtonLabels { battle_log: "registro del combate", continue_label: "Continuar" }),
    ("es_la", ButtonLabels { battle_log: "registro del combate", continue_label: "Continuar" }),
    ("fr", ButtonLabels { battle_log: "Journal de combat", continue_label: "Continuer" }),
    ("it", ButtonLabels { battle_log: "registro lotta", continue_label: "Continua" }),
    ("ptbr", ButtonLabels { battle_log: "Registro de Batalha", continue_label: "Continuar" }),
];

/// Needle set for the "Battle Log" label across all locales: the results-screen
/// tab (earliest reliable match-over signal) and the open drawer's title.
pub fn battle_log_needles() -> Vec<String> {
    BUTTON_LABELS.iter().map(|(_, l)| l.battle_log.to_string()).collect()
}

/// Needle set for the results screen's "Continue" button across all locales;
/// the Continue shield is placed over the first match.
pub fn continue_needles() -> Vec<String> {
    BUTTON_LABELS.iter().map(|(_, l)| l.continue_label.to_string()).collect()
}

/// Region of the results screen where the "Battle Log" tab label renders, as
/// fractions of the game picture (`coords::game_picture`). Measured live: the
/// tab's top-left sat at x=1220, y=1157 of the 2560x1440 picture across three
/// matches (only its width varied 119..127px with the localized label length),
/// and at y 0.80 of the picture fullscreen on a 16:10 display. Padded with
/// margin while staying ~1.5% of the frame, so recognizing it is ~4x cheaper
/// than the full window.
pub fn battle_log_region() -> RelativeRect {
    RelativeRect { x0: 0.43, y0: 0.75, x1: 0.58, y1: 0.86 }
}

/// Safety-net region for the results-screen Battle Log tab, as fractions of the
/// game picture. Used by the periodic sweep in place of a whole-frame OCR: a
/// lower band covers tab drift while still excluding the open drawer's header
/// tab near the top.
///
/// The band's bounds are a proposal, not a measured placement. Timings, measured
/// on a 2560x1496 capture back-to-back with the whole-frame sweep this replaces:
/// 140.6ms versus 221.4ms, holding at 36-58% across three runs. A tighter band
/// (x 0.30-0.70, y 0.70-0.92) measured 124.9ms, but Vision's cost flattens well
/// before full-frame size, so the wider band's extra pixels cost almost nothing
/// and margin is the entire purpose of a safety net.
pub fn battle_log_sweep_region() -> RelativeRect {
    RelativeRect { x0: 0.20, y0: 0.63, x1: 0.80, y1: 0.95 }
}

/// Region of the results screen where the Continue button label renders, as
/// fractions of the game picture. Measured live: "CONTINUE" at x 0.470-0.531,
/// y 0.937-0.960 windowed and y 0.940-0.956 fullscreen; padded so localized
/// labels still fit.
pub fn continue_region() -> RelativeRect {
    RelativeRect { x0: 0.40, y0: 0.89, x1: 0.60, y1: 1.0 }
}

/// Region at the top of the game picture where the open Battle Log drawer shows
/// its own "Battle Log" title. Seeing it keeps the results screen "present"
/// while the drawer covers the results-screen tab.
pub fn drawer_title_region() -> RelativeRect {
    RelativeRect { x0: 0.0, y0: 0.0, x1: 1.0, y1: 0.25 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn battle_log_needles_cover_english_and_german() {
        let needles = battle_log_needles();
        assert!(needles.iter().any(|n| n == "Battle Log"));
        assert!(needles.iter().any(|n| n == "Kampf-Protokoll"));
    }

    #[test]
    fn continue_needles_cover_english_and_german() {
        let needles = continue_needles();
        assert!(needles.iter().any(|n| n == "Continue"));
        assert!(needles.iter().any(|n| n == "Fortfahren"));
    }

    #[test]
    fn sweep_band_contains_the_measured_tab_box() {
        let tab = battle_log_region();
        let band = battle_log_sweep_region();
        assert!(band.x0 <= tab.x0 && band.x1 >= tab.x1);
        assert!(band.y0 <= tab.y0 && band.y1 >= tab.y1);
    }

    /// Measured top-edge y of the open drawer's own "Battle Log" header, as a
    /// fraction of the game picture (y=92 of a 1496px capture, below a 56px title bar).
    const DRAWER_HEADER_Y: f32 = (92.0 - 56.0) / 1440.0;

    #[test]
    fn sweep_band_excludes_the_drawer_header() {
        // This is the guarantee that matters: the band, not the y-threshold in
        // `is_results_screen_tab`, is what keeps the drawer from reading as the
        // results screen. Assert against the measured header position.
        let band = battle_log_sweep_region();
        assert!(
            band.y0 > DRAWER_HEADER_Y,
            "band starts at y {} but the drawer header sits at y {DRAWER_HEADER_Y}",
            band.y0
        );
    }

    #[test]
    fn sweep_band_is_far_smaller_than_the_frame() {
        let band = battle_log_sweep_region();
        let area = (band.x1 - band.x0) * (band.y1 - band.y0);
        assert!(area < 0.25, "band covers {:.1}% of the frame", area * 100.0);
    }

    #[test]
    fn every_locale_has_all_labels() {
        for (locale, labels) in BUTTON_LABELS {
            for (field, value) in [("battle_log", labels.battle_log), ("continue", labels.continue_label)] {
                assert!(!value.trim().is_empty(), "{locale} {field} must be non-empty");
            }
        }
    }
}
