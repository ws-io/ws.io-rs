#!/bin/bash

set -euo pipefail

SCRIPTS_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" &>/dev/null && pwd)"
cd "${SCRIPTS_DIR}"

[[ " $@ " =~ ' -c ' ]] && rm -rf ./Cargo.lock ./target

cargo upgrade -i allow
cargo update
