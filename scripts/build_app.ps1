# One-shot release build: Python engine sidecar + Tauri desktop app + installers.
#
# Replaces the previous build_exe.ps1 / installer.iss pair. Tauri's own bundler
# produces the MSI and NSIS installers, so there is no Inno Setup step any more, and
# no registry writing at install time — "start with Windows" is a runtime toggle
# handled by tauri-plugin-autostart.
#
# Run from the repository root:
#   .\scripts\build_app.ps1
#   .\scripts\build_app.ps1 -SkipEngine     # reuse the staged engine
#   .\scripts\build_app.ps1 -NoBundle       # faster, unoptimised, no installers

[CmdletBinding()]
param(
    # Reuse desktop/src-tauri/engine as-is. Only safe when no Python file changed.
    [switch]$SkipEngine,
    # Build a debug binary and skip bundling. Much faster for a smoke test.
    # Not named -Debug: that collides with CmdletBinding's common parameter.
    [switch]$NoBundle
)

$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
$desktop = Join-Path $repoRoot 'desktop'
$engineDir = Join-Path $desktop 'src-tauri\engine'

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

# ── 1. Python engine ──────────────────────────────────────────────────────────
if ($SkipEngine) {
    if (-not (Test-Path (Join-Path $engineDir 'wallpaper-changer-rpc.exe'))) {
        throw "-SkipEngine was passed but no engine is staged at $engineDir."
    }
    Write-Host '==> Reusing the staged engine' -ForegroundColor Yellow
}
else {
    & (Join-Path $PSScriptRoot 'build_engine.ps1')
}

# ── 2. Frontend + Tauri ───────────────────────────────────────────────────────
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

# ── 3. Report ─────────────────────────────────────────────────────────────────
$bundleDir = Join-Path $desktop 'src-tauri\target\release\bundle'
$artifacts = Get-ChildItem $bundleDir -Recurse -Include *.msi, *.exe -ErrorAction SilentlyContinue
if (-not $artifacts) { throw "The bundler produced no installers under $bundleDir." }

Write-Host ''
Write-Host '==> Installers' -ForegroundColor Green
foreach ($file in $artifacts) {
    '    {0,-42} {1,8:N1} MB' -f $file.Name, ($file.Length / 1MB) | Write-Host
}
