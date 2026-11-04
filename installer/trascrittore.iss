; Inno Setup installer for Trascrittore audio. Built by the GitHub Action (see .github/workflows/build.yml).
; Manual build:  ISCC.exe /DAppVersion=0.1.0 installer\trascrittore.iss   (after cargo build --release)
;
; Languages: the wizard starts in the language of Windows and the person can pick another one in the first dialog; the
; whole wizard switches at once. The chosen language is also written to config.toml (ui_language) at the end.
; To add a language: add a line to [Languages] and a block of lines to [CustomMessages] (see CONTRIBUTING.md).

#ifndef AppVersion
  #define AppVersion "0.1.0"
#endif

[Setup]
AppId={{8F2D5B1A-3C47-4E9B-A6D1-52C7B0E94F13}
AppName=Trascrittore audio
AppVersion={#AppVersion}
AppPublisher=aleDiPi
; Program in C:\Program Files\Trascrittore (whole PC, 64 bit). Writable data (config.toml, .env, log) in C:\ProgramData\Trascrittore
DefaultDirName={autopf}\Trascrittore
; Whoever had the previous version (ProgramData folder) is moved to Program Files
UsePreviousAppDir=no
DisableProgramGroupPage=yes
PrivilegesRequired=admin
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=Output
OutputBaseFilename=Trascrittore-Setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
; The first dialog proposes the language of Windows and lets the person pick another one; the wizard then switches at once
ShowLanguageDialog=yes
UsePreviousLanguage=no
LanguageDetectionMethod=uilanguage
UninstallDisplayIcon={app}\trascrittore.exe
SetupIconFile=..\assets\icon.ico
; Same mutex name used by the program: the installer asks to close it if it is running
AppMutex=Local\TrascrittoreAudioSingleInstance

[Languages]
; First page of the installer: notice about third-party AI services and privacy (it must be accepted to continue). One LicenseFile for each language.
Name: "english"; MessagesFile: "compiler:Default.isl"; LicenseFile: "..\legal\DISCLAIMER.en.txt"
Name: "italian"; MessagesFile: "compiler:Languages\Italian.isl"; LicenseFile: "..\legal\DISCLAIMER.it.txt"

[Types]
Name: "compact"; Description: "{cm:TypeCompact}"
Name: "full"; Description: "{cm:TypeFull}"
Name: "custom"; Description: "{cm:TypeCustom}"; Flags: iscustom

[Components]
Name: "app"; Description: "Trascrittore audio"; Types: compact full custom; Flags: fixed
Name: "devtools"; Description: "{cm:CompDevtools}"; Types: full

[Dirs]
; Program data: the users of the PC can write there (settings, keys, log). The exe stays outside, in Program Files, not modifiable without administrator rights
Name: "{commonappdata}\Trascrittore"; Permissions: users-modify

[InstallDelete]
; Leftovers of the previous version installed in ProgramData (program and scripts only: config and .env stay)
Type: files; Name: "{commonappdata}\Trascrittore\trascrittore.exe"
Type: files; Name: "{commonappdata}\Trascrittore\trascrittore.exe.old"
Type: filesandordirs; Name: "{commonappdata}\Trascrittore\scripts"

[Tasks]
Name: "autostart"; Description: "{cm:TaskAutostart}"

[Files]
Source: "..\target\release\trascrittore.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\scripts\setup-windows.ps1"; DestDir: "{app}\scripts"; Flags: ignoreversion
; Configuration and keys: created only if missing (not overwritten by updates); uninstalling deletes them (see [UninstallDelete])
Source: "..\config\config.example.toml"; DestDir: "{commonappdata}\Trascrittore"; DestName: "config.toml"; Flags: onlyifdoesntexist
Source: "..\.env.example"; DestDir: "{commonappdata}\Trascrittore"; DestName: ".env"; Flags: onlyifdoesntexist

[UninstallDelete]
; Uninstalling leaves nothing behind: data (config.toml, .env with the keys, log), leftovers of previous versions and temporary downloads
Type: filesandordirs; Name: "{commonappdata}\Trascrittore"
Type: filesandordirs; Name: "{userappdata}\Trascrittore"
Type: filesandordirs; Name: "{localappdata}\Programs\Trascrittore"
Type: filesandordirs; Name: "{localappdata}\Temp\Trascrittore-Update"
Type: filesandordirs; Name: "{app}"

[Icons]
Name: "{autoprograms}\Trascrittore audio"; Filename: "{app}\trascrittore.exe"
Name: "{autoprograms}\{cm:ConfigFolderShortcut}"; Filename: "{commonappdata}\Trascrittore"

[Registry]
; Autostart entry: removed on uninstall even if the tray menu created it
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: none; ValueName: "TrascrittoreAudio"; Flags: dontcreatekey uninsdeletevalue
; Autostart of the old per-user installation: replaced by the one below (if chosen)
; Same key and value used by the "Start with Windows" entry of the tray menu
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "TrascrittoreAudio"; ValueData: """{app}\trascrittore.exe"""; Tasks: autostart; Flags: uninsdeletevalue

[Run]
Filename: "powershell.exe"; Parameters: "-NoProfile -ExecutionPolicy Bypass -File ""{app}\scripts\setup-windows.ps1"""; Components: devtools; StatusMsg: "{cm:StatusDevtools}"; Flags: waituntilterminated
; Tells the program the language chosen in the wizard; it is saved only if no language was chosen before (see config.rs)
Filename: "{app}\trascrittore.exe"; Parameters: "--set-language {code:ProgramLanguage}"; Flags: runhidden waituntilterminated; Check: ChosenInWizard
Filename: "{app}\trascrittore.exe"; Description: "{cm:LaunchApp}"; Flags: nowait postinstall

[CustomMessages]
english.TypeCompact=Program only (recommended)
english.TypeFull=Program + development tools (Rust and Visual Studio Build Tools)
english.TypeCustom=Custom
english.CompDevtools=Rust + Visual Studio Build Tools (only needed to build from source, a download of several GB)
english.TaskAutostart=Start Trascrittore with Windows (starts minimized in the tray)
english.ConfigFolderShortcut=Trascrittore - configuration folder
english.StatusDevtools=Installing Rust and Visual Studio Build Tools (this can take several minutes)...
english.LaunchApp=Start Trascrittore audio
italian.TypeCompact=Solo il programma (consigliato)
italian.TypeFull=Programma + strumenti di sviluppo (Rust e Visual Studio Build Tools)
italian.TypeCustom=Personalizzata
italian.CompDevtools=Rust + Visual Studio Build Tools (servono solo per compilare dal sorgente, download di diversi GB)
italian.TaskAutostart=Avvia Trascrittore con Windows (parte ridotto nella tray)
italian.ConfigFolderShortcut=Trascrittore - cartella configurazione
italian.StatusDevtools=Installo Rust e Visual Studio Build Tools (puo' richiedere diversi minuti)...
italian.LaunchApp=Avvia Trascrittore audio

[Code]
{ Chi aveva la vecchia versione per-utente: porta con se' config.toml e .env (chiavi comprese) nella nuova cartella }
procedure MigrateOldUserFiles;
var
  OldDir, NewDir: String;
begin
  OldDir := ExpandConstant('{userappdata}\Trascrittore');
  NewDir := ExpandConstant('{commonappdata}\Trascrittore');
  ForceDirectories(NewDir);
  if FileExists(OldDir + '\config.toml') and not FileExists(NewDir + '\config.toml') then
    FileCopy(OldDir + '\config.toml', NewDir + '\config.toml', True);
  if FileExists(OldDir + '\.env') and not FileExists(NewDir + '\.env') then
    FileCopy(OldDir + '\.env', NewDir + '\.env', True);
end;

{ Language code of the program for the language chosen in the wizard (the file names in lang/ are the codes) }
function ProgramLanguage(Param: String): String;
begin
  Result := ActiveLanguage;
  if Result = 'italian' then Result := 'it'
  else if Result = 'english' then Result := 'en';
end;

{ Silent installations (self-update) never touch the language }
function ChosenInWizard: Boolean;
begin
  Result := not WizardSilent;
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssInstall then
    MigrateOldUserFiles;
end;
