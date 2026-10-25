# Trascrittore audio

Dettatura vocale per Windows, scritta in Rust (un solo `.exe`, nessun runtime da installare).

**Come funziona:** tieni premuta la scorciatoia (default `Ctrl+Win`), parla, rilascia. L'audio del microfono predefinito di Windows viene trascritto da Groq Whisper, rifinito da un LLM Groq con un prompt che dipende dall'app in primo piano (Outlook, Slack, VS Code, Word...) e il risultato viene incollato dove hai il cursore.

Parte ridotto a icona nella tray (nessuna finestra), usa pochissima RAM/CPU quando e' inattivo. Icona blu = pronto, rossa = registra, arancione = elabora, grigia = errore (passa il mouse per il dettaglio).

## Struttura della cartella

| Tipo | File | Su GitHub |
|---|---|---|
| Programma | `src/`, `Cargo.toml`, `config/config.example.toml`, `.github/workflows/` | si |
| Sicurezza | `.env.example`, `.gitignore`, `SECURITY.md` (e in locale `.env`, `config.toml`) | solo i modelli; `.env` e `config.toml` mai |
| Secondari | `target/`, `*.log`, `recordings/`, `local/`, `notes/`, `tmp/` | no (ignorati) |

## Installer (consigliato)

**Download diretto (senza clonare il repository):** [Trascrittore-Setup.exe](https://github.com/aleDiPi/audio_trascriptor/releases/latest/download/Trascrittore-Setup.exe) (il repository e' privato: serve essere collegati a GitHub con il proprio account). Ogni merge su `main` aggiorna la release "latest".

La GitHub Action produce `Trascrittore-Setup.exe` (artifact `trascrittore-setup`, nella pagina dell'esecuzione in *Actions*). L'installer:

- installa il programma per l'utente corrente (senza permessi di amministratore) e crea la voce nel menu Start;
- non chiede la chiave Groq (si inserisce dalle impostazioni, clic sull'icona nella tray); chiede invece, facoltativo, il token GitHub per gli aggiornamenti automatici (vedi sotto);
- crea `%APPDATA%\Trascrittore\config.toml` se manca (non lo sovrascrive negli aggiornamenti);
- offre l'avvio automatico con Windows;
- scegliendo il tipo **Programma + strumenti di sviluppo** installa anche Rust e Visual Studio Build Tools con lo stesso script di `scripts\setup-windows.ps1`. Servono solo per compilare dal sorgente, non per usare il programma.

Per generarlo a mano: `cargo build --release`, poi `ISCC.exe /DAppVersion=0.1.0 installer\trascrittore.iss` (richiede [Inno Setup 6](https://jrsoftware.org/isinfo.php)).

## Compilare ed eseguire su Windows dal sorgente

1. Installa Visual Studio Build Tools (strumenti C++) e Rust con un solo comando (PowerShell, usa winget e salta cio' che hai gia'; Windows chiedera' conferma UAC):
   ```powershell
   powershell -ExecutionPolicy Bypass -File scripts\setup-windows.ps1
   ```
   Poi chiudi e riapri il terminale.
2. Nella cartella del progetto:
   ```powershell
   git pull
   copy .env.example .env
   copy config\config.example.toml config.toml
   ```
3. Apri `.env` e scrivi la tua chiave: `GROQ_API_KEY=gsk_...` ([crea una chiave gratuita](https://console.groq.com/keys)).
4. `cargo run --release` (oppure `cargo build --release` e avvia `target\release\trascrittore.exe`).

In alternativa scarica l'exe gia' compilato dagli artifact della GitHub Action "Build Windows".

Se `config.toml` non e' nella cartella corrente o accanto all'exe, il programma usa `%APPDATA%\Trascrittore\` (creandolo con i valori predefiniti). Menu tray (tasto destro): **Avvia con Windows**, **Apri cartella configurazione**, **Ricarica configurazione**, **Esci**.

## Modalita' debug e log

Nei primi test attiva la **modalita' debug**: dal menu dell'icona nella tray (*Modalita' debug*), dalla finestra Impostazioni oppure con `debug = true` in `config.toml`. Il log (*Apri log* nel menu o nelle Impostazioni) si trova in `%APPDATA%\Trascrittore\trascrittore.log` (accanto a `config.toml`; la finestra Impostazioni ne mostra il percorso) e contiene data e ora UTC, e per ogni dettatura:

- hotkey premuta/rilasciata e app in primo piano (nome del programma e titolo della finestra);
- microfono usato, frequenza, durata registrata e livello del segnale (utile se il microfono predefinito di Windows e' sbagliato o muto);
- ogni chiamata a Groq: modello, esito HTTP, tempo in millisecondi, token usati; i modelli candidati e il passaggio a quelli di riserva;
- la regola per app scelta, quanti caratteri vengono incollati, con quale scorciatoia e se viene premuto Invio;
- i controlli di aggiornamento.

Senza debug il log contiene solo avvio, errori e avvisi. **Non** scrive mai chiavi API o token. Il testo dettato compare nel log solo se aggiungi anche `debug_log_text = true` (e' privato: attivalo solo per cercare un problema).

## Aggiornamenti automatici (facoltativi)

Il repository e' privato, quindi per aggiornarsi da solo il programma ha bisogno di un token GitHub di **sola lettura**:

1. GitHub, *Settings*, *Developer settings*, *Personal access tokens*, *Fine-grained tokens*, *Generate new token*.
2. *Repository access*: **Only select repositories**, solo `aleDiPi/audio_trascriptor`.
3. *Permissions*, *Repository permissions*: **Contents: Read-only** (nient'altro). Scegli una scadenza (es. 1 anno) e crea il token.
4. Incolla il token nell'installer (pagina "Aggiornamenti automatici") oppure nella finestra Impostazioni, *Verifica e salva*. Viene salvato solo sul tuo PC, in `%APPDATA%\Trascrittore\.env` (`TRASCRITTORE_UPDATE_TOKEN`); non e' mai dentro l'eseguibile ne' nel repository.

Come funziona: 20 secondi dopo l'avvio, e poi ogni 6 ore, il programma legge la release `latest`. Se il commit e' diverso dal suo, scarica **solo `trascrittore.exe`** (pochi MB), controlla lo SHA-256 dichiarato da GitHub, rinomina l'eseguibile in uso, mette il nuovo al suo posto e si riavvia. Se la verifica fallisce non cambia nulla; se lo scambio non e' possibile (cartella non scrivibile) usa l'installer completo in modalita' silenziosa. L'installer completo viene usato anche quando un aggiornamento e' marcato `[installer]` nel titolo della PR (modifiche strutturali). Durante una dettatura l'aggiornamento aspetta. Senza token gli aggiornamenti sono manuali: scarica `Trascrittore-Setup.exe` dalla pagina Release. Le build locali (`cargo build`) non si aggiornano mai.

## Impostazioni (clic sull'icona nella tray)

Un clic sinistro sull'icona (o *Impostazioni...* nel menu con il tasto destro) apre una finestra nativa che esiste solo finche' e' aperta. Mostra:

- la chiave API Groq: scrivila e premi **Verifica e salva**; il programma la prova su Groq e, se funziona, la salva in `.env` (si vedono solo le ultime 4 cifre);
- i modelli in uso: trascrizione `whisper-large-v3-turbo` e AI `openai/gpt-oss-120b`; in `config.toml` si possono cambiare;
- la versione installata e se c'e' un aggiornamento, con il campo per il token degli aggiornamenti;
- l'utilizzo di Groq: Groq **non** espone un credito residuo (il piano gratuito ha limiti di richieste e token). La finestra mostra i limiti e i valori rimasti letti dagli header di Groq e i contatori dall'avvio, e si aggiorna da sola. I valori compaiono dopo il primo utilizzo;
- i modelli di riserva: se Groq risponde "limite raggiunto" (HTTP 429) il programma passa in automatico al modello successivo (`fallback_models`, `stt_fallback_models`) e salta quello in pausa finche' il limite non si resetta;
- i prompt completi inviati all'AI per ogni caso (app non riconosciuta e ogni regola `[[apps]]`).

## Configurazione (`config.toml`)

- `hotkey`: scorciatoia, es. `"ctrl+win"`, `"ctrl+alt+space"`, `"f9"`.
- `mode`: `"hold"` (tieni premuto) o `"toggle"` (premi per iniziare/finire).
- `language`, `stt_model`, `llm_model`, `fallback_models`, `stt_fallback_models`, `max_seconds`, `restore_clipboard`.
- `default_prompt`: prompt di base sempre applicato (pulizia del testo dettato). Il `prompt` della regola dell'app viene aggiunto a questo.
- `dictionary`: parole/nomi da scrivere esattamente cosi'.
- `[[apps]]`: regole per app, con `exe` (nomi dei processi), `prompt`, `raw = true` per incollare la trascrizione senza AI, `send_enter = true` per premere Invio dopo l'incolla (es. Claude desktop, al posto dello script AutoHotkey `claude-auto-invio.ahk`), `paste = "shift+insert"` per i terminali che lo richiedono, `title_contains` per distinguere ad es. un sito nel browser.

Dopo ogni modifica usa **Ricarica configurazione** dalla tray.

## Note

- Il microfono e' quello predefinito di Windows nel momento in cui inizia la registrazione.
- Se l'AI non risponde, viene incollata la trascrizione grezza. Gli errori finiscono in `trascrittore.log` (senza testo dettato ne' chiavi).
- La parte specifica di Windows e' isolata in `src/platform.rs` e `src/hotkey.rs`: il resto e' portabile per un eventuale porting a macOS/Linux.
- Vedi `SECURITY.md` per la gestione dei segreti.


## Test

`cargo test` verifica il parsing della configurazione, l'interruttore `debug` in `config.toml`, il formato della data nel log, i prompt per app, il salvataggio della chiave in `.env`, la lettura della release di aggiornamento (URL ammessi, SHA-256, marcatore `[installer]`), il passaggio ai modelli di riserva, la scelta della regola per app (inclusi `send_enter` e `raw`) e il parsing della scorciatoia. La GitHub Action li esegue a ogni push e su ogni pull request, prima della build.

## Spazio occupato

- **Audio**: la registrazione sta solo in RAM e non viene mai scritta su disco. Il buffer e' limitato da `max_seconds` (120 s: circa 23 MB a 48 kHz durante la registrazione, qualche MB per una dettatura normale) e viene liberato appena finita l'elaborazione.
- **Log**: `trascrittore.log` (vedi sotto) e' limitato a due file da 512 KB al massimo (`.log` e `.log.1`).
- **File fissi**: `config.toml` e `.env` in `%APPDATA%\Trascrittore`, pochi KB. Il programma installato e' un solo `.exe` di pochi MB.
- **Aggiornamenti**: il nuovo `.exe` viene scaricato accanto al vecchio e il file `.old` viene cancellato al riavvio successivo; l'eventuale installer scaricato in `%TEMP%\Trascrittore-Update` viene cancellato all'avvio successivo.
- **Contatori di utilizzo**: sono in memoria (poche righe per modello) e si azzerano alla chiusura.
- A riposo il programma non fa nulla (icona nella tray e un controllo aggiornamenti ogni 6 ore, se hai il token); la finestra impostazioni esiste solo quando e' aperta.
