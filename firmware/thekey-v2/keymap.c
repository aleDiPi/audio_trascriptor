// Keymap for Trascrittore audio, Stack Overflow "The Key V2" (QMK keyboard drop/thekey/v2).
// Layer 0: the three keys send F13, F14 and F15 (keys no program uses; the factory firmware sends Ctrl, C, V).
//          Order: Stack Overflow key, C key, V key.
// LEDs: press the C and V keys together to enter the LED settings pages (same pages as Drop's "rgb-control"
//       keymap): 1 = on/off + effect, 2 = brightness, 3 = hue, 4 = saturation. In pages 1-4 the first key moves
//       to the next page (after page 4 you are back at layer 0), the other two keys change the setting.
// Based on Jason Wihardja's rgb-control keymap (GPL-2.0-or-later).
#include QMK_KEYBOARD_H

enum custom_keycodes {
    LAYER_SWITCH = SAFE_RANGE,
};

const uint16_t PROGMEM keymaps[][MATRIX_ROWS][MATRIX_COLS] = {
    [0] = LAYOUT(KC_F13, KC_F14, KC_F15),
    [1] = LAYOUT(LAYER_SWITCH, UG_TOGG, UG_NEXT),
    [2] = LAYOUT(LAYER_SWITCH, UG_VALD, UG_VALU),
    [3] = LAYOUT(LAYER_SWITCH, UG_HUED, UG_HUEU),
    [4] = LAYOUT(LAYER_SWITCH, UG_SATD, UG_SATU),
};

const uint16_t PROGMEM led_combo[] = {KC_F14, KC_F15, COMBO_END};
combo_t key_combos[] = {
    COMBO(led_combo, LAYER_SWITCH),
};

const rgblight_segment_t PROGMEM layer_indicator_0[] = RGBLIGHT_LAYER_SEGMENTS({0, 1, HSV_WHITE}, {1, 4, HSV_OFF});
const rgblight_segment_t PROGMEM layer_indicator_1[] = RGBLIGHT_LAYER_SEGMENTS({0, 1, HSV_OFF}, {1, 1, HSV_WHITE}, {2, 3, HSV_OFF});
const rgblight_segment_t PROGMEM layer_indicator_2[] = RGBLIGHT_LAYER_SEGMENTS({0, 2, HSV_OFF}, {2, 1, HSV_WHITE}, {3, 2, HSV_OFF});
const rgblight_segment_t PROGMEM layer_indicator_3[] = RGBLIGHT_LAYER_SEGMENTS({0, 3, HSV_OFF}, {3, 1, HSV_WHITE}, {4, 1, HSV_OFF});
const rgblight_segment_t PROGMEM layer_indicator_4[] = RGBLIGHT_LAYER_SEGMENTS({0, 4, HSV_OFF}, {4, 1, HSV_WHITE});

const rgblight_segment_t *const PROGMEM rgb_layers[] =
    RGBLIGHT_LAYERS_LIST(layer_indicator_0, layer_indicator_1, layer_indicator_2, layer_indicator_3, layer_indicator_4);

void keyboard_post_init_user(void) {
    rgblight_layers = rgb_layers;
}

static uint8_t page = 0;

bool process_record_user(uint16_t keycode, keyrecord_t *record) {
    if (keycode == LAYER_SWITCH) {
        if (record->event.pressed) {
            if (page > 0) {
                layer_off(page);
            }
            rgblight_unblink_layer(page);
            page = (page + 1) % 5;
            rgblight_blink_layer_repeat(page, 1000, 1);
            if (page > 0) {
                layer_on(page);
            }
        }
        return false;
    }
    return true;
}
