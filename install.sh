#!/usr/bin/env bash
set -euo pipefail

REPO="devpolo/wisprnito"
WISPRNITO_VERSION="${WISPRNITO_VERSION:-latest}"
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"

info()  { echo "==> $*"; }
error() { echo "ERROR: $*" >&2; exit 1; }

detect_os() {
    case "$(uname -s)" in
        Darwin) OS="darwin" ;;
        Linux)  OS="linux" ;;
        *)      error "Unsupported OS: $(uname -s)" ;;
    esac
}

detect_arch() {
    case "$(uname -m)" in
        x86_64|amd64)   ARCH="x86_64" ;;
        arm64|aarch64)   ARCH="arm64" ;;
        *)               error "Unsupported architecture: $(uname -m)" ;;
    esac
}

get_download_url() {
    local version="$1"
    local asset="wisprnito-${OS}-${ARCH}.tar.gz"

    if [ "$version" = "latest" ]; then
        echo "https://github.com/${REPO}/releases/latest/download/${asset}"
    else
        echo "https://github.com/${REPO}/releases/download/${version}/${asset}"
    fi
}

install_macos() {
    # Check Homebrew
    if ! command -v brew &>/dev/null; then
        info "Homebrew not found. Install it from https://brew.sh"
    fi

    # Check BlackHole
    if [ ! -d "/Library/Audio/Plug-Ins/HAL/BlackHole2ch.driver" ]; then
        info "BlackHole 2ch not found. Installing via Homebrew..."
        if command -v brew &>/dev/null; then
            brew install --cask blackhole-2ch
        else
            error "BlackHole not installed and Homebrew not available. Install BlackHole manually: https://existential.audio/blackhole/"
        fi
    else
        info "BlackHole 2ch is already installed."
    fi

    # Download binary
    local url
    url="$(get_download_url "$WISPRNITO_VERSION")"
    info "Downloading wisprnito from ${url}..."

    local tmpdir
    tmpdir="$(mktemp -d)"
    trap 'rm -rf "$tmpdir"' EXIT

    curl -fSL "$url" -o "${tmpdir}/wisprnito.tar.gz"
    tar -xzf "${tmpdir}/wisprnito.tar.gz" -C "${tmpdir}"

    # Remove quarantine
    xattr -d com.apple.quarantine "${tmpdir}/wisprnito" 2>/dev/null || true

    # Install
    info "Installing to ${INSTALL_DIR}/wisprnito..."
    sudo install -m 755 "${tmpdir}/wisprnito" "${INSTALL_DIR}/wisprnito"

    install_launchagent

    info "Done! Start now: wisprnito start"
    info "Then set BlackHole 2ch as mic in System Settings → Sound → Input"
    info "To uninstall: wisprnito uninstall"
}

install_launchagent() {
    local plist=~/Library/LaunchAgents/com.devpolo.wisprnito.plist
    cat > "$plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>Label</key><string>com.devpolo.wisprnito</string>
  <key>ProgramArguments</key>
  <array><string>/usr/local/bin/wisprnito</string><string>start</string></array>
  <key>RunAtLoad</key><true/>
</dict></plist>
PLIST
    launchctl load "$plist" 2>/dev/null || true
    info "LaunchAgent installed — wisprnito will auto-start on login."
}

install_linux() {
    # Check PulseAudio
    if ! command -v pactl &>/dev/null; then
        error "PulseAudio (pactl) not found. Install pulseaudio-utils first."
    fi

    info "PulseAudio is available."

    # Download binary
    local url
    url="$(get_download_url "$WISPRNITO_VERSION")"
    info "Downloading wisprnito from ${url}..."

    local tmpdir
    tmpdir="$(mktemp -d)"
    trap 'rm -rf "$tmpdir"' EXIT

    curl -fSL "$url" -o "${tmpdir}/wisprnito.tar.gz"
    tar -xzf "${tmpdir}/wisprnito.tar.gz" -C "${tmpdir}"

    # Install
    info "Installing to ${INSTALL_DIR}/wisprnito..."
    sudo install -m 755 "${tmpdir}/wisprnito" "${INSTALL_DIR}/wisprnito"

    # Setup null sink
    info "Setting up PulseAudio null sink..."
    pactl load-module module-null-sink sink_name=wisprnito sink_properties=device.description=Wisprnito 2>/dev/null || true
    pactl load-module module-loopback source=wisprnito.monitor 2>/dev/null || true
}

main() {
    info "Wisprnito installer"
    detect_os
    detect_arch
    info "Detected: ${OS}/${ARCH}"

    case "$OS" in
        darwin) install_macos ;;
        linux)  install_linux ;;
    esac

    # Verify
    if command -v wisprnito &>/dev/null; then
        info "Installation successful!"
        wisprnito --version
    else
        error "Installation failed: wisprnito not found in PATH"
    fi
}

main "$@"
