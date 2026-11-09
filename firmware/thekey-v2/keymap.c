// Keymap for Trascrittore audio, Stack Overflow "The Key V2" (QMK keyboard drop/thekey/v2) with VIA support.
// The three keys send F13 (Stack Overflow key), F14 (C key) and F15 (V key) instead of Ctrl, C, V: keys no
// program uses, so the app can tell them apart from a normal keyboard and they can be held.
// Keys and lighting can be changed later from the VIA app (usevia.app) with via-definition.json.
// SPDX-License-Identifier: GPL-2.0-or-later
#include QMK_KEYBOARD_H

const uint16_t PROGMEM keymaps[][MATRIX_ROWS][MATRIX_COLS] = {
    [0] = LAYOUT(KC_F13, KC_F14, KC_F15),
    [1] = LAYOUT(KC_TRNS, KC_TRNS, KC_TRNS),
    [2] = LAYOUT(KC_TRNS, KC_TRNS, KC_TRNS),
    [3] = LAYOUT(KC_TRNS, KC_TRNS, KC_TRNS),
};

// Key combinations for the LEDs (as on the original firmware). A combo replaces the keys it is made of, so the
// logo key still works on its own (hold to talk) and a chord only changes the lighting:
//   logo + C -> next effect, logo + V -> brightness (cycles), C + V -> colour (hue).
const uint16_t PROGMEM combo_logo_c[] = {KC_F13, KC_F14, COMBO_END};
const uint16_t PROGMEM combo_logo_v[] = {KC_F13, KC_F15, COMBO_END};
const uint16_t PROGMEM combo_c_v[]    = {KC_F14, KC_F15, COMBO_END};

combo_t key_combos[] = {
    COMBO(combo_logo_c, UG_NEXT),
    COMBO(combo_logo_v, UG_VALU),
    COMBO(combo_c_v, UG_HUEU),
};
