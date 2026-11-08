// Keymap for Trascrittore audio, Stack Overflow "The Key V2" (QMK keyboard drop/thekey/v2).
// The three keys send F13, F14 and F15: keys no program uses, so the app can tell them apart from a normal
// keyboard (the factory firmware sends Ctrl, C and V). Order: Stack Overflow key, C key, V key.
// SPDX-License-Identifier: GPL-2.0-or-later
#include QMK_KEYBOARD_H

const uint16_t PROGMEM keymaps[][MATRIX_ROWS][MATRIX_COLS] = {
    [0] = LAYOUT(KC_F13, KC_F14, KC_F15)
};
