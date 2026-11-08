import re, sys
src = open(sys.argv[1]).read()
src = re.sub(r'/\*.*?\*/', '', src, flags=re.S)
src = re.sub(r'//[^\n]*', '', src)
defs = {k: int(v) for k, v in re.findall(r'#define\s+(\w+)\s+\(?(\d+)\)?', src)}
out = ['// Generated from keyboards/air96_v2/ansi/side.h by gen_side_tables.py — do not edit by hand.', '']
for k in ['WAVE_TAB_LEN', 'BREATHE_TAB_LEN', 'FLOW_COLOUR_TAB_LEN']:
    out.append(f'pub const {k}: usize = {defs[k]};')
for name in ['breathe_data_tab', 'wave_data_tab', 'flow_rainbow_colour_tab', 'colour_lib']:
    m = re.search(r'const\s+uint8_t\s+' + name + r'\s*\[([^\]]*)\](\s*\[\s*3\s*\])?\s*=\s*\{(.*?)\};', src, re.S)
    dim, three, body = m.group(1).strip(), m.group(2), m.group(3)
    n = defs.get(dim, int(dim) if dim.isdigit() else None)
    nums = [int(x, 0) for x in re.findall(r'0[xX][0-9a-fA-F]+|\d+', body)]
    if three:
        assert len(nums) == 3 * n, (name, len(nums), n)
        rows = [nums[i:i+3] for i in range(0, len(nums), 3)]
        out.append(f'pub const {name.upper()}: [[u8; 3]; {n}] = [')
        out += ['    [' + ', '.join(f'0x{v:02X}' for v in r) + '],' for r in rows]
    else:
        assert len(nums) == n, (name, len(nums), n)
        out.append(f'pub const {name.upper()}: [u8; {n}] = [')
        out += ['    ' + ', '.join(str(v) for v in nums[i:i+16]) + ',' for i in range(0, len(nums), 16)]
    out.append('];')
print('\n'.join(out))
