# Trascrittore audio

**English** · [Italiano](README.it.md)

Voice dictation for **Windows** and **Linux** (Linux Mint, Ubuntu), written in Rust (no runtime to install).

**How it works:** hold the hotkey (default `Ctrl+Win`), speak, release. The audio from the system default microphone is transcribed by Groq Whisper, polished by a Groq LLM with a prompt that depends on the foreground app (Outlook, Slack, VS Code, Word...), and the result is pasted where your cursor is.

It starts minimized in the tray (no window) and uses very little RAM/CPU when idle. Blue icon = ready, red = recording, orange = processing, grey = error (hover for details).

**Languages:** the interface is available in English and Italian. The language follows the system language; you can change it in the installer (Windows) and at any time in *Settings > Language* (this is also how you change it on Linux, where there is no installer). Anyone can add a language by adding one text file: see [CONTRIBUTING.md](CONTRIBUTING.md) and the `lang/` folder.

**Export / import settings:** *Settings > Export / import settings*. *Export* lets you choose what to save (preferences, prompts, AI service keys) and an optional password; with a password the file is encrypted (Argon2id key derivation + XChaCha20-Poly1305), without it the file is plain text and contains your keys in clear: keep it private. *Import* takes the file (Browse, or drag it onto the window) and asks for the password if the file is encrypted; the previous `config.toml` is kept as `config.toml.bak`. The GitHub update token is never exported. On Linux the Browse button needs `zenity` or `kdialog`; otherwise type the path.

## Folder layout

| Kind | Files | On GitHub |
|---|---|---|
| Program | `src/`, `ui/`, `lang/`, `Cargo.toml`, `config/config.example.toml`, `installer/` (Windows and Linux), `.github/workflows/` | yes |
| Security | `.env.example`, `.gitignore`, `SECURITY.md` (and locally `.env`, `config.toml`) | only the templates; `.env` and `config.toml` never |
| Secondary | `target/`, `*.log`, `recordings/`, `local/`, `notes/`, `tmp/` | no (ignored) |

## Installer (recommended)

