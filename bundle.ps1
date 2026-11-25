# Builds the release version and puts the ready-to-install files in dist/:
#   dist/Ritmo-6.vst3   one single VST3 file (copy it to C:\Program Files\Common Files\VST3)
#   dist/Ritmo-6.clap   CLAP version of the same plugin
#   dist/Ritmo-6.exe    standalone app, with the ASIO driver as well as WASAPI
#
# ASIO needs two things at build time, fetched once into target\ (nothing is installed):
#   - Steinberg's ASIO SDK (used under its GPL-3 option, like Ritmo-6), from steinberg.net
#   - libclang, to read the SDK's C headers (the "libclang" Python package, via pip)
# Already set CPAL_ASIO_DIR / LIBCLANG_PATH are used as they are. If either can't be had,
# the app is built without ASIO (WASAPI only) and a warning says so.
$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot

function Get-AsioSdk {
    if ($env:CPAL_ASIO_DIR -and (Test-Path (Join-Path $env:CPAL_ASIO_DIR "common\asio.h"))) { return $env:CPAL_ASIO_DIR }
    $sdk = Join-Path $PSScriptRoot "target\asio-sdk-unpack\ASIOSDK"
    if (-not (Test-Path (Join-Path $sdk "common\asio.h"))) {
        $zip = Join-Path $PSScriptRoot "target\asio_sdk.zip"
        New-Item -ItemType Directory -Force (Split-Path $zip) | Out-Null
        Invoke-WebRequest -Uri "https://www.steinberg.net/asiosdk" -OutFile $zip
        Expand-Archive -Path $zip -DestinationPath (Join-Path $PSScriptRoot "target\asio-sdk-unpack") -Force
        $found = Get-ChildItem (Join-Path $PSScriptRoot "target\asio-sdk-unpack") -Directory |
            Where-Object { Test-Path (Join-Path $_.FullName "common\asio.h") } | Select-Object -First 1
        if (-not $found) { throw "ASIO SDK: common\asio.h not found in the download" }
        $sdk = $found.FullName
    }
    return $sdk
}

function Get-Libclang {
    if ($env:LIBCLANG_PATH -and (Test-Path (Join-Path $env:LIBCLANG_PATH "libclang.dll"))) { return $env:LIBCLANG_PATH }
    $dir = Join-Path $PSScriptRoot "target\libclang\clang\native"
    if (-not (Test-Path (Join-Path $dir "libclang.dll"))) {
        python -m pip install --quiet --target (Join-Path $PSScriptRoot "target\libclang") libclang
        if (-not (Test-Path (Join-Path $dir "libclang.dll"))) { throw "libclang.dll not found after pip install" }
    }
    return $dir
}

$features = @()
try {
    $env:CPAL_ASIO_DIR = Get-AsioSdk
    $env:LIBCLANG_PATH = Get-Libclang
    $features = @("--features", "ritmo6/asio")
    Write-Host "ASIO: SDK in $env:CPAL_ASIO_DIR, libclang in $env:LIBCLANG_PATH"
} catch {
    Write-Warning "Building without ASIO (WASAPI only): $_"
}

cargo build --release @features
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
$dist = Join-Path $PSScriptRoot "dist"
New-Item -ItemType Directory -Force $dist | Out-Null
$vst = Join-Path $dist "Ritmo-6.vst3"
if (Test-Path $vst -PathType Container) { Remove-Item -Recurse -Force $vst }
Copy-Item "target\release\ritmo6_vst3.dll" $vst -Force
Copy-Item "target\release\ritmo6_vst3.dll" (Join-Path $dist "Ritmo-6.clap") -Force
Copy-Item "target\release\ritmo6.exe" (Join-Path $dist "Ritmo-6.exe") -Force
Get-ChildItem $dist -File | Select-Object Name, Length
