#include <stdio.h>
#include <string.h>
#include <stdlib.h>
#include "rgb_matrix.h"
uint32_t mock_time;
/* newlib rand(): the C firmware's libc */
static unsigned long long rand_next = 1;
int rand(void) { rand_next = rand_next * 6364136223846793005ULL + 1; return (int)((rand_next >> 32) & 0x7fffffff); }
static uint8_t buf[RGB_MATRIX_LED_COUNT][3], frame[RGB_MATRIX_LED_COUNT][3];
static int flushed;
static void d_init(void) {}
static void d_flush(void) { memcpy(frame, buf, sizeof buf); flushed = 1; }
static void d_set(int i, uint8_t r, uint8_t g, uint8_t b) { buf[i][0]=r; buf[i][1]=g; buf[i][2]=b; }
static void d_all(uint8_t r, uint8_t g, uint8_t b) { for (int i=0;i<RGB_MATRIX_LED_COUNT;i++) d_set(i,r,g,b); }
const rgb_matrix_driver_t rgb_matrix_driver = { .init = d_init, .flush = d_flush, .set_color = d_set, .set_color_all = d_all };
#include "default_keyboard.c"
extern rgb_config_t rgb_matrix_config;
/* Render one flushed frame starting at time t. */
static void frame_at(uint32_t t) {
    mock_time = t; flushed = 0;
    for (int i = 0; i < 64 && !flushed; i++) rgb_matrix_task();
    /* sync state: next task call restarts once FLUSH_LIMIT elapsed */
}
int main(int argc, char **argv) {
    (void)argc; (void)argv;
    rgb_matrix_init();
    rgb_matrix_sethsv_noeeprom(0, 255, 255);
    int speeds[] = {0, 127, 255}; uint8_t hsvs[][3] = {{255,255,151},{0,255,255},{170,128,200},{20,0,255}};
    uint32_t times[] = {0, 16, 37, 500, 1234, 4000, 30000, 65520, 70001, 1000003};
    for (int mode = 1; mode < RGB_MATRIX_EFFECT_MAX; mode++)
    for (int si = 0; si < 3; si++) for (int hi = 0; hi < 4; hi++) {
        rgb_matrix_set_speed_noeeprom(speeds[si]);
        rgb_matrix_sethsv_noeeprom(hsvs[hi][0], hsvs[hi][1], hsvs[hi][2]);
        rgb_matrix_mode_noeeprom(mode);
        for (int ti = 0; ti < 10; ti++) {
            static const uint8_t keys[5][2] = {{3,1},{1,5},{4,9},{2,18},{5,6}};
            rgb_matrix_handle_key_event(keys[ti % 5][0], keys[ti % 5][1], true);
            frame_at(times[ti]);
            printf("%d %d %d %u", mode, speeds[si], hi, times[ti]);
            for (int i = 0; i < 100; i++) printf(" %02x%02x%02x", frame[i][0], frame[i][1], frame[i][2]);
            printf("\n");
        }
    }
    return 0;
}
