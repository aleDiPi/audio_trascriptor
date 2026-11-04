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
; Programma in C:\Program Files\Trascrittore (per tutto il PC, 64 bit). Dati scrivibili (config.toml, .env, log) in C:\ProgramData\Trascrittore
DefaultDirName={autopf}\Trascrittore
; Chi aveva la versione precedente (cartella ProgramData) viene spostato in Program Files
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
UninstallDisplayIcon={app}\trascrittore.exe
SetupIconFile=..\assets\icon.ico
; Stesso nome del mutex usato dal programma: l'installer chiede di chiuderlo se e' in esecuzione
AppMutex=Local\TrascrittoreAudioSingleInstance

[Languages]
; Prima pagina dell'installer: avviso su servizi AI di terzi e privacy (obbligo di accettarlo per proseguire). Una riga LicenseFile per ogni lingua.
Name: "italian"; MessagesFile: "compiler:Languages\Italian.isl"; LicenseFile: "..\legal\DISCLAIMER.it.txt"

[Types]
Name: "compact"; Description: "Solo il programma (consigliato)"
Name: "full"; Description: "Programma + strumenti di sviluppo (Rust e Visual Studio Build Tools)"
Name: "custom"; Description: "Personalizzata"; Flags: iscustom

[Components]
Name: "app"; Description: "Trascrittore audio"; Types: compact full custom; Flags: fixed
Name: "devtools"; Description: "Rust + Visual Studio Build Tools (servono solo per compilare dal sorgente, download di diversi GB)"; Types: full

[Dirs]
; Dati del programma: gli utenti del PC possono scriverci (impostazioni, chiavi, log). L'exe sta fuori, in Program Files, non modificabile senza amministratore
Name: "{commonappdata}\Trascrittore"; Permissions: users-modify

[InstallDelete]
; Resti della versione precedente installata in ProgramData (solo programma e script: config e .env restano)
Type: files; Name: "{commonappdata}\Trascrittore\trascrittore.exe"
Type: files; Name: "{commonappdata}\Trascrittore\trascrittore.exe.old"
Type: filesandordirs; Name: "{commonappdata}\Trascrittore\scripts"

[Tasks]
Name: "autostart"; Description: "Avvia Trascrittore con Windows (parte ridotto nella tray)"

[Files]
Source: "..\target\release\trascrittore.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\scripts\setup-windows.ps1"; DestDir: "{app}\scripts"; Flags: ignoreversion
; Configurazione e chiavi: create solo se mancano (non sovrascritte dagli aggiornamenti); la disinstallazione le cancella (vedi [UninstallDelete])
Source: "..\config\config.example.toml"; DestDir: "{commonappdata}\Trascrittore"; DestName: "config.toml"; Flags: onlyifdoesntexist
Source: "..\.env.example"; DestDir: "{commonappdata}\Trascrittore"; DestName: ".env"; Flags: onlyifdoesntexist

[UninstallDelete]
; La disinstallazione non lascia nulla in giro: dati (config.toml, .env con le chiavi, log), resti di versioni precedenti e download temporanei
Type: filesandordirs; Name: "{commonappdata}\Trascrittore"
Type: filesandordirs; Name: "{userappdata}\Trascrittore"
Type: filesandordirs; Name: "{localappdata}\Programs\Trascrittore"
Type: filesandordirs; Name: "{localappdata}\Temp\Trascrittore-Update"
Type: filesandordirs; Name: "{app}"

[Icons]
Name: "{autoprograms}\Trascrittore audio"; Filename: "{app}\trascrittore.exe"
Name: "{autoprograms}\Trascrittore - cartella configurazione"; Filename: "{commonappdata}\Trascrittore"

[Registry]
; Voce di avvio automatico: rimossa alla disinstallazione anche se l'ha creata il menu della tray
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: none; ValueName: "TrascrittoreAudio"; Flags: dontcreatekey uninsdeletevalue
; Eventuale avvio automatico della vecchia installazione per-utente: viene sostituito da quello sotto (se scelto)
; Stessa chiave e valore usati dalla voce "Avvia con Windows" del menu nella tray
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "TrascrittoreAudio"; ValueData: """{app}\trascrittore.exe"""; Tasks: autostart; Flags: uninsdeletevalue

[Run]
Filename: "powershell.exe"; Parameters: "-NoProfile -ExecutionPolicy Bypass -File ""{app}\scripts\setup-windows.ps1"""; Components: devtools; StatusMsg: "Installo Rust e Visual Studio Build Tools (puo' richiedere diversi minuti)..."; Flags: waituntilterminated
Filename: "{app}\trascrittore.exe"; Description: "Avvia Trascrittore audio"; Flags: nowait postinstall

[Code]
var
  TokenPage: TInputQueryWizardPage;
  { 0 = non ancora controllato, 1 = nessun token salvato, 2 = token salvato gia'' valido (o non verificabile: si tiene),
    3 = token salvato rifiutato da GitHub }
  SavedState: Integer;

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

{ Esito HTTP di una GET su api.github.com con il token (0 = rete non raggiungibile) }
function GitHubStatus(Url, Token: String): Integer;
var
  Req: Variant;
begin
  Result := 0;
  try
    Req := CreateOleObject('WinHttp.WinHttpRequest.5.1');
    Req.Open('GET', Url, False);
    Req.SetRequestHeader('User-Agent', 'trascrittore-installer');
    Req.SetRequestHeader('Accept', 'application/vnd.github+json');
    Req.SetRequestHeader('Authorization', 'Bearer ' + Token);
    Req.SetTimeouts(5000, 5000, 10000, 10000);
    Req.Send('');
    Result := Req.Status;
  except
    Result := 0;
  end;
