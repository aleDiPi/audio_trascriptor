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

**Download diretto (senza clonare il repository):** [Trascrittore-Setup.exe](https://github.com/aleDiPi/audio_trascriptor/releases/latest/download/Trascrittore-Setup.exe) (il repository e' privato: serve essere collegati a GitHub con il proprio account). Ogni merge su `main` pubblica una nuova release con la sua versione (vedi *Versioni*), e il link porta sempre all'ultima.

La GitHub Action produce `Trascrittore-Setup.exe` (artifact `trascrittore-setup`, nella pagina dell'esecuzione in *Actions*). L'installer:

- installa il programma per tutto il PC in `C:\Program Files\Trascrittore` (chiede i permessi di amministratore; build a 64 bit, vedi sotto) e crea la voce nel menu Start. Configurazione, chiavi e log stanno in `C:\ProgramData\Trascrittore`, scrivibile dagli utenti del PC;
- non chiede la chiave Groq (si inserisce dalle impostazioni, clic sull'icona nella tray); chiede invece, facoltativo, il token GitHub per gli aggiornamenti automatici (la pagina contiene i passaggi per crearlo e il link diretto a GitHub);
- crea `config.toml` e `.env` nella stessa cartella se mancano (non li sovrascrive negli aggiornamenti); se arrivi dalla vecchia installazione per-utente, li copia da `%APPDATA%\Trascrittore`; se il programma era in `C:\ProgramData\Trascrittore` lo sposta in Program Files;
- offre l'avvio automatico con Windows;
- **la disinstallazione cancella tutto**: programma, `C:\ProgramData\Trascrittore` (config, chiavi in `.env`, log), le vecchie cartelle `%APPDATA%\Trascrittore` e `%LOCALAPPDATA%\Programs\Trascrittore`, i download temporanei degli aggiornamenti e la voce di avvio automatico dell'utente che disinstalla (non si tocca nulla negli altri profili utente);
- scegliendo il tipo **Programma + strumenti di sviluppo** installa anche Rust e Visual Studio Build Tools con lo stesso script di `scripts\setup-windows.ps1`. Servono solo per compilare dal sorgente, non per usare il programma.

Per generarlo a mano: `cargo build --release`, poi `ISCC.exe /DAppVersion=2026.10.25.1 installer\trascrittore.iss` (richiede [Inno Setup 6](https://jrsoftware.org/isinfo.php)).

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

Il programma cerca `config.toml` nella cartella corrente, accanto all'exe, in `%ProgramData%\Trascrittore\` e infine in `%APPDATA%\Trascrittore\`; se non lo trova usa quest'ultima (creandolo con i valori predefiniti). Menu tray (tasto destro): **Avvia con Windows**, **Apri cartella configurazione**, **Ricarica configurazione**, **Esci**.

## Modalita' debug e log

Quando disattivi la modalita' debug, i file di log gia' presenti (`trascrittore.log` e `trascrittore.log.1`) vengono cancellati.

Nei primi test attiva la **modalita' debug**: dal menu dell'icona nella tray (*Modalita' debug*), dalla finestra Impostazioni oppure con `debug = true` in `config.toml`. Il log (*Apri log* nel menu o nelle Impostazioni) si trova accanto a `config.toml` (con l'installer `C:\ProgramData\Trascrittore\trascrittore.log`; la finestra Impostazioni ne mostra il percorso) e contiene data e ora UTC, e per ogni dettatura:

- hotkey premuta/rilasciata e app in primo piano (nome del programma e titolo della finestra);
- microfono usato, frequenza, durata registrata e livello del segnale (utile se il microfono predefinito di Windows e' sbagliato o muto);
- ogni chiamata a Groq: modello, esito HTTP, tempo in millisecondi, token usati; i modelli candidati e il passaggio a quelli di riserva;
- la regola per app scelta, quanti caratteri vengono incollati, con quale scorciatoia e se viene premuto Invio;
- i controlli di aggiornamento.

Senza debug il log contiene solo avvio, errori e avvisi. **Non** scrive mai chiavi API o token. Il testo dettato compare nel log solo se aggiungi anche `debug_log_text = true` (e' privato: attivalo solo per cercare un problema).

## Versioni

Ogni versione ha un numero **AAAA.MM.GG.N**: la data della build (ora italiana) e un numero progressivo del giorno (2026.10.25.1, 2026.10.25.2, ...). La GitHub Action lo calcola a ogni merge su `main`, lo scrive nell'eseguibile e nell'installer e pubblica una release con il tag `vAAAA.MM.GG.N`; la piu' recente e' quella segnalata come *Latest*. La versione si legge in *Impostazioni*, sezione *Info* (una build locale mostra "locale"). Resta anche una release fissa `latest` che serve solo alle installazioni precedenti al sistema di versioni: dopo che tutti i PC si sono aggiornati si puo' cancellare.

## Architettura (x64, ARM64)

Il programma e l'installer sono compilati per **x64** (`x86_64-pc-windows-msvc`, il target predefinito di `windows-latest` nella GitHub Action) e si installano in modalita' 64 bit. Su Windows 11 per ARM64 l'installer parte e il programma gira tramite l'emulazione x64 di Windows, ma non c'e' una build ARM64 nativa e non e' stato provato. Windows a 32 bit non e' supportato.

## Aggiornamenti automatici (facoltativi)

Il repository e' privato, quindi per aggiornarsi da solo il programma ha bisogno di un token GitHub di **sola lettura**:

1. Apri direttamente <https://github.com/settings/personal-access-tokens/new> (*Settings*, *Developer settings*, *Personal access tokens*, *Fine-grained tokens*, *Generate new token*).
2. *Repository access*: **Only select repositories**, solo `aleDiPi/audio_trascriptor`.
3. *Permissions*, *Repository permissions*: **Contents: Read-only** (nient'altro). Scegli una scadenza (es. 1 anno) e crea il token.
4. Incolla il token nell'installer (pagina "Aggiornamenti automatici": lo verifica subito con GitHub e, se non funziona, ti dice perche' e ti lascia correggerlo o proseguire senza) oppure nella finestra Impostazioni, *Verifica e salva*. Sempre dalle Impostazioni puoi cambiarlo, rimuoverlo (*Rimuovi token*) e attivare o disattivare gli aggiornamenti con la casella *Aggiornamenti automatici attivi* (`auto_update` in `config.toml`). Viene salvato solo sul tuo PC, in `.env` accanto al programma (`TRASCRITTORE_UPDATE_TOKEN`); non e' mai dentro l'eseguibile ne' nel repository.

Come funziona: 20 secondi dopo l'avvio, e poi ogni 6 ore, il programma legge l'ultima release. Se la sua versione e' piu' recente di quella installata (si confrontano i numeri: 2026.10.25.10 viene dopo 2026.10.25.9), scarica **solo `trascrittore.exe`** (pochi MB), controlla lo SHA-256 dichiarato da GitHub, rinomina l'eseguibile in uso, mette il nuovo al suo posto e si riavvia (succede solo se la cartella del programma e' scrivibile, per esempio in una build locale). Con l'installer in Program Files il programma non puo' sostituirsi da solo senza amministratore: scarica `Trascrittore-Setup.exe` e lo avvia in modalita' silenziosa, e Windows mostra **una richiesta di amministratore (UAC) a ogni aggiornamento**; se la rifiuti non cambia nulla e riprova al controllo successivo. L'installer completo viene usato anche quando un aggiornamento e' marcato `[installer]` nel titolo della PR (modifiche strutturali). Durante una dettatura l'aggiornamento aspetta. Sicurezza: l'exe in Program Files non e' modificabile senza amministratore, ma gli utenti del PC possono leggere e modificare `.env` in `C:\ProgramData\Trascrittore` (chiavi comprese): l'installazione e' pensata per un PC personale. Se il controllo fallisce, il messaggio dice la causa: token rifiutato (errato o scaduto), token che non vede il repository (controlla *Resource owner* aleDiPi, *Repository access* e *Contents: Read-only*; per GitHub un repository privato non visibile risponde 404), oppure nessuna release ancora pubblicata (dopo ogni merge ci vuole un paio di minuti). Dalle Impostazioni, sezione *Impostazioni*, i pulsanti **Verifica aggiornamenti** e **Aggiorna ora** controllano e installano subito, anche con gli aggiornamenti automatici spenti. Senza token gli aggiornamenti sono manuali: scarica `Trascrittore-Setup.exe` dalla pagina Release. Le build locali (`cargo build`) non si aggiornano mai.

## Impostazioni (clic sull'icona nella tray)

Un clic sinistro sull'icona (o *Impostazioni...* nel menu con il tasto destro) apre una finestra nativa che esiste solo finche' e' aperta. E' divisa in sei sezioni, scelte dai pulsanti in alto:

1. **API**: la chiave Groq (**Verifica e salva**: la prova su Groq e, se funziona, la salva in `.env`; si vedono solo le ultime 4 cifre; **Crea chiave Groq** apre la pagina), i modelli in uso, l'utilizzo di Groq e i modelli di riserva. Groq **non** espone un credito residuo (il piano gratuito ha limiti di richieste e token): la finestra mostra i limiti e i valori rimasti letti dagli header e i contatori dall'avvio. Se Groq risponde "limite raggiunto" (HTTP 429) il programma passa al modello successivo (`fallback_models`, `stt_fallback_models`) e salta quello in pausa. Altri servizi di riserva: sezione **Altre AI**.
2. **Altre AI**: ordine di priorita' dei servizi (`providers`), chiavi OpenAI e Anthropic (**Verifica e salva**, salvate in `.env`) e uno stato per servizio.
3. **GitHub**: il token degli aggiornamenti (inserimento, **Verifica e salva**, **Rimuovi token**, **Crea token GitHub**).
4. **Impostazioni**: aggiornamenti automatici, icona con alone (`overlay`), modalita' debug, testo dettato nel log (`debug_log_text`), apertura di log e cartella di configurazione.
5. **Prompt**: i prompt completi inviati all'AI, scelti da un elenco per tipo di applicazione (app non riconosciuta e ogni regola `[[apps]]`, con il nome dato da `name`).
6. **Info**: versione e stato degli aggiornamenti, percorsi, scorciatoia, modelli e descrizione del programma.

I testi sono selezionabili e copiabili.

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


## Piu' servizi AI e crediti

`providers = ["groq", "openai", "anthropic"]` in `config.toml` (o dalla sezione *Altre AI*) e' l'ordine di priorita'. Per ogni dettatura il programma prova il primo servizio che ha una chiave; se risponde che i crediti sono finiti (HTTP 402, `insufficient_quota`, "credit balance is too low"), che la chiave e' rifiutata o che non e' raggiungibile, lo mette in pausa e passa al successivo (i crediti esauriti e le chiavi rifiutate vengono riprovati dopo un'ora). Groq prova prima i suoi modelli di riserva. Anthropic non trascrive l'audio, quindi per la trascrizione si usano Groq e OpenAI; per la correzione del testo tutti e tre. Modelli: `openai_stt_model`, `openai_llm_model`, `anthropic_llm_model`; chiavi in `.env`: `GROQ_API_KEY`, `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`.

**Credito residuo:** nessuno dei tre servizi lo fornisce tramite una normale chiave API, quindi il programma non puo' leggerlo. La sezione *Altre AI* mostra per ognuno la chiave, se e' pronto o in pausa per crediti esauriti, le chiamate e i token dall'avvio e il link alla pagina dove il credito si legge.

## Icona del programma

`assets/icon.ico` (16-256 px: microfono bianco con barre di voce su fondo a gradiente indaco-viola-rosa) e' incorporata nell'exe tramite `build.rs` e `assets/app.rc`: la usano Esplora risorse, il menu Start, l'installer, la voce di disinstallazione e la finestra Impostazioni. L'icona nella tray ha lo stesso microfono con il fondo che cambia con lo stato (blu pronto, rosso registra, ambra elabora, grigio errore). Per rigenerarla: `python assets/make_icon.py` (serve Pillow).

## Icona con alone

Mentre tieni premuta la scorciatoia compare in basso a destra (sullo schermo della finestra attiva) un cerchio scuro con l'icona del programma in cui verra' incollato il testo, circondato da un alone rosso che si allarga con il volume del microfono e si restringe quando taci. Durante la trascrizione l'alone diventa ambra e "respira"; poi sparisce. La finestra non prende il focus, non intercetta i clic ed e' nascosta (nessun timer) quando non serve. Si disattiva dalla sezione Impostazioni o con `overlay = false` in `config.toml`.

## Test

`cargo test` verifica il parsing della configurazione, l'interruttore `debug` in `config.toml`, il formato della data nel log, i prompt per app, il salvataggio della chiave in `.env`, la lettura della release di aggiornamento (URL ammessi, SHA-256, marcatore `[installer]`), il passaggio ai modelli di riserva, la scelta della regola per app (inclusi `send_enter` e `raw`) il parsing della scorciatoia e la matematica dell'alone (livello in dB, raggio, lisciatura). Il disegno della finestra non e' testabile in automatico. La GitHub Action li esegue a ogni push e su ogni pull request, prima della build.

## Spazio occupato

- **Audio**: la registrazione sta solo in RAM e non viene mai scritta su disco. Il buffer e' limitato da `max_seconds` (120 s: circa 23 MB a 48 kHz durante la registrazione, qualche MB per una dettatura normale) e viene liberato appena finita l'elaborazione.
- **Log**: `trascrittore.log` (vedi sotto) e' limitato a due file da 512 KB al massimo (`.log` e `.log.1`).
- **File fissi**: `config.toml` e `.env` nella cartella del programma (`C:\ProgramData\Trascrittore` con l'installer), pochi KB. Il programma installato e' un solo `.exe` di pochi MB.
- **Aggiornamenti**: il nuovo `.exe` viene scaricato accanto al vecchio e il file `.old` viene cancellato al riavvio successivo; l'eventuale installer scaricato in `%TEMP%\Trascrittore-Update` viene cancellato all'avvio successivo.
- **Contatori di utilizzo**: sono in memoria (poche righe per modello) e si azzerano alla chiusura.
- A riposo il programma non fa nulla (icona nella tray e un controllo aggiornamenti ogni 6 ore, se hai il token); la finestra impostazioni esiste solo quando e' aperta.
