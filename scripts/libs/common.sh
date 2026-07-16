#!/usr/bin/env bash

# shellcheck shell=bash
# shellcheck disable=SC1091

SCRIPT_LIBS_DIR="$(cd -P -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"

# Preserve the top-level script context before entering the shared public libs.
# Otherwise public/common.sh would treat this entry point as the calling script.
: "${SCRIPT_NAME:=$(basename -- "${BASH_SOURCE[1]:-$0}" .sh)}"
: "${SCRIPT_DIR:=$(cd -P -- "$(dirname -- "${BASH_SOURCE[1]:-$0}")" && pwd)}"

source "${SCRIPT_LIBS_DIR}/public/common.sh"
