# Informativa sulla privacy - Trascrittore Audio

Versione 1.0 - Versione inglese: [PRIVACY.en.md](PRIVACY.en.md). Le due versioni hanno lo stesso valore; in caso di differenze prevale quella inglese. Questa informativa e' un'informazione, non una consulenza legale.

## In breve

- Il programma gira sul tuo computer. L'Autore (aleDiPi) non ha server, non ha account utente e **non riceve nessun tuo dato**. Non c'e' **nessuna telemetria e nessuna analisi d'uso**.
- La tua voce e il tuo testo lasciano il computer **solo** verso i servizi che configuri tu (Groq, OpenAI, Anthropic) o per i download e gli aggiornamenti facoltativi elencati sotto.
- Per tenere tutto sul tuo PC, scarica un modello Whisper locale e lascia vuote tutte le chiavi AI (o escludi i servizi in rete dalla sezione AI).
- Cosa fanno quei servizi di terzi con i tuoi dati lo decidono loro. Vedi [DISCLAIMER.it.txt](DISCLAIMER.it.txt).

## Cosa tratta il programma

| Dato | Cosa succede | Dove va |
|---|---|---|
| Voce dal microfono predefinito | Registrata solo mentre tieni premuta la scorciatoia. Resta in RAM, **non viene mai scritta su disco**, liberata dopo l'elaborazione. Limitata da `max_seconds`. | Al servizio di trascrizione configurato (Groq od OpenAI), oppure elaborata in locale da Whisper. |
| Testo trascritto | Inviato con un prompt di correzione (quello generale, quello della regola dell'app corrispondente e le parole del tuo dizionario). | Al servizio di correzione configurato (Groq, OpenAI o Anthropic). Se non c'e' nessuna chiave (solo Whisper locale) si usa il testo grezzo e non si chiama nessuna AI. Whisper locale e' per impostazione predefinita solo una riserva: finche' un servizio in rete ha una chiave, viene provato prima. |
| Testo dettato, all'incolla | Messo temporaneamente negli appunti di sistema per incollarlo dove hai il cursore, poi viene ripristinato il contenuto precedente (se `restore_clipboard = true`). | Resta sul tuo PC. Eventuali gestori degli appunti installati sul PC possono conservarne una copia. |
| Nome e titolo della finestra in primo piano | Letti in locale per scegliere il prompt adatto. | Restano sul tuo PC. |
| Impostazioni (`config.toml`) | File locale. | Restano sul tuo PC. |
| Chiavi API ed eventuale token GitHub (`.env`) | File locale in **testo semplice**. Proteggi il PC di conseguenza. Mai scritte nei log. | Inviate solo al servizio corrispondente, per autenticarti. |
| File di log | Errori ed eventi di avvio. Con la modalita' debug piu' dettagli; il testo dettato e' scritto **solo** se attivi anche `debug_log_text`. Due file da 512 KB al massimo. | Restano sul tuo PC. Disattivando il debug, o disinstallando, i log vengono cancellati. |
| Modelli Whisper locali | Scaricati solo se lo chiedi. | Salvati sul tuo PC. |

## Connessioni di rete

1. **Servizi AI configurati da te**: `api.groq.com`, `api.openai.com`, `api.anthropic.com` (HTTPS). Solo quelli che hanno una chiave, nell'ordine che hai scelto.
2. **GitHub** (`api.github.com`): controllo e download degli aggiornamenti, solo se hai salvato un token di aggiornamento o chiedi un aggiornamento. GitHub vede il tuo indirizzo IP e il token, secondo l'[informativa di GitHub](https://docs.github.com/en/site-policy/privacy-policies/github-general-privacy-statement).
3. **Hugging Face** (`huggingface.co`): download di un modello Whisper, solo quando premi il pulsante di download.

Non vengono fatte altre connessioni. Il programma non apre porte in ascolto.

## Ruoli ai sensi del GDPR (Regolamento (UE) 2016/679)

- L'Autore non riceve ne' tratta la tua voce, i tuoi testi o le tue chiavi, quindi non agisce come titolare o responsabile del trattamento dei dati che detti.
- Se usi il programma **per fini esclusivamente personali o domestici**, il GDPR in genere non si applica ai tuoi trattamenti (art. 2, par. 2, lett. c).
- Se lo usi **in ambito professionale o aziendale**, tu (o la tua organizzazione) sei titolare di cio' che detti. Devi avere una base giuridica, informare gli interessati, valutare i trasferimenti di dati fuori dall'UE (i fornitori sono di solito societa' statunitensi) e predisporre il contratto (accordo sul trattamento dei dati) e le impostazioni che il fornitore mette a disposizione. L'Autore non puo' farlo al posto tuo.
- I fornitori AI agiscono secondo i propri termini: a seconda del fornitore e del piano, come responsabili, come titolari autonomi o entrambi. Tempi di conservazione e usi (per esempio il miglioramento dei modelli) li definiscono loro. Leggi: [Groq](https://groq.com/privacy-policy), [OpenAI](https://openai.com/policies/privacy-policy), [Anthropic](https://www.anthropic.com/legal/privacy).
- Per esercitare i tuoi diritti (accesso, cancellazione, opposizione...) sui dati in mano a un fornitore, rivolgiti a quel fornitore. L'Autore non conserva dati su di te; per domande su questa informativa apri una issue nel repository.

## Conservazione e rimozione

Il programma non conserva nulla delle tue dettature. Impostazioni, chiavi, log e modelli restano finche' non li cancelli; la disinstallazione li rimuove. I dati gia' inviati a un fornitore restano soggetti alle sue regole di conservazione.

## Minori

Il programma non e' rivolto ai minori e non verifica l'eta'. L'uso dei servizi AI da parte dei minori e' regolato dai termini di quei servizi.

## Modifiche

Se il programma iniziera' a trattare i dati in modo diverso, questa informativa sara' aggiornata nel repository, con il numero di versione modificato.
