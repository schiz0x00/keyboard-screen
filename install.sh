#!/bin/sh
# Install keyboard-screen: Claude usage limits on the Aula F75 Max screen.
#
#   curl -fsSL https://raw.githubusercontent.com/schiz0x00/keyboard-screen/main/install.sh | sh
#
# Fetches the latest release binary; falls back to building from source when no
# release is available or the machine is not x86_64. Then installs the udev rule
# so the screen is writable without root.
set -eu

REPO="${KEYBOARD_SCREEN_REPO:-schiz0x00/keyboard-screen}"
BIN_DIR="${KEYBOARD_SCREEN_BIN_DIR:-$HOME/.local/bin}"
UDEV_RULE="/etc/udev/rules.d/99-aula-f75max.rules"
SKIP_UDEV="${KEYBOARD_SCREEN_SKIP_UDEV:-0}"

# Epomaker x Aula F75 Max. uaccess hands the logged-in user the hidraw nodes.
# The screen is wired-only; the 2.4GHz dongle has no pixel pipe.
UDEV_RULE_TEXT='KERNEL=="hidraw*", ATTRS{idVendor}=="0c45", ATTRS{idProduct}=="800a", TAG+="uaccess"'
udev_manual=0
NAME="keyboard-screen"

say()  { printf '\033[1;34m::\033[0m %s\n' "$1"; }
warn() { printf '\033[1;33m!!\033[0m %s\n' "$1" >&2; }
die()  { printf '\033[1;31mxx\033[0m %s\n' "$1" >&2; exit 1; }

need() { command -v "$1" >/dev/null 2>&1; }

# Prefer sudo only when we are not already root.
SUDO=""
if [ "$(id -u)" -ne 0 ]; then
    if need sudo; then SUDO="sudo"; else warn "no sudo found; the udev rule will be skipped"; fi
fi

# ---------------------------------------------------------------- fetch binary
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

download_release() {
    need curl || return 1
    [ "$(uname -s)" = "Linux" ] || return 1
    [ "$(uname -m)" = "x86_64" ] || return 1

    say "Looking for the latest release of $REPO"
    url="$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" 2>/dev/null \
        | grep -o '"browser_download_url": *"[^"]*linux-x86_64[^"]*"' \
        | head -n1 | cut -d'"' -f4)" || return 1
    [ -n "$url" ] || return 1

    say "Downloading $url"
    curl -fsSL "$url" -o "$tmp/dl" || return 1

    # Verify against the release's SHA256SUMS when we can. A failed checksum is
    # fatal: silently installing a corrupted or tampered binary is worse than
    # not installing at all.
    sums="$(printf '%s' "$url" | sed 's|/[^/]*$|/SHA256SUMS|')"
    if need sha256sum && curl -fsSL "$sums" -o "$tmp/SHA256SUMS" 2>/dev/null; then
        want="$(awk -v f="$(basename "$url")" '$2 == f || $2 == "./" f {print $1}' "$tmp/SHA256SUMS" | head -n1)"
        if [ -n "$want" ]; then
            got="$(sha256sum "$tmp/dl" | cut -d" " -f1)"
            [ "$want" = "$got" ] || die "checksum mismatch for $(basename "$url")
  expected $want
  got      $got"
            say "Checksum verified"
        else
            warn "No checksum listed for $(basename "$url"); continuing unverified"
        fi
    else
        warn "Could not verify the download checksum"
    fi

    # Releases may ship the bare binary or a tarball.
    case "$url" in
        *.tar.gz|*.tgz) tar -xzf "$tmp/dl" -C "$tmp" || return 1
                        found="$(find "$tmp" -type f -name "$NAME" | head -n1)"
                        [ -n "$found" ] || return 1
                        mv "$found" "$tmp/$NAME" ;;
        *)              mv "$tmp/dl" "$tmp/$NAME" ;;
    esac
    chmod +x "$tmp/$NAME"
    return 0
}

build_locally() {
    need cargo || die "no release available and cargo is not installed.
Install Rust from https://rustup.rs and re-run, or grab a release from
https://github.com/$REPO/releases"

    # Run from the repo when the script sits in it; otherwise fetch the source.
    src="$(unset CDPATH; cd -- "$(dirname -- "$0")" && pwd)"
    if [ ! -f "$src/Cargo.toml" ]; then
        need git || die "need git to fetch the source"
        say "Cloning $REPO"
        git clone --depth 1 "https://github.com/$REPO" "$tmp/src" >/dev/null 2>&1 \
            || die "could not clone https://github.com/$REPO"
        src="$tmp/src"
    fi

    say "Building from source in $src (this takes a minute)"
    ( cd "$src" && cargo build --release ) || die "build failed"
    cp "$src/target/release/$NAME" "$tmp/$NAME"
}

if ! download_release; then
    warn "No release binary available; building from source instead."
    build_locally
fi

# --------------------------------------------------------------- install binary
mkdir -p "$BIN_DIR"
install -m 755 "$tmp/$NAME" "$BIN_DIR/$NAME"
say "Installed $BIN_DIR/$NAME"

case ":$PATH:" in
    *":$BIN_DIR:"*) ;;
    *) warn "$BIN_DIR is not on your PATH. Add this to your shell profile:"
       # shellcheck disable=SC2016  # $PATH is meant to stay literal here
       printf '       export PATH="%s:$PATH"\n' "$BIN_DIR" ;;
esac

# ------------------------------------------------------------------- udev rule
# The screen is driven over hidraw; without this the device nodes are root-only.
if [ "$SKIP_UDEV" = "1" ]; then
    say "Skipping the udev rule (KEYBOARD_SCREEN_SKIP_UDEV=1)"
elif [ -n "$SUDO" ] || [ "$(id -u)" -eq 0 ]; then
    say "Installing udev rule at $UDEV_RULE (may prompt for your password)"
    # Never fatal: the binary is already installed and useful, and the rule can
    # be added by hand. A non-interactive shell has no tty for sudo to prompt on.
    if printf '%s\n' "$UDEV_RULE_TEXT" | $SUDO tee "$UDEV_RULE" >/dev/null 2>&1; then
        $SUDO udevadm control --reload >/dev/null 2>&1 || warn "udevadm reload failed"
        $SUDO udevadm trigger >/dev/null 2>&1 || warn "udevadm trigger failed"
        say "Replug the keyboard for the rule to take effect."
    else
        warn "Could not write $UDEV_RULE (no sudo password prompt available?)."
        udev_manual=1
    fi
else
    udev_manual=1
fi

if [ "${udev_manual:-0}" = "1" ]; then
    warn "The udev rule was not installed. Without it the screen needs root."
    printf '   Run this yourself:\n\n       echo %s | sudo tee %s\n       sudo udevadm control --reload && sudo udevadm trigger\n\n' \
        "'$UDEV_RULE_TEXT'" "$UDEV_RULE"
fi

# ---------------------------------------------------------------------- finish
cat <<EOF

Done. Try it with the keyboard connected over USB-C:

    $NAME                 # refresh once and push to the screen
    $NAME --watch 60      # keep it updated

To run it continuously:

    mkdir -p ~/.config/systemd/user
    curl -fsSL https://raw.githubusercontent.com/$REPO/main/systemd/keyboard-screen.service \\
        -o ~/.config/systemd/user/keyboard-screen.service
    systemctl --user enable --now keyboard-screen

EOF
