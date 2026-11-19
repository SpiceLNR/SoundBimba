# SPDX-License-Identifier: AGPL-3.0-or-later
# Снимки тем для окна «Настройки → Тема»: themes/<тема>/preview.jpg (640×400).
# Снимает интерфейс в браузерном просмотре (заглушка вместо Rust) через Edge без окна.
# Нужно: запущенный просмотр — в app: npm run dev (http://localhost:5173).
# Запуск из корня репозитория (Windows PowerShell): powershell -File tools/theme-previews.ps1
# Поменяли вид темы — запустите снова: снимки переснимутся.
param([string]$Url = "http://localhost:5173")
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$edge = Join-Path ${env:ProgramFiles(x86)} "Microsoft\Edge\Application\msedge.exe"
if (-not (Test-Path $edge)) { throw "Не найден Edge: $edge" }
try { Invoke-WebRequest -UseBasicParsing -TimeoutSec 5 $Url | Out-Null } catch { throw "Просмотр не запущен на $Url — в папке app выполните npm run dev" }
Add-Type -AssemblyName System.Drawing
# Свой временный профиль: ваш Edge, его вкладки и настройки не трогаются.
$profile = Join-Path $env:TEMP "sb-theme-shots-profile"
$jpeg = [System.Drawing.Imaging.ImageCodecInfo]::GetImageEncoders() | Where-Object { $_.MimeType -eq "image/jpeg" }
$quality = New-Object System.Drawing.Imaging.EncoderParameters 1
$quality.Param[0] = New-Object System.Drawing.Imaging.EncoderParameter ([System.Drawing.Imaging.Encoder]::Quality), 85L
foreach ($dir in Get-ChildItem (Join-Path $root "themes") -Directory) {
    $id = $dir.Name
    $png = Join-Path $env:TEMP "sb-theme-shot-$id.png"
    if (Test-Path $png) { Remove-Item $png }
    $args = @("--headless=new", "--disable-gpu", "--hide-scrollbars", "--no-first-run", "--no-default-browser-check",
        "--user-data-dir=$profile", "--window-size=1280,800", "--virtual-time-budget=6000", "--screenshot=$png", "$Url/?shot=$id")
    Start-Process -FilePath $edge -ArgumentList $args -Wait -WindowStyle Hidden
    if (-not (Test-Path $png)) { throw "Edge не сделал снимок темы $id" }
    $src = [System.Drawing.Image]::FromFile($png)
    $dst = New-Object System.Drawing.Bitmap 640, 400
    $g = [System.Drawing.Graphics]::FromImage($dst)
    $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $g.DrawImage($src, 0, 0, 640, 400)
    $out = Join-Path $dir.FullName "preview.jpg"
    $dst.Save($out, $jpeg, $quality)
    $g.Dispose(); $dst.Dispose(); $src.Dispose()
    Remove-Item $png
    Write-Host ("{0}: {1:N0} КБ" -f $id, ((Get-Item $out).Length / 1KB))
}
Write-Host "Готово."
