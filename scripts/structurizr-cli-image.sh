#!/usr/bin/env bash
# Single source for the Structurizr CLI container image reference.
#
# scripts/structurizr-validate.sh, scripts/structurizr-export.sh, and the
# structurizr-validate workflow all obtain the image from here. Do not embed
# structurizr/cli:latest or a second digest literal at the call sites.
set -euo pipefail

STRUCTURIZR_CLI_IMAGE="structurizr/cli@sha256:717e320e0ad52335ea9939bf5fae092620cc3deccecf6f280a5b6fee99763c53"

printf '%s\n' "${STRUCTURIZR_CLI_IMAGE}"
