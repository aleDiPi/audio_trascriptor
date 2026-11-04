# Privacy notice - Trascrittore Audio

Version 1.0 - Italian version: [PRIVACY.it.md](PRIVACY.it.md). The two versions have the same value; if they differ, the English one prevails. This notice is information, not legal advice.

## In short

- The program runs on your computer. The Author (aleDiPi) has no server, no user accounts and **receives none of your data**. There is **no telemetry and no analytics**.
- Your voice and your text leave your computer **only** for the services you configure yourself (Groq, OpenAI, Anthropic) or for the optional downloads and updates listed below.
- To keep everything on your PC, download a local Whisper model and leave all AI keys empty (or switch the cloud services off in the AI section).
- What those third-party services do with your data is decided by them. See [DISCLAIMER.en.txt](DISCLAIMER.en.txt).

## What the program processes

| Data | What happens | Where it goes |
|---|---|---|
| Voice from the default microphone | Recorded only while you hold the shortcut. Kept in RAM, **never written to disk**, released after processing. Capped by `max_seconds`. | To the transcription service you configured (Groq or OpenAI), or processed locally by Whisper. |
| Transcribed text | Sent with a text-correction prompt (the general prompt, the prompt of the matching app rule, and your custom dictionary words). | To the correction service you configured (Groq, OpenAI or Anthropic). If no key is configured (only local Whisper), the raw text is used and no AI is called. Local Whisper is by default only a fallback: while a cloud service has a key, it is tried first. |
| Dictated text, on paste | Placed temporarily in the system clipboard to paste it at the cursor, then the previous clipboard content is restored (if `restore_clipboard = true`). | Stays on your PC. Clipboard managers installed on your PC may keep a copy. |
| Name and window title of the foreground app | Read locally to choose the matching prompt. | Stays on your PC. |
| Settings (`config.toml`) | Local file. | Stays on your PC. |
| API keys (`.env`) | Local **plain text** file. Protect your PC accordingly. Never logged. | Sent only to the matching service, as authentication. |
| Log file | Errors and start-up events. With debug mode on, more details; the dictated text is logged **only** if you also enable `debug_log_text`. Two files of at most 512 KB. | Stays on your PC. Turning debug off, or uninstalling, deletes the logs. |
| Local Whisper models | Downloaded only if you ask for it. | Stored on your PC. |

## Network connections

1. **AI services you configure**: `api.groq.com`, `api.openai.com`, `api.anthropic.com` (HTTPS). Only the services that have a key, in the order you set.
2. **GitHub** (`api.github.com`): update check (every 6 hours, unless you turn automatic updates off) and download of the new version. No account or token is used. GitHub sees your IP address, under [GitHub's privacy statement](https://docs.github.com/en/site-policy/privacy-policies/github-general-privacy-statement).
3. **Hugging Face** (`huggingface.co`): download of a Whisper model, only when you press the download button.

No other connection is made. The program opens no listening ports.

## Roles under the GDPR (Regulation (EU) 2016/679)

- The Author does not receive or process your voice, texts or keys, so does not act as a controller or processor of the data you dictate.
- If you use the program **for purely personal or household purposes**, the GDPR generally does not apply to your own processing (art. 2(2)(c)).
- If you use it **in a professional or business context**, you (or your organization) are the controller of what you dictate. You must have a legal basis, inform the people concerned, assess transfers of data outside the EU (the providers are usually US companies), and put in place the contract (data processing agreement) and settings your provider offers. The Author cannot do this for you.
- The AI providers act under their own terms: depending on the provider and plan, as processors, as independent controllers, or both. Their retention periods and uses (for example model improvement) are theirs to define. Read: [Groq](https://groq.com/privacy-policy), [OpenAI](https://openai.com/policies/privacy-policy), [Anthropic](https://www.anthropic.com/legal/privacy).
- To exercise your rights (access, erasure, objection...) over data held by a provider, contact that provider. The Author holds no data about you; for questions about this notice open an issue on the repository.

## Retention and removal

The program keeps nothing about your dictations. Settings, keys, logs and models stay until you delete them; uninstalling removes them. Data already sent to a provider stays under that provider's retention rules.

## Children

The program is not directed at children and has no age checks. Their use of AI services is governed by those services' terms.

## Changes

If the program starts handling data differently, this notice will be updated in the repository, with the version number changed.
