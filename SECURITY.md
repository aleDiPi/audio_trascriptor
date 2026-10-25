# Sicurezza

- La chiave Groq sta solo in `.env` (o nella variabile d'ambiente `GROQ_API_KEY`). `.env` e `config.toml` sono in `.gitignore`.
- Se una chiave finisce per sbaglio su GitHub: revocala subito su https://console.groq.com/keys e creane una nuova.
- Audio e testo vengono inviati solo a `api.groq.com` (HTTPS). Nessuna registrazione viene salvata su disco.
- Il programma non apre porte di rete e non invia telemetria.
- Il testo negli appunti viene sostituito solo per il tempo dell'incolla e poi ripristinato (se `restore_clipboard = true`).
- Per segnalare un problema di sicurezza apri una issue privata su GitHub.
