<#
.SYNOPSIS
    Compila WallpaperChanger para um executavel Windows (.exe) usando PyInstaller.

.DESCRIPTION
    1. Instala / atualiza PyInstaller no ambiente uv
    2. Remove builds anteriores (build/ e dist/)
    3. Executa PyInstaller com o wallpaper_changer.spec
    4. Cria as pastas de runtime necesarias dentro de dist/
    5. Exibe o caminho do executavel gerado

.USAGE
    cd wallpaper-changer
    .\scripts\build_exe.ps1
#>

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

# -- Raiz do projeto -----------------------------------------------------------
$ProjectRoot = Split-Path -Parent $PSScriptRoot
Set-Location $ProjectRoot
Write-Host "Raiz do projeto: $ProjectRoot" -ForegroundColor Cyan

# -- Instalar / atualizar PyInstaller -----------------------------------------
Write-Host "`n[1/5] Sincronizando dependencias (uv sync --dev)..." -ForegroundColor Yellow
uv sync --dev
if ($LASTEXITCODE -ne 0) { throw "uv sync falhou." }

# -- Encerrar processo em execucao (evita bloqueio de DLL) --------------------
$running = Get-Process WallpaperChanger -ErrorAction SilentlyContinue
if ($running) {
    Write-Host "  Encerrando WallpaperChanger.exe em execucao (PID $($running.Id))..."
    $running | Stop-Process -Force
    Start-Sleep 1
}

# -- Limpar builds anteriores --------------------------------------------------
Write-Host "`n[2/5] Limpando builds anteriores..." -ForegroundColor Yellow
foreach ($dir in @("build", "dist")) {
    if (Test-Path $dir) {
        Remove-Item $dir -Recurse -Force
        Write-Host "  Removido: $dir"
    }
}

# -- Compilar com PyInstaller --------------------------------------------------
Write-Host "`n[3/5] Compilando com PyInstaller..." -ForegroundColor Yellow
$PythonExe = Join-Path $ProjectRoot ".venv\Scripts\python.exe"
& $PythonExe -m PyInstaller wallpaper_changer.spec --noconfirm
if ($LASTEXITCODE -ne 0) { throw "PyInstaller falhou. Verifique os erros acima." }

# -- Criar pastas de runtime ---------------------------------------------------
Write-Host "`n[4/5] Criando pastas de runtime em dist/WallpaperChanger/..." -ForegroundColor Yellow
$DistRoot = Join-Path $ProjectRoot "dist\WallpaperChanger"
$RuntimeFolders = @(
    "assets\wallpapers",
    "assets\output",
    "config"
)
foreach ($folder in $RuntimeFolders) {
    $full = Join-Path $DistRoot $folder
    if (-not (Test-Path $full)) {
        New-Item -ItemType Directory -Path $full -Force | Out-Null
        Write-Host "  Criado: $folder"
    } else {
        Write-Host "  Ja existe: $folder"
    }
}

# Copiar settings.toml se nao foi empacotado pela spec (seguranca extra)
$SettingsSrc = Join-Path $ProjectRoot "config\settings.toml"
$SettingsDst = Join-Path $DistRoot "config\settings.toml"
if ((Test-Path $SettingsSrc) -and (-not (Test-Path $SettingsDst))) {
    Copy-Item $SettingsSrc $SettingsDst
    Write-Host "  Copiado: config\settings.toml"
}

# -- Resultado -----------------------------------------------------------------
$ExePath = Join-Path $DistRoot "WallpaperChanger.exe"
Write-Host "`n[5/5] Build concluido!" -ForegroundColor Green

if (Test-Path $ExePath) {
    $sizeMB = [math]::Round((Get-Item $ExePath).Length / 1MB, 1)
    Write-Host "  Executavel : $ExePath" -ForegroundColor Green
    Write-Host "  Tamanho    : ${sizeMB} MB" -ForegroundColor Green
    Write-Host ""
    Write-Host "Para distribuir, copie toda a pasta:" -ForegroundColor Cyan
    Write-Host "  $DistRoot" -ForegroundColor Cyan
} else {
    Write-Warning "Executavel nao encontrado em $ExePath - verifique os logs do PyInstaller."
}
