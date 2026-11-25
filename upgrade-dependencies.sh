#!/bin/bash

set -euo pipefail

SCRIPT_DIR="$(cd -P -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
cd "${SCRIPT_DIR}"

[[ " $@ " =~ ' -c ' ]] && rm -rf ./Cargo.lock ./target

# cargo upgrade ignores registry.global-min-publish-age until cargo-edit can read Cargo's config (expected after
# Cargo 1.102), so pick for each crates.io dependency the newest version that is old enough and pass it explicitly.
# Once cargo-edit honors the setting, go back to plain `cargo upgrade -i allow`.
min_publish_age="$(cargo -Zunstable-options config get --format json-value registry.global-min-publish-age 2>/dev/null || true)"
upgrade_args_text="$(
    cargo metadata --no-deps --format-version 1 | python3 -I -c '
import json
import re
import sys
import tomllib
import urllib.request
from datetime import datetime, timedelta, timezone

UNITS = {"second": 1, "minute": 60, "hour": 3600, "day": 86400, "week": 604800, "month": 2629800}

def parse_age(text):
    if not text:
        return timedelta(0)
    match = re.fullmatch(r"\s*(\d+)\s*([a-z]+?)s?\s*", json.loads(text))
    if not match or match[2] not in UNITS:
        sys.exit(f"unsupported registry.global-min-publish-age: {text}")
    return timedelta(seconds=int(match[1]) * UNITS[match[2]])

def index_path(name):
    if len(name) <= 2:
        return f"{len(name)}/{name}"
    if len(name) == 3:
        return f"3/{name[0]}/{name}"
    return f"{name[:2]}/{name[2:4]}/{name}"

def version_key(version):
    return tuple(int(part) for part in version.split("+")[0].split("."))

cutoff = datetime.now(timezone.utc) - parse_age(sys.argv[1])
metadata = json.load(sys.stdin)
# Cargo keeps versions already in Cargo.lock even when they are newer than the cutoff, so they count as old enough.
locked = {}
try:
    with open(sys.argv[2], "rb") as file:
        for package in tomllib.load(file).get("package", []):
            if package.get("source", "").startswith("registry+") and "-" not in package["version"]:
                locked.setdefault(package["name"], []).append(package["version"])
except FileNotFoundError:
    pass

requirements = {}
for package in metadata["packages"]:
    for dependency in package["dependencies"]:
        requirement = dependency["req"]
        # Pinned (=), prerelease and non-crates.io requirements are left to the user, as cargo upgrade does by default.
        if dependency["source"] != "registry+https://github.com/rust-lang/crates.io-index" or requirement.startswith("=") or "-" in requirement:
            continue
        floor = version_key(re.sub(r"^[\^~>= ]+", "", requirement).split(",")[0].strip().rstrip(".*") or "0")
        requirements[dependency["name"]] = max(floor, requirements.get(dependency["name"], ()))

for name, floor in sorted(requirements.items()):
    request = urllib.request.Request(f"https://index.crates.io/{index_path(name.lower())}", headers={"User-Agent": "upgrade-dependencies"})
    with urllib.request.urlopen(request, timeout=30) as response:
        entries = [json.loads(line) for line in response.read().decode().splitlines() if line]
    candidates = [
        entry["vers"]
        for entry in entries
        if not entry.get("yanked") and "-" not in entry["vers"].split("+")[0]
        # Versions without pubtime are not held back by Cargo either.
        and (not entry.get("pubtime") or datetime.fromisoformat(entry["pubtime"].replace("Z", "+00:00")) <= cutoff)
    ] + locked.get(name, [])
    newest = max(candidates, key=version_key, default=None)
    # Never lower a requirement that already points at a version newer than the cutoff.
    if newest and version_key(newest) >= floor:
        print(f"--package={name}@{newest}")
' "${min_publish_age}" ./Cargo.lock
)"

mapfile -t upgrade_args <<<"${upgrade_args_text}"
[[ -n "${upgrade_args_text}" ]] || upgrade_args=()

((${#upgrade_args[@]})) && cargo upgrade -i allow "${upgrade_args[@]}"
cargo update
