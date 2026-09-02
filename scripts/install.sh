#!/bin/sh
#
# Assay Installer
# https://getassay.dev
#
# Usage:
#   curl -fsSL https://getassay.dev/install.sh | sh
#   curl -fsSL https://getassay.dev/install.sh | ASSAY_VERSION=1.3.0 sh
#
# Canonical explicit input is X.Y.Z (no leading v). ASSAY_VERSION=1.3.0 and
# ASSAY_VERSION=v1.3.0 both install the v1.3.0 release tag and archive.
# `latest` is unchanged. Any other value is rejected before network access.
#

set -e

# --- Configuration ---
GITHUB_REPO="Rul1an/assay"

INSTALL_DIR="${ASSAY_INSTALL_DIR:-$HOME/.local/bin}"
case "$INSTALL_DIR" in
    /*) ;;
    *) INSTALL_DIR="$(pwd)/$INSTALL_DIR" ;;
esac
# Unset defaults to latest. An explicitly empty value is malformed and must
# not be rewritten to latest (that would hit the network).
if [ -z "${ASSAY_VERSION+x}" ]; then
    VERSION="latest"
else
    VERSION="$ASSAY_VERSION"
fi

# Provenance verification is opt-in because it requires authenticated GitHub CLI
# access (an authenticated session or GH_TOKEN) and the attestation service. Any explicitly set value other
# than 1 is malformed; it must not silently become checksum-only mode.
if [ -z "${ASSAY_REQUIRE_PROVENANCE+x}" ]; then
    REQUIRE_PROVENANCE=0
elif [ "$ASSAY_REQUIRE_PROVENANCE" = "1" ]; then
    REQUIRE_PROVENANCE=1
else
    printf '%s\n' "ASSAY_REQUIRE_PROVENANCE must be unset or exactly 1" >&2
    exit 1
fi

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

# One predicate for "this is a published stable software tag". Total /
# fail-closed for empty, whitespace, slash, and `..` so line-oriented
# grep cannot accept a multiline extraction. Explicit and latest both
# use this function; normalize does not repeat the reject rule.
is_stable_release_tag() {
    case "$1" in
        ""|*[[:space:]]*|*/*|*..*)
            return 1
            ;;
    esac
    printf '%s\n' "$1" | grep -Eq '^v[0-9]+[.][0-9]+[.][0-9]+$'
}

# latest is preserved. Exactly one optional leading v is accepted; the
# result is the vX.Y.Z tag/archive form, or a failure before any download.
normalize_install_version() {
    if [ "$1" = "latest" ]; then
        printf '%s\n' "latest"
        return 0
    fi
    _candidate="$1"
    case "$_candidate" in
        v*) ;;
        *) _candidate="v${_candidate}" ;;
    esac
    if ! is_stable_release_tag "$_candidate"; then
        return 1
    fi
    printf '%s\n' "$_candidate"
}

compute_sha256() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{print $1}'
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | awk '{print $1}'
    else
        log_error "sha256sum or shasum is required but not found."
    fi
}

download_file() {
    _download_url="$1"
    _download_path="$2"
    _download_limit="${3:-}"
    if [ -n "$_download_limit" ]; then
        _http_code=$(curl -fsSL --max-filesize "$_download_limit" -w "%{http_code}" -o "$_download_path" "$_download_url") || \
            log_error "Download failed. URL: $_download_url"
    elif ! _http_code=$(curl -fsSL -w "%{http_code}" -o "$_download_path" "$_download_url"); then
        log_error "Download failed. URL: $_download_url"
    fi
    if [ "$_http_code" != "200" ]; then
        log_error "Download failed (HTTP $_http_code). URL: $_download_url"
    fi
}

