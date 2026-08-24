//! Lays out the 128x128 usage dashboard.

use crate::canvas::{Canvas, Rgb, W};
use crate::state::{Snapshot, Source};
use crate::usage::{human_duration, Metric};

const BG: Rgb = Rgb(0x16, 0x14, 0x12);
const CARD: Rgb = Rgb(0x24, 0x21, 0x1d);
const TRACK: Rgb = Rgb(0x35, 0x31, 0x2b);
const TEXT: Rgb = Rgb(0xf2, 0xee, 0xe6);
const MUTED: Rgb = Rgb(0x8c, 0x84, 0x78);
const ACCENT: Rgb = Rgb(0xd9, 0x77, 0x57);

const OK: Rgb = Rgb(0x5c, 0xc9, 0x7a);
const WARN: Rgb = Rgb(0xe8, 0xb3, 0x39);
const CRIT: Rgb = Rgb(0xe0, 0x5c, 0x4a);

const MARGIN: i32 = 7;
const BAR_H: i32 = 9;

fn severity(pct: f64) -> Rgb {
    if pct >= 90.0 {
        CRIT
    } else if pct >= 65.0 {
        WARN
    } else {
        OK
    }
}

pub fn dashboard(snap: &Snapshot) -> Canvas {
    let mut c = Canvas::new(BG);

    // Header: accent rule + title.
    c.fill_rect(MARGIN, 6, 4, 8, ACCENT);
    c.text(MARGIN + 8, 6, "CLAUDE LIMITS", 1, TEXT);
    c.fill_rect(MARGIN, 18, W as i32 - MARGIN * 2, 1, CARD);

    block(&mut c, 24, "5 HOUR", snap.session);
    block(&mut c, 73, "WEEKLY", snap.week);

    // Footer: provenance and age, so a frozen screen cannot pass for a live one.
    // Anything not measured this minute says so.
    let (footer, color) = match (snap.source, snap.age) {
        (Source::Live, _) => ("LIVE".to_string(), MUTED),
        (src, age) if age > 900 => (format!("{} {}", src.label(), human_duration(age)), CRIT),
        (src, age) => (format!("{} {}", src.label(), human_duration(age.max(1))), WARN),
    };
    c.text(MARGIN, 120, &footer, 1, color);
    c
}

/// One labelled metric: title, big percentage, progress bar, reset countdown.
fn block(c: &mut Canvas, top: i32, label: &str, metric: Option<Metric>) {
    let right = W as i32 - MARGIN;

    let Some(m) = metric else {
        c.text(MARGIN, top, label, 1, MUTED);
        c.text_right(right, top, "N/A", 1, MUTED);
        c.fill_round_rect(MARGIN, top + 16, right - MARGIN, BAR_H, BAR_H / 2, TRACK);
        return;
    };

    let color = severity(m.percent_used);
    c.text(MARGIN, top, label, 1, MUTED);

    // Percentage: digits at scale 2, the '%' kept small so it reads as a unit.
    let digits = format!("{}", m.percent_used.round() as i64);
    let pct_w = crate::font::text_width(&digits, 2) as i32 + 2 + 5;
    let pct_x = right - pct_w;
    c.text(pct_x, top - 4, &digits, 2, TEXT);
    c.text(right - 5, top + 3, "%", 1, MUTED);

    // Bar.
    let bar_y = top + 16;
    let bar_w = right - MARGIN;
    c.fill_round_rect(MARGIN, bar_y, bar_w, BAR_H, BAR_H / 2, TRACK);

    let filled = ((bar_w as f64) * (m.percent_used / 100.0)).round() as i32;
    if filled > 0 {
        // Below the corner radius a rounded rect degenerates; clamp to keep the
        // left cap round instead of vanishing.
        let filled = filled.max(BAR_H);
        c.fill_round_rect(MARGIN, bar_y, filled, BAR_H, BAR_H / 2, color);
    }

    // Countdown under the bar.
    let reset = match m.resets_in {
        Some(s) => format!("RESETS IN {}", human_duration(s)),
        None => "RESET UNKNOWN".to_string(),
    };
    c.text(MARGIN, bar_y + BAR_H + 4, &reset, 1, MUTED);
}
