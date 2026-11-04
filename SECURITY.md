# Security

- The Groq key lives only in `.env` (or in the `GROQ_API_KEY` environment variable). `.env` and `config.toml` are in `.gitignore`.
- If a key ends up on GitHub by mistake: revoke it right away at https://console.groq.com/keys and create a new one.
- Audio and text are sent only to the AI services you configured (`api.groq.com`, `api.openai.com`, `api.anthropic.com`, over HTTPS). No recording is saved to disk.
- The program opens no network ports and sends no telemetry.
- The text on the clipboard is replaced only for the time of the paste and then restored (if `restore_clipboard = true`).
- To report a security problem open a private issue on GitHub.

---

# Sicurezza

- La chiave Groq sta solo in `.env` (o nella variabile d'ambiente `GROQ_API_KEY`). `.env` e `config.toml` sono in `.gitignore`.
- Se una chiave finisce per sbaglio su GitHub: revocala subito su https://console.groq.com/keys e creane una nuova.
- Audio e testo vengono inviati solo ai servizi AI che configuri (`api.groq.com`, `api.openai.com`, `api.anthropic.com`, in HTTPS). Nessuna registrazione viene salvata su disco.
- Il programma non apre porte di rete e non invia telemetria.
- Il testo negli appunti viene sostituito solo per il tempo dell'incolla e poi ripristinato (se `restore_clipboard = true`).
- Informativa privacy e avvisi legali: cartella `legal/`.
- Per segnalare un problema di sicurezza apri una issue privata su GitHub.
