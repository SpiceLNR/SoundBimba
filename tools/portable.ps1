# SPDX-License-Identifier: AGPL-3.0-or-later
# Портативная версия: папка SoundBimba с программой, памяткой, лицензиями и SHA256SUMS.txt,
# упакованная в zip. Нужна уже собранная программа (npx tauri build).
# Запуск из корня репозитория: powershell -File tools/portable.ps1 [-Name SoundBimba_0.5.1_x64-portable]
param([string]$Name = "")
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$exe = Join-Path $root "target/release/soundbimba.exe"
if (-not (Test-Path $exe)) { throw "Нет $exe — сначала соберите программу (npx tauri build)" }
if (-not $Name) {
    $version = (Get-Content (Join-Path $root "app/src-tauri/tauri.conf.json") -Raw | ConvertFrom-Json).version
    $Name = "SoundBimba_${version}_x64-portable"
}
$out = Join-Path $root "target/release/portable"
$dir = Join-Path $out "SoundBimba"
if (Test-Path $out) { Remove-Item -Recurse -Force $out }
New-Item -ItemType Directory -Force $dir | Out-Null
Copy-Item $exe (Join-Path $dir "SoundBimba.exe")
Copy-Item (Join-Path $root "packaging/portable/README.txt") $dir
Copy-Item (Join-Path $root "LICENSE") $dir
Copy-Item (Join-Path $root "THIRD_PARTY_NOTICES.md") $dir
$hash = (Get-FileHash (Join-Path $dir "SoundBimba.exe") -Algorithm SHA256).Hash.ToLower()
Set-Content -Encoding ascii (Join-Path $dir "SHA256SUMS.txt") "$hash  SoundBimba.exe"
$zip = Join-Path $out "$Name.zip"
Compress-Archive -Path $dir -DestinationPath $zip
"$((Get-FileHash $zip -Algorithm SHA256).Hash.ToLower())  $Name.zip" | Set-Content -Encoding ascii (Join-Path $out "$Name.zip.sha256")
Write-Host "Готово: $zip"
