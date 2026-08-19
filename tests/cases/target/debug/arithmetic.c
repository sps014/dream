#include "dream_rt_native.h"
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef dream_ptr (*dream_fn_ptr__ptr)(dream_ptr);
typedef int32_t (*dream_fn_ptr_i32__i32)(dream_ptr, int32_t);
typedef int32_t (*dream_fn_ptr_i64__i32)(dream_ptr, int64_t);
typedef int32_t (*dream_fn_ptr_f32__i32)(dream_ptr, float);
typedef int32_t (*dream_fn_ptr_f64__i32)(dream_ptr, double);
typedef int32_t (*dream_fn_ptr_ptr__i32)(dream_ptr, dream_ptr);
typedef int32_t (*dream_fn_ptr__i32)(dream_ptr);
typedef dream_ptr (*dream_fn_ptr_ptr__ptr)(dream_ptr, dream_ptr);
typedef void (*dream_fn_ptr_ptr__v)(dream_ptr, dream_ptr);
typedef dream_ptr (*dream_fn_ptr_i32__ptr)(dream_ptr, int32_t);
static struct { int32_t size; int32_t header_pad; int32_t tag; int32_t rc; int32_t len; int32_t pad; uint16_t u[4]; } __ds0_blk = {0, 0, TAG_STRING, INT32_MAX, 4, 0, {110, 117, 108, 108}};
static const dream_ptr __ds0 = (dream_ptr)((char *)&__ds0_blk + 16);
static struct { int32_t size; int32_t header_pad; int32_t tag; int32_t rc; int32_t len; int32_t pad; uint16_t u[8]; } __ds1_blk = {0, 0, TAG_STRING, INT32_MAX, 8, 0, {60, 111, 98, 106, 101, 99, 116, 62}};
static const dream_ptr __ds1 = (dream_ptr)((char *)&__ds1_blk + 16);
static struct { int32_t size; int32_t header_pad; int32_t tag; int32_t rc; int32_t len; int32_t pad; uint16_t u[1]; } __ds2_blk = {0, 0, TAG_STRING, INT32_MAX, 1, 0, {91}};
static const dream_ptr __ds2 = (dream_ptr)((char *)&__ds2_blk + 16);
static struct { int32_t size; int32_t header_pad; int32_t tag; int32_t rc; int32_t len; int32_t pad; uint16_t u[1]; } __ds3_blk = {0, 0, TAG_STRING, INT32_MAX, 1, 0, {93}};
static const dream_ptr __ds3 = (dream_ptr)((char *)&__ds3_blk + 16);
static struct { int32_t size; int32_t header_pad; int32_t tag; int32_t rc; int32_t len; int32_t pad; uint16_t u[2]; } __ds4_blk = {0, 0, TAG_STRING, INT32_MAX, 2, 0, {44, 32}};
static const dream_ptr __ds4 = (dream_ptr)((char *)&__ds4_blk + 16);
static struct { int32_t size; int32_t header_pad; int32_t tag; int32_t rc; int32_t len; int32_t pad; uint16_t u[1]; } __ds5_blk = {0, 0, TAG_STRING, INT32_MAX, 1, 0, {40}};
static const dream_ptr __ds5 = (dream_ptr)((char *)&__ds5_blk + 16);
static struct { int32_t size; int32_t header_pad; int32_t tag; int32_t rc; int32_t len; int32_t pad; uint16_t u[1]; } __ds6_blk = {0, 0, TAG_STRING, INT32_MAX, 1, 0, {41}};
static const dream_ptr __ds6 = (dream_ptr)((char *)&__ds6_blk + 16);
static struct { int32_t size; int32_t header_pad; int32_t tag; int32_t rc; int32_t len; int32_t pad; uint16_t u[4]; } __ds7_blk = {0, 0, TAG_STRING, INT32_MAX, 4, 0, {116, 114, 117, 101}};
static const dream_ptr __ds7 = (dream_ptr)((char *)&__ds7_blk + 16);
static struct { int32_t size; int32_t header_pad; int32_t tag; int32_t rc; int32_t len; int32_t pad; uint16_t u[5]; } __ds8_blk = {0, 0, TAG_STRING, INT32_MAX, 5, 0, {102, 97, 108, 115, 101}};
static const dream_ptr __ds8 = (dream_ptr)((char *)&__ds8_blk + 16);
static struct { int32_t size; int32_t header_pad; int32_t tag; int32_t rc; int32_t len; int32_t pad; uint16_t u[1]; } __ds9_blk = {0, 0, TAG_STRING, INT32_MAX, 1, 0, {45}};
static const dream_ptr __ds9 = (dream_ptr)((char *)&__ds9_blk + 16);
static struct { int32_t size; int32_t header_pad; int32_t tag; int32_t rc; int32_t len; int32_t pad; uint16_t u[1]; } __ds10_blk = {0, 0, TAG_STRING, INT32_MAX, 0, 0, {0}};
static const dream_ptr __ds10 = (dream_ptr)((char *)&__ds10_blk + 16);
static struct { int32_t size; int32_t header_pad; int32_t tag; int32_t rc; int32_t len; int32_t pad; uint16_t u[26]; } __ds11_blk = {0, 0, TAG_STRING, INT32_MAX, 26, 0, {112, 97, 110, 105, 99, 58, 32, 105, 110, 100, 101, 120, 32, 111, 117, 116, 32, 111, 102, 32, 98, 111, 117, 110, 100, 115}};
static const dream_ptr __ds11 = (dream_ptr)((char *)&__ds11_blk + 16);
static struct { int32_t size; int32_t header_pad; int32_t tag; int32_t rc; int32_t len; int32_t pad; uint16_t u[32]; } __ds12_blk = {0, 0, TAG_STRING, INT32_MAX, 32, 0, {112, 97, 110, 105, 99, 58, 32, 97, 116, 116, 101, 109, 112, 116, 32, 116, 111, 32, 100, 105, 118, 105, 100, 101, 32, 98, 121, 32, 122, 101, 114, 111}};
static const dream_ptr __ds12 = (dream_ptr)((char *)&__ds12_blk + 16);
static struct { int32_t size; int32_t header_pad; int32_t tag; int32_t rc; int32_t len; int32_t pad; uint16_t u[19]; } __ds13_blk = {0, 0, TAG_STRING, INT32_MAX, 19, 0, {112, 97, 110, 105, 99, 58, 32, 105, 110, 118, 97, 108, 105, 100, 32, 99, 97, 115, 116}};
static const dream_ptr __ds13 = (dream_ptr)((char *)&__ds13_blk + 16);
static struct { int32_t size; int32_t header_pad; int32_t tag; int32_t rc; int32_t len; int32_t pad; uint16_t u[48]; } __ds14_blk = {0, 0, TAG_STRING, INT32_MAX, 48, 0, {112, 97, 110, 105, 99, 58, 32, 97, 99, 99, 101, 115, 115, 32, 116, 111, 32, 100, 101, 97, 108, 108, 111, 99, 97, 116, 101, 100, 32, 39, 117, 110, 111, 119, 110, 101, 100, 39, 32, 114, 101, 102, 101, 114, 101, 110, 99, 101}};
static const dream_ptr __ds14 = (dream_ptr)((char *)&__ds14_blk + 16);
_Thread_local dream_ptr g0 = 0;
static void * dream_ft[2];
void main_dream(void);
static void destroy_object(dream_ptr p);
static void destroy_object(dream_ptr p) {
  if (!p) return;
  int32_t tag = dream_object_tag(p);
  switch (tag) {
    case TAG_STRING:
      dream_release(p);
      return;
    default: dream_release(p);
  }
}

