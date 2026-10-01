#!/usr/bin/env bash

# shellcheck disable=SC1091

set -euo pipefail

SCRIPT_DIR="$(cd -P -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
readonly SCRIPT_DIR
source "${SCRIPT_DIR}/libs/common.sh"

# Constants/Variables
# Pin Mordant and its build toolchain together; leave the project's toolchain unchanged.
readonly MORDANT_REV='c5e56d0d9ca6e2adbb3488b7677353f26cd99ba8'
readonly MORDANT_TOOLCHAIN='nightly-2026-09-01'

# Run
main() {
    prepend_cargo_bin_to_path
    require_cmd cargo rustup

    log_info 'Installing Mordant...'
    rustup toolchain install "${MORDANT_TOOLCHAIN}" \
        --profile minimal \
        --component rustc-dev \
        --component llvm-tools-preview

    exec cargo +"${MORDANT_TOOLCHAIN}" install --locked \
        --git https://github.com/scarletindustries/mordant \
        --rev "${MORDANT_REV}"
}

main "$@"
