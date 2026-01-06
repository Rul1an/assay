#!/bin/sh
#
# Assay Installer
# https://getassay.dev
#
# Usage:
#   curl -fsSL https://getassay.dev/install.sh | sh
#   curl -fsSL https://getassay.dev/install.sh | ASSAY_VERSION=1.3.0 sh
#

set -e

# --- Configuration ---
GITHUB_REPO="Rul1an/assay"
BINARY_NAME="assay"
INSTALL_DIR="${ASSAY_INSTALL_DIR:-$HOME/.local/bin}"
VERSION="${ASSAY_VERSION:-latest}"

# --- Colors ---
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
BOLD='\033[1m'
NC='\033[0m'

# --- Helpers ---
log_info() { printf "${BLUE}${BOLD}[INFO]${NC} %s\n" "$1"; }
log_success() { printf "${GREEN}${BOLD}[OK]${NC} %s\n" "$1"; }
log_warn() { printf "${YELLOW}${BOLD}[WARN]${NC} %s\n" "$1"; }
log_error() { printf "${RED}${BOLD}[ERROR]${NC} %s\n" "$1"; exit 1; }

# --- Main ---
main() {
    printf "${BOLD}✨ Assay Installer${NC}\n"
    printf "\n"

    # 1. Detect OS & Arch
    OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
    ARCH="$(uname -m)"

    case "$OS" in
        linux)
            TARGET_OS="unknown-linux-gnu"
            ;;
        darwin)
            TARGET_OS="apple-darwin"
            ;;
        mingw*|msys*)
            OS="windows"
            TARGET_OS="pc-windows-msvc"
            ;;
        *)
            log_error "Unsupported OS: $OS"
            ;;
    esac

    case "$ARCH" in
        x86_64|amd64)
            TARGET_ARCH="x86_64"
            ;;
        arm64|aarch64)
            TARGET_ARCH="aarch64"
            ;;
        *)
            log_error "Unsupported architecture: $ARCH"
            ;;
    esac

    TARGET="${TARGET_ARCH}-${TARGET_OS}"
    log_info "Detected platform: $OS/$ARCH ($TARGET)"

    # 2. Resolve Version
    if [ "$VERSION" = "latest" ]; then
        log_info "Resolving latest version..."
        # Fetch latest release tag from GitHub API
        RELEASE_JSON=$(curl -fsSL "https://api.github.com/repos/$GITHUB_REPO/releases/latest")
        if [ -z "$RELEASE_JSON" ]; then
             log_error "Failed to contact GitHub API."
        fi
        VERSION=$(echo "$RELEASE_JSON" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')
        if [ -z "$VERSION" ]; then
            log_error "Failed to resolve latest version."
        fi
    fi

    log_info "Target version: $VERSION"

    # 3. Construct Download URL
    if [ "$OS" = "windows" ]; then
        ARCHIVE_NAME="assay-${VERSION}-${TARGET}.zip"
    else
        ARCHIVE_NAME="assay-${VERSION}-${TARGET}.tar.gz"
    fi

    DOWNLOAD_URL="https://github.com/$GITHUB_REPO/releases/download/$VERSION/$ARCHIVE_NAME"

    # 4. Download
    TMP_DIR=$(mktemp -d)
    trap 'rm -rf "$TMP_DIR"' EXIT

    log_info "Downloading from $DOWNLOAD_URL ..."
    # Check if curl supports -w
    if command -v curl >/dev/null 2>&1; then
        HTTP_CODE=$(curl -fsSL -w "%{http_code}" -o "$TMP_DIR/$ARCHIVE_NAME" "$DOWNLOAD_URL")
        if [ "$HTTP_CODE" != "200" ]; then
            log_error "Download failed (HTTP $HTTP_CODE). URL: $DOWNLOAD_URL"
        fi
    else
        log_error "curl is required but not found."
    fi

    # 5. Extract
    cd "$TMP_DIR"
    log_info "Extracting ..."
    EXTRACTED_DIR="assay-${VERSION}-${TARGET}"

    if [ "$OS" = "windows" ]; then
        if ! command -v unzip >/dev/null 2>&1; then
             log_error "unzip is required for Windows installation."
        fi
        unzip -q "$ARCHIVE_NAME"
    else
        tar xzkf "$ARCHIVE_NAME"
    fi

    # 6. Install
    if [ ! -d "$INSTALL_DIR" ]; then
        mkdir -p "$INSTALL_DIR"
    fi

    if [ "$OS" = "windows" ]; then
        # Check if extracted dir structure is correct, fallback to look for binary
        if [ -f "$EXTRACTED_DIR/assay.exe" ]; then
             mv "$EXTRACTED_DIR/assay.exe" "$INSTALL_DIR/assay.exe"
        elif [ -f "assay.exe" ]; then
             mv "assay.exe" "$INSTALL_DIR/assay.exe"
        else
             log_error "Could not find assay.exe after extraction"
        fi
    else
        if [ -f "$EXTRACTED_DIR/assay" ]; then
             mv "$EXTRACTED_DIR/assay" "$INSTALL_DIR/assay"
        elif [ -f "assay" ]; then
             mv "assay" "$INSTALL_DIR/assay"
        else
             log_error "Could not find assay binary after extraction"
        fi
        chmod +x "$INSTALL_DIR/assay"
    fi

    printf "\n"
    log_success "Assay installed to: $INSTALL_DIR/assay"

    # 7. Path Check (POSIX compliant)
    case ":$PATH:" in
        *":$INSTALL_DIR:"*) ;;
        *)
            printf "\n"
            log_warn "Your path is missing $INSTALL_DIR"
            printf "   Add this to your shell config (~/.zshrc, ~/.bashrc):\n"
            printf "   ${BOLD}export PATH=\"\$PATH:$INSTALL_DIR\"${NC}\n"
            printf "\n"
            ;;
    esac

    printf "Run ${BOLD}assay --help${NC} to get started.\n"
}

main "$@"