dream_ptr dream_object_to_string(dream_ptr p) {
  int32_t tag;
  if (!p) return __ds0;
  tag = dream_object_tag(p);
  switch (tag) {
    case TAG_INT: return dream_int_to_string(*(int32_t *)dream_p(p));
    case TAG_UINT: return dream_uint_to_string(*(int32_t *)dream_p(p));
    case TAG_LONG: return dream_long_to_string(*(int64_t *)dream_p(p));
    case TAG_ULONG: return dream_ulong_to_string(*(int64_t *)dream_p(p));
    case TAG_BYTE: return dream_byte_to_string(*(int32_t *)dream_p(p));
    case TAG_BOOL: return dream_bool_to_string(*(int32_t *)dream_p(p));
    case TAG_CHAR: return dream_char_to_string(*(int32_t *)dream_p(p));
    case TAG_FLOAT: return dream_float_to_string(*(float *)dream_p(p));
    case TAG_DOUBLE: return dream_double_to_string(*(double *)dream_p(p));
    case TAG_STRING:
      dream_retain(p);
      return p;
    case TAG_ARRAY: return dream_array_to_string(p);
    default: return __ds1;
  }
}

void dream_print_object(dream_ptr p) {
  print_string(dream_object_to_string(p));
}

int32_t dream_object_hash_code(dream_ptr p) {
  int32_t tag;
  if (!p) return 0;
  tag = dream_object_tag(p);
  switch (tag) {
    case TAG_INT:
    case TAG_UINT:
    case TAG_BOOL:
    case TAG_CHAR:
    case TAG_BYTE: return *(int32_t *)dream_p(p);
    case TAG_LONG:
    case TAG_ULONG: return dream_hash_long(*(int64_t *)dream_p(p));
    case TAG_FLOAT: return dream_bitcast_f32(*(float *)dream_p(p));
    case TAG_DOUBLE: return dream_hash_double(*(double *)dream_p(p));
    case TAG_STRING: return dream_string_hash(p);
    default: return (int32_t)(uintptr_t)p;
  }
}

