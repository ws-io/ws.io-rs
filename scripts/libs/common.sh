#!/usr/bin/env bash

# shellcheck shell=bash
# shellcheck disable=SC1091

SCRIPT_LIBS_DIR="$(cd -P -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"

# Preserve the top-level script context before entering the shared public libs.
# Without this wrapper adjustment, public/common.sh sees this compatibility file
# as its caller and derives [common] plus libs/ as the active script context.
: "${SCRIPT_NAME:=$(basename -- "${BASH_SOURCE[1]:-$0}" .sh)}"
: "${SCRIPT_DIR:=$(cd -P -- "$(dirname -- "${BASH_SOURCE[1]:-$0}")" && pwd)}"

source "${SCRIPT_LIBS_DIR}/public/common.sh"
