; Inno Setup script para WallpaperChanger
; Gera o instalador WallpaperChanger_Setup.exe
;
; Pre-requisitos:
;   1. Inno Setup 6 instalado (https://jrsoftware.org/isinfo.php)
;   2. PyInstaller ja executado (pasta dist\WallpaperChanger existente)
;
; Para compilar manualmente:
;   "C:\Program Files (x86)\Inno Setup 6\ISCC.exe" installer.iss

#define MyAppName "WallpaperChanger"
#define MyAppVersion "1.0.0"
#define MyAppPublisher "WallpaperChanger"
#define MyAppExeName "WallpaperChanger.exe"

[Setup]
AppId={{B2F7A1C3-8D4E-4F6A-9B2C-1E3D5F7A8B9C}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
DefaultDirName={autopf}\{#MyAppName}
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes
OutputDir=dist
OutputBaseFilename=WallpaperChanger_Setup
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
UninstallDisplayName={#MyAppName}
SetupLogging=yes

[Languages]
Name: "portuguese"; MessagesFile: "compiler:Languages\BrazilianPortuguese.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "Criar atalho na area de trabalho"; GroupDescription: "Atalhos:"; Flags: unchecked
Name: "startup"; Description: "Iniciar com o Windows"; GroupDescription: "Opcoes:"; Flags: unchecked

[Files]
; Copia toda a pasta gerada pelo PyInstaller
Source: "dist\WallpaperChanger\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Dirs]
; Pastas de runtime que o app precisa
Name: "{app}\assets\wallpapers"
Name: "{app}\assets\output"
Name: "{app}\config"

[Icons]
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon

[Registry]
; Inicio automatico com o Windows (apenas se o usuario marcar a opcao)
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "{#MyAppName}"; ValueData: """{app}\{#MyAppExeName}"""; Flags: uninsdeletevalue; Tasks: startup

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "Iniciar {#MyAppName}"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
; Limpa arquivos temporarios gerados em runtime
Type: filesandordirs; Name: "{app}\assets\output"
Type: files; Name: "{app}\config\state.json"
