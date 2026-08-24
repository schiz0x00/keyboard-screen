# Contributing

## Getting set up

```sh
cargo build --release
KEYBOARD_SCREEN_SKIP_UDEV=1 ./install.sh   # or just run target/release/keyboard-screen
```

## Before opening a pull request

```sh
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test
shellcheck -s sh install.sh
```

## Testing changes to the screen

Use `--test-pattern`, not the real dashboard. If the usage numbers have not
moved, a successful push and a silently dropped one look identical — that
mistake cost real debugging time once already.

```sh
cargo run --release -- --test-pattern            # push an unmistakable frame
cargo run --release -- --dry-run -o /tmp/f.ppm   # render with no keyboard attached
```

## Releasing

Bump `version` in `Cargo.toml`, run `cargo build` so `Cargo.lock` picks up the
new version, add the entry to `CHANGELOG.md`, and merge to `main`. The release
workflow tags, builds, and publishes automatically.

Forgetting the lock file fails the build under `--locked`; CI checks this too,
so it surfaces on the pull request rather than at release time.

## A note on the wireless screen

Please read `docs/wireless-re.md` before proposing wireless display support. It
is not possible: the dongle exposes no bulk endpoint, the vendor's own software
no-ops in 2.4GHz mode, and four framings have already been tested on hardware.
Wireless *settings* (RGB, battery) do work and would be welcome.
