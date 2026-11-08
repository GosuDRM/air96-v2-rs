"""Per-function stack frames of the firmware ELF and the worst call chain.
usage: python3 tools/stack_frames.py target/thumbv6m-none-eabi/release/air96-v2"""
import re, subprocess, sys
E = sys.argv[1]
dis = subprocess.run(['arm-none-eabi-objdump', '-d', E], capture_output=True, text=True).stdout
raw = subprocess.run(['arm-none-eabi-objdump', '-s', '-j', '.text', E], capture_output=True, text=True).stdout
mem = {}
for l in raw.splitlines():
    m = re.match(r'\s*([0-9a-f]{7,8}) ((?:[0-9a-f]{8} ?){1,4})', l)
    if m:
        a = int(m.group(1), 16)
        for i, w in enumerate(m.group(2).split()):
            mem[a + 4 * i] = int.from_bytes(bytes.fromhex(w), 'little')
frames, calls, addr2fn = {}, {}, {}
fn = None
pending = None
for l in dis.splitlines():
    m = re.match(r'^([0-9a-f]+) <(.+)>:$', l)
    if m:
        fn = m.group(2); frames[fn] = 0; calls[fn] = set(); addr2fn[int(m.group(1), 16)] = fn; continue
    if fn is None:
        continue
    m = re.search(r'push\t\{([^}]*)\}', l)
    if m:
        frames[fn] += 4 * len(m.group(1).split(','))
    m = re.search(r'sub\tsp, #(\d+)', l)
    if m:
        frames[fn] += int(m.group(1))
    m = re.search(r'ldr\t(r\d+), \[pc.*@ \(([0-9a-f]+)', l)
    if m:
        pending = (m.group(1), int(m.group(2), 16))
    m = re.search(r'add\tsp, (r\d+)', l)
    if m and pending and pending[0] == m.group(1):
        v = mem.get(pending[1], 0)
        if v & 0x80000000:
            frames[fn] += 0x100000000 - v
    m = re.search(r'\tbl\t[0-9a-f]+ <([^>+]+)>', l)
    if m:
        calls[fn].add(m.group(1))
memo = {}
def depth(f, seen=()):
    if f in seen:
        return 0, [f + ' (recursion)']
    if f in memo:
        return memo[f]
    best = (0, [])
    for c in calls.get(f, ()):
        d, p = depth(c, seen + (f,))
        if d > best[0]:
            best = (d, p)
    r = (frames.get(f, 0) + best[0], [f] + best[1])
    memo[f] = r
    return r
main = [f for f in frames if '__cortex_m_rt_main' in f][0]
d, path = depth(main)
short = lambda f: re.sub(r'17h[0-9a-f]{16}E$', '', f)[-70:]
print(f'worst main call chain: {d} bytes')
for f in path:
    print(f'  {frames.get(f, 0):5} {short(f)}')
