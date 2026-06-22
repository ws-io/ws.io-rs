#!/bin/bash

set -euo pipefail

SCRIPT_DIR="$(cd -P -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
cd "${SCRIPT_DIR}"

git config --replace-all core.filemode true
find . -name target -prune -o \( -type f -exec chmod 600 {} + \)
find . -name target -prune -o \( -type d -exec chmod 700 {} + \)
find . -name target -prune -o \( -name '*.sh' -type f -exec chmod 700 {} + \)
