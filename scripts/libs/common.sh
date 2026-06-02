#!/usr/bin/env bash

# shellcheck shell=bash
# shellcheck disable=SC1091

SCRIPT_LIBS_DIR="$(cd -P -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_LIBS_DIR}/public/common.sh"
