# Builds the release version and puts the ready-to-install files in dist/:
#   dist/Ritmo-6.vst3   one single VST3 file (copy it to C:\Program Files\Common Files\VST3)
#   dist/Ritmo-6.clap   CLAP version of the same plugin
#   dist/Ritmo-6.exe    standalone app
$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot
cargo build --release
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
$dist = Join-Path $PSScriptRoot "dist"
New-Item -ItemType Directory -Force $dist | Out-Null
$vst = Join-Path $dist "Ritmo-6.vst3"
if (Test-Path $vst -PathType Container) { Remove-Item -Recurse -Force $vst }
Copy-Item "target\release\ritmo6_vst3.dll" $vst -Force
Copy-Item "target\release\ritmo6_vst3.dll" (Join-Path $dist "Ritmo-6.clap") -Force
Copy-Item "target\release\ritmo6.exe" (Join-Path $dist "Ritmo-6.exe") -Force
Get-ChildItem $dist -File | Select-Object Name, Length