static void * dream_iface_0_0[13];
dream_ptr __iface_dispatch_0_0(dream_ptr this) {
  int32_t tag = dream_object_tag(this);
  dream_fn_ptr__ptr fn = (dream_fn_ptr__ptr)dream_iface_0_0[tag];
  if (!fn) abort();
  return (fn)(this);
}

static void * dream_iface_0_1[13];
dream_ptr __iface_dispatch_0_1(dream_ptr this) {
  int32_t tag = dream_object_tag(this);
  dream_fn_ptr__ptr fn = (dream_fn_ptr__ptr)dream_iface_0_1[tag];
  if (!fn) abort();
  return (fn)(this);
}

static void * dream_iface_1_0[13];
dream_ptr __iface_dispatch_1_0(dream_ptr this) {
  int32_t tag = dream_object_tag(this);
  dream_fn_ptr__ptr fn = (dream_fn_ptr__ptr)dream_iface_1_0[tag];
  if (!fn) abort();
  return (fn)(this);
}

static void * dream_iface_2_0[13];
int32_t __iface_dispatch_2_0(dream_ptr this, int32_t a0) {
  int32_t tag = dream_object_tag(this);
  dream_fn_ptr_i32__i32 fn = (dream_fn_ptr_i32__i32)dream_iface_2_0[tag];
  if (!fn) abort();
  return (fn)(this, a0);
}

static void * dream_iface_3_0[13];
int32_t __iface_dispatch_3_0(dream_ptr this, int64_t a0) {
  int32_t tag = dream_object_tag(this);
  dream_fn_ptr_i64__i32 fn = (dream_fn_ptr_i64__i32)dream_iface_3_0[tag];
  if (!fn) abort();
  return (fn)(this, a0);
}

static void * dream_iface_4_0[13];
int32_t __iface_dispatch_4_0(dream_ptr this, int32_t a0) {
  int32_t tag = dream_object_tag(this);
  dream_fn_ptr_i32__i32 fn = (dream_fn_ptr_i32__i32)dream_iface_4_0[tag];
  if (!fn) abort();
  return (fn)(this, a0);
}

