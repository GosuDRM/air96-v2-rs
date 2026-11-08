#pragma once
#include <stdint.h>
extern uint32_t mock_time;
static inline uint32_t sync_timer_read32(void) { return mock_time; }
static inline uint32_t sync_timer_elapsed32(uint32_t t) { return mock_time - t; }
static inline uint16_t sync_timer_read(void) { return (uint16_t)mock_time; }
static inline uint16_t sync_timer_elapsed(uint16_t t) { return (uint16_t)(mock_time - t); }
static inline uint32_t timer_read32(void) { return mock_time; }
static inline uint16_t timer_read(void) { return (uint16_t)mock_time; }
static inline uint32_t timer_elapsed32(uint32_t t) { return mock_time - t; }
static inline uint16_t timer_elapsed(uint16_t t) { return (uint16_t)(mock_time - t); }
