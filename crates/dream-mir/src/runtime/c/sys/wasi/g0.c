#include "dream_rt_wasm32.h"

int32_t dream_tid_get(void);
void dream_tid_set(int32_t v);
int32_t dream_priv_fl0_get(void);
int32_t dream_priv_fl1_get(void);
int32_t dream_priv_fl2_get(void);
int32_t dream_priv_fl3_get(void);
int32_t dream_priv_fl4_get(void);
int32_t dream_priv_fl5_get(void);
int32_t dream_priv_fl6_get(void);
int32_t dream_priv_fl7_get(void);
int32_t dream_priv_fl8_get(void);
int32_t dream_priv_fl9_get(void);
int32_t dream_priv_fl10_get(void);
int32_t dream_priv_fl11_get(void);
int32_t dream_priv_fl12_get(void);
void dream_priv_fl0_set(int32_t v);
void dream_priv_fl1_set(int32_t v);
void dream_priv_fl2_set(int32_t v);
void dream_priv_fl3_set(int32_t v);
void dream_priv_fl4_set(int32_t v);
void dream_priv_fl5_set(int32_t v);
void dream_priv_fl6_set(int32_t v);
void dream_priv_fl7_set(int32_t v);
void dream_priv_fl8_set(int32_t v);
void dream_priv_fl9_set(int32_t v);
void dream_priv_fl10_set(int32_t v);
void dream_priv_fl11_set(int32_t v);
void dream_priv_fl12_set(int32_t v);

int32_t dream_priv_fl_get(int32_t idx) {
    switch (idx) {
    case 0:
        return dream_priv_fl0_get();
    case 1:
        return dream_priv_fl1_get();
    case 2:
        return dream_priv_fl2_get();
    case 3:
        return dream_priv_fl3_get();
    case 4:
        return dream_priv_fl4_get();
    case 5:
        return dream_priv_fl5_get();
    case 6:
        return dream_priv_fl6_get();
    case 7:
        return dream_priv_fl7_get();
    case 8:
        return dream_priv_fl8_get();
    case 9:
        return dream_priv_fl9_get();
    case 10:
        return dream_priv_fl10_get();
    case 11:
        return dream_priv_fl11_get();
    default:
        return dream_priv_fl12_get();
    }
}

void dream_priv_fl_set(int32_t idx, int32_t v) {
    switch (idx) {
    case 0:
        dream_priv_fl0_set(v);
        return;
    case 1:
        dream_priv_fl1_set(v);
        return;
    case 2:
        dream_priv_fl2_set(v);
        return;
    case 3:
        dream_priv_fl3_set(v);
        return;
    case 4:
        dream_priv_fl4_set(v);
        return;
    case 5:
        dream_priv_fl5_set(v);
        return;
    case 6:
        dream_priv_fl6_set(v);
        return;
    case 7:
        dream_priv_fl7_set(v);
        return;
    case 8:
        dream_priv_fl8_set(v);
        return;
    case 9:
        dream_priv_fl9_set(v);
        return;
    case 10:
        dream_priv_fl10_set(v);
        return;
    case 11:
        dream_priv_fl11_set(v);
        return;
    default:
        dream_priv_fl12_set(v);
    }
}

int32_t dream_instance_tid(void) {
    int32_t t = dream_tid_get();
    if (t == 0) {
        t = dream_next_tid();
        dream_tid_set(t);
    }
    return t;
}
