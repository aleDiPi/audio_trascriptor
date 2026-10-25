# Trascrittore audio

Dettatura vocale per Windows in stile OpenWhispr, scritta in Rust (un solo `.exe`, nessun runtime da installare).

**Come funziona:** tieni premuta la scorciatoia (default `Ctrl+Win`), parla, rilascia. L'audio del microfono predefinito di Windows viene trascritto da Groq Whisper, rifinito da un LLM Groq con un prompt che dipende dall'app in primo piano (Outlook, Slack, VS Code, Word...) e il risultato viene incollato dove hai il cursore.

Parte ridotto a icona nella tray (nessuna finestra), usa pochissima RAM/CPU quando e' inattivo. Icona blu = pronto, rossa = registra, arancione = elabora, grigia = errore (passa il mouse per il dettaglio).

## Struttura della cartella

| Tipo | File | Su GitHub |
|---|---|---|
| Programma | `src/`, `Cargo.toml`, `config/config.example.toml`, `.github/workflows/` | si |
| Sicurezza | `.env.example`, `.gitignore`, `SECURITY.md` (e in locale `.env`, `config.toml`) | solo i modelli; `.env` e `config.toml` mai |
| Secondari | `target/`, `*.log`, `recordings/`, `local/`, `notes/`, `tmp/` | no (ignorati) |

## Compilare ed eseguire su Windows

1. Installa [Rust](https://rustup.rs) (con gli strumenti C++ di Visual Studio Build Tools, richiesti da rustup).
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

## Configurazione (`config.toml`)

- `hotkey`: scorciatoia, es. `"ctrl+win"`, `"ctrl+alt+space"`, `"f9"`.
- `mode`: `"hold"` (tieni premuto) o `"toggle"` (premi per iniziare/finire).
- `language`, `stt_model`, `llm_model`, `max_seconds`, `restore_clipboard`.
- `default_prompt`: prompt di base sempre applicato (prompt di pulizia di [OpenWhispr](https://github.com/OpenWhispr/openwhispr), licenza MIT). Il `prompt` della regola dell'app viene aggiunto a questo.
- `dictionary`: parole/nomi da scrivere esattamente cosi'.
- `[[apps]]`: regole per app, con `exe` (nomi dei processi), `prompt`, `raw = true` per incollare la trascrizione senza AI, `paste = "shift+insert"` per i terminali che lo richiedono, `title_contains` per distinguere ad es. un sito nel browser.

Dopo ogni modifica usa **Ricarica configurazione** dalla tray.

## Note

- Il microfono e' quello predefinito di Windows nel momento in cui inizia la registrazione.
- Se l'AI non risponde, viene incollata la trascrizione grezza. Gli errori finiscono in `trascrittore.log` (senza testo dettato ne' chiavi).
- La parte specifica di Windows e' isolata in `src/platform.rs` e `src/hotkey.rs`: il resto e' portabile per un eventuale porting a macOS/Linux.
- Vedi `SECURITY.md` per la gestione dei segreti.

## Crediti

Il prompt di pulizia del testo deriva da [OpenWhispr](https://github.com/OpenWhispr/openwhispr) (MIT, © OpenWhispr Team).
