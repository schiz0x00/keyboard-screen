//! Reads live subscription usage off the `anthropic-ratelimit-unified-*` headers
//! of a tiny `/v1/messages` ping, authenticated with the OAuth token Claude Code
//! stores in `~/.claude/.credentials.json`.
//!
//! Billing safety: the only credential this module will ever send is a Claude
//! Code OAuth access token (`sk-ant-oat...`), which draws on the subscription.
//! `ANTHROPIC_API_KEY` is never read, no `x-api-key` header is ever set, and an
//! unrecognised credential is refused rather than guessed at - an API key
//! (`sk-ant-api03-...`) would spend prepaid credits, i.e. real money.

use crate::json::{self, Value};
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

const MESSAGES_URL: &str = "https://api.anthropic.com/v1/messages";
const TOKEN_URL: &str = "https://platform.claude.com/v1/oauth/token";
const CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";

/// Only ever this prefix. An allowlist, not a denylist.
const OAUTH_TOKEN_PREFIX: &str = "sk-ant-oat";

/// `/v1/messages` rejects an OAuth bearer token outright without this opt-in.
const OAUTH_BETA: &str = "oauth-2025-04-20";

/// The cheapest model, pinned to the dated id rather than the moving alias so a
/// realias cannot silently change what this spends.
const PING_MODEL: &str = "claude-haiku-4-5-20251001";
/// One character in, one token out - the reply is discarded; we send this
/// request for its response headers.
const PING_TEXT: &str = "h";

const VERSION_FALLBACK: &str = "2.1.241";
/// Refresh the token this long before it actually expires.
const TOKEN_SKEW_MS: i64 = 5 * 60 * 1000;

#[derive(Debug, Clone, Copy)]
pub struct Window {
    /// 0-100.
    pub percent_used: f64,
    /// Unix epoch seconds.
    pub resets_at: i64,
}

#[derive(Debug, Clone, Default)]
pub struct Live {
    pub five_hour: Option<Window>,
    pub seven_day: Option<Window>,
}

impl Live {
    pub fn is_empty(&self) -> bool {
        self.five_hour.is_none() && self.seven_day.is_none()
    }
}

#[derive(Debug)]
pub enum ApiError {
    /// Waiting will not fix it; the user must re-authenticate Claude Code.
    Auth(String),
    Transport(String),
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiError::Auth(m) => write!(f, "{m}"),
            ApiError::Transport(m) => write!(f, "{m}"),
        }
    }
}

fn credentials_path() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
    std::path::Path::new(&home).join(".claude/.credentials.json")
}

struct Oauth {
    access: String,
    refresh: String,
    expires_at_ms: i64,
}

fn load_oauth() -> Result<Oauth, ApiError> {
    let path = credentials_path();
    let text = std::fs::read_to_string(&path).map_err(|e| {
        ApiError::Auth(format!(
            "no Claude Code credentials at {} ({e}): run `claude` and sign in",
            path.display()
        ))
    })?;
    let root = json::parse(&text).map_err(|e| ApiError::Auth(format!("{}: {e}", path.display())))?;
    let node = root.get("claudeAiOauth").ok_or_else(|| {
        ApiError::Auth("credentials file has no claudeAiOauth entry: run `claude` and sign in".into())
    })?;

    Ok(Oauth {
        access: node
            .get("accessToken")
            .and_then(Value::as_str)
            .ok_or_else(|| ApiError::Auth("credentials have no accessToken".into()))?
            .to_string(),
        refresh: node
            .get("refreshToken")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        expires_at_ms: node
            .get("expiresAt")
            .and_then(Value::as_f64)
            .unwrap_or(0.0) as i64,
    })
}

/// Write the refreshed token back into the file Claude Code owns, preserving
/// every sibling key, atomically, at 0600.
fn save_access_token(oauth: &Oauth) -> Result<(), ApiError> {
    let path = credentials_path();
    let text = std::fs::read_to_string(&path)
        .map_err(|e| ApiError::Auth(format!("re-reading credentials: {e}")))?;
    let mut root = json::parse(&text).map_err(ApiError::Auth)?;

    if let Value::Obj(ref mut top) = root {
        if let Some(Value::Obj(entry)) = top.get_mut("claudeAiOauth") {
            entry.insert("accessToken".into(), Value::Str(oauth.access.clone()));
            entry.insert("refreshToken".into(), Value::Str(oauth.refresh.clone()));
            entry.insert("expiresAt".into(), Value::Num(oauth.expires_at_ms as f64));
        }
    }

    let tmp = path.with_extension("json.tmp");
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp)
            .map_err(|e| ApiError::Auth(format!("writing credentials: {e}")))?;
        f.write_all(json::to_string(&root).as_bytes())
            .and_then(|_| f.sync_all())
            .map_err(|e| ApiError::Auth(format!("writing credentials: {e}")))?;
    }
    std::fs::rename(&tmp, &path).map_err(|e| ApiError::Auth(format!("replacing credentials: {e}")))
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn expired(oauth: &Oauth) -> bool {
    oauth.expires_at_ms != 0 && now_ms() + TOKEN_SKEW_MS >= oauth.expires_at_ms
}

