#pragma once
#include <stdint.h>
#include <stdbool.h>
#include "info_config.h"
#define MATRIX_ROWS 6
#define MATRIX_COLS 21
#define RGB_MATRIX_LED_COUNT 110
#define RGB_MATRIX_DEFAULT_MODE RGB_MATRIX_CYCLE_LEFT_RIGHT
static inline bool is_keyboard_master(void) { return true; }
static inline bool is_keyboard_left(void) { return true; }
typedef uint32_t matrix_row_t;
