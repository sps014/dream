#ifndef DREAM_RT_H
#define DREAM_RT_H

#include "dream_abi.h"
#include <stddef.h>
#include <stdint.h>

#define EXPORT(name) __attribute__((export_name(name)))
#define IMPORT(name) __attribute__((import_module("env"), import_name(name)))

/* Per-program interned strings; defined by the Dream emitter before splice. */
extern int32_t __rt_str_empty;
extern int32_t __rt_str_true;
extern int32_t __rt_str_false;
extern int32_t __rt_str_minus;

extern int32_t free_list_head;
extern int32_t live_objects;
extern int32_t total_allocations;
extern int32_t weak_list_head;

static inline int32_t i32_load(int32_t addr) {
    return *(int32_t *)(uintptr_t)(uint32_t)addr;
}
static inline void i32_store(int32_t addr, int32_t v) {
    *(int32_t *)(uintptr_t)(uint32_t)addr = v;
}
static inline int64_t i64_load(int32_t addr) {
    return *(int64_t *)(uintptr_t)(uint32_t)addr;
}
static inline void i64_store(int32_t addr, int64_t v) {
    *(int64_t *)(uintptr_t)(uint32_t)addr = v;
}
static inline float f32_load(int32_t addr) {
    return *(float *)(uintptr_t)(uint32_t)addr;
}
static inline void f32_store(int32_t addr, float v) {
    *(float *)(uintptr_t)(uint32_t)addr = v;
}
static inline double f64_load(int32_t addr) {
    return *(double *)(uintptr_t)(uint32_t)addr;
}
static inline void f64_store(int32_t addr, double v) {
    *(double *)(uintptr_t)(uint32_t)addr = v;
}
static inline uint8_t u8_load(int32_t addr) {
    return *(uint8_t *)(uintptr_t)(uint32_t)addr;
}
static inline void u8_store(int32_t addr, uint8_t v) {
    *(uint8_t *)(uintptr_t)(uint32_t)addr = v;
}
static inline uint16_t u16_load(int32_t addr) {
    return *(uint16_t *)(uintptr_t)(uint32_t)addr;
}
static inline void u16_store(int32_t addr, uint16_t v) {
    *(uint16_t *)(uintptr_t)(uint32_t)addr = v;
}

static inline void mem_copy(int32_t dst, int32_t src, int32_t n) {
    int32_t i = 0;
    while (i < n) {
        u8_store(dst + i, u8_load(src + i));
        i = i + 1;
    }
}

#ifdef __wasm__
static inline int32_t wasm_memory_size(void) {
    return (int32_t)__builtin_wasm_memory_size(0);
}
static inline int32_t wasm_memory_grow(int32_t delta) {
    return (int32_t)__builtin_wasm_memory_grow(0, (size_t)delta);
}
#endif

static inline int32_t atomic_load_i32(int32_t addr) {
    return __c11_atomic_load((_Atomic int32_t *)(uintptr_t)(uint32_t)addr, __ATOMIC_SEQ_CST);
}
static inline void atomic_store_i32(int32_t addr, int32_t v) {
    __c11_atomic_store((_Atomic int32_t *)(uintptr_t)(uint32_t)addr, v, __ATOMIC_SEQ_CST);
}
static inline int32_t atomic_fetch_add_i32(int32_t addr, int32_t v) {
    return __c11_atomic_fetch_add((_Atomic int32_t *)(uintptr_t)(uint32_t)addr, v,
                                  __ATOMIC_SEQ_CST);
}
static inline int32_t atomic_fetch_sub_i32(int32_t addr, int32_t v) {
    return __c11_atomic_fetch_sub((_Atomic int32_t *)(uintptr_t)(uint32_t)addr, v,
                                  __ATOMIC_SEQ_CST);
}

#endif
