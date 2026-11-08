#pragma once
#include "rgb_matrix_types.h"
#define EECONFIG_DEBOUNCE_HELPER(name, offset, config) \
    static inline void eeconfig_init_##name(void) {} \
    static inline void eeconfig_flush_##name(bool f) { (void)f; } \
    static inline void eeconfig_flush_##name##_task(uint16_t t) { (void)t; }
static inline bool eeconfig_is_enabled(void) { return true; }
static inline void eeconfig_read_rgb_matrix(rgb_config_t *c) { (void)c; }
static inline void eeconfig_update_rgb_matrix_raw(void) {}
#define EECONFIG_RGB_MATRIX 0
static inline void eeconfig_flag_rgb_matrix(bool f) { (void)f; }