verify_archive_checksum() {
    _archive_path="$1"
    _sidecar_path="$2"
    _archive_name="$3"

    _expected_record_bytes=$((64 + 2 + ${#_archive_name} + 1))
    _sidecar_bytes=$(wc -c < "$_sidecar_path" | tr -d '[:space:]')
    if [ "$_sidecar_bytes" != "$_expected_record_bytes" ]; then
        log_error "Checksum sidecar must contain exactly one newline-terminated record."
    fi
    _line_count=$(wc -l < "$_sidecar_path" | tr -d '[:space:]')
    if [ "$_line_count" != "1" ]; then
        log_error "Checksum sidecar must contain exactly one newline-terminated record."
    fi
    if ! IFS= read -r _checksum_line < "$_sidecar_path"; then
        log_error "Checksum sidecar is empty."
    fi
    _record_bytes=$(printf '%s\n' "$_checksum_line" | wc -c | tr -d '[:space:]')
    if [ "$_sidecar_bytes" != "$_record_bytes" ]; then
        log_error "Checksum sidecar must contain exactly one newline-terminated record."
    fi

    _checksum_suffix="  $_archive_name"
    case "$_checksum_line" in
        *"$_checksum_suffix")
            _expected_sha256=${_checksum_line%"$_checksum_suffix"}
            ;;
        *)
            log_error "Checksum sidecar does not name the selected archive."
            ;;
    esac
    if [ "${#_expected_sha256}" -ne 64 ]; then
        log_error "Checksum sidecar does not contain a 64-character SHA-256 digest."
    fi
    case "$_expected_sha256" in
        *[!0-9a-f]*)
            log_error "Checksum sidecar SHA-256 must be lowercase hexadecimal."
            ;;
    esac

    _actual_sha256=$(compute_sha256 "$_archive_path")
    if [ "$_actual_sha256" != "$_expected_sha256" ]; then
        log_error "Archive checksum mismatch for $_archive_name."
    fi
    log_success "verification=checksum_verified asset=$_archive_name sha256=$_actual_sha256"
}

resolve_release_commit() {
    if ! _tag_object=$(gh api "repos/$GITHUB_REPO/git/ref/tags/$VERSION" --jq '.object.type + " " + .object.sha'); then
        log_error "Failed to resolve the release tag object for provenance verification."
    fi

    _peel_count=0
    while :; do
        _old_ifs=$IFS
        IFS=' '
        # The gh query emits exactly two whitespace-free fields.
        # shellcheck disable=SC2086
        set -- $_tag_object
        IFS=$_old_ifs
        if [ "$#" -ne 2 ]; then
            log_error "Release tag lookup returned an unexpected shape."
        fi
        _object_type="$1"
        _object_sha="$2"

        if [ "${#_object_sha}" -ne 40 ]; then
            log_error "Release tag lookup returned an invalid object digest."
        fi
        case "$_object_sha" in
            *[!0-9a-f]*)
                log_error "Release tag lookup returned an invalid object digest."
                ;;
        esac

        if [ "$_object_type" = "commit" ]; then
            printf '%s\n' "$_object_sha"
            return 0
        fi
        if [ "$_object_type" != "tag" ] || [ "$_peel_count" -ge 4 ]; then
            log_error "Release tag did not resolve to a bounded commit object."
        fi
        if ! _tag_object=$(gh api "repos/$GITHUB_REPO/git/tags/$_object_sha" --jq '.object.type + " " + .object.sha'); then
            log_error "Failed to peel the release tag for provenance verification."
        fi
        _peel_count=$((_peel_count + 1))
    done
}

verify_archive_provenance() {
    _archive_path="$1"
    _source_digest=$(resolve_release_commit)
    if ! gh attestation verify "$_archive_path" \
        --repo "$GITHUB_REPO" \
        --signer-workflow "$GITHUB_REPO/.github/workflows/release.yml" \
        --cert-oidc-issuer "https://token.actions.githubusercontent.com" \
        --predicate-type "https://slsa.dev/provenance/v1" \
        --source-digest "$_source_digest" \
        --deny-self-hosted-runners >/dev/null; then
        log_error "Release provenance verification failed."
    fi
    log_success "verification=provenance_verified asset=$ARCHIVE_NAME source_digest=$_source_digest"
}