fn refresh(oauth: &mut Oauth) -> Result<(), ApiError> {
    if oauth.refresh.is_empty() {
        return Err(ApiError::Auth(
            "access token expired and no refresh token is stored: run `claude` and sign in".into(),
        ));
    }
    let body = format!(
        r#"{{"grant_type":"refresh_token","refresh_token":{},"client_id":"{}"}}"#,
        json::quote(&oauth.refresh),
        CLIENT_ID
    );

    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(std::time::Duration::from_secs(20)))
        .build()
        .into();

    let mut resp = agent
        .post(TOKEN_URL)
        .header("Content-Type", "application/json")
        .send(&body)
        .map_err(|e| ApiError::Transport(format!("token refresh: {e}")))?;

    let status = resp.status().as_u16();
    if status != 200 {
        return Err(ApiError::Auth(format!(
            "token refresh rejected (HTTP {status}): run `claude` and sign in"
        )));
    }

    let text = resp
        .body_mut()
        .read_to_string()
        .map_err(|e| ApiError::Transport(format!("token refresh body: {e}")))?;
    let v = json::parse(&text).map_err(ApiError::Auth)?;

    oauth.access = v
        .get("access_token")
        .and_then(Value::as_str)
        .ok_or_else(|| ApiError::Auth("token refresh returned no access_token".into()))?
        .to_string();
    if let Some(r) = v.get("refresh_token").and_then(Value::as_str) {
        oauth.refresh = r.to_string();
    }
    if let Some(secs) = v.get("expires_in").and_then(Value::as_f64) {
        oauth.expires_at_ms = now_ms() + (secs as i64) * 1000;
    }
    save_access_token(oauth)?;
    Ok(())
}

/// The header block claude-cli puts on every authenticated request.
fn cli_version() -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    let dir = std::path::Path::new(&home).join(".local/share/claude/versions");
    let mut best: Option<(Vec<u32>, String)> = None;
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let parts: Option<Vec<u32>> = name.split('.').map(|p| p.parse().ok()).collect();
            // Numeric comparison, so 2.1.220 beats 2.1.99.
            if let Some(nums) = parts {
                if best.as_ref().is_none_or(|(b, _)| nums > *b) {
                    best = Some((nums, name));
                }
            }
        }
    }
    best.map(|(_, n)| n).unwrap_or_else(|| VERSION_FALLBACK.to_string())
}

/// Send the ping and read the usage windows off its response headers.
///
/// A 429 is data, not an error: the refusal carries the same headers a success
/// would, and reading it is what lets the caller wait out an exhausted window
/// instead of retrying into a wall.
pub fn fetch() -> Result<Live, ApiError> {
    let mut oauth = load_oauth()?;
    if expired(&oauth) {
        refresh(&mut oauth)?;
    }

    match ping(&oauth) {
        Ok(live) => Ok(live),
        Err(ApiError::Auth(_)) if !oauth.refresh.is_empty() => {
            // A 401 on a token we believed was live: refresh once and retry.
            refresh(&mut oauth)?;
            ping(&oauth)
        }
        Err(e) => Err(e),
    }
}

fn ping(oauth: &Oauth) -> Result<Live, ApiError> {
    if !oauth.access.starts_with(OAUTH_TOKEN_PREFIX) {
        return Err(ApiError::Auth(format!(
            "refusing to send: the stored credential is not a Claude Code OAuth token \
             (expected {OAUTH_TOKEN_PREFIX}...). Usage must only ever be measured against \
             the subscription, never against API billing."
        )));
    }

    let body = format!(
        r#"{{"model":"{}","max_tokens":1,"messages":[{{"role":"user","content":{}}}]}}"#,
        PING_MODEL,
        json::quote(PING_TEXT)
    );

    // Status codes must not be raised as errors: a 429 carries the very same
    // rate-limit headers a 200 does, and reading them is the whole point.
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(std::time::Duration::from_secs(20)))
        .build()
        .into();

    let session_id = pseudo_uuid();
    let result = agent
        .post(MESSAGES_URL)
        .header("Authorization", &format!("Bearer {}", oauth.access))
        .header("anthropic-version", "2023-06-01")
        .header("anthropic-beta", OAUTH_BETA)
        .header("Content-Type", "application/json")
        .header("x-app", "cli")
        .header("User-Agent", &format!("claude-cli/{} (external, cli)", cli_version()))
        .header("X-Claude-Code-Session-Id", &session_id)
        .send(&body);

    let resp = result.map_err(|e| ApiError::Transport(format!("usage ping: {e}")))?;
    let status = resp.status().as_u16();
    let live = windows_from(resp.headers());

    // A 429 means a window is spent - which the headers already told us. Only
    // treat it as a failure if it arrived without them.
    if !live.is_empty() {
        return Ok(live);
    }
    match status {
        401 | 403 => Err(ApiError::Auth(format!(
            "token rejected (HTTP {status}): run `claude` and sign in"
        ))),
        200 => Err(ApiError::Transport(
            "usage ping succeeded but carried no anthropic-ratelimit-unified-* headers".into(),
        )),
        _ => Err(ApiError::Transport(format!("usage ping failed (HTTP {status})"))),
    }
}

fn windows_from(headers: &ureq::http::HeaderMap) -> Live {
    let read = |prefix: &str| -> Option<Window> {
        let get = |suffix: &str| -> Option<&str> {
            headers
                .get(format!("anthropic-ratelimit-unified-{prefix}-{suffix}"))?
                .to_str()
                .ok()
        };
        // Headers give a 0-1 fraction and a unix epoch.
        let utilization: f64 = get("utilization")?.parse().ok()?;
        let reset: i64 = get("reset")?.parse().ok()?;
        Some(Window {
            percent_used: (utilization * 100.0).clamp(0.0, 100.0),
            resets_at: reset,
        })
    };
    Live {
        five_hour: read("5h"),
        seven_day: read("7d"),
    }
}

/// A random v4-shaped session id; claude-cli sends one per session.
fn pseudo_uuid() -> String {
    let mut b = [0u8; 16];
    // getrandom(2) - no crate needed.
    unsafe {
        libc::getrandom(b.as_mut_ptr() as *mut libc::c_void, b.len(), 0);
    }
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: Vec<String> = b.iter().map(|x| format!("{x:02x}")).collect();
    let h = h.concat();
    format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32])
}
