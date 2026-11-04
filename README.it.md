# Trascrittore audio

[English](README.md) · **Italiano**

Dettatura vocale per **Windows** e **Linux** (Linux Mint, Ubuntu), scritta in Rust (nessun runtime da installare).

**Come funziona:** tieni premuta la scorciatoia (default `Ctrl+Win`), parla, rilascia. L'audio del microfono predefinito di Windows viene trascritto da Groq Whisper, rifinito da un LLM Groq con un prompt che dipende dall'app in primo piano (Outlook, Slack, VS Code, Word...) e il risultato viene incollato dove hai il cursore.

(Su Linux: microfono predefinito del sistema, vedi la sezione *Linux* piu' sotto.)

**Lingue:** l'interfaccia e' disponibile in italiano e in inglese. La lingua segue quella del sistema; si cambia nell'installer (Windows) e in qualsiasi momento da *Impostazioni > Lingua* (e' anche il modo di cambiarla su Linux, dove non c'e' un installer). Chiunque puo' aggiungere una lingua con un solo file di testo: vedi [CONTRIBUTING.md](CONTRIBUTING.md) e la cartella `lang/`.

Parte ridotto a icona nella tray (nessuna finestra), usa pochissima RAM/CPU quando e' inattivo. Icona blu = pronto, rossa = registra, arancione = elabora, grigia = errore (passa il mouse per il dettaglio).

## Struttura della cartella

| Tipo | File | Su GitHub |
|---|---|---|
| Programma | `src/`, `ui/`, `Cargo.toml`, `config/config.example.toml`, `installer/` (Windows e Linux), `.github/workflows/` | si |
| Sicurezza | `.env.example`, `.gitignore`, `SECURITY.md` (e in locale `.env`, `config.toml`) | solo i modelli; `.env` e `config.toml` mai |
| Secondari | `target/`, `*.log`, `recordings/`, `local/`, `notes/`, `tmp/` | no (ignorati) |

## Installer (consigliato)