# --- Main ---
main() {
    printf "%b✨ Assay Installer%b\n" "${BOLD}" "${NC}"
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
    VERSION="$(normalize_install_version "$VERSION")" || log_error "ASSAY_VERSION must be latest or a stable X.Y.Z (optional leading v)"

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
        if ! is_stable_release_tag "$VERSION"; then
            log_error "latest Assay release is not a stable software tag: $VERSION"
        fi
    fi

    log_info "Target version: $VERSION"

    if [ "$REQUIRE_PROVENANCE" -eq 1 ] && ! command -v gh >/dev/null 2>&1; then
        log_error "gh is required when ASSAY_REQUIRE_PROVENANCE=1."
    fi

    # 3. Construct Download URLs from the selected asset once.
    if [ "$OS" = "windows" ]; then
        ARCHIVE_NAME="assay-${VERSION}-${TARGET}.zip"
    else
        ARCHIVE_NAME="assay-${VERSION}-${TARGET}.tar.gz"
    fi

    DOWNLOAD_URL="https://github.com/$GITHUB_REPO/releases/download/$VERSION/$ARCHIVE_NAME"
    CHECKSUM_URL="${DOWNLOAD_URL}.sha256"

    # 4. Download
    TMP_ROOT="${TMPDIR:-/tmp}"
    TMP_DIR=$(mktemp -d "${TMP_ROOT%/}/assay-install.XXXXXX")
    INSTALL_CANDIDATE=""
    cleanup_install() {
        rm -rf "$TMP_DIR"
        if [ -n "$INSTALL_CANDIDATE" ]; then
            rm -f "$INSTALL_CANDIDATE"
        fi
    }
    trap cleanup_install EXIT
    trap 'exit 129' HUP
    trap 'exit 130' INT
    trap 'exit 143' TERM

    log_info "Downloading from $DOWNLOAD_URL ..."
    if command -v curl >/dev/null 2>&1; then
        download_file "$DOWNLOAD_URL" "$TMP_DIR/$ARCHIVE_NAME"
        CHECKSUM_MAX_BYTES=$((64 + 2 + ${#ARCHIVE_NAME} + 1))
        download_file "$CHECKSUM_URL" "$TMP_DIR/${ARCHIVE_NAME}.sha256" "$CHECKSUM_MAX_BYTES"
    else
        log_error "curl is required but not found."
    fi

    verify_archive_checksum \
        "$TMP_DIR/$ARCHIVE_NAME" \
        "$TMP_DIR/${ARCHIVE_NAME}.sha256" \
        "$ARCHIVE_NAME"

    if [ "$REQUIRE_PROVENANCE" -eq 1 ]; then
        verify_archive_provenance "$TMP_DIR/$ARCHIVE_NAME"
    else
        log_info "verification=provenance_not_requested"
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
        if [ -f "$EXTRACTED_DIR/assay.exe" ]; then
             EXTRACTED_BINARY="$EXTRACTED_DIR/assay.exe"
        elif [ -f "assay.exe" ]; then
             EXTRACTED_BINARY="assay.exe"
        else
             log_error "Could not find assay.exe after extraction"
        fi
        INSTALL_CANDIDATE=$(mktemp "$INSTALL_DIR/.assay.exe.install.XXXXXX")
        cp "$EXTRACTED_BINARY" "$INSTALL_CANDIDATE"
        mv -f "$INSTALL_CANDIDATE" "$INSTALL_DIR/assay.exe"
        INSTALL_CANDIDATE=""
    else
        if [ -f "$EXTRACTED_DIR/assay" ]; then
             EXTRACTED_BINARY="$EXTRACTED_DIR/assay"
        elif [ -f "assay" ]; then
             EXTRACTED_BINARY="assay"
        else
             log_error "Could not find assay binary after extraction"
        fi
        INSTALL_CANDIDATE=$(mktemp "$INSTALL_DIR/.assay.install.XXXXXX")
        cp "$EXTRACTED_BINARY" "$INSTALL_CANDIDATE"
        chmod 755 "$INSTALL_CANDIDATE"
        mv -f "$INSTALL_CANDIDATE" "$INSTALL_DIR/assay"
        INSTALL_CANDIDATE=""
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
            printf "   %bexport PATH=\"\$PATH:%s\"%b\n" "${BOLD}" "$INSTALL_DIR" "${NC}"
            printf "\n"
            ;;
    esac

    printf "Run %bassay --help%b to get started.\n" "${BOLD}" "${NC}"
}

main "$@"
