# Build Windows release assets without embedding the builder's local profile paths.
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$previousFlags = $env:CARGO_ENCODED_RUSTFLAGS
$previousRustFlags = $env:RUSTFLAGS
$previousLocation = Get-Location

try {
    Set-Location -LiteralPath $projectRoot
    $flags = @('-Cdebuginfo=0', '-Cstrip=symbols', '-Clink-arg=/PDBALTPATH:folderbridge.pdb')
    $mappings = @(
        @($env:USERPROFILE, '/user'),
        @($env:CARGO_HOME, '/cargo'),
        @($env:RUSTUP_HOME, '/rustup'),
        @($projectRoot, '/folderbridge')
    )
    foreach ($mapping in $mappings) {
        if ($mapping[0]) {
            $prefix = [IO.Path]::GetFullPath($mapping[0]).TrimEnd('\', '/')
            $flags += "--remap-path-prefix=$prefix=$($mapping[1])"
            $forward = $prefix.Replace('\', '/')
            if ($forward -ne $prefix) {
                $flags += "--remap-path-prefix=$forward=$($mapping[1])"
            }
        }
    }
    # Cargo's encoded form preserves paths with spaces as individual arguments.
    $env:CARGO_ENCODED_RUSTFLAGS = $flags -join [char]0x1f
    $env:RUSTFLAGS = $null
    & npm.cmd run tauri -- build
    if ($LASTEXITCODE -ne 0) { throw 'Tauri release build failed.' }

    $version = (Get-Content -LiteralPath 'package.json' -Raw | ConvertFrom-Json).version
    $output = Join-Path $projectRoot "release\public-v$version"
    New-Item -ItemType Directory -Path $output -Force | Out-Null
    $appName = "FolderBridge_$version.exe"
    $setupName = "FolderBridge_${version}_x64-setup.exe"
    Copy-Item -LiteralPath 'src-tauri\target\release\folderbridge.exe' -Destination (Join-Path $output $appName)
    Copy-Item -LiteralPath "src-tauri\target\release\bundle\nsis\$setupName" -Destination (Join-Path $output $setupName)
    Copy-Item -LiteralPath 'LICENSE' -Destination (Join-Path $output 'LICENSE.txt')
    $checksums = foreach ($name in @($appName, $setupName, 'LICENSE.txt')) {
        $hash = (Get-FileHash -LiteralPath (Join-Path $output $name) -Algorithm SHA256).Hash.ToLowerInvariant()
        "$hash  $name"
    }
    $checksums | Set-Content -LiteralPath (Join-Path $output 'SHA256SUMS.txt') -Encoding ASCII
    Write-Output "Release assets: $output"
}
finally {
    $env:CARGO_ENCODED_RUSTFLAGS = $previousFlags
    $env:RUSTFLAGS = $previousRustFlags
    Set-Location -LiteralPath $previousLocation.Path
}
