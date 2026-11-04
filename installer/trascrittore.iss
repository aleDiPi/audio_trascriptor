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
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "italian"; MessagesFile: "compiler:Languages\Italian.isl"

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
english.TokenPageTitle=Automatic updates (optional)
english.TokenPageSub=Read-only GitHub token
english.TokenPageIntro=With a token the program updates itself. Leave the field empty to update by hand by downloading the new installer. The token stays only on your PC (.env file), it is not inside the program.
english.TokenLabel=GitHub token:
english.TokenSteps=How to create the token (click the link above, you need GitHub access with the aleDiPi account):%n1. Token name: for example "Trascrittore audio".%n2. Expiration: choose an expiry (for example 1 year; when it expires you create another one from Settings).%n3. Resource owner: aleDiPi.%n4. Repository access: "Only select repositories" and choose only aleDiPi/audio_trascriptor.%n5. Permissions, Repository permissions: Contents = "Read-only". Do not add other permissions.%n6. Press "Generate token" and copy the code right away (it starts with github_pat_): GitHub shows it only once.%n7. Paste it in the field above. You can also do it later from the program Settings (click the icon in the tray).
english.TokenNoSpaces=The token must not contain spaces.
english.TokenNoNetwork=Cannot connect to GitHub (network missing or blocked): the token was not verified.
english.TokenRejected=GitHub does not accept the token (wrong, revoked or expired).
english.TokenNoRepo=The token does not see the repository aleDiPi/audio_trascriptor. Check: Resource owner = aleDiPi, Repository access = only audio_trascriptor, Permissions > Contents = Read-only.
english.TokenHttpError=GitHub answered with HTTP error %1.
english.TokenSavedRejected=The saved token is no longer accepted by GitHub: paste a new one (or leave it empty to keep it as it is).
english.TokenMemoHeader=Automatic updates:
english.TokenMemoKept=GitHub token already present: it is kept
english.TokenAskFix=%1%n%nYes = fix the token%nNo = continue without a token (manual updates)
italian.TypeCompact=Solo il programma (consigliato)
italian.TypeFull=Programma + strumenti di sviluppo (Rust e Visual Studio Build Tools)
italian.TypeCustom=Personalizzata
italian.CompDevtools=Rust + Visual Studio Build Tools (servono solo per compilare dal sorgente, download di diversi GB)
italian.TaskAutostart=Avvia Trascrittore con Windows (parte ridotto nella tray)
italian.ConfigFolderShortcut=Trascrittore - cartella configurazione
italian.StatusDevtools=Installo Rust e Visual Studio Build Tools (puo' richiedere diversi minuti)...
italian.LaunchApp=Avvia Trascrittore audio
italian.TokenPageTitle=Aggiornamenti automatici (facoltativo)
italian.TokenPageSub=Token GitHub di sola lettura
italian.TokenPageIntro=Con un token il programma si aggiorna da solo. Lascia il campo vuoto per aggiornare a mano scaricando il nuovo installer. Il token resta solo sul tuo PC (file .env), non e' dentro il programma.
italian.TokenLabel=Token GitHub:
italian.TokenSteps=Come creare il token (clic sul link qui sopra, serve l'accesso a GitHub con l'account aleDiPi):%n1. Token name: per esempio "Trascrittore audio".%n2. Expiration: scegli una scadenza (per esempio 1 anno; alla scadenza ne crei un altro dalle Impostazioni).%n3. Resource owner: aleDiPi.%n4. Repository access: "Only select repositories" e scegli solo aleDiPi/audio_trascriptor.%n5. Permissions, Repository permissions: Contents = "Read-only". Non aggiungere altri permessi.%n6. Premi "Generate token" e copia subito il codice (inizia con github_pat_): GitHub lo mostra una volta sola.%n7. Incollalo nel campo qui sopra. Puoi anche farlo dopo dalle Impostazioni del programma (clic sull'icona nella tray).
italian.TokenNoSpaces=Il token non deve contenere spazi.
italian.TokenNoNetwork=Non riesco a collegarmi a GitHub (rete assente o bloccata): il token non e' stato verificato.
italian.TokenRejected=GitHub non accetta il token (errato, revocato o scaduto).
italian.TokenNoRepo=Il token non vede il repository aleDiPi/audio_trascriptor. Controlla: Resource owner = aleDiPi, Repository access = solo audio_trascriptor, Permissions > Contents = Read-only.
italian.TokenHttpError=GitHub ha risposto con errore HTTP %1.
italian.TokenSavedRejected=Il token salvato non e' piu' accettato da GitHub: incollane uno nuovo (o lascia vuoto per tenerlo cosi' com'e').
italian.TokenMemoHeader=Aggiornamenti automatici:
italian.TokenMemoKept=token GitHub gia' presente: viene mantenuto
italian.TokenAskFix=%1%n%nSi' = correggi il token%nNo = continua senza token (aggiornamenti manuali)

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
  TokenPage := CreateInputQueryPage(wpSelectTasks, CustomMessage('TokenPageTitle'), CustomMessage('TokenPageSub'), CustomMessage('TokenPageIntro'));
  TokenPage.Add(CustomMessage('TokenLabel'), True);

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
  Steps.Text := CustomMessage('TokenSteps');
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

{ '' = usable token, otherwise the reason (translated). Rejected = GitHub refused the token or it cannot see the repository }
function CheckToken(Token: String; var Rejected: Boolean): String;
var
  Release, Repo: Integer;
begin
  Result := '';
  Rejected := False;
  if (Pos(' ', Token) > 0) or (Pos(#9, Token) > 0) then
  begin
    Result := CustomMessage('TokenNoSpaces');
    exit;
  end;
  Release := GitHubStatus('https://api.github.com/repos/aleDiPi/audio_trascriptor/releases/tags/latest', Token);
  if Release = 200 then exit;
  if Release = 0 then
  begin
    Result := CustomMessage('TokenNoNetwork');
    exit;
  end;
  if Release = 401 then
  begin
    Result := CustomMessage('TokenRejected');
    Rejected := True;
    exit;
  end;
  if (Release = 404) or (Release = 403) then
  begin
    Repo := GitHubStatus('https://api.github.com/repos/aleDiPi/audio_trascriptor', Token);
    if Repo = 200 then exit; { the token sees the repository: the "latest" release is being rebuilt, that is fine }
    if Repo = 401 then
      Result := CustomMessage('TokenRejected')
    else
      Result := CustomMessage('TokenNoRepo');
    Rejected := True;
    exit;
  end;
  Result := FmtMessage(CustomMessage('TokenHttpError'), [IntToStr(Release)]);
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
  Rejected: Boolean;
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
    Reason := CheckToken(Token, Rejected);
    if Rejected then
      SavedState := 3
    else
      SavedState := 2; { valid, or network missing or server error: when in doubt it is left alone }
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
    TokenPage.SubCaptionLabel.Caption := CustomMessage('TokenSavedRejected');
end;

function UpdateReadyMemo(Space, NewLine, MemoUserInfoInfo, MemoDirInfo, MemoTypeInfo, MemoComponentsInfo, MemoGroupInfo, MemoTasksInfo: String): String;
begin
  Result := MemoDirInfo;
  if SavedTokenState = 2 then
    Result := Result + NewLine + NewLine + CustomMessage('TokenMemoHeader') + NewLine + Space + CustomMessage('TokenMemoKept');
end;

function NextButtonClick(CurPageID: Integer): Boolean;
var
  Token, Reason: String;
  Rejected: Boolean;
begin
  Result := True;
  if CurPageID <> TokenPage.ID then exit;
  Token := Trim(TokenPage.Values[0]);
  if Token = '' then exit;
  WizardForm.NextButton.Enabled := False;
  try
    Reason := CheckToken(Token, Rejected);
  finally
    WizardForm.NextButton.Enabled := True;
  end;
  if Reason = '' then exit;
  if MsgBox(FmtMessage(CustomMessage('TokenAskFix'), [Reason]), mbError, MB_YESNO) = IDYES then
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

  { campo vuoto o installazione silenziosa (aggiornamento): il token gia' salvato non viene toccato }
  if (CurStep = ssPostInstall) and (Trim(TokenPage.Values[0]) <> '') then
    SaveToken(Trim(TokenPage.Values[0]));
end;
