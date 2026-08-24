//! Draw the Claude subscription usage limits onto the Aula F75 Max screen.

mod api;
mod canvas;
mod display;
mod font;
mod hid;
mod json;
mod probe;
mod render;
mod state;
mod usage;

use state::{Snapshot, Source};
use std::time::{Duration, Instant};

const USAGE: &str = "\
keyboard-screen - Claude usage limits on the Aula F75 Max 128x128 screen

USAGE:
    keyboard-screen [OPTIONS]

OPTIONS:
    -w, --watch [SECS]   Keep refreshing (default 60s, minimum 60s)
    -s, --slot N         Display slot to write, 1-255 (default 1)
    -n, --no-refresh     Do not ping the API; use cached / Claude Code data only
    -o, --preview FILE   Also write the frame as a PPM image for inspection
        --dry-run        Render but do not touch the keyboard
    -v, --verbose        Log what is happening
        --probe-wireless Investigate the 2.4GHz dongle (read-only) and exit
    -h, --help           Show this help

Usage is read from the anthropic-ratelimit-unified-* headers of a ~9-token
Haiku ping, authenticated with the Claude Code OAuth token. It spends
subscription quota, never API credits.
";

struct Args {
    watch: Option<u64>,
    slot: u8,
    refresh: bool,
    preview: Option<String>,
    dry_run: bool,
    verbose: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut a = Args {
        watch: None,
        slot: 1,
        refresh: true,
        preview: None,
        dry_run: false,
        verbose: false,
    };
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < argv.len() {
        match argv[i].as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                std::process::exit(0);
            }
            "-w" | "--watch" => {
                // The interval is optional; only consume a following numeric arg.
                let secs = argv
                    .get(i + 1)
                    .and_then(|v| v.parse::<u64>().ok())
                    .inspect(|_| i += 1)
                    .unwrap_or(60);
                // A ping costs quota; below a minute it is pure waste.
                a.watch = Some(secs.max(60));
            }
            "-s" | "--slot" => {
                i += 1;
                a.slot = argv
                    .get(i)
                    .ok_or("--slot needs a value")?
                    .parse()
                    .map_err(|_| "--slot must be 1-255")?;
            }
            "-n" | "--no-refresh" => a.refresh = false,
            "-o" | "--preview" => {
                i += 1;
                a.preview = Some(argv.get(i).ok_or("--preview needs a path")?.clone());
            }
            "--dry-run" => a.dry_run = true,
            "--probe-wireless" => {
                if let Err(e) = crate::probe::run(10) {
                    eprintln!("error: {e}");
                    std::process::exit(1);
                }
                std::process::exit(0);
            }
            "-v" | "--verbose" => a.verbose = true,
            other => return Err(format!("unknown option: {other}")),
        }
        i += 1;
    }
    Ok(a)
}

fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: {e}\n\n{USAGE}");
            std::process::exit(2);
        }
    };

    match args.watch {
        None => {
            if let Err(e) = tick(&args) {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        Some(interval) => watch(&args, interval),
    }
}

/// One refresh-render-upload cycle.
fn tick(args: &Args) -> Result<(), String> {
    let snap = collect(args);
    if args.verbose {
        eprintln!(
            "source={} age={}s 5h={:?} week={:?}",
            snap.source.label(),
            snap.age,
            snap.session.map(|m| m.percent_used),
            snap.week.map(|m| m.percent_used),
        );
    }

    let canvas = render::dashboard(&snap);

    if let Some(path) = &args.preview {
        write_ppm(path, &canvas)?;
        if args.verbose {
            eprintln!("preview written to {path}");
        }
    }
    if args.dry_run {
        return Ok(());
    }
    display::upload(&display::encode(&canvas), args.slot, args.verbose)
}

/// Live ping first; fall back through our cache to Claude Code's own file, so
/// the screen degrades to "old numbers, labelled old" rather than to nothing.
fn collect(args: &Args) -> Snapshot {
    if args.refresh {
        match api::fetch() {
            Ok(live) if !live.is_empty() => {
                state::save_cache(&live);
                return Snapshot::from_live(&live);
            }
            Ok(_) => eprintln!("warning: ping returned no usage headers"),
            Err(e) => eprintln!("warning: live refresh failed: {e}"),
        }
    }
    state::load_cache()
        .or_else(state::load_claude_code)
        .unwrap_or(Snapshot {
            session: None,
            week: None,
            source: Source::Cache,
            age: 0,
        })
}

/// Refresh forever.
///
/// Two things make this steadier than a plain `sleep(interval)` loop:
/// a suspended laptop wakes to a wall clock that has jumped, and a failed ping
/// must not wait a full interval before trying again.
fn watch(args: &Args, interval: u64) {
    let mut wall = state::now();
    let mut failures: u32 = 0;

    loop {
        let started = Instant::now();
        let ok = tick(args).map_err(|e| eprintln!("error: {e}")).is_ok();
        failures = if ok { 0 } else { (failures + 1).min(6) };

        // Back off on repeated failure, but never past one interval * 8.
        let mut wait = if failures == 0 {
            interval
        } else {
            (interval * 2u64.pow(failures.min(3))).min(interval * 8)
        };
        wait = wait.saturating_sub(started.elapsed().as_secs());

        // Sleep in short slices so a resumed machine is noticed promptly: if the
        // wall clock jumped further than the monotonic clock, we were suspended
        // and the numbers on screen are older than they claim.
        let deadline = Instant::now() + Duration::from_secs(wait);
        while Instant::now() < deadline {
            std::thread::sleep(Duration::from_secs(2).min(deadline - Instant::now()));
            let now = state::now();
            let drift = now - wall - 2;
            wall = now;
            if drift > 30 {
                if args.verbose {
                    eprintln!("clock jumped {drift}s (resume?) - refreshing now");
                }
                break;
            }
        }
        wall = state::now();
    }
}

/// Binary PPM, for eyeballing the frame without a keyboard attached.
fn write_ppm(path: &str, c: &canvas::Canvas) -> Result<(), String> {
    let mut out = format!("P6\n{} {}\n255\n", canvas::W, canvas::H).into_bytes();
    for y in 0..canvas::H as i32 {
        for x in 0..canvas::W as i32 {
            let p = c.get(x, y);
            out.extend_from_slice(&[p.0, p.1, p.2]);
        }
    }
    std::fs::write(path, out).map_err(|e| format!("writing {path}: {e}"))
}
