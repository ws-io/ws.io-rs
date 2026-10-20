#!/usr/bin/env bash

# shellcheck disable=SC1091

set -euo pipefail

SCRIPT_DIR="$(cd -P -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
readonly SCRIPT_DIR
source "${SCRIPT_DIR}/../libs/common.sh"
source "${SCRIPT_DIR}/../libs/windows/msvc.sh"

# Run
main() {
    prepare_windows_msvc
    cd -- "${REPO_ROOT}"
    exec cargo xwin "$@"
}

main "$@"