static void * dream_iface_5_0[13];
int32_t __iface_dispatch_5_0(dream_ptr this, int64_t a0) {
  int32_t tag = dream_object_tag(this);
  dream_fn_ptr_i64__i32 fn = (dream_fn_ptr_i64__i32)dream_iface_5_0[tag];
  if (!fn) abort();
  return (fn)(this, a0);
}

static void * dream_iface_6_0[13];
int32_t __iface_dispatch_6_0(dream_ptr this, int32_t a0) {
  int32_t tag = dream_object_tag(this);
  dream_fn_ptr_i32__i32 fn = (dream_fn_ptr_i32__i32)dream_iface_6_0[tag];
  if (!fn) abort();
  return (fn)(this, a0);
}

static void * dream_iface_7_0[13];
int32_t __iface_dispatch_7_0(dream_ptr this, int32_t a0) {
  int32_t tag = dream_object_tag(this);
  dream_fn_ptr_i32__i32 fn = (dream_fn_ptr_i32__i32)dream_iface_7_0[tag];
  if (!fn) abort();
  return (fn)(this, a0);
}

static void * dream_iface_8_0[13];
int32_t __iface_dispatch_8_0(dream_ptr this, float a0) {
  int32_t tag = dream_object_tag(this);
  dream_fn_ptr_f32__i32 fn = (dream_fn_ptr_f32__i32)dream_iface_8_0[tag];
  if (!fn) abort();
  return (fn)(this, a0);
}

static void * dream_iface_9_0[13];
int32_t __iface_dispatch_9_0(dream_ptr this, double a0) {
  int32_t tag = dream_object_tag(this);
  dream_fn_ptr_f64__i32 fn = (dream_fn_ptr_f64__i32)dream_iface_9_0[tag];
  if (!fn) abort();
  return (fn)(this, a0);
}

static void * dream_iface_10_0[13];
int32_t __iface_dispatch_10_0(dream_ptr this, dream_ptr a0) {
  int32_t tag = dream_object_tag(this);
  dream_fn_ptr_ptr__i32 fn = (dream_fn_ptr_ptr__i32)dream_iface_10_0[tag];
  if (!fn) abort();
  return (fn)(this, a0);
}

static void * dream_iface_11_0[13];
int32_t __iface_dispatch_11_0(dream_ptr this) {
  int32_t tag = dream_object_tag(this);
  dream_fn_ptr__i32 fn = (dream_fn_ptr__i32)dream_iface_11_0[tag];
  if (!fn) abort();
  return (fn)(this);
}

static void * dream_iface_11_1[13];
dream_ptr __iface_dispatch_11_1(dream_ptr this) {
  int32_t tag = dream_object_tag(this);
  dream_fn_ptr__ptr fn = (dream_fn_ptr__ptr)dream_iface_11_1[tag];
  if (!fn) abort();
  return (fn)(this);
}

static void * dream_iface_11_2[13];
int32_t __iface_dispatch_11_2(dream_ptr this) {
  int32_t tag = dream_object_tag(this);
  dream_fn_ptr__i32 fn = (dream_fn_ptr__i32)dream_iface_11_2[tag];
  if (!fn) abort();
  return (fn)(this);
}

static void * dream_iface_11_3[13];
int32_t __iface_dispatch_11_3(dream_ptr this, dream_ptr a0) {
  int32_t tag = dream_object_tag(this);
  dream_fn_ptr_ptr__i32 fn = (dream_fn_ptr_ptr__i32)dream_iface_11_3[tag];
  if (!fn) abort();
  return (fn)(this, a0);
}

static void * dream_iface_11_4[13];
int32_t __iface_dispatch_11_4(dream_ptr this, dream_ptr a0) {
  int32_t tag = dream_object_tag(this);
  dream_fn_ptr_ptr__i32 fn = (dream_fn_ptr_ptr__i32)dream_iface_11_4[tag];
  if (!fn) abort();
  return (fn)(this, a0);
}

