// SPDX-License-Identifier: GPL-2.0-or-later
#pragma once

// First start (and after a settings reset): every LED a different colour, spread over the whole rainbow.
// Everything can then be changed from the VIA app (keys and lighting), see README.md.
#define RGBLIGHT_DEFAULT_MODE RGBLIGHT_MODE_STATIC_GRADIENT
#define RGBLIGHT_DEFAULT_HUE 0
#define RGBLIGHT_DEFAULT_SAT 255
#define RGBLIGHT_DEFAULT_VAL 160

// Time (ms) within which two keys count as a chord; also the small delay before a single key is sent.
#define COMBO_TERM 40
