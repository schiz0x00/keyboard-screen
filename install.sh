#!/bin/sh
# Install keyboard-screen: Claude usage limits on the Aula F75 Max screen.
#
#   curl -fsSL https://raw.githubusercontent.com/OWNER/keyboard-screen/main/install.sh | sh
#
# Fetches the latest release binary; falls back to building from source when no
# release is available or the machine is not x86_64. Then installs the udev rule
# so the screen is writable without root.
set -eu

REPO="${KEYBOARD_SCREEN_REPO:-OWNER/keyboard-screen}"
BIN_DIR="${KEYBOARD_SCREEN_BIN_DIR:-$HOME/.local/bin}"
UDEV_RULE="/etc/udev/rules.d/99-aula-f75max.rules"
SKIP_UDEV="${KEYBOARD_SCREEN_SKIP_UDEV:-0}"
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
    say "Installing udev rule at $UDEV_RULE"
    $SUDO sh -c "cat > '$UDEV_RULE'" <<'RULE'
# Epomaker x Aula F75 Max - let the logged-in user drive the 128x128 screen
# without root. The screen is wired-only; the 2.4GHz dongle has no pixel pipe.
KERNEL=="hidraw*", ATTRS{idVendor}=="0c45", ATTRS{idProduct}=="800a", TAG+="uaccess"
RULE
    $SUDO udevadm control --reload >/dev/null 2>&1 || warn "udevadm reload failed"
    $SUDO udevadm trigger >/dev/null 2>&1 || warn "udevadm trigger failed"
    say "Replug the keyboard for the rule to take effect."
else
    warn "Skipped the udev rule. Install it yourself with:"
    printf '       sudo tee %s <<< %s\n' "$UDEV_RULE" \
        '"KERNEL==\"hidraw*\", ATTRS{idVendor}==\"0c45\", ATTRS{idProduct}==\"800a\", TAG+=\"uaccess\""'
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
