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
; Cartella legata al PC, non all'utente: C:\ProgramData\Trascrittore (programma, config.toml, .env e log insieme)
DefaultDirName={commonappdata}\Trascrittore
DisableProgramGroupPage=yes
PrivilegesRequired=admin
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

[Dirs]
; Gli utenti del PC possono scrivere qui: serve all'aggiornamento automatico (sostituisce l'exe senza UAC) e alle impostazioni
Name: "{app}"; Permissions: users-modify

[Tasks]
Name: "autostart"; Description: "Avvia Trascrittore con Windows (parte ridotto nella tray)"

[Files]
Source: "..\target\release\trascrittore.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\scripts\setup-windows.ps1"; DestDir: "{app}\scripts"; Flags: ignoreversion
; Configurazione e segreti dell'utente: creati solo se mancano, mai cancellati dalla disinstallazione
Source: "..\config\config.example.toml"; DestDir: "{app}"; DestName: "config.toml"; Flags: onlyifdoesntexist uninsneveruninstall
Source: "..\.env.example"; DestDir: "{app}"; DestName: ".env"; Flags: onlyifdoesntexist uninsneveruninstall

[Icons]
Name: "{autoprograms}\Trascrittore audio"; Filename: "{app}\trascrittore.exe"
Name: "{autoprograms}\Trascrittore - cartella configurazione"; Filename: "{app}"

[Registry]
; Eventuale avvio automatico della vecchia installazione per-utente: viene sostituito da quello sotto (se scelto)
; Stessa chiave e valore usati dalla voce "Avvia con Windows" del menu nella tray
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "TrascrittoreAudio"; ValueData: """{app}\trascrittore.exe"""; Tasks: autostart; Flags: uninsdeletevalue

[Run]
Filename: "powershell.exe"; Parameters: "-NoProfile -ExecutionPolicy Bypass -File ""{app}\scripts\setup-windows.ps1"""; Components: devtools; StatusMsg: "Installo Rust e Visual Studio Build Tools (puo' richiedere diversi minuti)..."; Flags: waituntilterminated
Filename: "{app}\trascrittore.exe"; Description: "Avvia Trascrittore audio"; Flags: nowait postinstall

[Code]
var
  TokenPage: TInputQueryWizardPage;

procedure OpenTokenPage(Sender: TObject);
var
  ErrorCode: Integer;
begin
  ShellExec('open', 'https://github.com/settings/personal-access-tokens/new', '', '', SW_SHOWNORMAL, ewNoWait, ErrorCode);
end;

procedure InitializeWizard;
var
  Steps: TNewMemo;
  Link: TNewStaticText;
begin
  TokenPage := CreateInputQueryPage(wpSelectTasks, 'Aggiornamenti automatici (facoltativo)',
    'Token GitHub di sola lettura',
    'Con un token il programma si aggiorna da solo. Lascia il campo vuoto per aggiornare a mano scaricando il nuovo installer. ' +
    'Il token resta solo sul tuo PC (file .env), non e'' dentro il programma.');
  TokenPage.Add('Token GitHub:', True);

  Link := TNewStaticText.Create(TokenPage);
  Link.Parent := TokenPage.Surface;
  Link.Caption := 'https://github.com/settings/personal-access-tokens/new';
  Link.Cursor := crHand;
  Link.Font.Style := Link.Font.Style + [fsUnderline];
  Link.Font.Color := clBlue;
  Link.Left := 0;
  Link.Top := TokenPage.Edits[0].Top + TokenPage.Edits[0].Height + ScaleY(8);
  Link.OnClick := @OpenTokenPage;

  Steps := TNewMemo.Create(TokenPage);
  Steps.Parent := TokenPage.Surface;
  Steps.Left := 0;
  Steps.Width := TokenPage.SurfaceWidth;
  Steps.Top := Link.Top + Link.Height + ScaleY(6);
  Steps.Height := TokenPage.SurfaceHeight - Steps.Top;
  Steps.ReadOnly := True;
  Steps.ScrollBars := ssVertical;
  Steps.Text :=
    'Come creare il token (clic sul link qui sopra, serve l''accesso a GitHub con l''account aleDiPi):' + #13#10 +
    '1. Token name: per esempio "Trascrittore audio".' + #13#10 +
    '2. Expiration: scegli una scadenza (per esempio 1 anno; alla scadenza ne crei un altro dalle Impostazioni).' + #13#10 +
    '3. Resource owner: aleDiPi.' + #13#10 +
    '4. Repository access: "Only select repositories" e scegli solo aleDiPi/audio_trascriptor.' + #13#10 +
    '5. Permissions, Repository permissions: Contents = "Read-only". Non aggiungere altri permessi.' + #13#10 +
    '6. Premi "Generate token" e copia subito il codice (inizia con github_pat_): GitHub lo mostra una volta sola.' + #13#10 +
    '7. Incollalo nel campo qui sopra. Puoi anche farlo dopo dalle Impostazioni del programma (clic sull''icona nella tray).';
end;

procedure SaveToken(Token: String);
var
  Path: String;
  Lines: TArrayOfString;
  I, N: Integer;
  Found: Boolean;
begin
  Path := ExpandConstant('{app}\.env');
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

{ Chi aveva la vecchia versione per-utente: porta con se' config.toml e .env (chiavi comprese) nella nuova cartella }
procedure MigrateOldUserFiles;
var
  OldDir, NewDir: String;
begin
  OldDir := ExpandConstant('{userappdata}\Trascrittore');
  NewDir := ExpandConstant('{app}');
  ForceDirectories(NewDir);
  if FileExists(OldDir + '\config.toml') and not FileExists(NewDir + '\config.toml') then
    FileCopy(OldDir + '\config.toml', NewDir + '\config.toml', True);
  if FileExists(OldDir + '\.env') and not FileExists(NewDir + '\.env') then
    FileCopy(OldDir + '\.env', NewDir + '\.env', True);
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssInstall then
    MigrateOldUserFiles;
  { campo vuoto o installazione silenziosa (aggiornamento): il token gia' salvato non viene toccato }
  if (CurStep = ssPostInstall) and (Trim(TokenPage.Values[0]) <> '') then
    SaveToken(Trim(TokenPage.Values[0]));
end;
