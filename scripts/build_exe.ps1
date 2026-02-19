<#
.SYNOPSIS
    Compila WallpaperChanger para um executavel Windows (.exe) e gera o instalador.

.DESCRIPTION
    1. Sincroniza dependencias com uv
    2. Remove builds anteriores (build/ e dist/)
    3. Executa PyInstaller com o wallpaper_changer.spec
    4. Cria as pastas de runtime necessarias dentro de dist/
    5. (Opcional) Gera o instalador com Inno Setup
    6. Exibe o caminho do executavel gerado

.USAGE
    cd wallpaper-changer
    .\scripts\build_exe.ps1
    .\scripts\build_exe.ps1 -NoInstaller   # pula a geracao do instalador
#>

param(
    [switch]$NoInstaller
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

# -- Raiz do projeto -----------------------------------------------------------
$ProjectRoot = Split-Path -Parent $PSScriptRoot
Set-Location $ProjectRoot
Write-Host "Raiz do projeto: $ProjectRoot" -ForegroundColor Cyan

# -- Instalar / atualizar dependencias ----------------------------------------
Write-Host "`n[1/6] Sincronizando dependencias (uv sync --dev)..." -ForegroundColor Yellow
uv sync --dev
if ($LASTEXITCODE -ne 0) { throw "uv sync falhou." }

# -- Encerrar processo em execucao --------------------------------------------
$running = Get-Process WallpaperChanger -ErrorAction SilentlyContinue
if ($running) {
    Write-Host "  Encerrando WallpaperChanger.exe em execucao (PID $($running.Id))..."
    $running | Stop-Process -Force
    Start-Sleep 1
}

# -- Limpar builds anteriores --------------------------------------------------
Write-Host "`n[2/6] Limpando builds anteriores..." -ForegroundColor Yellow
foreach ($dir in @("build", "dist")) {
    if (Test-Path $dir) {
        Remove-Item $dir -Recurse -Force
        Write-Host "  Removido: $dir"
    }
}

# -- Compilar com PyInstaller --------------------------------------------------
Write-Host "`n[3/6] Compilando com PyInstaller..." -ForegroundColor Yellow
$PythonExe = Join-Path $ProjectRoot ".venv\Scripts\python.exe"
& $PythonExe -m PyInstaller wallpaper_changer.spec --noconfirm
if ($LASTEXITCODE -ne 0) { throw "PyInstaller falhou. Verifique os erros acima." }

# -- Criar pastas de runtime ---------------------------------------------------
Write-Host "`n[4/6] Criando pastas de runtime em dist/WallpaperChanger/..." -ForegroundColor Yellow
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

# Copiar settings.toml se nao foi empacotado pela spec
$SettingsSrc = Join-Path $ProjectRoot "config\settings.toml"
$SettingsDst = Join-Path $DistRoot "config\settings.toml"
if ((Test-Path $SettingsSrc) -and (-not (Test-Path $SettingsDst))) {
    Copy-Item $SettingsSrc $SettingsDst
    Write-Host "  Copiado: config\settings.toml"
}

# -- Verificar executavel ------------------------------------------------------
Write-Host "`n[5/6] Verificando executavel..." -ForegroundColor Yellow
$ExePath = Join-Path $DistRoot "WallpaperChanger.exe"
if (Test-Path $ExePath) {
    $sizeMB = [math]::Round((Get-Item $ExePath).Length / 1MB, 1)
    Write-Host "  Executavel : $ExePath" -ForegroundColor Green
    Write-Host "  Tamanho    : ${sizeMB} MB" -ForegroundColor Green
} else {
    Write-Warning "Executavel nao encontrado em $ExePath"
    exit 1
}

# -- Gerar instalador com Inno Setup ------------------------------------------
if (-not $NoInstaller) {
    Write-Host "`n[6/6] Gerando instalador com Inno Setup..." -ForegroundColor Yellow

    # Procurar ISCC.exe
    $IsccPaths = @(
        "C:\Program Files (x86)\Inno Setup 6\ISCC.exe",
        "C:\Program Files\Inno Setup 6\ISCC.exe",
        "${env:LOCALAPPDATA}\Programs\Inno Setup 6\ISCC.exe"
    )
    $Iscc = $null
    foreach ($p in $IsccPaths) {
        if (Test-Path $p) { $Iscc = $p; break }
    }

    if ($Iscc) {
        $IssFile = Join-Path $ProjectRoot "installer.iss"
        & $Iscc $IssFile
        if ($LASTEXITCODE -eq 0) {
            $InstallerPath = Join-Path $ProjectRoot "dist\WallpaperChanger_Setup.exe"
            if (Test-Path $InstallerPath) {
                $instSize = [math]::Round((Get-Item $InstallerPath).Length / 1MB, 1)
                Write-Host "  Instalador : $InstallerPath" -ForegroundColor Green
                Write-Host "  Tamanho    : ${instSize} MB" -ForegroundColor Green
            }
        } else {
            Write-Warning "Inno Setup falhou. O executavel standalone ainda esta disponivel em dist\WallpaperChanger\"
        }
    } else {
        Write-Host "  Inno Setup nao encontrado. Pulando geracao do instalador." -ForegroundColor Yellow
        Write-Host "  Instale: https://jrsoftware.org/isdl.php" -ForegroundColor Yellow
        Write-Host "  Ou compile manualmente: ISCC.exe installer.iss" -ForegroundColor Yellow
    }
} else {
    Write-Host "`n[6/6] Instalador pulado (-NoInstaller)." -ForegroundColor Yellow
}

Write-Host ""
Write-Host "Build concluido!" -ForegroundColor Green
Write-Host "  Pasta portavel: $DistRoot" -ForegroundColor Cyan