end;

{ '' = token utilizzabile, altrimenti il motivo }
function CheckToken(Token: String): String;
var
  Release, Repo: Integer;
begin
  Result := '';
  if (Pos(' ', Token) > 0) or (Pos(#9, Token) > 0) then
  begin
    Result := 'Il token non deve contenere spazi.';
    exit;
  end;
  Release := GitHubStatus('https://api.github.com/repos/aleDiPi/audio_trascriptor/releases/tags/latest', Token);
  if Release = 200 then exit;
  if Release = 0 then
  begin
    Result := 'Non riesco a collegarmi a GitHub (rete assente o bloccata): il token non e'' stato verificato.';
    exit;
  end;
  if Release = 401 then
  begin
    Result := 'GitHub non accetta il token (errato, revocato o scaduto).';
    exit;
  end;
  if (Release = 404) or (Release = 403) then
  begin
    Repo := GitHubStatus('https://api.github.com/repos/aleDiPi/audio_trascriptor', Token);
    if Repo = 200 then exit; { il token vede il repository: la release "latest" e'' in ricostruzione, va bene }
    if Repo = 401 then
      Result := 'GitHub non accetta il token (errato, revocato o scaduto).'
    else
      Result := 'Il token non vede il repository aleDiPi/audio_trascriptor. Controlla: Resource owner = aleDiPi, Repository access = solo audio_trascriptor, Permissions > Contents = Read-only.';
    exit;
  end;
  Result := 'GitHub ha risposto con errore HTTP ' + IntToStr(Release) + '.';
end;

{ Token gia'' scritto in un file .env (cartella dati per tutto il PC, poi la vecchia per-utente); '' = nessuno }
function ReadTokenFrom(EnvPath: String): String;
var
  Lines: TArrayOfString;
  I: Integer;
  Line: String;
begin
  Result := '';
  if not LoadStringsFromFile(EnvPath, Lines) then exit;
  for I := 0 to GetArrayLength(Lines) - 1 do
  begin
    Line := Trim(Lines[I]);
    if Pos('TRASCRITTORE_UPDATE_TOKEN=', Line) = 1 then
      Result := Trim(Copy(Line, Length('TRASCRITTORE_UPDATE_TOKEN=') + 1, Length(Line)));
  end;
end;

function FindSavedToken: String;
begin
  Result := ReadTokenFrom(ExpandConstant('{commonappdata}\Trascrittore\.env'));
  if Result = '' then
    Result := ReadTokenFrom(ExpandConstant('{userappdata}\Trascrittore\.env'));
end;

{ La pagina del token si salta se ce n''e'' gia'' uno e GitHub non lo rifiuta: va tenuto cosi'' com''e'' }
function SavedTokenState: Integer;
var
  Token, Reason: String;
begin
  if SavedState <> 0 then
  begin
    Result := SavedState;
    exit;
  end;
  Token := FindSavedToken;
  if Token = '' then
    SavedState := 1
  else
  begin
    Reason := CheckToken(Token);
    if (Pos('GitHub non accetta il token', Reason) = 1) or (Pos('Il token non vede il repository', Reason) = 1) then
      SavedState := 3
    else
      SavedState := 2; { valido, oppure rete assente o errore del server: nel dubbio non lo tocco }
  end;
  Result := SavedState;
end;

function ShouldSkipPage(PageID: Integer): Boolean;
begin
  Result := False;
  if PageID <> TokenPage.ID then exit;
  if SavedTokenState = 2 then
    Result := True
  else if SavedTokenState = 3 then
    TokenPage.SubCaptionLabel.Caption := 'Il token salvato non e'' piu'' accettato da GitHub: incollane uno nuovo (o lascia vuoto per tenerlo cosi'' com''e'').';
end;

function UpdateReadyMemo(Space, NewLine, MemoUserInfoInfo, MemoDirInfo, MemoTypeInfo, MemoComponentsInfo, MemoGroupInfo, MemoTasksInfo: String): String;
begin
  Result := MemoDirInfo;
  if SavedTokenState = 2 then
    Result := Result + NewLine + NewLine + 'Aggiornamenti automatici:' + NewLine + Space + 'token GitHub gia'' presente: viene mantenuto';
end;

function NextButtonClick(CurPageID: Integer): Boolean;
var
  Token, Reason: String;
begin
  Result := True;
  if CurPageID <> TokenPage.ID then exit;
  Token := Trim(TokenPage.Values[0]);
  if Token = '' then exit;
  WizardForm.NextButton.Enabled := False;
  try
    Reason := CheckToken(Token);
  finally
    WizardForm.NextButton.Enabled := True;
  end;
  if Reason = '' then exit;
  if MsgBox(Reason + #13#10#13#10 + 'Si = correggi il token' + #13#10 + 'No = continua senza token (aggiornamenti manuali)', mbError, MB_YESNO) = IDYES then
    Result := False
  else
    TokenPage.Values[0] := '';
end;

procedure SaveToken(Token: String);
var
  Path: String;
  Lines: TArrayOfString;
  I, N: Integer;
  Found: Boolean;
begin
  Path := ExpandConstant('{commonappdata}\Trascrittore\.env');
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
  NewDir := ExpandConstant('{commonappdata}\Trascrittore');
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
