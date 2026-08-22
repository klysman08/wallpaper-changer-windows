# One-shot release build: Tauri desktop app + installers.
#
# Replaces the previous build_exe.ps1 / installer.iss pair. Tauri's own bundler
# produces the NSIS installer, so there is no Inno Setup step any more, and
# no registry writing at install time — "start with Windows" is a runtime toggle
# handled by tauri-plugin-autostart.
#
# Run from the repository root:
#   .\scripts\build_app.ps1
#   .\scripts\build_app.ps1 -NoBundle       # faster, unoptimised, no installers
#   .\scripts\build_app.ps1 -Tag v5.2 -Notes "..."
#
# A bundled build is signed for the in-app updater, so it needs the signing key in
# the environment before it will run:
#
#   $env:TAURI_SIGNING_PRIVATE_KEY = Get-Content "$env:USERPROFILE\.tauri\wallpaper-changer.key" -Raw
#   $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = '<the key password, or "" if none>'
#
# It leaves dist/release/ holding exactly what a GitHub release needs: the installer
# and the latest.json manifest the installed app polls.

[CmdletBinding()]
param(
    # Build a debug binary and skip bundling. Much faster for a smoke test.
    # Not named -Debug: that collides with CmdletBinding's common parameter.
    [switch]$NoBundle,
    # The release tag the manifest will point its download URL at. Defaults to the
    # repository's habit: v5.1 for 5.1.0, v4.0.1 for 4.0.1.
    [string]$Tag,
    # Shown in the app's update dialog. The release body is not readable yet — the
    # manifest has to exist before the release does — so this defaults to a pointer.
    [string]$Notes
)

$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
$desktop = Join-Path $repoRoot 'desktop'

# Where the updater looks. Must match plugins.updater.endpoints in tauri.conf.json.
$repoUrl = 'https://github.com/klysman08/wallpaper-changer-windows'

function Invoke-Native {
    <#
      Native tools log progress to stderr, which Windows PowerShell 5.1 turns into a
      terminating error under $ErrorActionPreference='Stop'. Judge by exit code.
    #>
    param([scriptblock]$Command, [string]$What)
    $previous = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try { & $Command } finally { $ErrorActionPreference = $previous }
    if ($LASTEXITCODE -ne 0) { throw "$What failed with exit code $LASTEXITCODE" }
}

# ── Locate bun ────────────────────────────────────────────────────────────────
# Generated tauri-ui projects require bun. winget installs only bun.exe, so the
# canonical ~/.bun/bin may be the only place both bun and bunx exist.
$bun = Get-Command bun -ErrorAction SilentlyContinue
if (-not $bun) {
    $candidate = Join-Path $env:USERPROFILE '.bun\bin'
    if (Test-Path (Join-Path $candidate 'bun.exe')) {
        $env:PATH = "$candidate;$env:PATH"
    }
    else {
        throw "bun was not found. Install it from https://bun.sh, then re-run."
    }
}

# ── 0. Signing key ────────────────────────────────────────────────────────────
# Checked before the long build rather than after it. Without the key the bundler
# emits no .sig, and nothing would go wrong until an installed copy refused the
# update — far too late to notice.
if (-not $NoBundle -and -not $env:TAURI_SIGNING_PRIVATE_KEY) {
    throw @'
TAURI_SIGNING_PRIVATE_KEY is not set, so the installer cannot be signed for the
in-app updater. Set it and re-run:

  $env:TAURI_SIGNING_PRIVATE_KEY = Get-Content "$env:USERPROFILE\.tauri\wallpaper-changer.key" -Raw
  $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = '<the key password, or "" if none>'

No key yet? Generate one once, keep it somewhere safe, and put the matching .pub
contents in desktop/src-tauri/tauri.conf.json under plugins.updater.pubkey:

  cd desktop; bun run tauri signer generate -w "$env:USERPROFILE\.tauri\wallpaper-changer.key"

Or pass -NoBundle to skip installers entirely.
'@
}

# Demanded explicitly rather than defaulted. The signer prompts on stdin when this
# is undefined, which hangs an unattended build *after* the twenty-minute compile
# and bundle have already finished — the most expensive possible place to stop.
# Guessing an empty value is no better: a key that does have a password gets the
# same prompt anyway.
if (-not $NoBundle -and $null -eq $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD) {
    throw @'
TAURI_SIGNING_PRIVATE_KEY_PASSWORD is not set. Set it even when the key has no
password, or the signer will stop and wait for a prompt nobody is there to answer:

  $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = Read-Host 'signing key password'
  # ...or, for a key with no password:
  $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = ''
'@
}

# The engine used to be built and staged here by build_engine.ps1, as a frozen Python
# sidecar of about 145 MB. It is native now and compiles as part of the app, so that
# step and the -SkipEngine switch that skipped it are both gone. What still ships
# beside the executable is libmpv, as a Tauri resource.

