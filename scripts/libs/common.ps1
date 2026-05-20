function Ensure-CargoTarget([string]$Target) {
    if (-not (Get-Command rustup -ErrorAction SilentlyContinue)) {
        throw 'missing rustup'
    }

    $installedTargets = & rustup target list --installed
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }

    if ($installedTargets -notcontains $Target) {
        & rustup target add $Target
        if ($LASTEXITCODE -ne 0) {
            exit $LASTEXITCODE
        }
    }
}

function Invoke-WithEncodedRustflags([string[]]$RustFlags, [string]$Command, [string[]]$Arguments) {
    $old = [Environment]::GetEnvironmentVariable('CARGO_ENCODED_RUSTFLAGS', 'Process')

    try {
        if ($RustFlags.Length -gt 0) {
            $sep = [string][char]0x1f
            [Environment]::SetEnvironmentVariable('CARGO_ENCODED_RUSTFLAGS', [string]::Join($sep, $RustFlags), 'Process')
        }

        & $Command @Arguments
        return $LASTEXITCODE
    } finally {
        [Environment]::SetEnvironmentVariable('CARGO_ENCODED_RUSTFLAGS', $old, 'Process')
    }
}
