// Keymap for Trascrittore audio, Stack Overflow "The Key V2" (QMK keyboard drop/thekey/v2).
// Normal use: the three keys send F13 (Stack Overflow key), F14 (C key) and F15 (V key) instead of Ctrl, C, V.
//   The Stack Overflow key can be held (F13 stays down while you hold it).
// LEDs: tap the Stack Overflow key twice to enter LED mode (the first LED blinks). Then:
//   C key = next LED effect, V key = next colour, Stack Overflow key = back to normal use.
// Brightness and on/off are not mapped; the settings are saved in the keyboard.
// SPDX-License-Identifier: GPL-2.0-or-later
#include QMK_KEYBOARD_H

enum { TD_LOGO = 0 };

const uint16_t PROGMEM keymaps[][MATRIX_ROWS][MATRIX_COLS] = {
    [0] = LAYOUT(TD(TD_LOGO), KC_F14, KC_F15),
    [1] = LAYOUT(TO(0), UG_NEXT, UG_HUEU),
};

const rgblight_segment_t PROGMEM led_mode_blink[] = RGBLIGHT_LAYER_SEGMENTS({0, 5, HSV_WHITE});
const rgblight_segment_t *const PROGMEM rgb_layers[] = RGBLIGHT_LAYERS_LIST(led_mode_blink);

void keyboard_post_init_user(void) {
    rgblight_layers = rgb_layers;
}

layer_state_t layer_state_set_user(layer_state_t state) {
    // blink when LED mode starts or ends
    rgblight_blink_layer_repeat(0, 150, 1);
    return state;
}

typedef struct {
    bool f13_down;
} logo_state_t;
static logo_state_t logo = {false};

static void logo_finished(tap_dance_state_t *state, void *user_data) {
    if (state->count >= 2) {
        layer_on(1);
        return;
    }
    // one tap or hold: F13 (held while the key is held)
    register_code(KC_F13);
    logo.f13_down = true;
}

static void logo_reset(tap_dance_state_t *state, void *user_data) {
    if (logo.f13_down) {
        unregister_code(KC_F13);
        logo.f13_down = false;
    }
}

tap_dance_action_t tap_dance_actions[] = {
    [TD_LOGO] = ACTION_TAP_DANCE_FN_ADVANCED(NULL, logo_finished, logo_reset),
};
