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
#define MyAppVersion "3.0.0"
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
SetupIconFile=assets\icon\WallpaperChanger.ico
UninstallDisplayIcon={app}\WallpaperChanger.ico

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "portuguese"; MessagesFile: "compiler:Languages\BrazilianPortuguese.isl"
Name: "japanese"; MessagesFile: "compiler:Languages\Japanese.isl"

[Tasks]
Name: "desktopicon"; Description: "Create desktop shortcut"; GroupDescription: "Shortcuts:"; Flags: unchecked
Name: "startup"; Description: "Start with Windows"; GroupDescription: "Options:"; Flags: unchecked

[Files]
; Copia toda a pasta gerada pelo PyInstaller
Source: "dist\WallpaperChanger\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "assets\icon\WallpaperChanger.ico"; DestDir: "{app}"; Flags: ignoreversion

[Dirs]
; Pastas de runtime que o app precisa
Name: "{app}\assets\wallpapers"
Name: "{app}\assets\output"
Name: "{app}\config"

[Icons]
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; IconFilename: "{app}\WallpaperChanger.ico"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; IconFilename: "{app}\WallpaperChanger.ico"; Tasks: desktopicon

[Registry]
; Inicio automatico com o Windows (apenas se o usuario marcar a opcao)
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "{#MyAppName}"; ValueData: """{app}\{#MyAppExeName}"" --startup"; Flags: uninsdeletevalue; Tasks: startup

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "Launch {#MyAppName}"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
; Limpa arquivos temporarios gerados em runtime
Type: filesandordirs; Name: "{app}\assets\output"
Type: files; Name: "{app}\config\state.json"

[Code]
var
  LanguagePage: TInputOptionWizardPage;

procedure InitializeWizard;
begin
  { Create a custom page for application language selection }
  LanguagePage := CreateInputOptionPage(
    wpSelectTasks,
    'Application Language', 'Choose the language for the WallpaperChanger interface.',
    'Select the language you want to use inside the application:',
    True, False);
  LanguagePage.Add('English');
  LanguagePage.Add('Português (Brasil)');
  LanguagePage.Add('日本語 (Japanese)');
  { Default to English }
  LanguagePage.SelectedValueIndex := 0;
end;

function GetAppLanguageCode: String;
begin
  case LanguagePage.SelectedValueIndex of
    0: Result := 'en';
    1: Result := 'pt_BR';
    2: Result := 'ja';
  else
    Result := 'en';
  end;
end;

procedure WriteSettingsToml;
var
  Lines: TArrayOfString;
  ConfigPath: String;
begin
  ConfigPath := ExpandConstant('{app}\config\settings.toml');

  SetArrayLength(Lines, 23);
  Lines[0]  := '[general]';
  Lines[1]  := 'mode = "collage"';
  Lines[2]  := 'selection = "random"';
  Lines[3]  := 'interval = 300';
  Lines[4]  := 'collage_count = 4';
  Lines[5]  := 'collage_same_for_all = false';
  Lines[6]  := 'language = "' + GetAppLanguageCode + '"';
  Lines[7]  := '';
  Lines[8]  := '[paths]';
  Lines[9]  := 'wallpapers_folder = "C:\\Users\\Public\\Pictures"';
  Lines[10] := 'output_folder = "assets/output"';
  Lines[11] := 'default_wallpaper = ""';
  Lines[12] := '';
  Lines[13] := '[display]';
  Lines[14] := 'fit_mode = "fill"';
  Lines[15] := '';
  Lines[16] := '[hotkeys]';
  Lines[17] := 'next_wallpaper = "ctrl+alt+right"';
  Lines[18] := 'prev_wallpaper = "ctrl+alt+left"';
  Lines[19] := 'stop_watch = "ctrl+alt+s"';
  Lines[20] := 'default_wallpaper = "ctrl+alt+d"';
  Lines[21] := '';
  Lines[22] := '';

  { Only write if settings.toml does not exist yet (fresh install) }
  if not FileExists(ConfigPath) then
    SaveStringsToUTF8File(ConfigPath, Lines, False);
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssPostInstall then
    WriteSettingsToml;
end;
