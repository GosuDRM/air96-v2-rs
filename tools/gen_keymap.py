import json, re, sys, os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from qmk_keycodes import table, resolve, C
kc, names = table()
kbj = json.load(open(f'{C}/keyboards/air96_v2/ansi/keyboard.json'))
pos = [tuple(k['matrix']) for k in kbj['layouts']['LAYOUT']['layout']]
src = open(f'{C}/keyboards/air96_v2/ansi/keymaps/default/keymap.c').read()
src = src[src.index('keymaps[]'):src.index('g_is31_leds')]
layers = []
for m in re.finditer(r'\[(\d)\]\s*=\s*LAYOUT\(', src):
    i = j = m.end(); depth = 1
    while depth:
        depth += {'(': 1, ')': -1}.get(src[j], 0); j += 1
    toks = [t for t in src[i:j-1].split(',') if t.strip()]
    grid = [[kc['KC_NO']] * 21 for _ in range(6)]
    for (r, c), t in zip(pos, toks):
        grid[r][c] = resolve(t, kc)
    layers.append((m.group(1), grid))
assert len(layers) == 5
rep = open(f'{C}/tmk_core/protocol/report.h').read()
usage = {n: int(v, 16) for n, v in re.findall(r'^\s+(\w+)\s*=\s*(0x[0-9A-Fa-f]+)', rep, re.M)}
def switch(fn):
    body = rep[rep.index(fn):]
    body = body[:body.index('default:')]
    return [(kc[a], usage[b]) for a, b in re.findall(r'case (\w+):\s*return (\w+);', body)]
o = []
w = o.append
w('//! Keymap and keycodes — generated from the C firmware by `gen_keymap.py`')
w('//! (keyboards/air96_v2/ansi/keymaps/default/keymap.c, keyboard.json LAYOUT,')
w('//! quantum/keycodes.h, tmk_core/protocol/report.h). Do not edit by hand.')
w('//!')
w('//! Keycodes are QMK\'s numeric values, so VIA reads and writes them as it does')
w('//! with the C VIA firmware. Positions outside the LAYOUT are KC_NO.')
w('')
w('pub const LAYER_COUNT: usize = 5;')
w(f'pub const KC_NO: u16 = 0x{kc["KC_NO"]:04X};')
w(f'pub const KC_TRNS: u16 = 0x{kc["KC_TRANSPARENT"]:04X};')
w('')
w('// ── ansi.h custom keycodes (QK_KB_0 + n) ──')
for n in names:
    w(f'pub const KC_{n}: u16 = 0x{kc[n]:04X};')
w('')
w('// ── QMK keycode ranges (quantum/keycodes.h) ──')
for n in ['QK_BASIC','QK_MODS','QK_MODS_MAX','QK_MOD_TAP','QK_MOD_TAP_MAX','QK_LAYER_TAP','QK_LAYER_TAP_MAX','QK_LAYER_MOD','QK_LAYER_MOD_MAX','QK_TO','QK_TO_MAX','QK_MOMENTARY','QK_MOMENTARY_MAX','QK_DEF_LAYER','QK_DEF_LAYER_MAX','QK_TOGGLE_LAYER','QK_TOGGLE_LAYER_MAX','QK_ONE_SHOT_LAYER','QK_ONE_SHOT_LAYER_MAX','QK_ONE_SHOT_MOD','QK_ONE_SHOT_MOD_MAX','QK_LAYER_TAP_TOGGLE','QK_LAYER_TAP_TOGGLE_MAX','QK_MACRO','QK_MACRO_MAX','QK_KB','QK_KB_MAX','QK_BOOT','QK_REBOOT','QK_CLEAR_EEPROM','QK_GRAVE_ESCAPE','QK_LOCK','QK_UNDERGLOW_TOGGLE','QK_UNDERGLOW_MODE_NEXT','QK_UNDERGLOW_MODE_PREVIOUS','QK_UNDERGLOW_HUE_UP','QK_UNDERGLOW_HUE_DOWN','QK_UNDERGLOW_SATURATION_UP','QK_UNDERGLOW_SATURATION_DOWN','QK_UNDERGLOW_VALUE_UP','QK_UNDERGLOW_VALUE_DOWN','QK_UNDERGLOW_SPEED_UP','QK_UNDERGLOW_SPEED_DOWN','QK_RGB_MATRIX_ON','QK_RGB_MATRIX_OFF','QK_RGB_MATRIX_TOGGLE','QK_RGB_MATRIX_MODE_NEXT','QK_RGB_MATRIX_MODE_PREVIOUS','QK_RGB_MATRIX_HUE_UP','QK_RGB_MATRIX_HUE_DOWN','QK_RGB_MATRIX_SATURATION_UP','QK_RGB_MATRIX_SATURATION_DOWN','QK_RGB_MATRIX_VALUE_UP','QK_RGB_MATRIX_VALUE_DOWN','QK_RGB_MATRIX_SPEED_UP','QK_RGB_MATRIX_SPEED_DOWN','KC_A','KC_EXSEL','KC_SYSTEM_POWER','KC_SYSTEM_WAKE','KC_AUDIO_MUTE','KC_LAUNCHPAD','KC_MS_UP','KC_MS_DOWN','KC_MS_LEFT','KC_MS_RIGHT','KC_MS_BTN1','KC_MS_BTN8','KC_MS_WH_UP','KC_MS_WH_DOWN','KC_MS_WH_LEFT','KC_MS_WH_RIGHT','KC_MS_ACCEL0','KC_MS_ACCEL1','KC_MS_ACCEL2','KC_LEFT_CTRL','KC_RIGHT_GUI','KC_ESCAPE','KC_GRAVE','KC_NUM_LOCK','KC_CAPS_LOCK','KC_LOCKING_CAPS_LOCK','KC_BACKSLASH','KC_BACKSPACE','KC_9','KC_0','KC_ENTER','KC_SPACE','KC_LEFT_SHIFT','KC_RIGHT_SHIFT','KC_LEFT_ALT','KC_RIGHT_ALT','KC_LEFT_GUI','KC_RIGHT_CTRL','QK_DEBUG_TOGGLE','QK_ONE_SHOT_ON','QK_ONE_SHOT_OFF','QK_ONE_SHOT_TOGGLE','QK_TRI_LAYER_LOWER','QK_TRI_LAYER_UPPER'] + sorted([n for n in kc if n.startswith('QK_MAGIC_') or n.startswith('QK_SPACE_CADET_')], key=lambda n: kc[n]):
    w(f'pub const {n}: u16 = 0x{kc[n]:04X};')
