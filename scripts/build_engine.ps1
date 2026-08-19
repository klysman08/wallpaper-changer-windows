# Builds the Python RPC sidecar and stages it for the Tauri bundler.
#
# The Tauri app (desktop/) ships this as a resource directory and spawns it as its
# engine. Run from the repository root:
#   .\scripts\build_engine.ps1
#
# Skip this during development: with no bundled engine present, the Rust side falls
# back to `uv run wallpaper-changer-rpc` against the checkout (see engine.rs).

[CmdletBinding()]
param(
    # Keep the PyInstaller work directory, useful when debugging a missing import.
    [switch]$KeepBuildDir
)

$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
$specFile = Join-Path $repoRoot 'wallpaper_changer_rpc.spec'
$distDir = Join-Path $repoRoot 'dist\wallpaper-changer-rpc'
$targetDir = Join-Path $repoRoot 'desktop\src-tauri\engine'

Write-Host '==> Building the RPC sidecar with PyInstaller' -ForegroundColor Cyan
Push-Location $repoRoot
try {
    # PyInstaller logs progress to stderr. Under Windows PowerShell 5.1 that would
    # trip $ErrorActionPreference='Stop' even on a successful build, so relax it here
    # and judge the outcome by the exit code instead.
    $previous = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        uv run pyinstaller $specFile --noconfirm --distpath (Join-Path $repoRoot 'dist') --workpath (Join-Path $repoRoot 'build')
    }
    finally {
        $ErrorActionPreference = $previous
    }
    if ($LASTEXITCODE -ne 0) { throw "PyInstaller failed with exit code $LASTEXITCODE" }
}
finally {
    Pop-Location
}

$exePath = Join-Path $distDir 'wallpaper-changer-rpc.exe'
if (-not (Test-Path $exePath)) {
    throw "Expected the sidecar at $exePath but it was not produced."
}

# Ship a default settings.toml as the seed for a fresh install: on first run the
# engine migrates it into %APPDATA%\WallpaperChanger and works there from then on.
# It has to sit next to the executable because config.py derives PROJECT_ROOT from
# sys.executable when frozen, whereas PyInstaller 6 stages data under _internal/.
$configSrc = Join-Path $repoRoot 'config\settings.toml'
$configDst = Join-Path $distDir 'config'
New-Item -ItemType Directory -Force $configDst | Out-Null
Copy-Item $configSrc $configDst -Force

Write-Host '==> Verifying the frozen sidecar speaks the protocol' -ForegroundColor Cyan
# A missing hidden import or a mislaid data file only shows up at runtime, so
# exercise the real binary rather than trusting that the build succeeded.
# get_config is the important probe: it is what catches settings.toml landing
# somewhere the frozen engine cannot see.
$probes = @(
    @{ Name = 'ping'; Json = '{"id":1,"method":"ping"}' },
    @{ Name = 'get_config'; Json = '{"id":2,"method":"get_config"}' },
    @{ Name = 'get_monitors'; Json = '{"id":3,"method":"get_monitors"}' },
    @{ Name = 'get_capabilities'; Json = '{"id":4,"method":"get_capabilities"}' }
)
$script = ($probes | ForEach-Object { $_.Json }) -join "`n"
# The engine logs to stderr — it announces the scroll hook whenever the machine
# running the build has that setting on. Under Windows PowerShell 5.1 each of those
# lines becomes a NativeCommandError, which $ErrorActionPreference='Stop' turns into
# a failed build even though the probe itself succeeded. Same treatment as the
# PyInstaller call above: relax it here and judge by what came back on stdout.
$previous = $ErrorActionPreference
$ErrorActionPreference = 'Continue'
try {
    $responses = @($script | & $exePath 2>$null)
}
finally {
    $ErrorActionPreference = $previous
}
foreach ($probe in $probes) {
    $line = $responses | Where-Object { $_ -match "`"id`":\s*$($probes.IndexOf($probe) + 1)\b" } | Select-Object -First 1
    if ($line -notmatch '"ok":\s*true') {
        throw "Frozen sidecar failed the '$($probe.Name)' probe. Got: $line"
    }
    Write-Host "    $($probe.Name) ok" -ForegroundColor Green
}

# The scroll-transparency probe that used to live here checked that PyInstaller had
# found pynput's dynamically-chosen backend. The hook is native now, so the sidecar
# has no such import to miss and `has_scroll_transparency` no longer says anything
# about the frozen build.

Write-Host "==> Staging into $targetDir" -ForegroundColor Cyan
if (Test-Path $targetDir) { Remove-Item $targetDir -Recurse -Force }
New-Item -ItemType Directory -Force $targetDir | Out-Null
Copy-Item (Join-Path $distDir '*') $targetDir -Recurse -Force

if (-not $KeepBuildDir) {
    $workDir = Join-Path $repoRoot 'build\wallpaper_changer_rpc'
    if (Test-Path $workDir) { Remove-Item $workDir -Recurse -Force }
}

$size = '{0:N0} MB' -f ((Get-ChildItem $targetDir -Recurse | Measure-Object -Property Length -Sum).Sum / 1MB)
Write-Host "==> Engine staged ($size). Now run: cd desktop; bun run tauri build" -ForegroundColor Green