**Direct download (no need to clone the repository):** [Trascrittore-Setup.exe](https://github.com/aleDiPi/audio_trascriptor/releases/latest/download/Trascrittore-Setup.exe) (if the repository is private you must be signed in to GitHub with an account that has access). Every merge to `main` publishes a new release with its own version (see *Versions*), and the link always points to the latest.

The GitHub Action produces `Trascrittore-Setup.exe` (artifact `trascrittore-setup`, on the run page in *Actions*). The installer:

- starts in the **language of Windows** and lets you pick another language in its first dialog; the whole wizard switches at once, and the choice becomes the language of the program (changeable later in Settings);
- installs the program for the whole PC in `C:\Program Files\Trascrittore` (asks for administrator rights; 64-bit build, see below) and creates the Start menu entry. Configuration, keys and logs live in `C:\ProgramData\Trascrittore`, writable by the users of the PC;
- does not ask for the Groq key (enter it in Settings, click the tray icon) and asks for nothing else: automatic updates need no token. The keys already saved in `.env` (Groq, OpenAI, Anthropic) are never touched by the installer;
- creates `config.toml` and `.env` in the same folder if missing (it does not overwrite them on updates); if you come from the old per-user installation, it copies them from `%APPDATA%\Trascrittore`; if the program was in `C:\ProgramData\Trascrittore` it moves it to Program Files;
- offers to start with Windows;
- **uninstalling deletes everything**: the program, `C:\ProgramData\Trascrittore` (config, keys in `.env`, logs), the old `%APPDATA%\Trascrittore` and `%LOCALAPPDATA%\Programs\Trascrittore` folders, the temporary update downloads and the autostart entry of the user who uninstalls (nothing is touched in other user profiles);
- choosing the **Program + development tools** type also installs Rust and Visual Studio Build Tools with the same script as `scripts\setup-windows.ps1`. They are only needed to build from source, not to use the program.

To build it by hand: `cargo build --release`, then `ISCC.exe /DAppVersion=2026.10.25.1 installer\trascrittore.iss` (requires [Inno Setup 6](https://jrsoftware.org/isinfo.php)).

## Build and run on Windows from source

1. Install Visual Studio Build Tools (C++ tools) and Rust with one command (PowerShell, uses winget and skips what you already have; Windows will ask for UAC confirmation):
   ```powershell
   powershell -ExecutionPolicy Bypass -File scripts\setup-windows.ps1
   ```
   Then close and reopen the terminal.
2. In the project folder:
   ```powershell
   git pull
   copy .env.example .env
   copy config\config.example.toml config.toml
   ```
3. Open `.env` and write your key: `GROQ_API_KEY=gsk_...` ([create a free key](https://console.groq.com/keys)).
4. `cargo run --release` (or `cargo build --release` and start `target\release\trascrittore.exe`).

Alternatively download the already built exe from the artifacts of the "Build" GitHub Action.

The program looks for `config.toml` in the current folder, next to the exe, in `%ProgramData%\Trascrittore\` and finally in `%APPDATA%\Trascrittore\`; if it finds none it uses the latter (creating it with the default values). Tray menu (right click): **Start with Windows**, **Open configuration folder**, **Reload configuration**, **Quit**.

## Debug mode and logs

When you turn debug mode off, the existing log files (`trascrittore.log` and `trascrittore.log.1`) are deleted.

In your first tests turn **debug mode** on: from the tray icon menu (*Debug mode*), from the Settings window, or with `debug = true` in `config.toml`. The log (*Open log* in the menu or in Settings) is next to `config.toml` (with the installer `C:\ProgramData\Trascrittore\trascrittore.log`; the Settings window shows its path) and contains the UTC date and time and, for every dictation:

- hotkey pressed/released and foreground app (program name and window title);
- microphone used, sample rate, recorded duration and signal level (useful if the Windows default microphone is wrong or muted);
- every call to Groq: model, HTTP result, time in milliseconds, tokens used; the candidate models and the switch to the fallback ones;
- the per-app rule chosen, how many characters are pasted, with which shortcut and whether Enter is pressed;
- update checks.

Without debug the log contains only startup, errors and warnings. It **never** writes API keys or tokens. The dictated text appears in the log only if you also add `debug_log_text = true` (it is private: turn it on only to look for a problem).

## Linux (Linux Mint 22, Ubuntu 24.04)

The `trascrittore.deb` package is in the same GitHub release as the Windows installer (same version number). The package is built on Ubuntu 24.04, so you need **Linux Mint 22** or newer (version 21 and older have a system library that is too old).

**Installation:** download `trascrittore.deb` from the Releases page (if the repository is private you must be signed in to GitHub) and install it with a double click (Software Manager) or:

```bash
sudo apt install ./trascrittore.deb
```

apt adds the libraries that are needed. Uninstall with `sudo apt remove trascrittore` (the user's configuration folder stays: `rm -r ~/.config/trascrittore` to delete that too).

**What is different from Windows**

- There is no installer, so the language is set inside the program: *Settings > Language* (the default follows the system language, read from `LC_ALL`, `LC_MESSAGES`, `LANGUAGE` or `LANG`). Or set `ui_language = "it"` in `config.toml`.
- An **X11** graphical session is needed (it is the default on Linux Mint Cinnamon). With Wayland the global hotkey does not work, and the program says so in the log.
- Hotkey: `Ctrl+Win` (Super key) as on Windows. On Linux the keys are not "swallowed": a combination of modifiers only, or a key that writes nothing (`f9`, ...), works best, otherwise the key also reaches the application. If `Ctrl+Super` clashes with another Cinnamon shortcut, change `hotkey` in `config.toml` and choose *Reload configuration* from the icon menu.
- The program starts at login, minimized in the notification area (entry in `/etc/xdg/autostart`). Turn it off from the icon menu (*Start at login*) or from Settings; the user's choice lives in `~/.config/autostart/trascrittore.desktop`. If you start it from the applications menu while it is already running, the Settings window opens.
- Configuration, keys (`.env`, readable only by you) and logs live in `~/.config/trascrittore/`.
- The per-app rules use the name of the foreground program (`thunderbird`, `code`, `firefox`...); the `.exe` suffix and capitals do not matter, so `chrome.exe` also matches `chrome`. For terminals (gnome-terminal, xfce4-terminal, konsole...) paste with `Ctrl+Shift+V` (`paste = "ctrl+shift+v"`).
- On Linux there is no on-screen badge while recording: the sounds and the icon colour remain.
- The clipboard is handled by X11; keys are simulated with the XTest extension. Sounds need `paplay`, `pw-play` or `aplay` (present in Mint); the update notification needs `notify-send` (`libnotify-bin`).
- **Updates:** the program checks for new versions every 6 hours and shows a notification; **Settings > Info > Update now** downloads the `.deb` of the release, verifies its SHA-256 and installs it with `apt` (a password prompt appears, `pkexec`), then restarts. You can also update by hand by reinstalling the new `.deb`.

**Build from source (Linux):**

```bash
sudo apt install build-essential cmake clang pkg-config libasound2-dev libgtk-3-dev libayatana-appindicator3-dev libxdo-dev libfontconfig1-dev libxkbcommon-dev
cargo build --release          # target/release/trascrittore
cargo install cargo-deb --locked && cargo deb --no-build   # target/debian/*.deb
```

The tests (`cargo test`) run without a graphical server; the two tests that use a real X11 server (`paste_smoke`, `foreground_smoke`) are `#[ignore]` and are run by hand, for example with Xvfb.

## Versions

Every version has a number **YYYY.MM.DD.N**: the build date (Italian time) and a progressive number of the day (2026.10.25.1, 2026.10.25.2, ...). The GitHub Action computes it at every merge to `main`, writes it into the executable and the installer and publishes a release with the tag `vYYYY.MM.DD.N`; the most recent one is marked *Latest*. The version can be read in *Home* and in *Settings*, *Info* section (a local build shows "local"). A fixed `latest` release also remains, used only by installations that predate the version system: it can be deleted once all PCs have updated.

## Architecture (x64, ARM64)

The program and the installer are built for **x64** (`x86_64-pc-windows-msvc`, the default target of `windows-latest` in the GitHub Action) and install in 64-bit mode. On Windows 11 for ARM64 the installer starts and the program runs through the x64 emulation of Windows, but there is no native ARM64 build and it has not been tried. 32-bit Windows is not supported.

## Automatic updates (optional)

The repository is public, so updates need **no token and no account**: the program reads the public releases of GitHub. (Older versions asked for a GitHub token: if you still have a `TRASCRITTORE_UPDATE_TOKEN=...` line in `.env` it is ignored and you can delete it.) Turn updates on or off with the *Automatic updates on* box in Settings > Info (`auto_update` in `config.toml`).

How it works: 20 seconds after startup, and then every 6 hours, the program reads the latest release. If its version is newer than the installed one (the numbers are compared: 2026.10.25.10 comes after 2026.10.25.9), it downloads **only `trascrittore.exe`** (a few MB), checks the SHA-256 declared by GitHub, renames the running executable, puts the new one in its place and restarts (this only happens if the program folder is writable, for example in a local build). With the installer in Program Files the program cannot replace itself without administrator rights: it downloads `Trascrittore-Setup.exe` and starts it in silent mode, and Windows shows **an administrator (UAC) prompt at every update**; if you refuse nothing changes and it tries again at the next check. The full installer is also used when an update is marked `[installer]` in the PR title (structural changes). During a dictation the update waits. Security: the exe in Program Files cannot be modified without administrator rights, but the users of the PC can read and modify `.env` in `C:\ProgramData\Trascrittore` (keys included): the installation is meant for a personal PC. If the check fails, the message states the cause (for example no release published yet: after every merge it takes a couple of minutes, or too many requests to GitHub). In Settings, *Info* section, the buttons **Check for updates** and **Update now** check and install right away, even with automatic updates off. With updates off, download `Trascrittore-Setup.exe` from the Releases page. Local builds (`cargo build`) never update.

## Settings (click the tray icon)

A left click on the icon (or *Settings...* in the right-click menu) opens a window ([Slint](https://slint.dev) interface with the software renderer, sharp at any Windows scale) that exists only while it is open. The sections are in the left bar:

1. **Home**: summary on opening: the AI and transcription model that will answer now, services in priority order and whether they have a key, hotkey, version and updates, usage since start.
2. **AI**: one tile per service (Groq, OpenAI, Anthropic and, last, local transcription with Whisper). Each tile shows the status (ready, paused and why, no key), key (**Verify and save**, **Remove**, **Create key**; only the last 4 characters are shown; saved in `.env`), models, counters since start and the link to the credit page (none of the three exposes it via API). **Drag** the tiles by the handle on the left to change the priority (`providers` in `config.toml`); the *Use* box excludes a service. If a service runs out of credit or rejects the key, the next one is used; for Groq also the fallback models (`fallback_models`, `stt_fallback_models`) with limits read from the headers.
3. **Prompt**: the complete prompts sent to the AI, chosen from a list by type of application (unrecognized app and every `[[apps]]` rule, with the name given by `name`).
4. **Settings**: icon with halo (`overlay`), start/end **sounds** (on/off, type *Soft*, *Click* or *Chime*, volume, *Try* button; `sounds`, `sound_style`, `sound_volume` in `config.toml`), the **Language** of the interface and opening the configuration folder.
5. **Info**: version, automatic updates (**Check for updates**, **Update now**), debug mode, dictated text in the log (`debug_log_text`), opening the log, paths and a description of the program.

The exe has a per-monitor DPI manifest (`assets/app.manifest`): without it Windows enlarges windows as images and the text looks blurry. The halo icon stays unscaled by the system, as before.

Texts are selectable and copyable.

## Local transcription (Whisper)

As a **last resort**, when no network service answers (credit finished, key rejected, no network), the audio is transcribed on your PC with Whisper ([whisper.cpp](https://github.com/ggml-org/whisper.cpp)). It is the fourth tile of the **AI** section (`local` in `providers`, last by default, draggable and excludable with *Use*). Whoever already had a `config.toml` finds it added at the end once (`local_migrated = true`); if you remove it, it is not put back.

- The model is **not in the installer**: choose the size (`local_model`: `tiny` ~75 MB, `base` ~142 MB recommended, `small` ~466 MB, `medium` ~1.5 GB; multilingual models) and download it with the **Download** button (with progress and **Cancel**) from Hugging Face into `models\` next to `config.toml`. The downloaded file is checked with the magic number and, when Hugging Face declares it, with the SHA-256. **Delete** removes it; uninstalling removes the whole data folder, models included.
- The model is loaded only during transcription and freed right after: at rest it takes no RAM. On CPU it is slower than the cloud (a few seconds for a sentence with `base`, more with `small` and `medium`).
- Local Whisper **only transcribes**: if the correction AI is also down, the text is not corrected and the raw transcription is pasted. If instead a network service still works for correction, the text is corrected as usual.
- No data leaves the PC while the local service is used. No credit or key is needed.
- The build compiles whisper.cpp (needs CMake and libclang; on the GitHub runner they are already there, see `LIBCLANG_PATH` in `.github/workflows/build.yml`). The exe grows by a few MB.

## Computer sound removal (echo)

Off by default (`echo_cancel = true` in `config.toml`, or Settings > *Computer sound*). While you dictate, the app also records what goes to the speakers (Windows: WASAPI loopback; Linux: `parec` on the default output monitor, package `pulseaudio-utils`) and, when you stop, subtracts it from the microphone with an adaptive filter, after finding the delay between the two. With headphones there is nothing to remove and the microphone stays untouched. It is a software filter, not a miracle: very loud speakers or heavy room echo may leave traces. If the option is not available on your system, it is greyed out.

## Sounds

A short sound (less than 0.25 s) marks the start and another the end of the recording. They are tones synthesized in memory (no file) and played asynchronously. They do not end up in the recording: at the start the program discards the first instants of the audio (sound duration plus a margin), at the end the microphone is already closed when the sound starts. So start talking after the sound. Adjust them in Settings or with `sounds`, `sound_style` (`soft`, `click`, `chime`) and `sound_volume` (0-100) in `config.toml`.

## Configuration (`config.toml`)

- `hotkey`: shortcut, e.g. `"ctrl+win"`, `"ctrl+alt+space"`, `"f9"`.
- `macropad`, `macropad_logo`, `macropad_c`, `macropad_v`, `macropad_ids`: support for the Stack Overflow "The Key" 3-key keyboard. When it is plugged in, a card appears in Settings and its buttons start the dictation (defaults: logo = normal with AI + Enter, C = no AI + Enter, V = transcription only). The device is recognised by its USB name; if it is not, add its `vid:pid` to `macropad_ids` (turn on debug mode: the log lists every keyboard with its ids). The keyboard must send its factory keys (Ctrl, C, V). Independent project, not affiliated with Stack Overflow.
- `mode`: `"hold"` (hold down) or `"toggle"` (press to start/finish).
- `language`: language of the speech you dictate (ISO-639-1 code such as `"en"`, `"it"`; empty = automatic detection).
- `ui_language`: language of the menus and windows: `"auto"` (system language) or a code such as `"en"`, `"it"`. Also changeable in Settings.
- `stt_model`, `llm_model`, `fallback_models`, `stt_fallback_models`, `max_seconds`, `restore_clipboard`.
- `default_prompt`: base prompt, always applied (cleaning up the dictated text). The `prompt` of the app rule is added to it.
- `dictionary`: words/names to write exactly like this.
- `[[apps]]`: per-app rules, with `exe` (process names), `prompt`, `raw = true` to paste the transcription without AI, `send_enter = true` to press Enter after pasting (e.g. Claude desktop), `paste = "shift+insert"` for terminals that need it, `title_contains` to tell apart e.g. a website in the browser.

After every change use **Reload configuration** from the tray.

## Languages

The texts of the program are in `lang/<code>.toml` (`en.toml`, `it.toml`). The language is chosen like this: the installer language on Windows (written to `ui_language` the first time), or `ui_language` in `config.toml`, or *Settings > Language*; with `"auto"` the system language is used (and English if there is no file for it). Any text missing from a language falls back to English. To add a language, copy `lang/_template.toml` to `lang/<code>.toml`, translate it and open a pull request: the file is picked up by the build automatically. You can also try a language without building: put the file in a `lang` folder next to `config.toml` and restart the program. The step-by-step guide is in [CONTRIBUTING.md](CONTRIBUTING.md).

The prompts in `config.toml` (`default_prompt`, `[[apps]]`) are written in English and tell the AI to reply in the language of the dictated text; you can translate or change them freely.

## Notes

- The microphone is the Windows default one at the moment the recording starts.
- If the AI does not answer, the raw transcription is pasted. Errors go to `trascrittore.log` (without dictated text or keys).
- The Windows-specific part is isolated in `src/platform_windows.rs` and `src/hotkey_windows.rs` (Linux in the `_linux` files): the rest is portable.
- See `SECURITY.md` for how secrets are handled.

## More AI services and credit

`providers = ["groq", "openai", "anthropic"]` in `config.toml` (or from the *AI* section) is the priority order. For every dictation the program tries the first service that has a key; if it answers that credit is finished (HTTP 402, `insufficient_quota`, "credit balance is too low"), that the key is rejected or that it is unreachable, it pauses it and moves to the next (finished credit and rejected keys are retried after an hour). Groq tries its fallback models first. Anthropic does not transcribe audio, so Groq and OpenAI are used for transcription; all three for text correction. Models (light by default: `gpt-4o-mini-transcribe` for audio, `gpt-4.1-nano` for correction, `claude-haiku-5-5` for Anthropic; change them in the *AI* section or in `config.toml`): `openai_stt_model`, `openai_llm_model`, `anthropic_llm_model`. OpenAI "reasoning" models (gpt-5..., o3/o4) are called without temperature, which they do not accept; keys in `.env`: `GROQ_API_KEY`, `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`.

**Remaining credit:** none of the three services provides it through a normal API key, so the program cannot read it. The *AI* section shows for each one the key, whether it is ready or paused for finished credit, calls and tokens since start and the link to the page where the credit can be read.

## Program icon

`assets/icon.ico` (16-256 px: white microphone with voice bars on an indigo-violet-pink gradient background) is embedded in the exe through `build.rs` and `assets/app.rc`: it is used by Explorer, the Start menu, the installer, the uninstall entry and the Settings window. The tray icon has the same microphone with a background that changes with the state (blue ready, red recording, amber processing, grey error). To regenerate it: `python assets/make_icon.py` (needs Pillow).

## Halo icon

While you hold the hotkey, a dark circle appears at the bottom right (on the screen of the active window) with the icon of the program the text will be pasted into, surrounded by a red halo that widens with the microphone volume and narrows when you are silent. During transcription the halo turns amber and "breathes"; then it disappears. The window does not take focus, does not intercept clicks and is hidden (no timer) when not needed. Turn it off in Settings or with `overlay = false` in `config.toml`.

## Tests

`cargo test` checks the configuration parsing, the `debug` switch in `config.toml`, the date format in the log, the per-app prompts, saving the key in `.env`, reading the update release (allowed URLs, SHA-256, the `[installer]` marker), the switch to fallback models, the choice of the per-app rule (including `send_enter` and `raw`), the hotkey parsing, the halo maths (level in dB, radius, smoothing) and the language files (same keys and placeholders in every language, every text used by the code exists, language detection from the system locale, discovery of new files). The drawing of the window cannot be tested automatically. The GitHub Action runs them at every push and on every pull request, before the build.

## Space used

- **Audio**: the recording lives only in RAM and is never written to disk. The buffer is limited by `max_seconds` (120 s: about 23 MB at 48 kHz while recording, a few MB for a normal dictation) and is freed as soon as processing ends.
- **Log**: `trascrittore.log` is limited to two files of at most 512 KB (`.log` and `.log.1`).
- **Fixed files**: `config.toml` and `.env` in the program folder (`C:\ProgramData\Trascrittore` with the installer), a few KB. The installed program is a single `.exe` of a few MB.
- **Updates**: the new `.exe` is downloaded next to the old one and the `.old` file is deleted at the next restart; any installer downloaded to `%TEMP%\Trascrittore-Update` is deleted at the next startup.
- **Usage counters**: they are in memory (a few lines per model) and reset on exit.
- At rest the program does nothing (tray icon and an update check every 6 hours); the settings window exists only while it is open.

## License and legal notes

The code is **readable but not reusable** (source-available): you may read it and use the programs published in the Releases for free; you may not copy it, redistribute it or make derivative works. Contributions (for example new languages) are welcome, see [CONTRIBUTING.md](CONTRIBUTING.md).

| | English | Italiano |
|---|---|---|
| License | [LICENSE](LICENSE) | [LICENSE.it.txt](legal/LICENSE.it.txt) |
| Privacy notice | [PRIVACY.en.md](legal/PRIVACY.en.md) | [PRIVACY.it.md](legal/PRIVACY.it.md) |
| Notice about third-party AI services | [DISCLAIMER.en.txt](legal/DISCLAIMER.en.txt) | [DISCLAIMER.it.txt](legal/DISCLAIMER.it.txt) |
| Third-party components | [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) | |

**Important:** Groq, OpenAI and Anthropic process the data you send under their own terms; the Author does not guarantee how they handle it. These texts are not legal advice.

<a href="https://slint.dev"><img src="https://raw.githubusercontent.com/slint-ui/slint/master/logo/MadeWithSlint-logo-whitebg.png" alt="Made with Slint" height="48"></a>
