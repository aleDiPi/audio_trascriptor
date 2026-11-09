// Copyright 2023 Oleg Titov <oleg.titov@gmail.com>
// SPDX-License-Identifier: GPL-2.0-or-later

#pragma once

// Unique keyboard id
#define VIAL_KEYBOARD_UID {0x08, 0x3B, 0x57, 0xCA, 0xC1, 0xE4, 0x65, 0xB4}

// Unlock with left + right keys
#define VIAL_UNLOCK_COMBO_ROWS { 0, 0 }
#define VIAL_UNLOCK_COMBO_COLS { 0, 2 }

// First start: every LED a different colour over the rainbow (changeable from the Vial app)
#define RGBLIGHT_DEFAULT_MODE RGBLIGHT_MODE_STATIC_GRADIENT
#define RGBLIGHT_DEFAULT_HUE 0
#define RGBLIGHT_DEFAULT_SAT 255
#define RGBLIGHT_DEFAULT_VAL 160

// Keep only a few effects so the firmware fits in the atmega32u4 together with Vial
#undef RGBLIGHT_EFFECT_ALTERNATING
#undef RGBLIGHT_EFFECT_CHRISTMAS
#undef RGBLIGHT_EFFECT_KNIGHT
#undef RGBLIGHT_EFFECT_RGB_TEST
#undef RGBLIGHT_EFFECT_SNAKE
#undef RGBLIGHT_EFFECT_TWINKLE

#define NO_ACTION_ONESHOT
#define NO_ACTION_MACRO
#define NO_ACTION_FUNCTION
