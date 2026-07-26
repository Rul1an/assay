#!/usr/bin/env bash
set -euo pipefail

VERSION="${1:-}"
REPO="Rul1an/assay"

if [[ -z "$VERSION" ]]; then
  echo "::error::Assay version input is empty"
  exit 1
fi

if [[ "$VERSION" == "latest" ]]; then
  VERSION=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" |
    grep '"tag_name"' |
    cut -d'"' -f4)
  if [[ -z "$VERSION" ]]; then
    echo "::error::Failed to fetch latest Assay version"
    exit 1
  fi
  if [[ ! "$VERSION" =~ ^v[0-9]+[.][0-9]+[.][0-9]+$ ]]; then
    echo "::error::latest Assay release is not a stable software tag: $VERSION"
    exit 1
  fi
fi

if [[ "$VERSION" == *$'\n'* || "$VERSION" == *$'\r'* ]]; then
  echo "::error::Assay version must not contain line breaks"
  exit 1
fi

case "$VERSION" in
v*) ;;
*) VERSION="v${VERSION}" ;;
esac

case "$VERSION" in
v1) EXPECTED_VERSION="1.1.0" ;;
v2) EXPECTED_VERSION="2.12.0" ;;
*)
  EXPECTED_VERSION="${VERSION#v}"
  if [[ "$EXPECTED_VERSION" =~ ^[0-9]+[.][0-9]+$ ]]; then
    EXPECTED_VERSION="${EXPECTED_VERSION}.0"
  fi
  ;;
esac

echo "resolved_version=$VERSION" >>"$GITHUB_OUTPUT"
echo "resolved_version_plain=$EXPECTED_VERSION" >>"$GITHUB_OUTPUT"

if ! command -v assay &>/dev/null; then
  echo "skip_install=false" >>"$GITHUB_OUTPUT"
  exit 0
fi

INSTALLED_VERSION="$(assay --version 2>/dev/null | awk '{print $2}' || true)"
echo "installed_version=$INSTALLED_VERSION" >>"$GITHUB_OUTPUT"
echo "Assay already installed: ${INSTALLED_VERSION:-unknown}"

if [[ "$INSTALLED_VERSION" == "$EXPECTED_VERSION" ]]; then
  echo "skip_install=true" >>"$GITHUB_OUTPUT"
else
  echo "::notice::Installed Assay ${INSTALLED_VERSION:-unknown} does not match requested ${VERSION}; reinstalling"
  echo "skip_install=false" >>"$GITHUB_OUTPUT"
fi
