#ifndef DREAM_RT_H
#define DREAM_RT_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Matches crates/dream-mir/src/abi.rs — keep in lockstep (tested from Rust). */
enum {
    DREAM_TAG_INT = 1,
    DREAM_TAG_FLOAT = 2,
    DREAM_TAG_DOUBLE = 3,
    DREAM_TAG_BOOL = 4,
    DREAM_TAG_STRING = 5,
    DREAM_TAG_ARRAY = 6,
    DREAM_TAG_CHAR = 7,
    DREAM_TAG_LONG = 8,
    DREAM_TAG_UINT = 9,
    DREAM_TAG_ULONG = 10,
    DREAM_TAG_BYTE = 11,
    DREAM_HEAP_HEADER_SIZE = 12,
    DREAM_HEADER_TAG_OFFSET = 4,
    DREAM_HEADER_REFCOUNT_OFFSET = 8,
    DREAM_STRING_HEADER_SIZE = 8,
    DREAM_STRING_UTF8_OFFSET = 8,
    DREAM_STRING_BASE = 1024
};

void dream_rt_init(void);
uint8_t *dream_heap_base(void);
int32_t dream_heap_cap(void);
int32_t dream_malloc(int32_t size, int32_t tag);
void dream_free(int32_t ptr);
void dream_retain(int32_t ptr);
void dream_release(int32_t ptr);
void dream_retain_shared(int32_t ptr);
void dream_release_shared(int32_t ptr);

int32_t dream_load_i32(int32_t addr);
void dream_store_i32(int32_t addr, int32_t value);
int64_t dream_load_i64(int32_t addr);
void dream_store_i64(int32_t addr, int64_t value);
float dream_load_f32(int32_t addr);
void dream_store_f32(int32_t addr, float value);
double dream_load_f64(int32_t addr);
void dream_store_f64(int32_t addr, double value);
uint8_t dream_load_u8(int32_t addr);
void dream_store_u8(int32_t addr, uint8_t value);
void dream_memzero(int32_t addr, int32_t n);
void dream_memcpy(int32_t dst, int32_t src, int32_t n);
int32_t dream_realloc(int32_t ptr, int32_t new_size, int32_t tag);
int32_t dream_object_tag(int32_t ptr);
int32_t dream_i32_to_string(int32_t v);
int32_t dream_i64_to_string(int64_t v);
int32_t dream_hash_bytes(int32_t ptr);
void dream_lock_acquire(int32_t addr);
void dream_lock_release(int32_t addr);
int32_t dream_lock_try_acquire(int32_t addr);
int32_t dream_lock_try_acquire_for(int32_t addr, int32_t timeout_ms);
void dream_sem_acquire(int32_t obj);
void dream_sem_release(int32_t obj);
int32_t dream_sem_try_acquire(int32_t obj);
int32_t dream_sem_try_acquire_for(int32_t obj, int32_t timeout_ms);
void dream_unimplemented(const char *name);
int32_t dream_box_i32(int32_t v, int32_t tag);
int32_t dream_unbox_i32(int32_t ptr);
int32_t dream_box_i64(int64_t v, int32_t tag);
int64_t dream_unbox_i64(int32_t ptr);
int32_t dream_box_f32(float v);
float dream_unbox_f32(int32_t ptr);
int32_t dream_box_f64(double v);
double dream_unbox_f64(int32_t ptr);
int32_t dream_f32_to_string(float v);
int32_t dream_f64_to_string(double v);
int32_t dream_bool_to_string(int32_t v);
void dream_debug_enter(int32_t id);
void dream_debug_exit(int32_t id);
void dream_debug_line(int32_t file, int32_t line);
void dream_debug_install(void (*enter)(int32_t), void (*exit)(int32_t),
                         void (*line)(int32_t, int32_t));
void dream_debug_install_worker(void (*start)(uint32_t), void (*exit)(uint32_t));
void dream_debug_worker_start(uint32_t id);
void dream_debug_worker_exit(uint32_t id);
void dream_debug_set_print(void (*fn)(const char *, int));
void dream_debug_set_thread(int32_t id);
int32_t dream_debug_thread(void);