w('')
w('// ── Keycodes used by name in the firmware ──')
for alias, n in [('KC_BRID','KC_BRID'),('KC_BRIU','KC_BRIU'),('KC_MPRV','KC_MPRV'),('KC_MPLY','KC_MPLY'),('KC_MNXT','KC_MNXT'),('KC_MUTE','KC_MUTE'),('KC_VOLD','KC_VOLD'),('KC_VOLU','KC_VOLU'),('KC_RGB_MOD','RGB_MOD'),('KC_RGB_HUI','RGB_HUI'),('KC_RGB_VAI','RGB_VAI'),('KC_RGB_VAD','RGB_VAD'),('KC_RGB_SPI','RGB_SPI'),('KC_RGB_SPD','RGB_SPD')]:
    w(f'pub const {alias}: u16 = 0x{kc[n]:04X};')
w('')
w('/// `MO(layer)` → layer, as decoded by `action_for_keycode()`.')
w('pub fn mo_layer(kc: u16) -> Option<usize> {')
w('    if (QK_MOMENTARY..=QK_MOMENTARY_MAX).contains(&kc) { Some((kc & 0x1F) as usize) } else { None }')
w('}')
w('')
w('/// `IS_CONSUMER_KEYCODE()`.')
w('pub fn is_consumer_key(kc: u16) -> bool {')
w('    (KC_AUDIO_MUTE..=KC_LAUNCHPAD).contains(&kc)')
w('}')
w('')
w('/// `IS_SYSTEM_KEYCODE()`.')
w('pub fn is_system_key(kc: u16) -> bool {')
w('    (KC_SYSTEM_POWER..=KC_SYSTEM_WAKE).contains(&kc)')
w('}')
w('')
w('/// `KEYCODE2CONSUMER()`.')
w('pub fn consumer_usage(kc: u16) -> u16 {')
w('    match kc {')
for a, b in switch('KEYCODE2CONSUMER'):
    w(f'        0x{a:04X} => 0x{b:04X},')
w('        _ => 0,')
w('    }')
w('}')
w('')
w('/// `KEYCODE2SYSTEM()`.')
w('pub fn system_usage(kc: u16) -> u16 {')
w('    match kc {')
for a, b in switch('KEYCODE2SYSTEM'):
    w(f'        0x{a:04X} => 0x{b:04X},')
w('        _ => 0,')
w('    }')
w('}')
w('')
w('/// keymaps[][MATRIX_ROWS][MATRIX_COLS] from keymap.c.')
w('pub const KEYMAPS: [[[u16; 21]; 6]; LAYER_COUNT] = [')
for idx, grid in layers:
    w(f'    // layer {idx}')
    w('    [')
    for row in grid:
        w('        [' + ', '.join(f'0x{v:04X}' for v in row) + '],')
    w('    ],')
w('];')
w('')
w('/// `keycode_at_keymap_location_raw()`: KC_TRNS beyond the compiled layers.')
w('pub fn get_keycode(layer: usize, row: usize, col: usize) -> u16 {')
w('    if row < 6 && col < 21 && layer < LAYER_COUNT { KEYMAPS[layer][row][col] } else { KC_TRNS }')
w('}')
print('\n'.join(o))
