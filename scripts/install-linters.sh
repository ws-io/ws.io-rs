#!/usr/bin/env bash

# shellcheck disable=SC1091

set -euo pipefail

SCRIPT_DIR="$(cd -P -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
readonly SCRIPT_DIR
source "${SCRIPT_DIR}/libs/common.sh"

# Constants/Variables
# Pin Mordant and its build toolchain together; leave the project's toolchain unchanged.
readonly MORDANT_REV='6cc00ea760233b179e8d443488632772cae01040'
readonly MORDANT_TOOLCHAIN='nightly-2026-11-14'

# Run
main() {
    prepend_cargo_bin_to_path
    require_cmd cargo rustup
    cd -- "${REPO_ROOT}"

    log_info 'Installing Dylint...'
    cargo install --locked cargo-dylint dylint-link

    log_info 'Installing Mordant...'
    rustup toolchain install "${MORDANT_TOOLCHAIN}" \
        --profile minimal \
        --component llvm-tools-preview \
        --component rustc-dev

    exec cargo +"${MORDANT_TOOLCHAIN}" install --locked \
        --git https://github.com/scarletindustries/mordant \
        --rev "${MORDANT_REV}"
}

main "$@"
