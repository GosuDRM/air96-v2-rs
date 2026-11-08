#!/bin/sh
# Render every RGB matrix effect with QMK's own rgb_matrix.c (from the C
# firmware tree) and with the Rust port, then compare frame by frame.
# usage: tools/rgb_harness/run.sh /path/to/nuphy-air96-v2
set -e
C=${1:?path to the C firmware tree}
H=$(cd "$(dirname "$0")" && pwd)
W=$(mktemp -d)
cp "$C/.build/obj_air96_v2_ansi_via/src/info_config.h" "$W/info_config.h"
echo '#include "rgb_matrix/post_config.h"' >> "$W/info_config.h"
sed 's/^#include QMK_KEYBOARD_H.*//' "$C/.build/obj_air96_v2_ansi_via/src/default_keyboard.c" > "$W/default_keyboard.c"
cp "$C/.build/obj_air96_v2_ansi_via/src/default_keyboard.h" "$W/"
# The C VIA build's feature flags (USE_CIE1931_CURVE matters for colours).
FLAGS=$(tr ' ' '\n' < "$C/.build/obj_air96_v2_ansi_via/cflags.txt" | grep -E '^-D[A-Z0-9_]+$' | grep -v -E 'PROTOCOL_CHIBIOS|EEPROM|THUMB|CORTEX|MCU|PLATFORM|IS31FL3733' | tr '\n' ' ')
gcc -std=gnu11 -O1 -w $FLAGS -I"$H/stubs" -I"$W" -I"$C" -I"$C/quantum" -I"$C/quantum/rgb_matrix" \
    -I"$C/quantum/rgb_matrix/animations" -I"$C/quantum/rgb_matrix/animations/runners" -I"$C/lib" -I"$C/lib/lib8tion" \
    -include "$H/stubs/keyboard.h" "$H/harness.c" "$C/quantum/rgb_matrix/rgb_matrix.c" "$C/quantum/color.c" \
    "$C/lib/lib8tion/lib8tion.c" "$C/quantum/led_tables.c" -lm -o "$W/harness"
"$W/harness" > "$W/c.txt"
(cd "$H/../.." && RGB_DUMP="$W/rust.txt" cargo test -q --lib --target x86_64-unknown-linux-gnu dump_rgb_frames -- --ignored >/dev/null)
if cmp -s "$W/c.txt" "$W/rust.txt"; then echo "all $(wc -l < "$W/c.txt") frames match QMK"; else echo "MISMATCH: diff $W/c.txt $W/rust.txt"; exit 1; fi
