# Unique keys for the Stack Overflow "The Key V2"

By default the three keys send Ctrl, C and V. This keymap makes them send **F13, F14, F15** instead (keys that
Windows and normal programs never use), so Trascrittore audio can use them without clashing with anything.
The program understands both the factory keys and F13-F15, so reprogramming is optional.

## Keys and LEDs with the VIA app

The firmware has VIA support, so keys and LEDs are changed from a normal window, no more flashing:

1. Open [usevia.app](https://www.usevia.app/) in Chrome or Edge (it talks to the keyboard over USB).
2. Open the *Design* tab, turn on *Show Design tab* in *Settings* if you do not see it, and load `via-definition.json`
   from this folder (it is a VIA V3 definition; keep *Use V2 definitions* off).
3. The *Configure* tab then shows the three keys (change them by clicking) and *Lighting* (effect, brightness,
   colour, speed). Settings are saved in the keyboard.

On first start the LEDs show a different colour on every LED across the whole rainbow (static gradient). In the
*Lighting* tab choose the effect *Static Gradient* or *Rainbow Swirl* to get that look back.

## How to flash it (about 10 minutes)

1. Easiest: use the ready-made firmware (build it with the QMK command line from the files in this folder,
   see below). Or open the [QMK Configurator](https://config.qmk.fm/), choose the keyboard `drop/thekey/v2`, set the three keys to
   `F13`, `F14`, `F15` (left to right), then *Compile* and download the `.hex` file.
   (With the QMK command line: copy `keymap.c`, `config.h` and `rules.mk` to `keyboards/drop/thekey/v2/keymaps/trascrittore/` and run
   `qmk compile -kb drop/thekey/v2 -km trascrittore`.)
2. Install [QMK Toolbox](https://github.com/qmk/qmk_toolbox/releases), open the `.hex`; the MCU must read `atmega32u4`.
3. Put the keyboard in bootloader mode: unscrew the back and press the small button on the board while it is
   plugged in (see [Drop's guide](https://helpdesk.drop.com/hc/en-us/articles/45960109334673-How-to-Configure-Stack-Overflow-The-Key-and-The-Key-V2-Macropad)).
   QMK Toolbox shows a yellow "device connected" message.
4. Click *Flash*. When it says the device disconnected, unplug and replug it.

Afterwards you can enter the bootloader again by holding the first key while plugging the cable in (bootmagic).
The original behaviour (Ctrl, C, V) is restored by flashing the `default` keymap of the same keyboard.
