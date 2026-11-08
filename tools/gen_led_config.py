"""Generate src/led/led_config.rs from QMK's generated g_led_config
(.build/obj_air96_v2_ansi_via/src/default_keyboard.c in the C tree)."""
import re, sys
src = open(sys.argv[1]).read()
body = src[src.index('g_led_config = {'):]
body = body[:body.index('};')]
NO = 255
rows = re.findall(r'\{\s*((?:(?:\d+|NO_LED)\s*,\s*){20}(?:\d+|NO_LED))\s*\}', body)
matrix = [[NO if t.strip() == 'NO_LED' else int(t) for t in r.split(',')] for r in rows[:6]]
# led_point_t is uint8_t: C truncates fractional coordinates (17.5 -> 17).
pts_part = body[body.index('{ {', body.index(rows[5])):body.rindex('{')]
pts = [(int(float(x)), int(float(y))) for x, y in re.findall(r'\{\s*([\d.]+)\s*,\s*([\d.]+)\s*\}', pts_part)]
flags_part = body[body.rindex('{'):]
flags = [int(x) for x in re.findall(r'\d+', flags_part)]
assert len(matrix) == 6 and len(pts) == 110 and len(flags) == 110, (len(matrix), len(pts), len(flags))
o = ['//! Generated from QMK\'s g_led_config (default_keyboard.c) by tools/gen_led_config.py.', '', 'pub const NO_LED: u8 = 255;', '']
o.append('pub const MATRIX_CO: [[u8; 21]; 6] = [')
o += ['    [' + ', '.join(str(v) for v in r) + '],' for r in matrix]
o.append('];')
o.append('pub const POINT: [(u8, u8); 110] = [')
o += ['    ' + ', '.join(f'({x}, {y})' for x, y in pts[i:i + 8]) + ',' for i in range(0, 110, 8)]
o.append('];')
o.append('pub const FLAGS: [u8; 110] = [')
o += ['    ' + ', '.join(str(f) for f in flags[i:i + 22]) + ',' for i in range(0, 110, 22)]
o.append('];')
print('\n'.join(o))
