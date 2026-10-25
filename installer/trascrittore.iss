; Installer Inno Setup per Trascrittore audio. Compilato dalla GitHub Action (vedi .github/workflows/build.yml).
; Compilazione manuale:  ISCC.exe /DAppVersion=0.1.0 installer\trascrittore.iss   (dopo cargo build --release)

#ifndef AppVersion
  #define AppVersion "0.1.0"
#endif

[Setup]
AppId={{8F2D5B1A-3C47-4E9B-A6D1-52C7B0E94F13}
AppName=Trascrittore audio
AppVersion={#AppVersion}
AppPublisher=aleDiPi
DefaultDirName={autopf}\Trascrittore
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=Output
OutputBaseFilename=Trascrittore-Setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
UninstallDisplayIcon={app}\trascrittore.exe
; Stesso nome del mutex usato dal programma: l'installer chiede di chiuderlo se e' in esecuzione
AppMutex=Local\TrascrittoreAudioSingleInstance

[Languages]
Name: "italian"; MessagesFile: "compiler:Languages\Italian.isl"

[Types]
Name: "compact"; Description: "Solo il programma (consigliato)"
Name: "full"; Description: "Programma + strumenti di sviluppo (Rust e Visual Studio Build Tools)"
Name: "custom"; Description: "Personalizzata"; Flags: iscustom

[Components]
Name: "app"; Description: "Trascrittore audio"; Types: compact full custom; Flags: fixed
Name: "devtools"; Description: "Rust + Visual Studio Build Tools (servono solo per compilare dal sorgente, download di diversi GB)"; Types: full

[Tasks]
Name: "autostart"; Description: "Avvia Trascrittore con Windows (parte ridotto nella tray)"

[Files]
Source: "..\target\release\trascrittore.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\scripts\setup-windows.ps1"; DestDir: "{app}\scripts"; Flags: ignoreversion
; Configurazione e segreti dell'utente: creati solo se mancano, mai cancellati dalla disinstallazione
Source: "..\config\config.example.toml"; DestDir: "{userappdata}\Trascrittore"; DestName: "config.toml"; Flags: onlyifdoesntexist uninsneveruninstall
Source: "..\.env.example"; DestDir: "{userappdata}\Trascrittore"; DestName: ".env"; Flags: onlyifdoesntexist uninsneveruninstall

[Icons]
Name: "{autoprograms}\Trascrittore audio"; Filename: "{app}\trascrittore.exe"
Name: "{autoprograms}\Trascrittore - cartella configurazione"; Filename: "{userappdata}\Trascrittore"

[Registry]
; Stessa chiave e valore usati dalla voce "Avvia con Windows" del menu nella tray
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "TrascrittoreAudio"; ValueData: """{app}\trascrittore.exe"""; Tasks: autostart; Flags: uninsdeletevalue

[Run]
Filename: "powershell.exe"; Parameters: "-NoProfile -ExecutionPolicy Bypass -File ""{app}\scripts\setup-windows.ps1"""; Components: devtools; StatusMsg: "Installo Rust e Visual Studio Build Tools (puo' richiedere diversi minuti)..."; Flags: waituntilterminated
Filename: "{app}\trascrittore.exe"; Description: "Avvia Trascrittore audio"; Flags: nowait postinstall

[Code]
var
  TokenPage: TInputQueryWizardPage;

procedure InitializeWizard;
begin
  TokenPage := CreateInputQueryPage(wpSelectTasks, 'Aggiornamenti automatici (facoltativo)',
    'Token GitHub di sola lettura',
    'Con un token GitHub "fine-grained" (solo lettura dei contenuti di aleDiPi/audio_trascriptor) il programma si aggiorna da solo. ' +
    'Lascia il campo vuoto per aggiornare a mano scaricando il nuovo installer. Come crearlo: vedi il README. ' +
    'Il token resta solo sul tuo PC (file .env), non e'' dentro il programma.');
  TokenPage.Add('Token GitHub:', True);
end;

procedure SaveToken(Token: String);
var
  Path: String;
  Lines: TArrayOfString;
  I, N: Integer;
  Found: Boolean;
begin
  Path := ExpandConstant('{userappdata}\Trascrittore\.env');
  if not LoadStringsFromFile(Path, Lines) then
    SetArrayLength(Lines, 0);
  Found := False;
  for I := 0 to GetArrayLength(Lines) - 1 do
    if Pos('TRASCRITTORE_UPDATE_TOKEN=', Trim(Lines[I])) = 1 then
    begin
      Lines[I] := 'TRASCRITTORE_UPDATE_TOKEN=' + Token;
      Found := True;
    end;
  if not Found then
  begin
    N := GetArrayLength(Lines);
    SetArrayLength(Lines, N + 1);
    Lines[N] := 'TRASCRITTORE_UPDATE_TOKEN=' + Token;
  end;
  SaveStringsToFile(Path, Lines, False);
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  { campo vuoto o installazione silenziosa (aggiornamento): il token gia' salvato non viene toccato }
  if (CurStep = ssPostInstall) and (Trim(TokenPage.Values[0]) <> '') then
    SaveToken(Trim(TokenPage.Values[0]));
end;