static void * dream_iface_11_5[13];
int32_t __iface_dispatch_11_5(dream_ptr this, dream_ptr a0) {
  int32_t tag = dream_object_tag(this);
  dream_fn_ptr_ptr__i32 fn = (dream_fn_ptr_ptr__i32)dream_iface_11_5[tag];
  if (!fn) abort();
  return (fn)(this, a0);
}

static void * dream_iface_11_6[13];
int32_t __iface_dispatch_11_6(dream_ptr this, dream_ptr a0) {
  int32_t tag = dream_object_tag(this);
  dream_fn_ptr_ptr__i32 fn = (dream_fn_ptr_ptr__i32)dream_iface_11_6[tag];
  if (!fn) abort();
  return (fn)(this, a0);
}

static void * dream_iface_11_7[13];
dream_ptr __iface_dispatch_11_7(dream_ptr this, dream_ptr a0) {
  int32_t tag = dream_object_tag(this);
  dream_fn_ptr_ptr__ptr fn = (dream_fn_ptr_ptr__ptr)dream_iface_11_7[tag];
  if (!fn) abort();
  return (fn)(this, a0);
}

static void * dream_iface_11_8[13];
void __iface_dispatch_11_8(dream_ptr this, dream_ptr a0) {
  int32_t tag = dream_object_tag(this);
  dream_fn_ptr_ptr__v fn = (dream_fn_ptr_ptr__v)dream_iface_11_8[tag];
  if (!fn) abort();
  (fn)(this, a0);
  return;
}

static void * dream_iface_12_0[13];
int32_t __iface_dispatch_12_0(dream_ptr this) {
  int32_t tag = dream_object_tag(this);
  dream_fn_ptr__i32 fn = (dream_fn_ptr__i32)dream_iface_12_0[tag];
  if (!fn) abort();
  return (fn)(this);
}

static void * dream_iface_12_1[13];
dream_ptr __iface_dispatch_12_1(dream_ptr this) {
  int32_t tag = dream_object_tag(this);
  dream_fn_ptr__ptr fn = (dream_fn_ptr__ptr)dream_iface_12_1[tag];
  if (!fn) abort();
  return (fn)(this);
}

static void * dream_iface_12_2[13];
int32_t __iface_dispatch_12_2(dream_ptr this) {
  int32_t tag = dream_object_tag(this);
  dream_fn_ptr__i32 fn = (dream_fn_ptr__i32)dream_iface_12_2[tag];
  if (!fn) abort();
  return (fn)(this);
}

static void * dream_iface_12_3[13];
int32_t __iface_dispatch_12_3(dream_ptr this, dream_ptr a0) {
  int32_t tag = dream_object_tag(this);
  dream_fn_ptr_ptr__i32 fn = (dream_fn_ptr_ptr__i32)dream_iface_12_3[tag];
  if (!fn) abort();
  return (fn)(this, a0);
}

static void * dream_iface_12_4[13];
int32_t __iface_dispatch_12_4(dream_ptr this, dream_ptr a0) {
  int32_t tag = dream_object_tag(this);
  dream_fn_ptr_ptr__i32 fn = (dream_fn_ptr_ptr__i32)dream_iface_12_4[tag];
  if (!fn) abort();
  return (fn)(this, a0);
}

static void * dream_iface_12_5[13];
int32_t __iface_dispatch_12_5(dream_ptr this, dream_ptr a0) {
  int32_t tag = dream_object_tag(this);
  dream_fn_ptr_ptr__i32 fn = (dream_fn_ptr_ptr__i32)dream_iface_12_5[tag];
  if (!fn) abort();
  return (fn)(this, a0);
}

static void * dream_iface_12_6[13];
int32_t __iface_dispatch_12_6(dream_ptr this, dream_ptr a0) {
  int32_t tag = dream_object_tag(this);
  dream_fn_ptr_ptr__i32 fn = (dream_fn_ptr_ptr__i32)dream_iface_12_6[tag];
  if (!fn) abort();
  return (fn)(this, a0);
}

