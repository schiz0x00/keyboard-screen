//! What the screen is drawn from, and where it came from.
//!
//! Freshness is the whole point of this program, so a snapshot always carries
//! its own age and provenance - a number with no timestamp is how a screen ends
//! up quietly lying for hours.

use crate::api::Live;
use crate::json::{self, Value};
use crate::usage::{self, Metric};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// Straight off this run's ping. The only genuinely current source.
    Live,
    /// Our own last good ping, replayed after a failed one.
    Cache,
    /// Claude Code's `usage-exact.json` - only as current as its last session.
    ClaudeCode,
}

impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Source::Live => "LIVE",
            Source::Cache => "CACHED",
            Source::ClaudeCode => "CC FILE",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub session: Option<Metric>,
    pub week: Option<Metric>,
    pub source: Source,
    /// Seconds since the numbers were measured.
    pub age: i64,
}

pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

impl Snapshot {
    pub fn from_live(live: &Live) -> Self {
        let now = now();
        let conv = |w: Option<crate::api::Window>| {
            w.map(|w| Metric {
                percent_used: w.percent_used,
                resets_in: Some(w.resets_at - now),
            })
        };
        Snapshot {
            session: conv(live.five_hour),
            week: conv(live.seven_day),
            source: Source::Live,
            age: 0,
        }
    }
}

fn cache_path() -> std::path::PathBuf {
    let base = std::env::var("XDG_STATE_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
            std::path::Path::new(&home).join(".local/state")
        });
    base.join("keyboard-screen/usage.json")
}

/// Persist a good reading so a restart - or a network blip - still has numbers.
pub fn save_cache(live: &Live) {
    let mut fields = std::collections::BTreeMap::new();
    fields.insert("measured_at".to_string(), Value::Num(now() as f64));
    for (key, w) in [("five_hour", live.five_hour), ("seven_day", live.seven_day)] {
        if let Some(w) = w {
            let mut o = std::collections::BTreeMap::new();
            o.insert("percent_used".to_string(), Value::Num(w.percent_used));
            o.insert("resets_at".to_string(), Value::Num(w.resets_at as f64));
            fields.insert(key.to_string(), Value::Obj(o));
        }
    }
    let path = cache_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let tmp = path.with_extension("tmp");
    if std::fs::write(&tmp, json::to_string(&Value::Obj(fields))).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
}

pub fn load_cache() -> Option<Snapshot> {
    let text = std::fs::read_to_string(cache_path()).ok()?;
    let root = json::parse(&text).ok()?;
    let measured = root.get("measured_at")?.as_f64()? as i64;
    let now = now();

    let read = |key: &str| -> Option<Metric> {
        let node = root.get(key)?;
        Some(Metric {
            percent_used: node.get("percent_used")?.as_f64()?,
            resets_in: Some(node.get("resets_at")?.as_f64()? as i64 - now),
        })
    };

    let snap = Snapshot {
        session: read("five_hour"),
        week: read("seven_day"),
        source: Source::Cache,
        age: now - measured,
    };
    if snap.session.is_none() && snap.week.is_none() {
        return None;
    }
    Some(snap)
}

/// Last resort: whatever Claude Code last wrote for itself.
pub fn load_claude_code() -> Option<Snapshot> {
    let u = usage::load(&usage::default_path()).ok()?;
    if u.session.is_none() && u.week_all.is_none() {
        return None;
    }
    Some(Snapshot {
        session: u.session,
        week: u.week_all,
        source: Source::ClaudeCode,
        age: u.staleness.unwrap_or(0).max(0),
    })
}
