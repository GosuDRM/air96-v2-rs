"""Resolve QMK keycode names to values from the C tree's headers."""
import re
C = '/home/gosudrm/Desktop/nuphy-air96-v2'
def table():
    kc = {}
    h = open(f'{C}/quantum/keycodes.h').read() + open(f'{C}/quantum/quantum_keycodes_legacy.h').read()
    for n, v in re.findall(r'^\s+(\w+)\s*=\s*(0x[0-9A-Fa-f]+)\s*,', h, re.M):
        kc[n] = int(v, 16)
    for _ in range(4):
        for n, v in re.findall(r'^\s+(\w+)\s*=\s*(\w+)\s*,', h, re.M):
            if v in kc: kc[n] = kc[v]
        for n, v in re.findall(r'#define\s+(\w+)\s+(\w+)\s*$', h, re.M):
            if v in kc: kc[n] = kc[v]
    kc['_______'] = kc['KC_TRANSPARENT']
    # ansi.h custom keycodes (QK_KB_0 + n)
    a = open(f'{C}/keyboards/air96_v2/ansi/ansi.h').read()
    body = a[a.index('enum custom_keycodes'):]
    body = body[body.index('{') + 1:body.index('}')]
    names = [x.split('=')[0].strip() for x in body.split(',') if x.strip()]
    for i, n in enumerate(names):
        kc[n] = kc['QK_KB_0'] + i
    return kc, names
def resolve(tok, kc):
    tok = tok.strip()
    m = re.fullmatch(r'MO\((\d+)\)', tok)
    if m: return kc['QK_MOMENTARY'] | int(m.group(1))
    return kc[tok]
