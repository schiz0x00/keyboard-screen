//! Reads `~/.claude/usage-exact.json`, which Claude Code refreshes from the API
//! with the current 5-hour session and weekly limit consumption.

use crate::json::{self, Value};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy)]
pub struct Metric {
    pub percent_used: f64,
    /// Seconds until the window resets; negative once it is overdue.
    pub resets_in: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct Usage {
    pub session: Option<Metric>,
    pub week_all: Option<Metric>,
    /// Age of the file's own timestamp, in seconds.
    pub staleness: Option<i64>,
}

pub fn default_path() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
    std::path::Path::new(&home).join(".claude/usage-exact.json")
}

pub fn load(path: &std::path::Path) -> Result<Usage, String> {
    let text = std::fs::read_to_string(path).map_err(|e| {
        format!(
            "cannot read {}: {e}\n(run any Claude Code session once to create it)",
            path.display()
        )
    })?;
    let root = json::parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let now = unix_now();

    Ok(Usage {
        session: metric(&root, "metrics.session", now),
        week_all: metric(&root, "metrics.week_all", now),
        staleness: root
            .path("timestamp")
            .and_then(Value::as_str)
            .and_then(parse_rfc3339)
            .map(|t| now - t),
    })
}

fn metric(root: &Value, path: &str, now: i64) -> Option<Metric> {
    let node = root.path(path)?;
    if *node == Value::Null {
        return None;
    }
    // Prefer percent_used; fall back to 100 - percent_remaining.
    let percent_used = node
        .get("percent_used")
        .and_then(Value::as_f64)
        .or_else(|| {
            node.get("percent_remaining")
                .and_then(Value::as_f64)
                .map(|r| 100.0 - r)
        })?
        .clamp(0.0, 100.0);

    let resets_in = node
        .get("resets_at")
        .and_then(Value::as_str)
        .and_then(parse_rfc3339)
        .map(|t| t - now);

    Some(Metric { percent_used, resets_in })
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Parse the RFC 3339 forms Claude Code writes:
/// `2026-08-24T11:00:00.143063+00:00` and `2026-08-24T00:06:25Z`.
pub fn parse_rfc3339(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() < 19 {
        return None;
    }
    let num = |r: std::ops::Range<usize>| -> Option<i64> {
        std::str::from_utf8(b.get(r)?).ok()?.parse().ok()
    };
    let (y, mo, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (h, mi, sec) = (num(11..13)?, num(14..16)?, num(17..19)?);

    let mut epoch = days_from_civil(y, mo, d) * 86_400 + h * 3600 + mi * 60 + sec;

    // Skip the optional fractional seconds, then read the offset.
    let mut i = 19;
    if b.get(i) == Some(&b'.') {
        i += 1;
        while b.get(i).is_some_and(|c| c.is_ascii_digit()) {
            i += 1;
        }
    }
    match b.get(i) {
        None | Some(b'Z') | Some(b'z') => {}
        Some(&sign @ (b'+' | b'-')) => {
            let oh: i64 = std::str::from_utf8(b.get(i + 1..i + 3)?).ok()?.parse().ok()?;
            // Both `+01:00` and `+0100` appear in the wild.
            let mstart = if b.get(i + 3) == Some(&b':') { i + 4 } else { i + 3 };
            let om: i64 = std::str::from_utf8(b.get(mstart..mstart + 2)?)
                .ok()?
                .parse()
                .ok()?;
            let offset = oh * 3600 + om * 60;
            epoch += if sign == b'+' { -offset } else { offset };
        }
        _ => return None,
    }
    Some(epoch)
}

/// Howard Hinnant's civil-date-to-days algorithm.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Render a duration as a compact `4H12M` / `38M` / `NOW` label.
pub fn human_duration(seconds: i64) -> String {
    if seconds <= 0 {
        return "NOW".into();
    }
    let (h, m) = (seconds / 3600, (seconds % 3600) / 60);
    if h >= 24 {
        let d = h / 24;
        format!("{}D{}H", d, h % 24)
    } else if h > 0 {
        format!("{h}H{m:02}M")
    } else if m > 0 {
        format!("{m}M")
    } else {
        format!("{seconds}S")
    }
}
