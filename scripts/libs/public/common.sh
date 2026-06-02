#!/usr/bin/env bash
# Common entry point for reusable shell helpers.
# Source this file from scripts; do not execute it directly.

# shellcheck shell=bash
# shellcheck disable=SC2034,SC1091,SC2317

if [[ -n "${LINUX_CONFIGS_LIBS_COMMON_LOADED:-}" ]]; then
    return 0 2>/dev/null || true
fi

LINUX_CONFIGS_LIBS_COMMON_LOADED=1

LIBS_DIR="$(cd -P -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -P -- "${LIBS_DIR}/../../.." && pwd)"

: "${SCRIPT_NAME:=$(basename -- "${BASH_SOURCE[1]:-$0}" .sh)}"
: "${SCRIPT_DIR:=$(cd -P -- "$(dirname -- "${BASH_SOURCE[1]:-$0}")" && pwd)}"

source "${LIBS_DIR}/log.sh"
source "${LIBS_DIR}/command.sh"
source "${LIBS_DIR}/file.sh"
source "${LIBS_DIR}/path.sh"
source "${LIBS_DIR}/platform.sh"
source "${LIBS_DIR}/prompt.sh"
source "${LIBS_DIR}/rust.sh"