# ── 1. Frontend + Tauri ───────────────────────────────────────────────────────
Push-Location $desktop
try {
    Write-Host '==> Installing frontend dependencies' -ForegroundColor Cyan
    Invoke-Native { bun install --frozen-lockfile } 'bun install'

    if ($NoBundle) {
        Write-Host '==> Building the app (debug, no installers)' -ForegroundColor Cyan
        Invoke-Native { bun run tauri build --debug --no-bundle } 'tauri build'
        Write-Host "==> Done: $desktop\src-tauri\target\debug\tauri-native.exe" -ForegroundColor Green
        return
    }

    Write-Host '==> Building the app and installers' -ForegroundColor Cyan
    Invoke-Native { bun run tauri build } 'tauri build'
}
finally {
    Pop-Location
}

# ── 2. Collect the installers ─────────────────────────────────────────────────
# tauri.conf.json is the version the updater compares against, so it is the only
# version this script trusts.
$conf = Get-Content (Join-Path $desktop 'src-tauri\tauri.conf.json') -Raw | ConvertFrom-Json
$version = $conf.version

$bundleDir = Join-Path $desktop 'src-tauri\target\release\bundle'
$found = Get-ChildItem $bundleDir -Recurse -Include *.msi, *.exe -ErrorAction SilentlyContinue
if (-not $found) { throw "The bundler produced no installers under $bundleDir." }

# target/ is never cleaned between builds, so installers from earlier versions sit
# here indefinitely. Everything downstream must come from *this* version's artifact:
# taking the first match instead would sign, stage and publish whichever version
# happens to sort first, which is the oldest one.
$artifacts = $found | Where-Object { $_.Name -like "*_${version}_*" }
if (-not $artifacts) {
    $names = ($found | ForEach-Object Name) -join ', '
    throw "The bundler produced no installer for $version under $bundleDir. Found: $names"
}

Write-Host ''
Write-Host "==> Installers for $version" -ForegroundColor Green
foreach ($file in $artifacts) {
    '    {0,-42} {1,8:N1} MB' -f $file.Name, ($file.Length / 1MB) | Write-Host
}
$stale = $found | Where-Object { $_.Name -notlike "*_${version}_*" }
if ($stale) {
    "    ({0} installer(s) from earlier versions ignored)" -f $stale.Count | Write-Host -ForegroundColor DarkGray
}

# ── 3. Updater manifest ───────────────────────────────────────────────────────
if (-not $Tag) {
    $parts = $version.Split('.')
    # v5.1 for 5.1.0, but v4.0.1 for 4.0.1 — the tags in this repo drop a zero patch.
    if ($parts[2] -eq '0') { $Tag = "v$($parts[0]).$($parts[1])" }
    else { $Tag = "v$version" }
}
if (-not $Notes) { $Notes = "See $repoUrl/releases/tag/$Tag" }

$installer = $artifacts | Where-Object { $_.Name -like '*-setup.exe' } | Select-Object -First 1
if (-not $installer) { throw "No NSIS -setup.exe found under $bundleDir." }

$sigFile = "$($installer.FullName).sig"
if (-not (Test-Path $sigFile)) {
    throw "The installer is unsigned ($sigFile is missing). Check TAURI_SIGNING_PRIVATE_KEY and rebuild."
}

# GitHub turns spaces in an uploaded asset's name into dots, so the download URL
# never matches the local file name. Staging under the served name keeps the URL
# written here and the asset actually uploaded in step with each other.
$assetName = $installer.Name -replace ' ', '.'
$stageDir = Join-Path $repoRoot 'dist\release'
if (Test-Path $stageDir) { Remove-Item $stageDir -Recurse -Force }
New-Item -ItemType Directory -Path $stageDir -Force | Out-Null
Copy-Item $installer.FullName (Join-Path $stageDir $assetName)

$manifest = [ordered]@{
    version   = $version
    notes     = $Notes
    pub_date  = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
    platforms = [ordered]@{
        'windows-x86_64' = [ordered]@{
            signature = (Get-Content $sigFile -Raw).Trim()
            url       = "$repoUrl/releases/download/$Tag/$assetName"
        }
    }
}
# Not Out-File: Windows PowerShell 5.1 writes UTF-8 *with* a BOM, and the byte order
# mark is a parse error for the Rust side that reads this manifest.
[System.IO.File]::WriteAllText(
    (Join-Path $stageDir 'latest.json'),
    ($manifest | ConvertTo-Json -Depth 5),
    (New-Object System.Text.UTF8Encoding $false)
)

Write-Host ''
Write-Host "==> Release $version staged in $stageDir" -ForegroundColor Green
Write-Host '    Upload BOTH files. A release without latest.json breaks the update' -ForegroundColor Yellow
Write-Host '    check for every installed copy, and a prerelease is never offered.' -ForegroundColor Yellow
Write-Host ''
Write-Host "    gh release create $Tag `"$stageDir\*`" --title `"$Tag`" --notes `"...`"" -ForegroundColor Cyan
