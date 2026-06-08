#!/usr/bin/env bash

# shellcheck shell=bash
# shellcheck disable=SC1091

SCRIPT_LIBS_DIR="$(cd -P -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"

# Preserve the top-level script name before entering the shared public libs.
# Without this wrapper adjustment, public/common.sh sees this compatibility file
# as its caller and log lines are prefixed with [common].
: "${SCRIPT_NAME:=$(basename -- "${BASH_SOURCE[1]:-$0}" .sh)}"

source "${SCRIPT_LIBS_DIR}/public/common.sh"
