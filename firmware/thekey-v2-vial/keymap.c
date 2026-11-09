// Vial keymap for Trascrittore audio, Stack Overflow "The Key V2": the keys send F13, F14, F15.
// SPDX-License-Identifier: GPL-2.0-or-later
#include QMK_KEYBOARD_H

const uint16_t PROGMEM keymaps[][MATRIX_ROWS][MATRIX_COLS] = {
    [0] = LAYOUT(KC_F13, KC_F14, KC_F15),
    [1] = LAYOUT(KC_TRNS, KC_TRNS, KC_TRNS),
    [2] = LAYOUT(KC_TRNS, KC_TRNS, KC_TRNS),
    [3] = LAYOUT(KC_TRNS, KC_TRNS, KC_TRNS),
};