int32_t dream_intern_utf8(const char *bytes, int32_t len);
int32_t dream_str_byte_size(int32_t ptr);
int32_t dream_str_scalar_len(int32_t ptr);
int32_t dream_string_eq(int32_t a, int32_t b);
int32_t dream_concat_strings(int32_t a, int32_t b);
int32_t dream_char_at(int32_t ptr, int32_t index);
int32_t dream_byte_at(int32_t ptr, int32_t index);
int32_t dream_array_len(int32_t ptr);
int32_t dream_string_alloc(int32_t n);
int32_t dream_string_from_utf8(int32_t bytes);
int32_t dream_string_from_utf8_prefix(int32_t bytes, int32_t len);
void dream_string_copy_utf8(int32_t dst, int32_t dst_off, int32_t src, int32_t src_off, int32_t count);
int32_t dream_string_clone(int32_t ptr);
int32_t dream_string_compare(int32_t a, int32_t b);
int32_t dream_string_substring_raw(int32_t ptr, int32_t start, int32_t end);
void dream_string_set(int32_t ptr, int32_t i, int32_t c);
int32_t dream_utf8_decode_at(int32_t ptr, int32_t byte_off);
int32_t dream_utf8_width_at(int32_t ptr, int32_t byte_off);
void dream_simd_f32x4_add(int32_t dest, int32_t doff, int32_t a, int32_t aoff, int32_t b, int32_t boff);
void dream_simd_f32x4_sub(int32_t dest, int32_t doff, int32_t a, int32_t aoff, int32_t b, int32_t boff);
void dream_simd_f32x4_mul(int32_t dest, int32_t doff, int32_t a, int32_t aoff, int32_t b, int32_t boff);
void dream_simd_i32x4_add(int32_t dest, int32_t doff, int32_t a, int32_t aoff, int32_t b, int32_t boff);
int64_t dream_nano_time(void);
int64_t dream_now_millis(void);
int32_t dream_date_local_offset_minutes(int64_t millis);
int32_t dream_date_zone_offset_minutes(int32_t zone_ptr, int64_t millis);
int32_t dream_date_local_zone_name(void);
double dream_math_abs(double x);
double dream_math_floor(double x);
double dream_math_ceil(double x);
double dream_math_round(double x);
double dream_math_sqrt(double x);
double dream_math_pow(double a, double b);
double dream_math_sin(double x);
double dream_math_cos(double x);
double dream_math_tan(double x);
double dream_math_asin(double x);
double dream_math_acos(double x);
double dream_math_atan(double x);
double dream_math_atan2(double y, double x);
int32_t dream_file_read(int32_t path);
int64_t dream_file_write(int32_t path, int32_t content);
int64_t dream_file_append(int32_t path, int32_t content);
int32_t dream_file_read_bytes(int32_t path);
int64_t dream_file_write_bytes(int32_t path, int32_t data);
int32_t dream_file_exists(int32_t path);
int32_t dream_file_delete(int32_t path);
int64_t dream_file_size(int32_t path);
int32_t dream_file_is_dir(int32_t path);
int32_t dream_dir_list(int32_t path);
int32_t dream_dir_create(int32_t path);
int32_t dream_dir_create_all(int32_t path);
int32_t dream_process_platform(void);
int32_t dream_process_os_family(void);
int32_t dream_process_cwd(void);
int32_t dream_process_set_cwd(int32_t path);
int32_t dream_process_args(void);
int32_t dream_process_env_get(int32_t key);
void dream_process_env_set(int32_t key, int32_t val);
int32_t dream_process_exe_path(void);
void dream_console_exit(int32_t code);
int32_t dream_console_read_line(void);
int32_t dream_console_read_key(void);
void dream_delay_ms(int32_t ms);
void dream_rt_set_args(int argc, char **argv);

void dream_print_int(int32_t v);
void dream_print_uint(int32_t v);
void dream_print_long(int64_t v);
void dream_print_ulong(int64_t v);
void dream_print_float(float v);
void dream_print_double(double v);
void dream_print_char(int32_t c);
void dream_print_string(int32_t ptr);
void dream_print_newline(void);
void dream_panic(int32_t msg_ptr);

void dream_weak_register(int32_t ptr);
void dream_weak_clear_all(int32_t obj);

void dream_async_enqueue(int32_t future);
int32_t dream_async_run(void);

void dream_user_main(void);

int32_t debug_get_live_objects(void);
int32_t debug_get_total_allocations(void);
int32_t debug_get_heap_ptr(void);
int32_t debug_get_free_list_head(void);
int32_t debug_get_ref_count(int32_t ptr);
void dream_drop(int32_t ptr);

#ifdef __cplusplus
}
#endif

#endif