static void * dream_iface_12_7[13];
dream_ptr __iface_dispatch_12_7(dream_ptr this, dream_ptr a0) {
  int32_t tag = dream_object_tag(this);
  dream_fn_ptr_ptr__ptr fn = (dream_fn_ptr_ptr__ptr)dream_iface_12_7[tag];
  if (!fn) abort();
  return (fn)(this, a0);
}

static void * dream_iface_12_8[13];
void __iface_dispatch_12_8(dream_ptr this, dream_ptr a0) {
  int32_t tag = dream_object_tag(this);
  dream_fn_ptr_ptr__v fn = (dream_fn_ptr_ptr__v)dream_iface_12_8[tag];
  if (!fn) abort();
  (fn)(this, a0);
  return;
}

static void * dream_iface_12_9[13];
dream_ptr __iface_dispatch_12_9(dream_ptr this, int32_t a0) {
  int32_t tag = dream_object_tag(this);
  dream_fn_ptr_i32__ptr fn = (dream_fn_ptr_i32__ptr)dream_iface_12_9[tag];
  if (!fn) abort();
  return (fn)(this, a0);
}

static void * dream_iface_12_10[13];
dream_ptr __iface_dispatch_12_10(dream_ptr this) {
  int32_t tag = dream_object_tag(this);
  dream_fn_ptr__ptr fn = (dream_fn_ptr__ptr)dream_iface_12_10[tag];
  if (!fn) abort();
  return (fn)(this);
}

static void * dream_iface_12_11[13];
dream_ptr __iface_dispatch_12_11(dream_ptr this) {
  int32_t tag = dream_object_tag(this);
  dream_fn_ptr__ptr fn = (dream_fn_ptr__ptr)dream_iface_12_11[tag];
  if (!fn) abort();
  return (fn)(this);
}

static void * dream_iface_13_0[13];
dream_ptr __iface_dispatch_13_0(dream_ptr this) {
  int32_t tag = dream_object_tag(this);
  dream_fn_ptr__ptr fn = (dream_fn_ptr__ptr)dream_iface_13_0[tag];
  if (!fn) abort();
  return (fn)(this);
}

void main_dream(void) {
  int64_t l0 = 0;
  int64_t l1 = 0;
  float l2 = 0;
  float l3 = 0;
  int64_t l4 = 0;
  int64_t l5 = 0;
  int64_t l6 = 0;
  int64_t l7 = 0;
  int64_t l8 = 0;
  float l9 = 0;
  float l10 = 0;
  int64_t l11 = 0;
  int64_t l12 = 0;
  int64_t l13 = 0;
  int64_t l14 = 0;
  goto L0;
L0:;
  print_int((int32_t)13);
  print_char(10);
  print_int((int32_t)7);
  print_char(10);
  print_int((int32_t)30);
  print_char(10);
  print_int((int32_t)3);
  print_char(10);
  print_int((int32_t)1);
  print_char(10);
  print_float((float)3.5f);
  print_char(10);
  print_float((float)3.0f);
  print_char(10);
  print_int((int32_t)7);
  print_char(10);
  print_int((int32_t)9);
  print_char(10);
  return;
}

static void dream_init_ft(void) {
  dream_ft[1] = (void *)main_dream;
}

void * dream_ft_get(int32_t i) {
  return i > 0 && i < 2 ? dream_ft[i] : 0;
}

static void dream_init_itables(void) {
}

dream_ptr dream_worker_invoke(int32_t fn, dream_ptr env, dream_ptr arg) {
  if (fn <= 0) return 0;
  g0 = env;
  dream_ptr result = (((dream_fn_ptr__ptr)dream_ft[fn]))(arg);
  return result;
}

int32_t dream_guest_entry(void) {
  dream_init_ft();
  dream_init_itables();
  dream_host_bind(dream_string_alloc, dream_array_new);
  main_dream();
  return 0;
}

int32_t main(void) {
  return dream_guest_entry();
}