**Download diretto (senza clonare il repository):** [Trascrittore-Setup.exe](https://github.com/aleDiPi/audio_trascriptor/releases/latest/download/Trascrittore-Setup.exe) (il repository e' privato: serve essere collegati a GitHub con il proprio account). Ogni merge su `main` pubblica una nuova release con la sua versione (vedi *Versioni*), e il link porta sempre all'ultima.

La GitHub Action produce `Trascrittore-Setup.exe` (artifact `trascrittore-setup`, nella pagina dell'esecuzione in *Actions*). L'installer:

- parte nella **lingua di Windows** e nella prima finestra permette di sceglierne un'altra (italiano o inglese): tutta la procedura cambia subito lingua, e la scelta diventa la lingua del programma (si cambia poi dalle Impostazioni);
- installa il programma per tutto il PC in `C:\Program Files\Trascrittore` (chiede i permessi di amministratore; build a 64 bit, vedi sotto) e crea la voce nel menu Start. Configurazione, chiavi e log stanno in `C:\ProgramData\Trascrittore`, scrivibile dagli utenti del PC;
- non chiede la chiave Groq (si inserisce dalle impostazioni, clic sull'icona nella tray); chiede invece, facoltativo, il token GitHub per gli aggiornamenti automatici (la pagina contiene i passaggi per crearlo e il link diretto a GitHub). Se un token e' gia' salvato in `.env` (anche nella vecchia cartella per-utente) e GitHub lo accetta, o non si riesce a verificarlo, la pagina viene saltata e il token resta com'e'; la pagina compare solo se GitHub lo rifiuta. Per sostituirlo: Impostazioni > Info. Le altre chiavi (Groq, OpenAI, Anthropic) non vengono mai toccate dall'installer;
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

In alternativa scarica l'exe gia' compilato dagli artifact della GitHub Action "Build".

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

## Linux (Linux Mint 22, Ubuntu 24.04)

Il pacchetto `trascrittore.deb` e' nella stessa release di GitHub dell'installer Windows (stesso numero di versione). Il pacchetto e' compilato su Ubuntu 24.04, quindi serve **Linux Mint 22** o piu' recente (le versioni 21 e precedenti hanno una libreria di sistema troppo vecchia).

**Installazione:** scarica `trascrittore.deb` dalla pagina Release (il repository e' privato: serve essere collegati a GitHub) e installalo con doppio clic (Gestore software) oppure:

```bash
sudo apt install ./trascrittore.deb
```

apt aggiunge da solo le librerie che servono. Si disinstalla con `sudo apt remove trascrittore` (la cartella di configurazione dell'utente resta: `rm -r ~/.config/trascrittore` per cancellare anche quella).

**Cosa cambia rispetto a Windows**

- Serve una sessione grafica **X11** (e' quella predefinita di Linux Mint Cinnamon). Con Wayland la scorciatoia globale non funziona, e il programma lo scrive nel log.
- Scorciatoia: `Ctrl+Win` (tasto Super) come su Windows. Su Linux i tasti non vengono "mangiati": conviene una combinazione di soli modificatori oppure un tasto che non scrive nulla (`f9`, ...), altrimenti il tasto arriva anche all'applicazione. Se `Ctrl+Super` interferisce con un'altra scorciatoia di Cinnamon, cambia `hotkey` in `config.toml` e scegli *Ricarica configurazione* dal menu dell'icona.
- Il programma parte all'accesso, ridotto nell'area di notifica (voce in `/etc/xdg/autostart`). Si spegne dal menu dell'icona (*Avvia all'accesso*) o dalle Impostazioni; la scelta dell'utente sta in `~/.config/autostart/trascrittore.desktop`. Se lo avvii dal menu delle applicazioni mentre e' gia' in esecuzione, si apre la finestra delle Impostazioni.
- Configurazione, chiavi (`.env`, leggibile solo da te) e log stanno in `~/.config/trascrittore/`.
- Le regole per applicazione usano il nome del programma in primo piano (`thunderbird`, `code`, `firefox`...); il suffisso `.exe` e le maiuscole non contano, quindi `chrome.exe` vale anche `chrome`. Per i terminali (gnome-terminal, xfce4-terminal, konsole...) incolla con `Ctrl+Maiusc+V` (`paste = "ctrl+shift+v"`).
- Su Linux non c'e' il bollino a schermo durante la registrazione: restano i suoni e il colore dell'icona.
- Gli appunti sono gestiti da X11; i tasti vengono simulati con l'estensione XTest. Per i suoni serve `paplay`, `pw-play` o `aplay` (presenti in Mint); per la notifica di aggiornamento `notify-send` (`libnotify-bin`).
- **Aggiornamenti:** con il token GitHub (Impostazioni > Info, come su Windows) il programma controlla le nuove versioni ogni 6 ore e mostra una notifica; **Impostazioni > Info > Aggiorna ora** scarica il `.deb` della release, ne verifica lo SHA-256 e lo installa con `apt` (compare la richiesta della password, `pkexec`), poi si riavvia. Senza token si aggiorna a mano reinstallando il nuovo `.deb`.

**Compilare dal sorgente (Linux):**

```bash
sudo apt install build-essential cmake clang pkg-config libasound2-dev libgtk-3-dev libayatana-appindicator3-dev libxdo-dev libfontconfig1-dev libxkbcommon-dev
cargo build --release          # target/release/trascrittore
cargo install cargo-deb --locked && cargo deb --no-build   # target/debian/*.deb
```

I test (`cargo test`) girano senza server grafico; i due test che usano un vero server X11 (`paste_smoke`, `foreground_smoke`) sono `#[ignore]` e si lanciano a mano, per esempio con Xvfb.

## Versioni

Ogni versione ha un numero **AAAA.MM.GG.N**: la data della build (ora italiana) e un numero progressivo del giorno (2026.10.25.1, 2026.10.25.2, ...). La GitHub Action lo calcola a ogni merge su `main`, lo scrive nell'eseguibile e nell'installer e pubblica una release con il tag `vAAAA.MM.GG.N`; la piu' recente e' quella segnalata come *Latest*. La versione si legge in *Home* e in *Impostazioni*, sezione *Info* (una build locale mostra "locale"). Resta anche una release fissa `latest` che serve solo alle installazioni precedenti al sistema di versioni: dopo che tutti i PC si sono aggiornati si puo' cancellare.

## Architettura (x64, ARM64)

Il programma e l'installer sono compilati per **x64** (`x86_64-pc-windows-msvc`, il target predefinito di `windows-latest` nella GitHub Action) e si installano in modalita' 64 bit. Su Windows 11 per ARM64 l'installer parte e il programma gira tramite l'emulazione x64 di Windows, ma non c'e' una build ARM64 nativa e non e' stato provato. Windows a 32 bit non e' supportato.

## Aggiornamenti automatici (facoltativi)

Il repository e' privato, quindi per aggiornarsi da solo il programma ha bisogno di un token GitHub di **sola lettura**:

1. Apri direttamente <https://github.com/settings/personal-access-tokens/new> (*Settings*, *Developer settings*, *Personal access tokens*, *Fine-grained tokens*, *Generate new token*).
2. *Repository access*: **Only select repositories**, solo `aleDiPi/audio_trascriptor`.
3. *Permissions*, *Repository permissions*: **Contents: Read-only** (nient'altro). Scegli una scadenza (es. 1 anno) e crea il token.
4. Incolla il token nell'installer (pagina "Aggiornamenti automatici": lo verifica subito con GitHub e, se non funziona, ti dice perche' e ti lascia correggerlo o proseguire senza) oppure in Impostazioni > Info, *Verifica e salva*. Sempre da Info puoi cambiarlo, rimuoverlo (*Rimuovi token*) e attivare o disattivare gli aggiornamenti con la casella *Aggiornamenti automatici attivi* (`auto_update` in `config.toml`). Viene salvato solo sul tuo PC, in `.env` accanto al programma (`TRASCRITTORE_UPDATE_TOKEN`); non e' mai dentro l'eseguibile ne' nel repository.

Come funziona: 20 secondi dopo l'avvio, e poi ogni 6 ore, il programma legge l'ultima release. Se la sua versione e' piu' recente di quella installata (si confrontano i numeri: 2026.10.25.10 viene dopo 2026.10.25.9), scarica **solo `trascrittore.exe`** (pochi MB), controlla lo SHA-256 dichiarato da GitHub, rinomina l'eseguibile in uso, mette il nuovo al suo posto e si riavvia (succede solo se la cartella del programma e' scrivibile, per esempio in una build locale). Con l'installer in Program Files il programma non puo' sostituirsi da solo senza amministratore: scarica `Trascrittore-Setup.exe` e lo avvia in modalita' silenziosa, e Windows mostra **una richiesta di amministratore (UAC) a ogni aggiornamento**; se la rifiuti non cambia nulla e riprova al controllo successivo. L'installer completo viene usato anche quando un aggiornamento e' marcato `[installer]` nel titolo della PR (modifiche strutturali). Durante una dettatura l'aggiornamento aspetta. Sicurezza: l'exe in Program Files non e' modificabile senza amministratore, ma gli utenti del PC possono leggere e modificare `.env` in `C:\ProgramData\Trascrittore` (chiavi comprese): l'installazione e' pensata per un PC personale. Se il controllo fallisce, il messaggio dice la causa: token rifiutato (errato o scaduto), token che non vede il repository (controlla *Resource owner* aleDiPi, *Repository access* e *Contents: Read-only*; per GitHub un repository privato non visibile risponde 404), oppure nessuna release ancora pubblicata (dopo ogni merge ci vuole un paio di minuti). Dalle Impostazioni, sezione *Info*, i pulsanti **Verifica aggiornamenti** e **Aggiorna ora** controllano e installano subito, anche con gli aggiornamenti automatici spenti. Senza token gli aggiornamenti sono manuali: scarica `Trascrittore-Setup.exe` dalla pagina Release. Le build locali (`cargo build`) non si aggiornano mai.

## Impostazioni (clic sull'icona nella tray)

Un clic sinistro sull'icona (o *Impostazioni...* nel menu con il tasto destro) apre una finestra (interfaccia [Slint](https://slint.dev) con renderer software, nitida a qualsiasi scala di Windows) che esiste solo finche' e' aperta. Le sezioni sono nella barra a sinistra:

1. **Home**: riepilogo all'apertura: modello AI e di trascrizione che risponderanno ora, servizi in ordine di priorita' e se hanno la chiave, scorciatoia, versione e aggiornamenti, utilizzo dall'avvio.
2. **AI**: una tessera per servizio (Groq, OpenAI, Anthropic e, ultima, la trascrizione locale con Whisper). Ogni tessera mostra stato (pronto, in pausa e perche', nessuna chiave), chiave (**Verifica e salva**, **Rimuovi**, **Crea chiave**; si vedono solo le ultime 4 cifre; salvata in `.env`), modelli, contatori dall'avvio e il link alla pagina del credito (nessuno dei tre lo espone via API). **Trascina** le tessere con la maniglia a sinistra per cambiare la priorita' (`providers` in `config.toml`); la casella *Usa* esclude un servizio. Se un servizio finisce i crediti o rifiuta la chiave si passa al successivo; per Groq anche ai modelli di riserva (`fallback_models`, `stt_fallback_models`) con i limiti letti dagli header.
3. **Prompt**: i prompt completi inviati all'AI, scelti da un elenco per tipo di applicazione (app non riconosciuta e ogni regola `[[apps]]`, con il nome dato da `name`).
4. **Impostazioni**: icona con alone (`overlay`), **suoni** di inizio e fine registrazione (attivi/spenti, tipo *Morbido*, *Click* o *Campanello*, volume, pulsante *Prova*; `sounds`, `sound_style`, `sound_volume` in `config.toml`) e apertura della cartella di configurazione.
5. **Info**: versione, aggiornamenti automatici (**Verifica aggiornamenti**, **Aggiorna ora**), token GitHub (**Verifica e salva**, **Rimuovi token**, **Crea token**), modalita' debug, testo dettato nel log (`debug_log_text`), apertura del log, percorsi e descrizione del programma.

L'exe ha un manifest con DPI per monitor (`assets/app.manifest`): senza, Windows ingrandisce le finestre come immagini e il testo risulta sfocato. L'icona con alone resta non scalata dal sistema, come prima.

I testi sono selezionabili e copiabili.

## Trascrizione locale (Whisper)

Come **ultima risorsa**, quando nessun servizio in rete risponde (crediti finiti, chiave rifiutata, niente rete), l'audio viene trascritto sul tuo PC con Whisper ([whisper.cpp](https://github.com/ggml-org/whisper.cpp)). E' la quarta tessera della sezione **AI** (`local` in `providers`, ultima per impostazione predefinita, trascinabile e escludibile con *Usa*). Chi aveva gia' un `config.toml` la trova aggiunta in coda una sola volta (`local_migrated = true`); se la togli non viene rimessa.

- Il modello **non e' nell'installer**: si sceglie la dimensione (`local_model`: `tiny` ~75 MB, `base` ~142 MB consigliato, `small` ~466 MB, `medium` ~1,5 GB; modelli multilingua, adatti all'italiano) e si scarica col pulsante **Scarica** (con avanzamento e **Annulla**) da Hugging Face in `models\` accanto a `config.toml`. Il file scaricato e' controllato con il numero magico e, quando Hugging Face lo dichiara, con lo SHA-256. **Elimina** lo cancella; la disinstallazione rimuove tutta la cartella dati, modelli compresi.
- Il modello viene caricato solo durante la trascrizione e liberato subito dopo: a riposo non occupa RAM. Su CPU e' piu' lento del cloud (qualche secondo per una frase con `base`, di piu' con `small` e `medium`).
- Whisper locale **trascrive soltanto**: se anche l'AI di correzione e' fuori uso, il testo non viene corretto e viene incollata la trascrizione grezza. Se invece un servizio in rete funziona ancora per la correzione, il testo viene corretto come sempre.
- Nessun dato esce dal PC mentre usa il locale. Non serve credito ne' chiave.
- La build compila whisper.cpp (serve CMake e libclang; sul runner di GitHub ci sono gia', vedi `LIBCLANG_PATH` in `.github/workflows/build.yml`). L'exe cresce di qualche MB.

## Suoni

Un suono breve (meno di 0,25 s) segnala l'inizio e un altro la fine della registrazione. Sono toni sintetizzati in memoria (nessun file) e suonati da Windows in modo asincrono. Non finiscono nella registrazione: all'inizio il programma scarta i primi istanti dell'audio (durata del suono piu' un margine), alla fine il microfono e' gia' chiuso quando il suono parte. Quindi comincia a parlare dopo il suono. Si regolano in Impostazioni o con `sounds`, `sound_style` (`soft`, `click`, `chime`) e `sound_volume` (0-100) in `config.toml`.

## Configurazione (`config.toml`)

- `hotkey`: scorciatoia, es. `"ctrl+win"`, `"ctrl+alt+space"`, `"f9"`.
- `mode`: `"hold"` (tieni premuto) o `"toggle"` (premi per iniziare/finire).
- `language`: lingua del parlato che detti (codice ISO-639-1, per esempio `"it"`, `"en"`; vuoto = rilevamento automatico).
- `ui_language`: lingua dei menu e delle finestre: `"auto"` (lingua del sistema) oppure un codice come `"it"`, `"en"`. Si cambia anche dalle Impostazioni.
- `stt_model`, `llm_model`, `fallback_models`, `stt_fallback_models`, `max_seconds`, `restore_clipboard`.
- `default_prompt`: prompt di base sempre applicato (pulizia del testo dettato). Il `prompt` della regola dell'app viene aggiunto a questo.
- `dictionary`: parole/nomi da scrivere esattamente cosi'.
- `[[apps]]`: regole per app, con `exe` (nomi dei processi), `prompt`, `raw = true` per incollare la trascrizione senza AI, `send_enter = true` per premere Invio dopo l'incolla (es. Claude desktop, al posto dello script AutoHotkey `claude-auto-invio.ahk`), `paste = "shift+insert"` per i terminali che lo richiedono, `title_contains` per distinguere ad es. un sito nel browser.

Dopo ogni modifica usa **Ricarica configurazione** dalla tray.

## Lingue

I testi del programma stanno in `lang/<codice>.toml` (`en.toml`, `it.toml`). La lingua si sceglie cosi': la lingua dell'installer su Windows (scritta in `ui_language` la prima volta), oppure `ui_language` in `config.toml`, oppure *Impostazioni > Lingua*; con `"auto"` si usa la lingua del sistema (e l'inglese se non c'e' il file). I testi mancanti in una lingua ripiegano sull'inglese. Per aggiungere una lingua copia `lang/_template.toml` in `lang/<codice>.toml`, traducilo e apri una pull request: la build lo trova da sola. Per provarla senza compilare, metti il file in una cartella `lang` accanto a `config.toml` e riavvia il programma. La guida passo passo e' in [CONTRIBUTING.md](CONTRIBUTING.md).

I prompt dell'esempio di `config.toml` sono in inglese e dicono all'AI di rispondere nella lingua del testo dettato; chi aveva gia' un `config.toml` mantiene i propri.

## Note

- Il microfono e' quello predefinito di Windows nel momento in cui inizia la registrazione.
- Se l'AI non risponde, viene incollata la trascrizione grezza. Gli errori finiscono in `trascrittore.log` (senza testo dettato ne' chiavi).
- La parte specifica di Windows e' isolata in `src/platform.rs` e `src/hotkey.rs`: il resto e' portabile per un eventuale porting a macOS/Linux.
- Vedi `SECURITY.md` per la gestione dei segreti.


## Piu' servizi AI e crediti

`providers = ["groq", "openai", "anthropic"]` in `config.toml` (o dalla sezione *Altre AI*) e' l'ordine di priorita'. Per ogni dettatura il programma prova il primo servizio che ha una chiave; se risponde che i crediti sono finiti (HTTP 402, `insufficient_quota`, "credit balance is too low"), che la chiave e' rifiutata o che non e' raggiungibile, lo mette in pausa e passa al successivo (i crediti esauriti e le chiavi rifiutate vengono riprovati dopo un'ora). Groq prova prima i suoi modelli di riserva. Anthropic non trascrive l'audio, quindi per la trascrizione si usano Groq e OpenAI; per la correzione del testo tutti e tre. Modelli (leggeri di default: `gpt-4o-mini-transcribe` per l'audio, `gpt-4.1-nano` per la correzione, `claude-haiku-5-5` per Anthropic; si cambiano dalla sezione *Altre AI* o in `config.toml`): `openai_stt_model`, `openai_llm_model`, `anthropic_llm_model`. I modelli OpenAI "ragionanti" (gpt-5..., o3/o4) vengono chiamati senza temperatura, che non accettano; chiavi in `.env`: `GROQ_API_KEY`, `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`.

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

## Licenza e note legali / License and legal notes

Il codice e' **visibile ma non riutilizzabile** (source-available): puoi leggerlo e usare gratuitamente i programmi pubblicati nelle Release, non puoi copiarlo, ridistribuirlo o farne derivati. Contributi (per esempio nuove lingue) benvenuti, vedi [CONTRIBUTING.md](CONTRIBUTING.md). *The source is **readable but not reusable** (source-available): you may read it and use the released programs for free; copying, redistribution and derivative works are not allowed. Contributions such as new languages are welcome.*

| | Italiano | English |
|---|---|---|
| Licenza | [LICENSE.it.txt](legal/LICENSE.it.txt) | [LICENSE](LICENSE) |
| Informativa privacy | [PRIVACY.it.md](legal/PRIVACY.it.md) | [PRIVACY.en.md](legal/PRIVACY.en.md) |
| Avviso sui servizi AI di terzi | [DISCLAIMER.it.txt](legal/DISCLAIMER.it.txt) | [DISCLAIMER.en.txt](legal/DISCLAIMER.en.txt) |
| Componenti di terzi | [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) | |

**Importante:** Groq, OpenAI e Anthropic trattano i dati che invii secondo i loro termini; l'Autore non garantisce come li trattano. *Groq, OpenAI and Anthropic process what you send under their own terms; the Author does not guarantee how they handle it.* Questi testi non sono una consulenza legale. / *These texts are not legal advice.*

<a href="https://slint.dev"><img src="https://raw.githubusercontent.com/slint-ui/slint/master/logo/MadeWithSlint-logo-whitebg.png" alt="Made with Slint" height="48"></a>
