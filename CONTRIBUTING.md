# Contributing / Contribuire

**English.** Contributions are welcome, especially **new languages**. The code is source-available (read-only), see [LICENSE](LICENSE): by sending a Contribution you accept section 4 of that license (you keep your copyright and grant the Author a broad license to use and relicense it).

1. Fork the repository on GitHub (allowed to prepare contributions), copy `lang/_template.toml` to `lang/<code>.toml` and translate it (see "Adding a language" below). Keep placeholders such as `{name}` unchanged.
2. Translate the legal texts only if you are able to do it faithfully: [legal/](legal/) holds the license translation, the privacy notice and the disclaimer. A translation of a legal text is a courtesy translation; the English text prevails.
3. Run `cargo test` and open a pull request describing the language you added. Please use your real name or a nickname you want credited.
4. Do not include code, images or text copied from other projects unless their license allows it.

**Italiano.** I contributi sono benvenuti, soprattutto **nuove lingue**. Il codice e' visibile ma non riutilizzabile (vedi [LICENSE](LICENSE)): inviando un Contributo accetti l'articolo 4 di quella licenza (conservi il tuo copyright e concedi all'Autore una licenza ampia per usarlo e rilicenziarlo).

1. Fai un fork su GitHub (consentito per preparare contributi), copia `lang/_template.toml` in `lang/<codice>.toml` e traducilo (vedi "Aggiungere una lingua" sotto). Lascia invariati i segnaposto come `{name}`.
2. Traduci i testi legali solo se puoi farlo con fedelta': in [legal/](legal/) ci sono licenza, informativa privacy e avviso. La traduzione di un testo legale e' di cortesia; prevale il testo inglese.
3. Esegui `cargo test` e apri una pull request indicando la lingua aggiunta. Usa il tuo nome reale o un soprannome con cui vuoi essere citato.
4. Non includere codice, immagini o testi copiati da altri progetti se la loro licenza non lo consente.

## Adding a language / Aggiungere una lingua

**English.** The program picks its language from the system, the installer (Windows) or *Settings > Language*. Every language is one text file in [`lang/`](lang/), named with its code (`fr.toml`, `pt_br.toml`...). New files are found automatically: there is no list to edit and no code to change.

1. Copy `lang/_template.toml` to `lang/<code>.toml`. The code is the ISO 639-1 language code, optionally with a region: `fr`, `de`, `pt_br`. (Files starting with `_` are templates and are ignored.)
2. Fill in `name` under `[meta]` with the name of the language **in that language** ("Français"). It is what the language list shows.
3. Translate the texts. Every key has the English text above it as a comment. Rules:
   - keep `{placeholders}` such as `{name}` or `{count}` exactly as they are (you may move them);
   - `\n` is a line break: keep it where the English text has it;
   - a text left as `""` falls back to English, so you can send a partial translation and finish it later;
   - `fmt.decimal_separator` is the decimal mark for sizes ("1.5 GB" or "1,5 GB").
4. Try it without building: put your file in a folder named `lang` next to `config.toml` (`%ProgramData%\Trascrittore\lang` on Windows, `~/.config/trascrittore/lang` on Linux), restart the program and choose it in *Settings > Language*. A file in that folder with the same code replaces the built-in one.
5. Check it: `cargo test` verifies that your file has the same keys as `en.toml` and the same placeholders in every text (a partial file with empty `""` values passes).
6. Windows installer (optional): to have your language in the installer wizard too, add a line to `[Languages]` of `installer/trascrittore.iss` (Inno Setup ships many `compiler:Languages\*.isl` files) and copy the `english.*` lines of `[CustomMessages]` with your own prefix; also map the name to your code in `ProgramLanguage`. `cargo test` checks that every message exists in every installer language. Keep that file plain ASCII (no accents): write `e'` or use the Inno `%` escapes.
7. Open a pull request as described above.

If you change or add a text in the program, add its key to `lang/en.toml` and `lang/it.toml` (and to `lang/_template.toml`): the tests fail if a text used by the code is missing or if the two languages differ.

**Italiano.** La lingua del programma segue il sistema, l'installer (Windows) o *Impostazioni > Lingua*. Ogni lingua e' un file di testo in [`lang/`](lang/) col suo codice (`fr.toml`, `pt_br.toml`...). I file nuovi vengono trovati da soli: nessun elenco da modificare, nessun codice da toccare.

1. Copia `lang/_template.toml` in `lang/<codice>.toml` (codice ISO 639-1, con regione se serve: `fr`, `de`, `pt_br`). I file che iniziano con `_` sono modelli e vengono ignorati.
2. Scrivi in `name`, sotto `[meta]`, il nome della lingua **nella lingua stessa** ("Français"): e' quello che compare nell'elenco.
3. Traduci i testi (sopra a ogni chiave c'e' il testo inglese in un commento). Lascia identici i `{segnaposto}` e metti `\n` dove c'e' un a capo. Un testo lasciato `""` ripiega sull'inglese, quindi si puo' mandare una traduzione parziale.
4. Per provarla senza compilare, metti il file in una cartella `lang` accanto a `config.toml` (`%ProgramData%\Trascrittore\lang` su Windows, `~/.config/trascrittore/lang` su Linux), riavvia e scegli la lingua in *Impostazioni > Lingua*.
5. `cargo test` controlla che il file abbia le stesse chiavi di `en.toml` e gli stessi segnaposto.
6. Installer Windows (facoltativo): aggiungi una riga a `[Languages]` di `installer/trascrittore.iss` e copia le righe `english.*` di `[CustomMessages]` col tuo prefisso; associa il nome al tuo codice in `ProgramLanguage`. Il file deve restare ASCII (niente accenti).
7. Apri una pull request.
