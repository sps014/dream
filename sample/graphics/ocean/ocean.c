#include "dream_rt_native.h"
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[45]; } __ds0_blk = {
  0, 0, TAG_STRING, INT32_MAX, 45, 0, { 87, 101, 98, 71, 80, 85, 32, 117, 110, 97, 118, 97, 105, 108, 97, 98, 108, 101, 32, 8212, 32, 111, 99, 101, 97, 110, 32, 110, 101, 101, 100, 115, 32, 71, 112, 117, 46, 116, 114, 121, 95, 105, 110, 105, 116 }
};
static const dream_ptr __ds0 = (dream_ptr)((char *)&__ds0_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[5]; } __ds1_blk = {
  0, 0, TAG_STRING, INT32_MAX, 5, 0, { 111, 99, 101, 97, 110 }
};
static const dream_ptr __ds1 = (dream_ptr)((char *)&__ds1_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[24]; } __ds2_blk = {
  0, 0, TAG_STRING, INT32_MAX, 24, 0, { 71, 112, 117, 83, 117, 114, 102, 97, 99, 101, 46, 99, 114, 101, 97, 116, 101, 32, 102, 97, 105, 108, 101, 100 }
};
static const dream_ptr __ds2 = (dream_ptr)((char *)&__ds2_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[6]; } __ds3_blk = {
  0, 0, TAG_STRING, INT32_MAX, 6, 0, { 115, 101, 97, 95, 118, 115 }
};
static const dream_ptr __ds3 = (dream_ptr)((char *)&__ds3_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[6]; } __ds4_blk = {
  0, 0, TAG_STRING, INT32_MAX, 6, 0, { 115, 101, 97, 95, 102, 115 }
};
static const dream_ptr __ds4 = (dream_ptr)((char *)&__ds4_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[34]; } __ds5_blk = {
  0, 0, TAG_STRING, INT32_MAX, 34, 0, { 71, 112, 117, 82, 101, 110, 100, 101, 114, 80, 105, 112, 101, 108, 105, 110, 101, 46, 99, 114, 101, 97, 116, 101, 95, 101, 120, 32, 102, 97, 105, 108, 101, 100 }
};
static const dream_ptr __ds5 = (dream_ptr)((char *)&__ds5_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[2]; } __ds6_blk = {
  0, 0, TAG_STRING, INT32_MAX, 2, 0, { 58, 32 }
};
static const dream_ptr __ds6 = (dream_ptr)((char *)&__ds6_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[11]; } __ds7_blk = {
  0, 0, TAG_STRING, INT32_MAX, 11, 0, { 85, 78, 65, 86, 65, 73, 76, 65, 66, 76, 69 }
};
static const dream_ptr __ds7 = (dream_ptr)((char *)&__ds7_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[7]; } __ds8_blk = {
  0, 0, TAG_STRING, INT32_MAX, 7, 0, { 84, 73, 77, 69, 79, 85, 84 }
};
static const dream_ptr __ds8 = (dream_ptr)((char *)&__ds8_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[10]; } __ds9_blk = {
  0, 0, TAG_STRING, INT32_MAX, 10, 0, { 86, 65, 76, 73, 68, 65, 84, 73, 79, 78 }
};
static const dream_ptr __ds9 = (dream_ptr)((char *)&__ds9_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[3]; } __ds10_blk = {
  0, 0, TAG_STRING, INT32_MAX, 3, 0, { 69, 73, 79 }
};
static const dream_ptr __ds10 = (dream_ptr)((char *)&__ds10_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[29]; } __ds11_blk = {
  0, 0, TAG_STRING, INT32_MAX, 29, 0, { 71, 112, 117, 83, 117, 114, 102, 97, 99, 101, 46, 102, 114, 111, 109, 95, 99, 97, 110, 118, 97, 115, 32, 102, 97, 105, 108, 101, 100 }
};
static const dream_ptr __ds11 = (dream_ptr)((char *)&__ds11_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[22]; } __ds12_blk = {
  0, 0, TAG_STRING, INT32_MAX, 22, 0, { 115, 117, 114, 102, 97, 99, 101, 32, 112, 114, 101, 115, 101, 110, 116, 32, 102, 97, 105, 108, 101, 100 }
};
static const dream_ptr __ds12 = (dream_ptr)((char *)&__ds12_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[31]; } __ds13_blk = {
  0, 0, TAG_STRING, INT32_MAX, 31, 0, { 71, 112, 117, 82, 101, 110, 100, 101, 114, 80, 105, 112, 101, 108, 105, 110, 101, 46, 99, 114, 101, 97, 116, 101, 32, 102, 97, 105, 108, 101, 100 }
};
static const dream_ptr __ds13 = (dream_ptr)((char *)&__ds13_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[19]; } __ds14_blk = {
  0, 0, TAG_STRING, INT32_MAX, 19, 0, { 71, 112, 117, 46, 116, 114, 121, 95, 105, 110, 105, 116, 32, 102, 97, 105, 108, 101, 100 }
};
static const dream_ptr __ds14 = (dream_ptr)((char *)&__ds14_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[11]; } __ds15_blk = {
  0, 0, TAG_STRING, INT32_MAX, 11, 0, { 100, 114, 97, 119, 32, 102, 97, 105, 108, 101, 100 }
};
static const dream_ptr __ds15 = (dream_ptr)((char *)&__ds15_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[4]; } __ds16_blk = {
  0, 0, TAG_STRING, INT32_MAX, 4, 0, { 110, 117, 108, 108 }
};
static const dream_ptr __ds16 = (dream_ptr)((char *)&__ds16_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[8]; } __ds17_blk = {
  0, 0, TAG_STRING, INT32_MAX, 8, 0, { 60, 111, 98, 106, 101, 99, 116, 62 }
};
static const dream_ptr __ds17 = (dream_ptr)((char *)&__ds17_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[1]; } __ds18_blk = {
  0, 0, TAG_STRING, INT32_MAX, 1, 0, { 91 }
};
static const dream_ptr __ds18 = (dream_ptr)((char *)&__ds18_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[1]; } __ds19_blk = {
  0, 0, TAG_STRING, INT32_MAX, 1, 0, { 93 }
};
static const dream_ptr __ds19 = (dream_ptr)((char *)&__ds19_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[2]; } __ds20_blk = {
  0, 0, TAG_STRING, INT32_MAX, 2, 0, { 44, 32 }
};
static const dream_ptr __ds20 = (dream_ptr)((char *)&__ds20_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[1]; } __ds21_blk = {
  0, 0, TAG_STRING, INT32_MAX, 1, 0, { 40 }
};
static const dream_ptr __ds21 = (dream_ptr)((char *)&__ds21_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[1]; } __ds22_blk = {
  0, 0, TAG_STRING, INT32_MAX, 1, 0, { 41 }
};
static const dream_ptr __ds22 = (dream_ptr)((char *)&__ds22_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[4]; } __ds23_blk = {
  0, 0, TAG_STRING, INT32_MAX, 4, 0, { 116, 114, 117, 101 }
};
static const dream_ptr __ds23 = (dream_ptr)((char *)&__ds23_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[5]; } __ds24_blk = {
  0, 0, TAG_STRING, INT32_MAX, 5, 0, { 102, 97, 108, 115, 101 }
};
static const dream_ptr __ds24 = (dream_ptr)((char *)&__ds24_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[1]; } __ds25_blk = {
  0, 0, TAG_STRING, INT32_MAX, 1, 0, { 45 }
};
static const dream_ptr __ds25 = (dream_ptr)((char *)&__ds25_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[1]; } __ds26_blk = {
  0, 0, TAG_STRING, INT32_MAX, 0, 0, { 0 }
};
static const dream_ptr __ds26 = (dream_ptr)((char *)&__ds26_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[9]; } __ds27_blk = {
  0, 0, TAG_STRING, INT32_MAX, 9, 0, { 86, 101, 114, 116, 101, 120, 32, 123, 32 }
};
static const dream_ptr __ds27 = (dream_ptr)((char *)&__ds27_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[5]; } __ds28_blk = {
  0, 0, TAG_STRING, INT32_MAX, 5, 0, { 112, 111, 115, 58, 32 }
};
static const dream_ptr __ds28 = (dream_ptr)((char *)&__ds28_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[2]; } __ds29_blk = {
  0, 0, TAG_STRING, INT32_MAX, 2, 0, { 32, 125 }
};
static const dream_ptr __ds29 = (dream_ptr)((char *)&__ds29_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[11]; } __ds30_blk = {
  0, 0, TAG_STRING, INT32_MAX, 11, 0, { 71, 112, 117, 69, 114, 114, 111, 114, 32, 123, 32 }
};
static const dream_ptr __ds30 = (dream_ptr)((char *)&__ds30_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[7]; } __ds31_blk = {
  0, 0, TAG_STRING, INT32_MAX, 7, 0, { 95, 99, 111, 100, 101, 58, 32 }
};
static const dream_ptr __ds31 = (dream_ptr)((char *)&__ds31_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[12]; } __ds32_blk = {
  0, 0, TAG_STRING, INT32_MAX, 12, 0, { 44, 32, 95, 109, 101, 115, 115, 97, 103, 101, 58, 32 }
};
static const dream_ptr __ds32 = (dream_ptr)((char *)&__ds32_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[10]; } __ds33_blk = {
  0, 0, TAG_STRING, INT32_MAX, 10, 0, { 71, 112, 117, 86, 101, 99, 50, 32, 123, 32 }
};
static const dream_ptr __ds33 = (dream_ptr)((char *)&__ds33_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[3]; } __ds34_blk = {
  0, 0, TAG_STRING, INT32_MAX, 3, 0, { 120, 58, 32 }
};
static const dream_ptr __ds34 = (dream_ptr)((char *)&__ds34_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[5]; } __ds35_blk = {
  0, 0, TAG_STRING, INT32_MAX, 5, 0, { 44, 32, 121, 58, 32 }
};
static const dream_ptr __ds35 = (dream_ptr)((char *)&__ds35_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[10]; } __ds36_blk = {
  0, 0, TAG_STRING, INT32_MAX, 10, 0, { 71, 112, 117, 86, 101, 99, 52, 32, 123, 32 }
};
static const dream_ptr __ds36 = (dream_ptr)((char *)&__ds36_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[5]; } __ds37_blk = {
  0, 0, TAG_STRING, INT32_MAX, 5, 0, { 44, 32, 122, 58, 32 }
};
static const dream_ptr __ds37 = (dream_ptr)((char *)&__ds37_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[5]; } __ds38_blk = {
  0, 0, TAG_STRING, INT32_MAX, 5, 0, { 44, 32, 119, 58, 32 }
};
static const dream_ptr __ds38 = (dream_ptr)((char *)&__ds38_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[13]; } __ds39_blk = {
  0, 0, TAG_STRING, INT32_MAX, 13, 0, { 71, 112, 117, 84, 101, 120, 116, 117, 114, 101, 32, 123, 32 }
};
static const dream_ptr __ds39 = (dream_ptr)((char *)&__ds39_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[4]; } __ds40_blk = {
  0, 0, TAG_STRING, INT32_MAX, 4, 0, { 105, 100, 58, 32 }
};
static const dream_ptr __ds40 = (dream_ptr)((char *)&__ds40_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[9]; } __ds41_blk = {
  0, 0, TAG_STRING, INT32_MAX, 9, 0, { 44, 32, 119, 105, 100, 116, 104, 58, 32 }
};
static const dream_ptr __ds41 = (dream_ptr)((char *)&__ds41_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[10]; } __ds42_blk = {
  0, 0, TAG_STRING, INT32_MAX, 10, 0, { 44, 32, 104, 101, 105, 103, 104, 116, 58, 32 }
};
static const dream_ptr __ds42 = (dream_ptr)((char *)&__ds42_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[10]; } __ds43_blk = {
  0, 0, TAG_STRING, INT32_MAX, 10, 0, { 44, 32, 102, 111, 114, 109, 97, 116, 58, 32 }
};
static const dream_ptr __ds43 = (dream_ptr)((char *)&__ds43_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[13]; } __ds44_blk = {
  0, 0, TAG_STRING, INT32_MAX, 13, 0, { 71, 112, 117, 83, 117, 114, 102, 97, 99, 101, 32, 123, 32 }
};
static const dream_ptr __ds44 = (dream_ptr)((char *)&__ds44_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[24]; } __ds45_blk = {
  0, 0, TAG_STRING, INT32_MAX, 24, 0, { 71, 112, 117, 82, 101, 110, 100, 101, 114, 80, 105, 112, 101, 108, 105, 110, 101, 68, 101, 115, 99, 32, 123, 32 }
};
static const dream_ptr __ds45 = (dream_ptr)((char *)&__ds45_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[10]; } __ds46_blk = {
  0, 0, TAG_STRING, INT32_MAX, 10, 0, { 116, 111, 112, 111, 108, 111, 103, 121, 58, 32 }
};
static const dream_ptr __ds46 = (dream_ptr)((char *)&__ds46_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[13]; } __ds47_blk = {
  0, 0, TAG_STRING, INT32_MAX, 13, 0, { 44, 32, 99, 117, 108, 108, 95, 109, 111, 100, 101, 58, 32 }
};
static const dream_ptr __ds47 = (dream_ptr)((char *)&__ds47_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[14]; } __ds48_blk = {
  0, 0, TAG_STRING, INT32_MAX, 14, 0, { 44, 32, 102, 114, 111, 110, 116, 95, 102, 97, 99, 101, 58, 32 }
};
static const dream_ptr __ds48 = (dream_ptr)((char *)&__ds48_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[17]; } __ds49_blk = {
  0, 0, TAG_STRING, INT32_MAX, 17, 0, { 44, 32, 100, 101, 112, 116, 104, 95, 101, 110, 97, 98, 108, 101, 100, 58, 32 }
};
static const dream_ptr __ds49 = (dream_ptr)((char *)&__ds49_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[15]; } __ds50_blk = {
  0, 0, TAG_STRING, INT32_MAX, 15, 0, { 44, 32, 100, 101, 112, 116, 104, 95, 119, 114, 105, 116, 101, 58, 32 }
};
static const dream_ptr __ds50 = (dream_ptr)((char *)&__ds50_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[17]; } __ds51_blk = {
  0, 0, TAG_STRING, INT32_MAX, 17, 0, { 44, 32, 100, 101, 112, 116, 104, 95, 99, 111, 109, 112, 97, 114, 101, 58, 32 }
};
static const dream_ptr __ds51 = (dream_ptr)((char *)&__ds51_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[17]; } __ds52_blk = {
  0, 0, TAG_STRING, INT32_MAX, 17, 0, { 44, 32, 98, 108, 101, 110, 100, 95, 101, 110, 97, 98, 108, 101, 100, 58, 32 }
};
static const dream_ptr __ds52 = (dream_ptr)((char *)&__ds52_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[16]; } __ds53_blk = {
  0, 0, TAG_STRING, INT32_MAX, 16, 0, { 44, 32, 115, 97, 109, 112, 108, 101, 95, 99, 111, 117, 110, 116, 58, 32 }
};
static const dream_ptr __ds53 = (dream_ptr)((char *)&__ds53_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[20]; } __ds54_blk = {
  0, 0, TAG_STRING, INT32_MAX, 20, 0, { 71, 112, 117, 82, 101, 110, 100, 101, 114, 80, 105, 112, 101, 108, 105, 110, 101, 32, 123, 32 }
};
static const dream_ptr __ds54 = (dream_ptr)((char *)&__ds54_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[19]; } __ds55_blk = {
  0, 0, TAG_STRING, INT32_MAX, 19, 0, { 71, 112, 117, 66, 117, 102, 102, 101, 114, 95, 86, 101, 114, 116, 101, 120, 32, 123, 32 }
};
static const dream_ptr __ds55 = (dream_ptr)((char *)&__ds55_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[5]; } __ds56_blk = {
  0, 0, TAG_STRING, INT32_MAX, 5, 0, { 95, 105, 100, 58, 32 }
};
static const dream_ptr __ds56 = (dream_ptr)((char *)&__ds56_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[11]; } __ds57_blk = {
  0, 0, TAG_STRING, INT32_MAX, 11, 0, { 44, 32, 95, 108, 101, 110, 103, 116, 104, 58, 32 }
};
static const dream_ptr __ds57 = (dream_ptr)((char *)&__ds57_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[11]; } __ds58_blk = {
  0, 0, TAG_STRING, INT32_MAX, 11, 0, { 44, 32, 95, 115, 116, 114, 105, 100, 101, 58, 32 }
};
static const dream_ptr __ds58 = (dream_ptr)((char *)&__ds58_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[3]; } __ds59_blk = {
  0, 0, TAG_STRING, INT32_MAX, 3, 0, { 79, 107, 40 }
};
static const dream_ptr __ds59 = (dream_ptr)((char *)&__ds59_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[7]; } __ds60_blk = {
  0, 0, TAG_STRING, INT32_MAX, 7, 0, { 118, 97, 108, 117, 101, 58, 32 }
};
static const dream_ptr __ds60 = (dream_ptr)((char *)&__ds60_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[4]; } __ds61_blk = {
  0, 0, TAG_STRING, INT32_MAX, 4, 0, { 69, 114, 114, 40 }
};
static const dream_ptr __ds61 = (dream_ptr)((char *)&__ds61_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[7]; } __ds62_blk = {
  0, 0, TAG_STRING, INT32_MAX, 7, 0, { 101, 114, 114, 111, 114, 58, 32 }
};
static const dream_ptr __ds62 = (dream_ptr)((char *)&__ds62_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[5]; } __ds63_blk = {
  0, 0, TAG_STRING, INT32_MAX, 5, 0, { 83, 111, 109, 101, 40 }
};
static const dream_ptr __ds63 = (dream_ptr)((char *)&__ds63_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[4]; } __ds64_blk = {
  0, 0, TAG_STRING, INT32_MAX, 4, 0, { 78, 111, 110, 101 }
};
static const dream_ptr __ds64 = (dream_ptr)((char *)&__ds64_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[26]; } __ds65_blk = {
  0, 0, TAG_STRING, INT32_MAX, 26, 0, { 112, 97, 110, 105, 99, 58, 32, 105, 110, 100, 101, 120, 32, 111, 117, 116, 32, 111, 102, 32, 98, 111, 117, 110, 100, 115 }
};
static const dream_ptr __ds65 = (dream_ptr)((char *)&__ds65_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[32]; } __ds66_blk = {
  0, 0, TAG_STRING, INT32_MAX, 32, 0, { 112, 97, 110, 105, 99, 58, 32, 97, 116, 116, 101, 109, 112, 116, 32, 116, 111, 32, 100, 105, 118, 105, 100, 101, 32, 98, 121, 32, 122, 101, 114, 111 }
};
static const dream_ptr __ds66 = (dream_ptr)((char *)&__ds66_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[19]; } __ds67_blk = {
  0, 0, TAG_STRING, INT32_MAX, 19, 0, { 112, 97, 110, 105, 99, 58, 32, 105, 110, 118, 97, 108, 105, 100, 32, 99, 97, 115, 116 }
};
static const dream_ptr __ds67 = (dream_ptr)((char *)&__ds67_blk + 16);

static struct { int32_t size, header_pad, tag, rc, len, pad; uint16_t u[48]; } __ds68_blk = {
  0, 0, TAG_STRING, INT32_MAX, 48, 0, { 112, 97, 110, 105, 99, 58, 32, 97, 99, 99, 101, 115, 115, 32, 116, 111, 32, 100, 101, 97, 108, 108, 111, 99, 97, 116, 101, 100, 32, 39, 117, 110, 111, 119, 110, 101, 100, 39, 32, 114, 101, 102, 101, 114, 101, 110, 99, 101 }
};
static const dream_ptr __ds68 = (dream_ptr)((char *)&__ds68_blk + 16);

_Thread_local dream_ptr g0 = 0;

dream_ptr gpuLastError(void);
int32_t gpuTryInit(void);
dream_ptr __async_gpuTryInit(void) {
  dream_ptr __f = dream_new_future(64, -1, 1);
  dream_async_complete(__f, (dream_ptr)(intptr_t)gpuTryInit());
  return __f;
}
int32_t gpuFrame(void);
dream_ptr __async_gpuFrame(void) {
  dream_ptr __f = dream_new_future(64, -1, 1);
  dream_async_complete(__f, (dream_ptr)(intptr_t)gpuFrame());
  return __f;
}
void gpuBufferWriteBytes(int32_t a0, dream_ptr a1);
int32_t gpuSurfaceCreate(dream_ptr a0, int32_t a1, int32_t a2);
int32_t gpuSurfacePresent(int32_t a0);
dream_ptr __async_gpuSurfacePresent(int32_t a0) {
  dream_ptr __f = dream_new_future(64, -1, 1);
  dream_async_complete(__f, (dream_ptr)(intptr_t)gpuSurfacePresent(a0));
  return __f;
}
int32_t gpuSurfaceCloseRequested(int32_t a0);
int32_t gpuSurfaceWidth(int32_t a0);
int32_t gpuSurfaceHeight(int32_t a0);
int32_t gpuRenderPipelineCreateEx(dream_ptr a0, dream_ptr a1, int32_t a2, int32_t a3, int32_t a4, int32_t a5, int32_t a6, int32_t a7, int32_t a8, int32_t a9);
dream_ptr __async_gpuRenderPipelineCreateEx(dream_ptr a0, dream_ptr a1, int32_t a2, int32_t a3, int32_t a4, int32_t a5, int32_t a6, int32_t a7, int32_t a8, int32_t a9) {
  dream_ptr __f = dream_new_future(64, -1, 1);
  dream_async_complete(__f, (dream_ptr)(intptr_t)gpuRenderPipelineCreateEx(a0, a1, a2, a3, a4, a5, a6, a7, a8, a9));
  return __f;
}
int32_t gpuRenderDrawEx(int32_t a0, int32_t a1, int32_t a2, int32_t a3, int32_t a4, dream_ptr a5, float a6, float a7, float a8, float a9, int32_t a10, int32_t a11);
dream_ptr __async_gpuRenderDrawEx(int32_t a0, int32_t a1, int32_t a2, int32_t a3, int32_t a4, dream_ptr a5, float a6, float a7, float a8, float a9, int32_t a10, int32_t a11) {
  dream_ptr __f = dream_new_future(64, -1, 1);
  dream_async_complete(__f, (dream_ptr)(intptr_t)gpuRenderDrawEx(a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11));
  return __f;
}
int32_t gpuBufferAllocVertexBytes(int32_t a0);
int64_t timeNowNanos(void);
__attribute__((weak)) int64_t timeNowNanos(void) {  return 0; }
void print_int(int32_t v);
void print_string(dream_ptr s);
void print_char(int32_t c);
void print_float(float v);
void print_double(double v);

static void *dream_ft[33];

dream_ptr make_vert(float l0, float l1);
dream_ptr main_dream(void);
int32_t poll_main_dream(dream_ptr __self);
dream_ptr GpuRenderPass_draw_ex__87(dream_ptr l0, dream_ptr l1, dream_ptr l2, int32_t l3, dream_ptr l4, dream_ptr l5);
int32_t poll_GpuRenderPass_draw_ex__87(dream_ptr __self);
void GpuError_constructor(dream_ptr l0, dream_ptr l1, dream_ptr l2);
dream_ptr GpuError_from_code(int32_t l0, dream_ptr l1);
dream_ptr GpuVec4_of(float l0, float l1, float l2, float l3);
dream_ptr Uniforms_pack_f32(dream_ptr l0);
dream_ptr GpuSurface_create(dream_ptr l0, int32_t l1, int32_t l2);
int32_t GpuSurface_width(dream_ptr l0);
int32_t GpuSurface_height(dream_ptr l0);
dream_ptr GpuSurface_present(dream_ptr l0);
int32_t poll_GpuSurface_present(dream_ptr __self);
int32_t GpuSurface_close_requested(dream_ptr l0);
dream_ptr GpuRenderPipelineDesc_overlay(void);
dream_ptr GpuRenderPipeline_create_ex(dream_ptr l0, dream_ptr l1, dream_ptr l2);
int32_t poll_GpuRenderPipeline_create_ex(dream_ptr __self);
dream_ptr Gpu_try_init(void);
int32_t poll_Gpu_try_init(dream_ptr __self);
dream_ptr Gpu_frame(void);
int32_t poll_Gpu_frame(dream_ptr __self);
int32_t Result_bool_GpuError_is_err(dream_ptr l0);
int32_t Result_GpuSurface_GpuError_is_err(dream_ptr l0);
dream_ptr Result_GpuSurface_GpuError_unwrap_or(dream_ptr l0, dream_ptr l1);
int32_t Result_GpuRenderPipeline_GpuError_is_err(dream_ptr l0);
dream_ptr Result_GpuRenderPipeline_GpuError_unwrap_or(dream_ptr l0, dream_ptr l1);
int32_t GpuBuffer_Vertex_get_id(dream_ptr l0);
dream_ptr GpuBuffer_Vertex_vertex_from(dream_ptr l0);
void GpuBuffer_Vertex_write(dream_ptr l0, dream_ptr l1);
dream_ptr GpuRenderPass_draw_instanced__87(dream_ptr l0, dream_ptr l1, dream_ptr l2, int32_t l3, int32_t l4, dream_ptr l5, dream_ptr l6, dream_ptr l7, int32_t l8);
int32_t poll_GpuRenderPass_draw_instanced__87(dream_ptr __self);

static void release_array_t87(dream_ptr p);
static void release_GpuError(dream_ptr p);
static void release_GpuVec2(dream_ptr p);
static void release_GpuVec4(dream_ptr p);
static void release_GpuTexture(dream_ptr p);
static void release_GpuSurface(dream_ptr p);
static void release_GpuRenderPipelineDesc(dream_ptr p);
static void release_GpuRenderPipeline(dream_ptr p);
static void release_GpuBuffer_Vertex(dream_ptr p);
static void release_Vertex(dream_ptr p);
static void release_Result_bool_GpuError(dream_ptr p);
static void release_Result_GpuSurface_GpuError(dream_ptr p);
static void release_Result_GpuRenderPipeline_GpuError(dream_ptr p);
static void release_Option_GpuTexture(dream_ptr p);

static void release_array_t87(dream_ptr p) {
  int32_t n; int32_t i;
  if (!p) return;
  { int32_t *rc = (int32_t *)((char *)dream_p(p) - 4); int32_t old = *rc; if (old <= 0 || old == INT32_MAX) return; *rc = old - 1; if (old != 1) return; }
  n = *(int32_t *)dream_p(p);
  for (i = 0; i < n; i++) {
  }
  dream_free(p);
}

static void release_GpuError(dream_ptr p) {
  if (!p) return;
  { int32_t *rc = (int32_t *)((char *)dream_p(p) - 4); int32_t old = *rc; if (old <= 0 || old == INT32_MAX) return; *rc = old - 1; if (old != 1) return; }
  dream_release(*(dream_ptr *)((char *)dream_p(p) + 0));
  dream_release(*(dream_ptr *)((char *)dream_p(p) + 8));
  dream_free(p);
}

static void release_GpuVec2(dream_ptr p) {
  if (!p) return;
  { int32_t *rc = (int32_t *)((char *)dream_p(p) - 4); int32_t old = *rc; if (old <= 0 || old == INT32_MAX) return; *rc = old - 1; if (old != 1) return; }
  dream_free(p);
}

static void release_GpuVec4(dream_ptr p) {
  if (!p) return;
  { int32_t *rc = (int32_t *)((char *)dream_p(p) - 4); int32_t old = *rc; if (old <= 0 || old == INT32_MAX) return; *rc = old - 1; if (old != 1) return; }
  dream_free(p);
}

static void release_GpuTexture(dream_ptr p) {
  if (!p) return;
  { int32_t *rc = (int32_t *)((char *)dream_p(p) - 4); int32_t old = *rc; if (old <= 0 || old == INT32_MAX) return; *rc = old - 1; if (old != 1) return; }
  dream_release(*(dream_ptr *)((char *)dream_p(p) + 16));
  dream_free(p);
}

static void release_GpuSurface(dream_ptr p) {
  if (!p) return;
  { int32_t *rc = (int32_t *)((char *)dream_p(p) - 4); int32_t old = *rc; if (old <= 0 || old == INT32_MAX) return; *rc = old - 1; if (old != 1) return; }
  dream_free(p);
}

static void release_GpuRenderPipelineDesc(dream_ptr p) {
  if (!p) return;
  { int32_t *rc = (int32_t *)((char *)dream_p(p) - 4); int32_t old = *rc; if (old <= 0 || old == INT32_MAX) return; *rc = old - 1; if (old != 1) return; }
  dream_free(p);
}

static void release_GpuRenderPipeline(dream_ptr p) {
  if (!p) return;
  { int32_t *rc = (int32_t *)((char *)dream_p(p) - 4); int32_t old = *rc; if (old <= 0 || old == INT32_MAX) return; *rc = old - 1; if (old != 1) return; }
  dream_free(p);
}

static void release_GpuBuffer_Vertex(dream_ptr p) {
  if (!p) return;
  { int32_t *rc = (int32_t *)((char *)dream_p(p) - 4); int32_t old = *rc; if (old <= 0 || old == INT32_MAX) return; *rc = old - 1; if (old != 1) return; }
  dream_free(p);
}

static void release_Vertex(dream_ptr p) {
  if (!p) return;
  { int32_t *rc = (int32_t *)((char *)dream_p(p) - 4); int32_t old = *rc; if (old <= 0 || old == INT32_MAX) return; *rc = old - 1; if (old != 1) return; }
  dream_free(p);
}

static void release_Result_bool_GpuError(dream_ptr p) {
  if (!p) return;
  { int32_t *rc = (int32_t *)((char *)dream_p(p) - 4); int32_t old = *rc; if (old <= 0 || old == INT32_MAX) return; *rc = old - 1; if (old != 1) return; }
  switch (*(int32_t *)dream_p(p)) {
    case 0:
      break;
    case 1:
      release_GpuError(*(dream_ptr *)((char *)dream_p(p) + 8));
      break;
    default: break;
  }
  dream_free(p);
}

static void release_Result_GpuSurface_GpuError(dream_ptr p) {
  if (!p) return;
  { int32_t *rc = (int32_t *)((char *)dream_p(p) - 4); int32_t old = *rc; if (old <= 0 || old == INT32_MAX) return; *rc = old - 1; if (old != 1) return; }
  switch (*(int32_t *)dream_p(p)) {
    case 0:
      release_GpuSurface(*(dream_ptr *)((char *)dream_p(p) + 8));
      break;
    case 1:
      release_GpuError(*(dream_ptr *)((char *)dream_p(p) + 8));
      break;
    default: break;
  }
  dream_free(p);
}

static void release_Result_GpuRenderPipeline_GpuError(dream_ptr p) {
  if (!p) return;
  { int32_t *rc = (int32_t *)((char *)dream_p(p) - 4); int32_t old = *rc; if (old <= 0 || old == INT32_MAX) return; *rc = old - 1; if (old != 1) return; }
  switch (*(int32_t *)dream_p(p)) {
    case 0:
      release_GpuRenderPipeline(*(dream_ptr *)((char *)dream_p(p) + 8));
      break;
    case 1:
      release_GpuError(*(dream_ptr *)((char *)dream_p(p) + 8));
      break;
    default: break;
  }
  dream_free(p);
}

static void release_Option_GpuTexture(dream_ptr p) {
  if (!p) return;
  { int32_t *rc = (int32_t *)((char *)dream_p(p) - 4); int32_t old = *rc; if (old <= 0 || old == INT32_MAX) return; *rc = old - 1; if (old != 1) return; }
  switch (*(int32_t *)dream_p(p)) {
    case 0:
      release_GpuTexture(*(dream_ptr *)((char *)dream_p(p) + 8));
      break;
    case 1:
      break;
    default: break;
  }
  dream_free(p);
}

dream_ptr array_to_string_t4(dream_ptr p);
dream_ptr array_to_string_t5(dream_ptr p);
dream_ptr array_to_string_t87(dream_ptr p);
dream_ptr GpuError_to_string(dream_ptr p);
int32_t GpuError_hash_code(dream_ptr p);
dream_ptr GpuVec2_to_string(dream_ptr p);
int32_t GpuVec2_hash_code(dream_ptr p);
dream_ptr GpuVec4_to_string(dream_ptr p);
int32_t GpuVec4_hash_code(dream_ptr p);
dream_ptr GpuTexture_to_string(dream_ptr p);
int32_t GpuTexture_hash_code(dream_ptr p);
dream_ptr GpuSurface_to_string(dream_ptr p);
int32_t GpuSurface_hash_code(dream_ptr p);
dream_ptr GpuRenderPipelineDesc_to_string(dream_ptr p);
int32_t GpuRenderPipelineDesc_hash_code(dream_ptr p);
dream_ptr GpuRenderPipeline_to_string(dream_ptr p);
int32_t GpuRenderPipeline_hash_code(dream_ptr p);
dream_ptr GpuBuffer_Vertex_to_string(dream_ptr p);
int32_t GpuBuffer_Vertex_hash_code(dream_ptr p);
dream_ptr Vertex_to_string(dream_ptr p);
int32_t Vertex_hash_code(dream_ptr p);
dream_ptr Result_bool_GpuError_to_string(dream_ptr p);
int32_t Result_bool_GpuError_hash_code(dream_ptr p);
dream_ptr Result_GpuSurface_GpuError_to_string(dream_ptr p);
int32_t Result_GpuSurface_GpuError_hash_code(dream_ptr p);
dream_ptr Result_GpuRenderPipeline_GpuError_to_string(dream_ptr p);
int32_t Result_GpuRenderPipeline_GpuError_hash_code(dream_ptr p);
dream_ptr Option_GpuTexture_to_string(dream_ptr p);
int32_t Option_GpuTexture_hash_code(dream_ptr p);

dream_ptr array_to_string_t4(dream_ptr p) {
  int32_t n = p ? *(int32_t *)dream_p(p) : 0;
  int32_t i;
  dream_ptr r = __ds18;
  for (i = 0; i < n; i++) {
    if (i) { dream_ptr __c = dream_concat_strings(r, __ds20); dream_release(r); r = __c; }
    { dream_ptr __p = dream_byte_to_string(*(uint8_t *)((char *)dream_p(p) + 4 + (size_t)i * 1)); dream_ptr __c = dream_concat_strings(r, __p); dream_release(r); dream_release(__p); r = __c; }
  }
  { dream_ptr __c = dream_concat_strings(r, __ds19); dream_release(r); return __c; }
}

dream_ptr array_to_string_t5(dream_ptr p) {
  int32_t n = p ? *(int32_t *)dream_p(p) : 0;
  int32_t i;
  dream_ptr r = __ds18;
  for (i = 0; i < n; i++) {
    if (i) { dream_ptr __c = dream_concat_strings(r, __ds20); dream_release(r); r = __c; }
    { dream_ptr __p = dream_float_to_string(*(float *)((char *)dream_p(p) + 4 + (size_t)i * 4)); dream_ptr __c = dream_concat_strings(r, __p); dream_release(r); dream_release(__p); r = __c; }
  }
  { dream_ptr __c = dream_concat_strings(r, __ds19); dream_release(r); return __c; }
}

dream_ptr array_to_string_t87(dream_ptr p) {
  int32_t n = p ? *(int32_t *)dream_p(p) : 0;
  int32_t i;
  dream_ptr r = __ds18;
  for (i = 0; i < n; i++) {
    if (i) { dream_ptr __c = dream_concat_strings(r, __ds20); dream_release(r); r = __c; }
    { dream_ptr __p = Vertex_to_string((dream_ptr)((char *)dream_p(p) + 4 + (size_t)i * 8)); dream_ptr __c = dream_concat_strings(r, __p); dream_release(r); dream_release(__p); r = __c; }
  }
  { dream_ptr __c = dream_concat_strings(r, __ds19); dream_release(r); return __c; }
}


dream_ptr GpuError_to_string(dream_ptr p) {
  if (!p) return __ds16;
  dream_ptr r = __ds30;
  { dream_ptr __c = dream_concat_strings(r, __ds31); dream_release(r); r = __c; }
  { dream_ptr __c = dream_concat_strings(r, *(dream_ptr *)((char *)dream_p(p) + 0)); dream_release(r); r = __c; }
  { dream_ptr __c = dream_concat_strings(r, __ds32); dream_release(r); r = __c; }
  { dream_ptr __c = dream_concat_strings(r, *(dream_ptr *)((char *)dream_p(p) + 8)); dream_release(r); r = __c; }
  { dream_ptr __c = dream_concat_strings(r, __ds29); dream_release(r); return __c; }
}

dream_ptr GpuVec2_to_string(dream_ptr p) {
  if (!p) return __ds16;
  dream_ptr r = __ds33;
  { dream_ptr __c = dream_concat_strings(r, __ds34); dream_release(r); r = __c; }
  { dream_ptr __p = dream_float_to_string(*(float *)((char *)dream_p(p) + 0)); dream_ptr __c = dream_concat_strings(r, __p); dream_release(r); dream_release(__p); r = __c; }
  { dream_ptr __c = dream_concat_strings(r, __ds35); dream_release(r); r = __c; }
  { dream_ptr __p = dream_float_to_string(*(float *)((char *)dream_p(p) + 4)); dream_ptr __c = dream_concat_strings(r, __p); dream_release(r); dream_release(__p); r = __c; }
  { dream_ptr __c = dream_concat_strings(r, __ds29); dream_release(r); return __c; }
}

dream_ptr GpuVec4_to_string(dream_ptr p) {
  if (!p) return __ds16;
  dream_ptr r = __ds36;
  { dream_ptr __c = dream_concat_strings(r, __ds34); dream_release(r); r = __c; }
  { dream_ptr __p = dream_float_to_string(*(float *)((char *)dream_p(p) + 0)); dream_ptr __c = dream_concat_strings(r, __p); dream_release(r); dream_release(__p); r = __c; }
  { dream_ptr __c = dream_concat_strings(r, __ds35); dream_release(r); r = __c; }
  { dream_ptr __p = dream_float_to_string(*(float *)((char *)dream_p(p) + 4)); dream_ptr __c = dream_concat_strings(r, __p); dream_release(r); dream_release(__p); r = __c; }
  { dream_ptr __c = dream_concat_strings(r, __ds37); dream_release(r); r = __c; }
  { dream_ptr __p = dream_float_to_string(*(float *)((char *)dream_p(p) + 8)); dream_ptr __c = dream_concat_strings(r, __p); dream_release(r); dream_release(__p); r = __c; }
  { dream_ptr __c = dream_concat_strings(r, __ds38); dream_release(r); r = __c; }
  { dream_ptr __p = dream_float_to_string(*(float *)((char *)dream_p(p) + 12)); dream_ptr __c = dream_concat_strings(r, __p); dream_release(r); dream_release(__p); r = __c; }
  { dream_ptr __c = dream_concat_strings(r, __ds29); dream_release(r); return __c; }
}

dream_ptr GpuTexture_to_string(dream_ptr p) {
  if (!p) return __ds16;
  dream_ptr r = __ds39;
  { dream_ptr __c = dream_concat_strings(r, __ds40); dream_release(r); r = __c; }
  { dream_ptr __p = dream_int_to_string(*(int32_t *)((char *)dream_p(p) + 0)); dream_ptr __c = dream_concat_strings(r, __p); dream_release(r); dream_release(__p); r = __c; }
  { dream_ptr __c = dream_concat_strings(r, __ds41); dream_release(r); r = __c; }
  { dream_ptr __p = dream_int_to_string(*(int32_t *)((char *)dream_p(p) + 4)); dream_ptr __c = dream_concat_strings(r, __p); dream_release(r); dream_release(__p); r = __c; }
  { dream_ptr __c = dream_concat_strings(r, __ds42); dream_release(r); r = __c; }
  { dream_ptr __p = dream_int_to_string(*(int32_t *)((char *)dream_p(p) + 8)); dream_ptr __c = dream_concat_strings(r, __p); dream_release(r); dream_release(__p); r = __c; }
  { dream_ptr __c = dream_concat_strings(r, __ds43); dream_release(r); r = __c; }
  { dream_ptr __c = dream_concat_strings(r, *(dream_ptr *)((char *)dream_p(p) + 16)); dream_release(r); r = __c; }
  { dream_ptr __c = dream_concat_strings(r, __ds29); dream_release(r); return __c; }
}

dream_ptr GpuSurface_to_string(dream_ptr p) {
  if (!p) return __ds16;
  dream_ptr r = __ds44;
  { dream_ptr __c = dream_concat_strings(r, __ds40); dream_release(r); r = __c; }
  { dream_ptr __p = dream_int_to_string(*(int32_t *)((char *)dream_p(p) + 0)); dream_ptr __c = dream_concat_strings(r, __p); dream_release(r); dream_release(__p); r = __c; }
  { dream_ptr __c = dream_concat_strings(r, __ds29); dream_release(r); return __c; }
}

dream_ptr GpuRenderPipelineDesc_to_string(dream_ptr p) {
  if (!p) return __ds16;
  dream_ptr r = __ds45;
  { dream_ptr __c = dream_concat_strings(r, __ds46); dream_release(r); r = __c; }
  { dream_ptr __p = dream_int_to_string(*(int32_t *)((char *)dream_p(p) + 0)); dream_ptr __c = dream_concat_strings(r, __p); dream_release(r); dream_release(__p); r = __c; }
  { dream_ptr __c = dream_concat_strings(r, __ds47); dream_release(r); r = __c; }
  { dream_ptr __p = dream_int_to_string(*(int32_t *)((char *)dream_p(p) + 4)); dream_ptr __c = dream_concat_strings(r, __p); dream_release(r); dream_release(__p); r = __c; }
  { dream_ptr __c = dream_concat_strings(r, __ds48); dream_release(r); r = __c; }
  { dream_ptr __p = dream_int_to_string(*(int32_t *)((char *)dream_p(p) + 8)); dream_ptr __c = dream_concat_strings(r, __p); dream_release(r); dream_release(__p); r = __c; }
  { dream_ptr __c = dream_concat_strings(r, __ds49); dream_release(r); r = __c; }
  { dream_ptr __p = dream_bool_to_string(*(uint8_t *)((char *)dream_p(p) + 12)); dream_ptr __c = dream_concat_strings(r, __p); dream_release(r); dream_release(__p); r = __c; }
  { dream_ptr __c = dream_concat_strings(r, __ds50); dream_release(r); r = __c; }
  { dream_ptr __p = dream_bool_to_string(*(uint8_t *)((char *)dream_p(p) + 13)); dream_ptr __c = dream_concat_strings(r, __p); dream_release(r); dream_release(__p); r = __c; }
  { dream_ptr __c = dream_concat_strings(r, __ds51); dream_release(r); r = __c; }
  { dream_ptr __p = dream_int_to_string(*(int32_t *)((char *)dream_p(p) + 16)); dream_ptr __c = dream_concat_strings(r, __p); dream_release(r); dream_release(__p); r = __c; }
  { dream_ptr __c = dream_concat_strings(r, __ds52); dream_release(r); r = __c; }
  { dream_ptr __p = dream_bool_to_string(*(uint8_t *)((char *)dream_p(p) + 20)); dream_ptr __c = dream_concat_strings(r, __p); dream_release(r); dream_release(__p); r = __c; }
  { dream_ptr __c = dream_concat_strings(r, __ds53); dream_release(r); r = __c; }
  { dream_ptr __p = dream_int_to_string(*(int32_t *)((char *)dream_p(p) + 24)); dream_ptr __c = dream_concat_strings(r, __p); dream_release(r); dream_release(__p); r = __c; }
  { dream_ptr __c = dream_concat_strings(r, __ds29); dream_release(r); return __c; }
}

dream_ptr GpuRenderPipeline_to_string(dream_ptr p) {
  if (!p) return __ds16;
  dream_ptr r = __ds54;
  { dream_ptr __c = dream_concat_strings(r, __ds40); dream_release(r); r = __c; }
  { dream_ptr __p = dream_int_to_string(*(int32_t *)((char *)dream_p(p) + 0)); dream_ptr __c = dream_concat_strings(r, __p); dream_release(r); dream_release(__p); r = __c; }
  { dream_ptr __c = dream_concat_strings(r, __ds29); dream_release(r); return __c; }
}

dream_ptr GpuBuffer_Vertex_to_string(dream_ptr p) {
  if (!p) return __ds16;
  dream_ptr r = __ds55;
  { dream_ptr __c = dream_concat_strings(r, __ds56); dream_release(r); r = __c; }
  { dream_ptr __p = dream_int_to_string(*(int32_t *)((char *)dream_p(p) + 0)); dream_ptr __c = dream_concat_strings(r, __p); dream_release(r); dream_release(__p); r = __c; }
  { dream_ptr __c = dream_concat_strings(r, __ds57); dream_release(r); r = __c; }
  { dream_ptr __p = dream_int_to_string(*(int32_t *)((char *)dream_p(p) + 4)); dream_ptr __c = dream_concat_strings(r, __p); dream_release(r); dream_release(__p); r = __c; }
  { dream_ptr __c = dream_concat_strings(r, __ds58); dream_release(r); r = __c; }
  { dream_ptr __p = dream_int_to_string(*(int32_t *)((char *)dream_p(p) + 8)); dream_ptr __c = dream_concat_strings(r, __p); dream_release(r); dream_release(__p); r = __c; }
  { dream_ptr __c = dream_concat_strings(r, __ds29); dream_release(r); return __c; }
}

dream_ptr Vertex_to_string(dream_ptr p) {
  if (!p) return __ds16;
  dream_ptr r = __ds27;
  { dream_ptr __c = dream_concat_strings(r, __ds28); dream_release(r); r = __c; }
  { dream_ptr __p = GpuVec2_to_string((dream_ptr)((char *)dream_p(p) + 0)); dream_ptr __c = dream_concat_strings(r, __p); dream_release(r); dream_release(__p); r = __c; }
  { dream_ptr __c = dream_concat_strings(r, __ds29); dream_release(r); return __c; }
}

dream_ptr Result_bool_GpuError_to_string(dream_ptr p) {
  int32_t d;
  dream_ptr r;
  if (!p) return __ds16;
  d = *(int32_t *)dream_p(p);
  r = __ds17;
  switch (d) {
    case 0: {
      r = __ds59;
      { dream_ptr __c = dream_concat_strings(r, __ds60); dream_release(r); r = __c; }
      { dream_ptr __p = dream_bool_to_string(*(uint8_t *)((char *)dream_p(p) + 4)); dream_ptr __c = dream_concat_strings(r, __p); dream_release(r); dream_release(__p); r = __c; }
      { dream_ptr __c = dream_concat_strings(r, __ds22); dream_release(r); r = __c; }
      break;
    }
    case 1: {
      r = __ds61;
      { dream_ptr __c = dream_concat_strings(r, __ds62); dream_release(r); r = __c; }
      { dream_ptr __p = GpuError_to_string(*(dream_ptr *)((char *)dream_p(p) + 8)); dream_ptr __c = dream_concat_strings(r, __p); dream_release(r); dream_release(__p); r = __c; }
      { dream_ptr __c = dream_concat_strings(r, __ds22); dream_release(r); r = __c; }
      break;
    }
    default: break;
  }
  return r;
}

dream_ptr Result_GpuSurface_GpuError_to_string(dream_ptr p) {
  int32_t d;
  dream_ptr r;
  if (!p) return __ds16;
  d = *(int32_t *)dream_p(p);
  r = __ds17;
  switch (d) {
    case 0: {
      r = __ds59;
      { dream_ptr __c = dream_concat_strings(r, __ds60); dream_release(r); r = __c; }
      { dream_ptr __p = GpuSurface_to_string(*(dream_ptr *)((char *)dream_p(p) + 8)); dream_ptr __c = dream_concat_strings(r, __p); dream_release(r); dream_release(__p); r = __c; }
      { dream_ptr __c = dream_concat_strings(r, __ds22); dream_release(r); r = __c; }
      break;
    }
    case 1: {
      r = __ds61;
      { dream_ptr __c = dream_concat_strings(r, __ds62); dream_release(r); r = __c; }
      { dream_ptr __p = GpuError_to_string(*(dream_ptr *)((char *)dream_p(p) + 8)); dream_ptr __c = dream_concat_strings(r, __p); dream_release(r); dream_release(__p); r = __c; }
      { dream_ptr __c = dream_concat_strings(r, __ds22); dream_release(r); r = __c; }
      break;
    }
    default: break;
  }
  return r;
}

dream_ptr Result_GpuRenderPipeline_GpuError_to_string(dream_ptr p) {
  int32_t d;
  dream_ptr r;
  if (!p) return __ds16;
  d = *(int32_t *)dream_p(p);
  r = __ds17;
  switch (d) {
    case 0: {
      r = __ds59;
      { dream_ptr __c = dream_concat_strings(r, __ds60); dream_release(r); r = __c; }
      { dream_ptr __p = GpuRenderPipeline_to_string(*(dream_ptr *)((char *)dream_p(p) + 8)); dream_ptr __c = dream_concat_strings(r, __p); dream_release(r); dream_release(__p); r = __c; }
      { dream_ptr __c = dream_concat_strings(r, __ds22); dream_release(r); r = __c; }
      break;
    }
    case 1: {
      r = __ds61;
      { dream_ptr __c = dream_concat_strings(r, __ds62); dream_release(r); r = __c; }
      { dream_ptr __p = GpuError_to_string(*(dream_ptr *)((char *)dream_p(p) + 8)); dream_ptr __c = dream_concat_strings(r, __p); dream_release(r); dream_release(__p); r = __c; }
      { dream_ptr __c = dream_concat_strings(r, __ds22); dream_release(r); r = __c; }
      break;
    }
    default: break;
  }
  return r;
}

dream_ptr Option_GpuTexture_to_string(dream_ptr p) {
  int32_t d;
  dream_ptr r;
  if (!p) return __ds16;
  d = *(int32_t *)dream_p(p);
  r = __ds17;
  switch (d) {
    case 0: {
      r = __ds63;
      { dream_ptr __c = dream_concat_strings(r, __ds60); dream_release(r); r = __c; }
      { dream_ptr __p = GpuTexture_to_string(*(dream_ptr *)((char *)dream_p(p) + 8)); dream_ptr __c = dream_concat_strings(r, __p); dream_release(r); dream_release(__p); r = __c; }
      { dream_ptr __c = dream_concat_strings(r, __ds22); dream_release(r); r = __c; }
      break;
    }
    case 1: {
      r = __ds64;
      break;
    }
    default: break;
  }
  return r;
}

int32_t GpuError_hash_code(dream_ptr p) {
  int32_t h = 17;
      h = h * 31 + dream_string_hash(*(dream_ptr *)((char *)dream_p(p) + 0));
      h = h * 31 + dream_string_hash(*(dream_ptr *)((char *)dream_p(p) + 8));
  return h;
}

int32_t GpuVec2_hash_code(dream_ptr p) {
  int32_t h = 17;
      h = h * 31 + dream_bitcast_f32(*(float *)((char *)dream_p(p) + 0));
      h = h * 31 + dream_bitcast_f32(*(float *)((char *)dream_p(p) + 4));
  return h;
}

int32_t GpuVec4_hash_code(dream_ptr p) {
  int32_t h = 17;
      h = h * 31 + dream_bitcast_f32(*(float *)((char *)dream_p(p) + 0));
      h = h * 31 + dream_bitcast_f32(*(float *)((char *)dream_p(p) + 4));
      h = h * 31 + dream_bitcast_f32(*(float *)((char *)dream_p(p) + 8));
      h = h * 31 + dream_bitcast_f32(*(float *)((char *)dream_p(p) + 12));
  return h;
}

int32_t GpuTexture_hash_code(dream_ptr p) {
  int32_t h = 17;
      h = h * 31 + (int32_t)(*(int32_t *)((char *)dream_p(p) + 0));
      h = h * 31 + (int32_t)(*(int32_t *)((char *)dream_p(p) + 4));
      h = h * 31 + (int32_t)(*(int32_t *)((char *)dream_p(p) + 8));
      h = h * 31 + dream_string_hash(*(dream_ptr *)((char *)dream_p(p) + 16));
  return h;
}

int32_t GpuSurface_hash_code(dream_ptr p) {
  int32_t h = 17;
      h = h * 31 + (int32_t)(*(int32_t *)((char *)dream_p(p) + 0));
  return h;
}

int32_t GpuRenderPipelineDesc_hash_code(dream_ptr p) {
  int32_t h = 17;
      h = h * 31 + (int32_t)(*(int32_t *)((char *)dream_p(p) + 0));
      h = h * 31 + (int32_t)(*(int32_t *)((char *)dream_p(p) + 4));
      h = h * 31 + (int32_t)(*(int32_t *)((char *)dream_p(p) + 8));
      h = h * 31 + (int32_t)(*(uint8_t *)((char *)dream_p(p) + 12));
      h = h * 31 + (int32_t)(*(uint8_t *)((char *)dream_p(p) + 13));
      h = h * 31 + (int32_t)(*(int32_t *)((char *)dream_p(p) + 16));
      h = h * 31 + (int32_t)(*(uint8_t *)((char *)dream_p(p) + 20));
      h = h * 31 + (int32_t)(*(int32_t *)((char *)dream_p(p) + 24));
  return h;
}

int32_t GpuRenderPipeline_hash_code(dream_ptr p) {
  int32_t h = 17;
      h = h * 31 + (int32_t)(*(int32_t *)((char *)dream_p(p) + 0));
  return h;
}

int32_t GpuBuffer_Vertex_hash_code(dream_ptr p) {
  int32_t h = 17;
      h = h * 31 + (int32_t)(*(int32_t *)((char *)dream_p(p) + 0));
      h = h * 31 + (int32_t)(*(int32_t *)((char *)dream_p(p) + 4));
      h = h * 31 + (int32_t)(*(int32_t *)((char *)dream_p(p) + 8));
  return h;
}

int32_t Vertex_hash_code(dream_ptr p) {
  int32_t h = 17;
      h = h * 31 + GpuVec2_hash_code((dream_ptr)((char *)dream_p(p) + 0));
  return h;
}

int32_t Result_bool_GpuError_hash_code(dream_ptr p) {
  int32_t d = p ? *(int32_t *)dream_p(p) : 0;
  int32_t h = 17 * 31 + d;
  switch (d) {
    case 0: {
      h = h * 31 + (int32_t)(*(uint8_t *)((char *)dream_p(p) + 4));
      break;
    }
    case 1: {
      h = h * 31 + GpuError_hash_code(*(dream_ptr *)((char *)dream_p(p) + 8));
      break;
    }
    default: break;
  }
  return h;
}

int32_t Result_GpuSurface_GpuError_hash_code(dream_ptr p) {
  int32_t d = p ? *(int32_t *)dream_p(p) : 0;
  int32_t h = 17 * 31 + d;
  switch (d) {
    case 0: {
      h = h * 31 + GpuSurface_hash_code(*(dream_ptr *)((char *)dream_p(p) + 8));
      break;
    }
    case 1: {
      h = h * 31 + GpuError_hash_code(*(dream_ptr *)((char *)dream_p(p) + 8));
      break;
    }
    default: break;
  }
  return h;
}

int32_t Result_GpuRenderPipeline_GpuError_hash_code(dream_ptr p) {
  int32_t d = p ? *(int32_t *)dream_p(p) : 0;
  int32_t h = 17 * 31 + d;
  switch (d) {
    case 0: {
      h = h * 31 + GpuRenderPipeline_hash_code(*(dream_ptr *)((char *)dream_p(p) + 8));
      break;
    }
    case 1: {
      h = h * 31 + GpuError_hash_code(*(dream_ptr *)((char *)dream_p(p) + 8));
      break;
    }
    default: break;
  }
  return h;
}

int32_t Option_GpuTexture_hash_code(dream_ptr p) {
  int32_t d = p ? *(int32_t *)dream_p(p) : 0;
  int32_t h = 17 * 31 + d;
  switch (d) {
    case 0: {
      h = h * 31 + GpuTexture_hash_code(*(dream_ptr *)((char *)dream_p(p) + 8));
      break;
    }
    case 1: {
      break;
    }
    default: break;
  }
  return h;
}

dream_ptr dream_object_to_string(dream_ptr p) {
  int32_t tag;
  if (!p) return __ds16;
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
    case TAG_STRING: dream_retain(p); return p;
    case TAG_ARRAY: return dream_array_to_string(p);
    case 12: return Vertex_to_string(p);
    case 13: return GpuError_to_string(p);
    case 14: return GpuVec2_to_string(p);
    case 15: return GpuVec4_to_string(p);
    case 16: return GpuTexture_to_string(p);
    case 17: return GpuSurface_to_string(p);
    case 18: return GpuRenderPipelineDesc_to_string(p);
    case 19: return GpuRenderPipeline_to_string(p);
    case 20: return GpuBuffer_Vertex_to_string(p);
    case 21: return Result_bool_GpuError_to_string(p);
    case 22: return Result_GpuSurface_GpuError_to_string(p);
    case 23: return Result_GpuRenderPipeline_GpuError_to_string(p);
    case 24: return Option_GpuTexture_to_string(p);
    default: return __ds17;
  }
}

void dream_print_object(dream_ptr p) { print_string(dream_object_to_string(p)); }

int32_t dream_object_hash_code(dream_ptr p) {
  int32_t tag;
  if (!p) return 0;
  tag = dream_object_tag(p);
  switch (tag) {
    case TAG_INT: case TAG_UINT: case TAG_BOOL: case TAG_CHAR: case TAG_BYTE: return *(int32_t *)dream_p(p);
    case TAG_LONG: case TAG_ULONG: return dream_hash_long(*(int64_t *)dream_p(p));
    case TAG_FLOAT: return dream_bitcast_f32(*(float *)dream_p(p));
    case TAG_DOUBLE: return dream_hash_double(*(double *)dream_p(p));
    case TAG_STRING: return dream_string_hash(p);
    case 12: return Vertex_hash_code(p);
    case 13: return GpuError_hash_code(p);
    case 14: return GpuVec2_hash_code(p);
    case 15: return GpuVec4_hash_code(p);
    case 16: return GpuTexture_hash_code(p);
    case 17: return GpuSurface_hash_code(p);
    case 18: return GpuRenderPipelineDesc_hash_code(p);
    case 19: return GpuRenderPipeline_hash_code(p);
    case 20: return GpuBuffer_Vertex_hash_code(p);
    case 21: return Result_bool_GpuError_hash_code(p);
    case 22: return Result_GpuSurface_GpuError_hash_code(p);
    case 23: return Result_GpuRenderPipeline_GpuError_hash_code(p);
    case 24: return Option_GpuTexture_hash_code(p);
    default: return (int32_t)(uintptr_t)p;
  }
}

static void *dream_iface_0_0[25];
dream_ptr __iface_dispatch_0_0(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_0_0[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_0_1[25];
dream_ptr __iface_dispatch_0_1(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_0_1[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_1_0[25];
dream_ptr __iface_dispatch_1_0(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_1_0[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_2_0[25];
dream_ptr __iface_dispatch_2_0(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_2_0[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_3_0[25];
dream_ptr __iface_dispatch_3_0(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_3_0[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_4_0[25];
dream_ptr __iface_dispatch_4_0(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_4_0[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_5_0[25];
dream_ptr __iface_dispatch_5_0(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_5_0[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_6_0[25];
dream_ptr __iface_dispatch_6_0(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_6_0[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_7_0[25];
dream_ptr __iface_dispatch_7_0(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_7_0[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_8_0[25];
dream_ptr __iface_dispatch_8_0(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_8_0[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_9_0[25];
dream_ptr __iface_dispatch_9_0(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_9_0[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_10_0[25];
dream_ptr __iface_dispatch_10_0(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_10_0[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_11_0[25];
dream_ptr __iface_dispatch_11_0(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_11_0[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_11_1[25];
dream_ptr __iface_dispatch_11_1(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_11_1[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_11_2[25];
dream_ptr __iface_dispatch_11_2(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_11_2[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_11_3[25];
dream_ptr __iface_dispatch_11_3(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_11_3[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_11_4[25];
dream_ptr __iface_dispatch_11_4(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_11_4[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_11_5[25];
dream_ptr __iface_dispatch_11_5(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_11_5[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_11_6[25];
dream_ptr __iface_dispatch_11_6(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_11_6[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_11_7[25];
dream_ptr __iface_dispatch_11_7(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_11_7[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_11_8[25];
dream_ptr __iface_dispatch_11_8(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_11_8[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_12_0[25];
dream_ptr __iface_dispatch_12_0(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_12_0[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_12_1[25];
dream_ptr __iface_dispatch_12_1(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_12_1[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_12_2[25];
dream_ptr __iface_dispatch_12_2(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_12_2[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_12_3[25];
dream_ptr __iface_dispatch_12_3(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_12_3[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_12_4[25];
dream_ptr __iface_dispatch_12_4(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_12_4[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_12_5[25];
dream_ptr __iface_dispatch_12_5(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_12_5[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_12_6[25];
dream_ptr __iface_dispatch_12_6(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_12_6[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_12_7[25];
dream_ptr __iface_dispatch_12_7(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_12_7[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_12_8[25];
dream_ptr __iface_dispatch_12_8(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_12_8[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_12_9[25];
dream_ptr __iface_dispatch_12_9(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_12_9[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_12_10[25];
dream_ptr __iface_dispatch_12_10(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_12_10[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_12_11[25];
dream_ptr __iface_dispatch_12_11(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_12_11[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_13_0[25];
dream_ptr __iface_dispatch_13_0(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_13_0[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_13_1[25];
dream_ptr __iface_dispatch_13_1(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_13_1[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_13_2[25];
dream_ptr __iface_dispatch_13_2(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_13_2[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_13_3[25];
dream_ptr __iface_dispatch_13_3(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_13_3[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_13_4[25];
dream_ptr __iface_dispatch_13_4(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_13_4[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_13_5[25];
dream_ptr __iface_dispatch_13_5(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_13_5[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_13_6[25];
dream_ptr __iface_dispatch_13_6(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_13_6[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_13_7[25];
dream_ptr __iface_dispatch_13_7(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_13_7[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_13_8[25];
dream_ptr __iface_dispatch_13_8(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_13_8[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_14_0[25];
dream_ptr __iface_dispatch_14_0(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_14_0[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_14_1[25];
dream_ptr __iface_dispatch_14_1(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_14_1[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_14_2[25];
dream_ptr __iface_dispatch_14_2(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_14_2[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_14_3[25];
dream_ptr __iface_dispatch_14_3(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_14_3[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_14_4[25];
dream_ptr __iface_dispatch_14_4(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_14_4[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_14_5[25];
dream_ptr __iface_dispatch_14_5(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_14_5[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_14_6[25];
dream_ptr __iface_dispatch_14_6(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_14_6[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_14_7[25];
dream_ptr __iface_dispatch_14_7(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_14_7[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_14_8[25];
dream_ptr __iface_dispatch_14_8(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_14_8[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_14_9[25];
dream_ptr __iface_dispatch_14_9(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_14_9[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_14_10[25];
dream_ptr __iface_dispatch_14_10(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_14_10[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_14_11[25];
dream_ptr __iface_dispatch_14_11(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_14_11[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_15_0[25];
dream_ptr __iface_dispatch_15_0(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_15_0[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_15_1[25];
dream_ptr __iface_dispatch_15_1(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_15_1[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_15_2[25];
dream_ptr __iface_dispatch_15_2(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_15_2[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_15_3[25];
dream_ptr __iface_dispatch_15_3(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_15_3[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_15_4[25];
dream_ptr __iface_dispatch_15_4(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_15_4[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_15_5[25];
dream_ptr __iface_dispatch_15_5(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_15_5[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_15_6[25];
dream_ptr __iface_dispatch_15_6(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_15_6[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_15_7[25];
dream_ptr __iface_dispatch_15_7(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_15_7[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_15_8[25];
dream_ptr __iface_dispatch_15_8(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_15_8[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_16_0[25];
dream_ptr __iface_dispatch_16_0(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_16_0[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_16_1[25];
dream_ptr __iface_dispatch_16_1(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_16_1[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_16_2[25];
dream_ptr __iface_dispatch_16_2(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_16_2[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_16_3[25];
dream_ptr __iface_dispatch_16_3(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_16_3[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_16_4[25];
dream_ptr __iface_dispatch_16_4(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_16_4[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_16_5[25];
dream_ptr __iface_dispatch_16_5(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_16_5[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_16_6[25];
dream_ptr __iface_dispatch_16_6(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_16_6[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_16_7[25];
dream_ptr __iface_dispatch_16_7(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_16_7[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_16_8[25];
dream_ptr __iface_dispatch_16_8(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_16_8[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_16_9[25];
dream_ptr __iface_dispatch_16_9(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_16_9[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_16_10[25];
dream_ptr __iface_dispatch_16_10(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_16_10[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_16_11[25];
dream_ptr __iface_dispatch_16_11(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_16_11[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_17_0[25];
dream_ptr __iface_dispatch_17_0(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_17_0[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_17_1[25];
dream_ptr __iface_dispatch_17_1(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_17_1[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_17_2[25];
dream_ptr __iface_dispatch_17_2(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_17_2[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_17_3[25];
dream_ptr __iface_dispatch_17_3(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_17_3[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_17_4[25];
dream_ptr __iface_dispatch_17_4(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_17_4[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_17_5[25];
dream_ptr __iface_dispatch_17_5(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_17_5[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_17_6[25];
dream_ptr __iface_dispatch_17_6(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_17_6[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_17_7[25];
dream_ptr __iface_dispatch_17_7(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_17_7[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_17_8[25];
dream_ptr __iface_dispatch_17_8(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_17_8[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_18_0[25];
dream_ptr __iface_dispatch_18_0(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_18_0[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_18_1[25];
dream_ptr __iface_dispatch_18_1(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_18_1[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_18_2[25];
dream_ptr __iface_dispatch_18_2(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_18_2[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_18_3[25];
dream_ptr __iface_dispatch_18_3(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_18_3[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_18_4[25];
dream_ptr __iface_dispatch_18_4(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_18_4[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_18_5[25];
dream_ptr __iface_dispatch_18_5(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_18_5[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_18_6[25];
dream_ptr __iface_dispatch_18_6(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_18_6[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_18_7[25];
dream_ptr __iface_dispatch_18_7(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_18_7[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_18_8[25];
dream_ptr __iface_dispatch_18_8(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_18_8[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_18_9[25];
dream_ptr __iface_dispatch_18_9(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_18_9[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_18_10[25];
dream_ptr __iface_dispatch_18_10(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_18_10[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_18_11[25];
dream_ptr __iface_dispatch_18_11(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_18_11[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_19_0[25];
dream_ptr __iface_dispatch_19_0(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_19_0[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_19_1[25];
dream_ptr __iface_dispatch_19_1(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_19_1[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_19_2[25];
dream_ptr __iface_dispatch_19_2(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_19_2[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_19_3[25];
dream_ptr __iface_dispatch_19_3(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_19_3[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_19_4[25];
dream_ptr __iface_dispatch_19_4(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_19_4[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_19_5[25];
dream_ptr __iface_dispatch_19_5(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_19_5[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_19_6[25];
dream_ptr __iface_dispatch_19_6(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_19_6[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_19_7[25];
dream_ptr __iface_dispatch_19_7(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_19_7[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_19_8[25];
dream_ptr __iface_dispatch_19_8(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_19_8[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_20_0[25];
dream_ptr __iface_dispatch_20_0(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_20_0[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_20_1[25];
dream_ptr __iface_dispatch_20_1(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_20_1[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_20_2[25];
dream_ptr __iface_dispatch_20_2(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_20_2[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_20_3[25];
dream_ptr __iface_dispatch_20_3(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_20_3[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_20_4[25];
dream_ptr __iface_dispatch_20_4(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_20_4[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_20_5[25];
dream_ptr __iface_dispatch_20_5(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_20_5[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_20_6[25];
dream_ptr __iface_dispatch_20_6(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_20_6[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_20_7[25];
dream_ptr __iface_dispatch_20_7(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_20_7[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_20_8[25];
dream_ptr __iface_dispatch_20_8(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_20_8[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_20_9[25];
dream_ptr __iface_dispatch_20_9(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_20_9[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_20_10[25];
dream_ptr __iface_dispatch_20_10(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_20_10[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_20_11[25];
dream_ptr __iface_dispatch_20_11(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_20_11[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_21_0[25];
dream_ptr __iface_dispatch_21_0(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_21_0[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_21_1[25];
dream_ptr __iface_dispatch_21_1(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_21_1[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_21_2[25];
dream_ptr __iface_dispatch_21_2(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_21_2[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_21_3[25];
dream_ptr __iface_dispatch_21_3(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_21_3[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_21_4[25];
dream_ptr __iface_dispatch_21_4(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_21_4[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_21_5[25];
dream_ptr __iface_dispatch_21_5(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_21_5[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_21_6[25];
dream_ptr __iface_dispatch_21_6(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_21_6[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_21_7[25];
dream_ptr __iface_dispatch_21_7(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_21_7[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_21_8[25];
dream_ptr __iface_dispatch_21_8(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_21_8[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_22_0[25];
dream_ptr __iface_dispatch_22_0(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_22_0[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_22_1[25];
dream_ptr __iface_dispatch_22_1(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_22_1[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_22_2[25];
dream_ptr __iface_dispatch_22_2(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_22_2[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_22_3[25];
dream_ptr __iface_dispatch_22_3(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_22_3[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_22_4[25];
dream_ptr __iface_dispatch_22_4(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_22_4[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_22_5[25];
dream_ptr __iface_dispatch_22_5(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_22_5[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_22_6[25];
dream_ptr __iface_dispatch_22_6(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_22_6[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_22_7[25];
dream_ptr __iface_dispatch_22_7(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_22_7[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_22_8[25];
dream_ptr __iface_dispatch_22_8(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_22_8[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_22_9[25];
dream_ptr __iface_dispatch_22_9(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_22_9[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_22_10[25];
dream_ptr __iface_dispatch_22_10(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_22_10[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_22_11[25];
dream_ptr __iface_dispatch_22_11(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_22_11[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_23_0[25];
dream_ptr __iface_dispatch_23_0(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_23_0[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_23_1[25];
dream_ptr __iface_dispatch_23_1(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_23_1[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_23_2[25];
dream_ptr __iface_dispatch_23_2(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_23_2[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_23_3[25];
dream_ptr __iface_dispatch_23_3(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_23_3[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_23_4[25];
dream_ptr __iface_dispatch_23_4(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_23_4[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_23_5[25];
dream_ptr __iface_dispatch_23_5(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_23_5[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_23_6[25];
dream_ptr __iface_dispatch_23_6(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_23_6[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_23_7[25];
dream_ptr __iface_dispatch_23_7(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_23_7[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_23_8[25];
dream_ptr __iface_dispatch_23_8(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_23_8[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_24_0[25];
dream_ptr __iface_dispatch_24_0(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_24_0[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_24_1[25];
dream_ptr __iface_dispatch_24_1(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_24_1[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_24_2[25];
dream_ptr __iface_dispatch_24_2(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_24_2[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_24_3[25];
dream_ptr __iface_dispatch_24_3(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_24_3[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_24_4[25];
dream_ptr __iface_dispatch_24_4(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_24_4[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_24_5[25];
dream_ptr __iface_dispatch_24_5(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_24_5[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_24_6[25];
dream_ptr __iface_dispatch_24_6(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_24_6[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_24_7[25];
dream_ptr __iface_dispatch_24_7(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_24_7[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_24_8[25];
dream_ptr __iface_dispatch_24_8(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_24_8[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_24_9[25];
dream_ptr __iface_dispatch_24_9(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_24_9[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_24_10[25];
dream_ptr __iface_dispatch_24_10(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_24_10[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_24_11[25];
dream_ptr __iface_dispatch_24_11(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_24_11[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_25_0[25];
dream_ptr __iface_dispatch_25_0(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_25_0[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_26_0[25];
dream_ptr __iface_dispatch_26_0(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_26_0[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_27_0[25];
dream_ptr __iface_dispatch_27_0(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_27_0[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_28_0[25];
dream_ptr __iface_dispatch_28_0(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_28_0[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_29_0[25];
dream_ptr __iface_dispatch_29_0(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_29_0[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

static void *dream_iface_30_0[25];
dream_ptr __iface_dispatch_30_0(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {
  int32_t tag = dream_object_tag(this);
  dream_fn fn = (dream_fn)dream_iface_30_0[tag];
  if (!fn) abort();
  return fn(this, a0, a1, a2, a3, a4, a5, a6);
}

dream_ptr make_vert(float l0, float l1) {
  _Alignas(8) unsigned char __vs2[8] = {0};
  dream_ptr l2 = (dream_ptr)(uintptr_t)__vs2;
  float l3 = 0;
  float l4 = 0;
  _Alignas(8) unsigned char __vs5[8] = {0};
  dream_ptr l5 = (dream_ptr)(uintptr_t)__vs5;
  goto L0;
L0:;
  ({ dream_ptr __v = ({ dream_ptr __o = dream_malloc(8, 12); memset(dream_p(__o), 0, 8); __o; }); memcpy(dream_p(l2), dream_p(__v), 8); dream_free(__v); });
  l4 = (l1);
  ({ dream_ptr __v = ({ dream_ptr __o = dream_malloc(8, 14); memset(dream_p(__o), 0, 8); __o; }); memcpy(dream_p(l5), dream_p(__v), 8); dream_free(__v); });
  *(float*)((char*)dream_p(l5) + 0) = (float)(l0);
  *(float*)((char*)dream_p(l5) + 4) = (float)(l4);
  memcpy((char*)dream_p(l2) + 0, dream_p(l5), 8); ;
  { dream_ptr __r = dream_malloc(8, 12); memcpy(dream_p(__r), dream_p(l2), 8); return __r; }
L1:;
  abort();
L2:;
  abort();
}

dream_ptr main_dream(void) {
  dream_ptr __self = dream_new_future(480, 26, 0);
  dream_enqueue(__self);
  return __self;
}

int32_t poll_main_dream(dream_ptr __self) {
  dream_ptr l0 = *(dream_ptr *)((char *)dream_p(__self) + 64);
  dream_ptr l1 = *(dream_ptr *)((char *)dream_p(__self) + 72);
  dream_ptr l2 = *(dream_ptr *)((char *)dream_p(__self) + 80);
  int64_t l3 = *(int64_t *)((char *)dream_p(__self) + 88);
  int64_t l4 = *(int64_t *)((char *)dream_p(__self) + 96);
  dream_ptr l5 = (dream_ptr)((char *)dream_p(__self) + 104);
  dream_ptr l6 = *(dream_ptr *)((char *)dream_p(__self) + 136);
  dream_ptr l7 = *(dream_ptr *)((char *)dream_p(__self) + 144);
  dream_ptr l8 = (dream_ptr)((char *)dream_p(__self) + 152);
  dream_ptr l9 = (dream_ptr)((char *)dream_p(__self) + 168);
  int64_t l10 = *(int64_t *)((char *)dream_p(__self) + 184);
  float l11 = *(float *)((char *)dream_p(__self) + 192);
  float l12 = *(float *)((char *)dream_p(__self) + 200);
  int64_t l13 = *(int64_t *)((char *)dream_p(__self) + 208);
  float l14 = *(float *)((char *)dream_p(__self) + 216);
  dream_ptr l15 = *(dream_ptr *)((char *)dream_p(__self) + 224);
  dream_ptr l16 = *(dream_ptr *)((char *)dream_p(__self) + 232);
  dream_ptr l17 = *(dream_ptr *)((char *)dream_p(__self) + 240);
  int32_t l18 = *(int32_t *)((char *)dream_p(__self) + 248);
  int32_t l19 = *(int32_t *)((char *)dream_p(__self) + 256);
  dream_ptr l20 = *(dream_ptr *)((char *)dream_p(__self) + 264);
  int32_t l21 = *(int32_t *)((char *)dream_p(__self) + 272);
  int32_t l22 = *(int32_t *)((char *)dream_p(__self) + 280);
  dream_ptr l23 = *(dream_ptr *)((char *)dream_p(__self) + 288);
  dream_ptr l24 = *(dream_ptr *)((char *)dream_p(__self) + 296);
  int32_t l25 = *(int32_t *)((char *)dream_p(__self) + 304);
  dream_ptr l26 = *(dream_ptr *)((char *)dream_p(__self) + 312);
  float l27 = *(float *)((char *)dream_p(__self) + 320);
  float l28 = *(float *)((char *)dream_p(__self) + 328);
  dream_ptr l29 = (dream_ptr)((char *)dream_p(__self) + 336);
  float l30 = *(float *)((char *)dream_p(__self) + 344);
  dream_ptr l31 = (dream_ptr)((char *)dream_p(__self) + 352);
  float l32 = *(float *)((char *)dream_p(__self) + 360);
  dream_ptr l33 = (dream_ptr)((char *)dream_p(__self) + 368);
  dream_ptr l34 = *(dream_ptr *)((char *)dream_p(__self) + 376);
  int32_t l35 = *(int32_t *)((char *)dream_p(__self) + 384);
  int32_t l36 = *(int32_t *)((char *)dream_p(__self) + 392);
  int32_t l37 = *(int32_t *)((char *)dream_p(__self) + 400);
  int32_t l38 = *(int32_t *)((char *)dream_p(__self) + 408);
  int64_t l39 = *(int64_t *)((char *)dream_p(__self) + 416);
  float l40 = *(float *)((char *)dream_p(__self) + 424);
  dream_ptr l41 = *(dream_ptr *)((char *)dream_p(__self) + 432);
  dream_ptr l42 = *(dream_ptr *)((char *)dream_p(__self) + 440);
  dream_ptr l43 = *(dream_ptr *)((char *)dream_p(__self) + 448);
  dream_ptr l44 = *(dream_ptr *)((char *)dream_p(__self) + 456);
  dream_ptr l45 = *(dream_ptr *)((char *)dream_p(__self) + 464);
  dream_ptr l46 = *(dream_ptr *)((char *)dream_p(__self) + 472);
  int32_t __st = *(int32_t *)dream_p(__self);
  switch (__st) {
    case 1: goto L1;
    case 2: goto L2;
    case 3: goto L3;
    case 4: goto L4;
    case 5: goto L5;
    case 6: goto L6;
    case 7: goto L7;
    case 8: goto L8;
    case 9: goto L9;
    case 10: goto L10;
    case 11: goto L11;
    case 12: goto L12;
    case 13: goto L13;
    case 14: goto L14;
    case 15: goto L15;
    case 16: goto L16;
    case 17: goto L17;
    case 18: goto L18;
    case 19: goto L19;
    case 20: goto L20;
    case 21: goto L21;
    case 22: goto L22;
    case 23: goto L23;
    case 24: goto L24;
    case 25: goto L25;
    case 26: goto L26;
    case 27: goto L27;
    case 28: goto L28;
    case 29: goto L29;
    default: break;
  }
  goto L0;
L0:;
  dream_release(l16);
  l16 = (Gpu_try_init());
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = l1;
  *(dream_ptr *)((char *)dream_p(__self) + 80) = l2;
  *(int64_t *)((char *)dream_p(__self) + 88) = l3;
  *(int64_t *)((char *)dream_p(__self) + 96) = l4;
  *(dream_ptr *)((char *)dream_p(__self) + 136) = l6;
  *(dream_ptr *)((char *)dream_p(__self) + 144) = l7;
  *(int64_t *)((char *)dream_p(__self) + 184) = l10;
  *(float *)((char *)dream_p(__self) + 192) = l11;
  *(float *)((char *)dream_p(__self) + 200) = l12;
  *(int64_t *)((char *)dream_p(__self) + 208) = l13;
  *(float *)((char *)dream_p(__self) + 216) = l14;
  *(dream_ptr *)((char *)dream_p(__self) + 224) = l15;
  *(dream_ptr *)((char *)dream_p(__self) + 232) = l16;
  *(dream_ptr *)((char *)dream_p(__self) + 240) = l17;
  *(int32_t *)((char *)dream_p(__self) + 248) = l18;
  *(int32_t *)((char *)dream_p(__self) + 256) = l19;
  *(dream_ptr *)((char *)dream_p(__self) + 264) = l20;
  *(int32_t *)((char *)dream_p(__self) + 272) = l21;
  *(int32_t *)((char *)dream_p(__self) + 280) = l22;
  *(dream_ptr *)((char *)dream_p(__self) + 288) = l23;
  *(dream_ptr *)((char *)dream_p(__self) + 296) = l24;
  *(int32_t *)((char *)dream_p(__self) + 304) = l25;
  *(dream_ptr *)((char *)dream_p(__self) + 312) = l26;
  *(float *)((char *)dream_p(__self) + 320) = l27;
  *(float *)((char *)dream_p(__self) + 328) = l28;
  *(float *)((char *)dream_p(__self) + 344) = l30;
  *(float *)((char *)dream_p(__self) + 360) = l32;
  *(dream_ptr *)((char *)dream_p(__self) + 376) = l34;
  *(int32_t *)((char *)dream_p(__self) + 384) = l35;
  *(int32_t *)((char *)dream_p(__self) + 392) = l36;
  *(int32_t *)((char *)dream_p(__self) + 400) = l37;
  *(int32_t *)((char *)dream_p(__self) + 408) = l38;
  *(int64_t *)((char *)dream_p(__self) + 416) = l39;
  *(float *)((char *)dream_p(__self) + 424) = l40;
  *(dream_ptr *)((char *)dream_p(__self) + 432) = l41;
  *(dream_ptr *)((char *)dream_p(__self) + 440) = l42;
  *(dream_ptr *)((char *)dream_p(__self) + 448) = l43;
  *(dream_ptr *)((char *)dream_p(__self) + 456) = l44;
  *(dream_ptr *)((char *)dream_p(__self) + 464) = l45;
  *(dream_ptr *)((char *)dream_p(__self) + 472) = l46;
  *(int32_t *)dream_p(__self) = 1;
  dream_await(__self, l16);
  return 0;
L1:;
  {
    dream_ptr __ch = *(dream_ptr *)((char *)dream_p(__self) + 36);
    l17 = (dream_ptr)*(dream_ptr *)((char *)dream_p(__ch) + 8);
  }
  l0 = (l17);
  l18 = (Result_bool_GpuError_is_err(l0));
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = l1;
  *(dream_ptr *)((char *)dream_p(__self) + 80) = l2;
  *(int64_t *)((char *)dream_p(__self) + 88) = l3;
  *(int64_t *)((char *)dream_p(__self) + 96) = l4;
  *(dream_ptr *)((char *)dream_p(__self) + 136) = l6;
  *(dream_ptr *)((char *)dream_p(__self) + 144) = l7;
  *(int64_t *)((char *)dream_p(__self) + 184) = l10;
  *(float *)((char *)dream_p(__self) + 192) = l11;
  *(float *)((char *)dream_p(__self) + 200) = l12;
  *(int64_t *)((char *)dream_p(__self) + 208) = l13;
  *(float *)((char *)dream_p(__self) + 216) = l14;
  *(dream_ptr *)((char *)dream_p(__self) + 224) = l15;
  *(dream_ptr *)((char *)dream_p(__self) + 232) = l16;
  *(dream_ptr *)((char *)dream_p(__self) + 240) = l17;
  *(int32_t *)((char *)dream_p(__self) + 248) = l18;
  *(int32_t *)((char *)dream_p(__self) + 256) = l19;
  *(dream_ptr *)((char *)dream_p(__self) + 264) = l20;
  *(int32_t *)((char *)dream_p(__self) + 272) = l21;
  *(int32_t *)((char *)dream_p(__self) + 280) = l22;
  *(dream_ptr *)((char *)dream_p(__self) + 288) = l23;
  *(dream_ptr *)((char *)dream_p(__self) + 296) = l24;
  *(int32_t *)((char *)dream_p(__self) + 304) = l25;
  *(dream_ptr *)((char *)dream_p(__self) + 312) = l26;
  *(float *)((char *)dream_p(__self) + 320) = l27;
  *(float *)((char *)dream_p(__self) + 328) = l28;
  *(float *)((char *)dream_p(__self) + 344) = l30;
  *(float *)((char *)dream_p(__self) + 360) = l32;
  *(dream_ptr *)((char *)dream_p(__self) + 376) = l34;
  *(int32_t *)((char *)dream_p(__self) + 384) = l35;
  *(int32_t *)((char *)dream_p(__self) + 392) = l36;
  *(int32_t *)((char *)dream_p(__self) + 400) = l37;
  *(int32_t *)((char *)dream_p(__self) + 408) = l38;
  *(int64_t *)((char *)dream_p(__self) + 416) = l39;
  *(float *)((char *)dream_p(__self) + 424) = l40;
  *(dream_ptr *)((char *)dream_p(__self) + 432) = l41;
  *(dream_ptr *)((char *)dream_p(__self) + 440) = l42;
  *(dream_ptr *)((char *)dream_p(__self) + 448) = l43;
  *(dream_ptr *)((char *)dream_p(__self) + 456) = l44;
  *(dream_ptr *)((char *)dream_p(__self) + 464) = l45;
  *(dream_ptr *)((char *)dream_p(__self) + 472) = l46;
  if (l18) goto L2; else goto L3;
L2:;
  print_string(__ds0);
  print_char(10);
  release_Result_GpuSurface_GpuError(l1);
  release_GpuSurface(l2);
  release_GpuRenderPipeline(l7);
  dream_release(l15);
  dream_release(l16);
  release_Result_bool_GpuError(l17);
  release_GpuSurface(l20);
  dream_release(l23);
  release_Result_GpuRenderPipeline_GpuError(l24);
  release_GpuRenderPipeline(l26);
  release_array_t87(l34);
  dream_release(l41);
  dream_release(l42);
  release_Result_bool_GpuError(l43);
  dream_release(l44);
  release_Result_bool_GpuError(l45);
  dream_release(l46);
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = l1;
  *(dream_ptr *)((char *)dream_p(__self) + 80) = l2;
  *(int64_t *)((char *)dream_p(__self) + 88) = l3;
  *(int64_t *)((char *)dream_p(__self) + 96) = l4;
  *(dream_ptr *)((char *)dream_p(__self) + 136) = l6;
  *(dream_ptr *)((char *)dream_p(__self) + 144) = l7;
  *(int64_t *)((char *)dream_p(__self) + 184) = l10;
  *(float *)((char *)dream_p(__self) + 192) = l11;
  *(float *)((char *)dream_p(__self) + 200) = l12;
  *(int64_t *)((char *)dream_p(__self) + 208) = l13;
  *(float *)((char *)dream_p(__self) + 216) = l14;
  *(dream_ptr *)((char *)dream_p(__self) + 224) = l15;
  *(dream_ptr *)((char *)dream_p(__self) + 232) = l16;
  *(dream_ptr *)((char *)dream_p(__self) + 240) = l17;
  *(int32_t *)((char *)dream_p(__self) + 248) = l18;
  *(int32_t *)((char *)dream_p(__self) + 256) = l19;
  *(dream_ptr *)((char *)dream_p(__self) + 264) = l20;
  *(int32_t *)((char *)dream_p(__self) + 272) = l21;
  *(int32_t *)((char *)dream_p(__self) + 280) = l22;
  *(dream_ptr *)((char *)dream_p(__self) + 288) = l23;
  *(dream_ptr *)((char *)dream_p(__self) + 296) = l24;
  *(int32_t *)((char *)dream_p(__self) + 304) = l25;
  *(dream_ptr *)((char *)dream_p(__self) + 312) = l26;
  *(float *)((char *)dream_p(__self) + 320) = l27;
  *(float *)((char *)dream_p(__self) + 328) = l28;
  *(float *)((char *)dream_p(__self) + 344) = l30;
  *(float *)((char *)dream_p(__self) + 360) = l32;
  *(dream_ptr *)((char *)dream_p(__self) + 376) = l34;
  *(int32_t *)((char *)dream_p(__self) + 384) = l35;
  *(int32_t *)((char *)dream_p(__self) + 392) = l36;
  *(int32_t *)((char *)dream_p(__self) + 400) = l37;
  *(int32_t *)((char *)dream_p(__self) + 408) = l38;
  *(int64_t *)((char *)dream_p(__self) + 416) = l39;
  *(float *)((char *)dream_p(__self) + 424) = l40;
  *(dream_ptr *)((char *)dream_p(__self) + 432) = l41;
  *(dream_ptr *)((char *)dream_p(__self) + 440) = l42;
  *(dream_ptr *)((char *)dream_p(__self) + 448) = l43;
  *(dream_ptr *)((char *)dream_p(__self) + 456) = l44;
  *(dream_ptr *)((char *)dream_p(__self) + 464) = l45;
  *(dream_ptr *)((char *)dream_p(__self) + 472) = l46;
  dream_async_complete(__self, 0); return 0;
L3:;
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = l1;
  *(dream_ptr *)((char *)dream_p(__self) + 80) = l2;
  *(int64_t *)((char *)dream_p(__self) + 88) = l3;
  *(int64_t *)((char *)dream_p(__self) + 96) = l4;
  *(dream_ptr *)((char *)dream_p(__self) + 136) = l6;
  *(dream_ptr *)((char *)dream_p(__self) + 144) = l7;
  *(int64_t *)((char *)dream_p(__self) + 184) = l10;
  *(float *)((char *)dream_p(__self) + 192) = l11;
  *(float *)((char *)dream_p(__self) + 200) = l12;
  *(int64_t *)((char *)dream_p(__self) + 208) = l13;
  *(float *)((char *)dream_p(__self) + 216) = l14;
  *(dream_ptr *)((char *)dream_p(__self) + 224) = l15;
  *(dream_ptr *)((char *)dream_p(__self) + 232) = l16;
  *(dream_ptr *)((char *)dream_p(__self) + 240) = l17;
  *(int32_t *)((char *)dream_p(__self) + 248) = l18;
  *(int32_t *)((char *)dream_p(__self) + 256) = l19;
  *(dream_ptr *)((char *)dream_p(__self) + 264) = l20;
  *(int32_t *)((char *)dream_p(__self) + 272) = l21;
  *(int32_t *)((char *)dream_p(__self) + 280) = l22;
  *(dream_ptr *)((char *)dream_p(__self) + 288) = l23;
  *(dream_ptr *)((char *)dream_p(__self) + 296) = l24;
  *(int32_t *)((char *)dream_p(__self) + 304) = l25;
  *(dream_ptr *)((char *)dream_p(__self) + 312) = l26;
  *(float *)((char *)dream_p(__self) + 320) = l27;
  *(float *)((char *)dream_p(__self) + 328) = l28;
  *(float *)((char *)dream_p(__self) + 344) = l30;
  *(float *)((char *)dream_p(__self) + 360) = l32;
  *(dream_ptr *)((char *)dream_p(__self) + 376) = l34;
  *(int32_t *)((char *)dream_p(__self) + 384) = l35;
  *(int32_t *)((char *)dream_p(__self) + 392) = l36;
  *(int32_t *)((char *)dream_p(__self) + 400) = l37;
  *(int32_t *)((char *)dream_p(__self) + 408) = l38;
  *(int64_t *)((char *)dream_p(__self) + 416) = l39;
  *(float *)((char *)dream_p(__self) + 424) = l40;
  *(dream_ptr *)((char *)dream_p(__self) + 432) = l41;
  *(dream_ptr *)((char *)dream_p(__self) + 440) = l42;
  *(dream_ptr *)((char *)dream_p(__self) + 448) = l43;
  *(dream_ptr *)((char *)dream_p(__self) + 456) = l44;
  *(dream_ptr *)((char *)dream_p(__self) + 464) = l45;
  *(dream_ptr *)((char *)dream_p(__self) + 472) = l46;
  goto L4;
L4:;
  release_Result_GpuSurface_GpuError(l1);
  dream_retain(__ds1);
  l1 = (GpuSurface_create(__ds1, 1280, 720));
  l19 = (Result_GpuSurface_GpuError_is_err(l1));
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = l1;
  *(dream_ptr *)((char *)dream_p(__self) + 80) = l2;
  *(int64_t *)((char *)dream_p(__self) + 88) = l3;
  *(int64_t *)((char *)dream_p(__self) + 96) = l4;
  *(dream_ptr *)((char *)dream_p(__self) + 136) = l6;
  *(dream_ptr *)((char *)dream_p(__self) + 144) = l7;
  *(int64_t *)((char *)dream_p(__self) + 184) = l10;
  *(float *)((char *)dream_p(__self) + 192) = l11;
  *(float *)((char *)dream_p(__self) + 200) = l12;
  *(int64_t *)((char *)dream_p(__self) + 208) = l13;
  *(float *)((char *)dream_p(__self) + 216) = l14;
  *(dream_ptr *)((char *)dream_p(__self) + 224) = l15;
  *(dream_ptr *)((char *)dream_p(__self) + 232) = l16;
  *(dream_ptr *)((char *)dream_p(__self) + 240) = l17;
  *(int32_t *)((char *)dream_p(__self) + 248) = l18;
  *(int32_t *)((char *)dream_p(__self) + 256) = l19;
  *(dream_ptr *)((char *)dream_p(__self) + 264) = l20;
  *(int32_t *)((char *)dream_p(__self) + 272) = l21;
  *(int32_t *)((char *)dream_p(__self) + 280) = l22;
  *(dream_ptr *)((char *)dream_p(__self) + 288) = l23;
  *(dream_ptr *)((char *)dream_p(__self) + 296) = l24;
  *(int32_t *)((char *)dream_p(__self) + 304) = l25;
  *(dream_ptr *)((char *)dream_p(__self) + 312) = l26;
  *(float *)((char *)dream_p(__self) + 320) = l27;
  *(float *)((char *)dream_p(__self) + 328) = l28;
  *(float *)((char *)dream_p(__self) + 344) = l30;
  *(float *)((char *)dream_p(__self) + 360) = l32;
  *(dream_ptr *)((char *)dream_p(__self) + 376) = l34;
  *(int32_t *)((char *)dream_p(__self) + 384) = l35;
  *(int32_t *)((char *)dream_p(__self) + 392) = l36;
  *(int32_t *)((char *)dream_p(__self) + 400) = l37;
  *(int32_t *)((char *)dream_p(__self) + 408) = l38;
  *(int64_t *)((char *)dream_p(__self) + 416) = l39;
  *(float *)((char *)dream_p(__self) + 424) = l40;
  *(dream_ptr *)((char *)dream_p(__self) + 432) = l41;
  *(dream_ptr *)((char *)dream_p(__self) + 440) = l42;
  *(dream_ptr *)((char *)dream_p(__self) + 448) = l43;
  *(dream_ptr *)((char *)dream_p(__self) + 456) = l44;
  *(dream_ptr *)((char *)dream_p(__self) + 464) = l45;
  *(dream_ptr *)((char *)dream_p(__self) + 472) = l46;
  if (l19) goto L5; else goto L6;
L5:;
  print_string(__ds2);
  print_char(10);
  release_Result_GpuSurface_GpuError(l1);
  release_GpuSurface(l2);
  release_GpuRenderPipeline(l7);
  dream_release(l15);
  dream_release(l16);
  release_Result_bool_GpuError(l17);
  release_GpuSurface(l20);
  dream_release(l23);
  release_Result_GpuRenderPipeline_GpuError(l24);
  release_GpuRenderPipeline(l26);
  release_array_t87(l34);
  dream_release(l41);
  dream_release(l42);
  release_Result_bool_GpuError(l43);
  dream_release(l44);
  release_Result_bool_GpuError(l45);
  dream_release(l46);
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = l1;
  *(dream_ptr *)((char *)dream_p(__self) + 80) = l2;
  *(int64_t *)((char *)dream_p(__self) + 88) = l3;
  *(int64_t *)((char *)dream_p(__self) + 96) = l4;
  *(dream_ptr *)((char *)dream_p(__self) + 136) = l6;
  *(dream_ptr *)((char *)dream_p(__self) + 144) = l7;
  *(int64_t *)((char *)dream_p(__self) + 184) = l10;
  *(float *)((char *)dream_p(__self) + 192) = l11;
  *(float *)((char *)dream_p(__self) + 200) = l12;
  *(int64_t *)((char *)dream_p(__self) + 208) = l13;
  *(float *)((char *)dream_p(__self) + 216) = l14;
  *(dream_ptr *)((char *)dream_p(__self) + 224) = l15;
  *(dream_ptr *)((char *)dream_p(__self) + 232) = l16;
  *(dream_ptr *)((char *)dream_p(__self) + 240) = l17;
  *(int32_t *)((char *)dream_p(__self) + 248) = l18;
  *(int32_t *)((char *)dream_p(__self) + 256) = l19;
  *(dream_ptr *)((char *)dream_p(__self) + 264) = l20;
  *(int32_t *)((char *)dream_p(__self) + 272) = l21;
  *(int32_t *)((char *)dream_p(__self) + 280) = l22;
  *(dream_ptr *)((char *)dream_p(__self) + 288) = l23;
  *(dream_ptr *)((char *)dream_p(__self) + 296) = l24;
  *(int32_t *)((char *)dream_p(__self) + 304) = l25;
  *(dream_ptr *)((char *)dream_p(__self) + 312) = l26;
  *(float *)((char *)dream_p(__self) + 320) = l27;
  *(float *)((char *)dream_p(__self) + 328) = l28;
  *(float *)((char *)dream_p(__self) + 344) = l30;
  *(float *)((char *)dream_p(__self) + 360) = l32;
  *(dream_ptr *)((char *)dream_p(__self) + 376) = l34;
  *(int32_t *)((char *)dream_p(__self) + 384) = l35;
  *(int32_t *)((char *)dream_p(__self) + 392) = l36;
  *(int32_t *)((char *)dream_p(__self) + 400) = l37;
  *(int32_t *)((char *)dream_p(__self) + 408) = l38;
  *(int64_t *)((char *)dream_p(__self) + 416) = l39;
  *(float *)((char *)dream_p(__self) + 424) = l40;
  *(dream_ptr *)((char *)dream_p(__self) + 432) = l41;
  *(dream_ptr *)((char *)dream_p(__self) + 440) = l42;
  *(dream_ptr *)((char *)dream_p(__self) + 448) = l43;
  *(dream_ptr *)((char *)dream_p(__self) + 456) = l44;
  *(dream_ptr *)((char *)dream_p(__self) + 464) = l45;
  *(dream_ptr *)((char *)dream_p(__self) + 472) = l46;
  dream_async_complete(__self, 0); return 0;
L6:;
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = l1;
  *(dream_ptr *)((char *)dream_p(__self) + 80) = l2;
  *(int64_t *)((char *)dream_p(__self) + 88) = l3;
  *(int64_t *)((char *)dream_p(__self) + 96) = l4;
  *(dream_ptr *)((char *)dream_p(__self) + 136) = l6;
  *(dream_ptr *)((char *)dream_p(__self) + 144) = l7;
  *(int64_t *)((char *)dream_p(__self) + 184) = l10;
  *(float *)((char *)dream_p(__self) + 192) = l11;
  *(float *)((char *)dream_p(__self) + 200) = l12;
  *(int64_t *)((char *)dream_p(__self) + 208) = l13;
  *(float *)((char *)dream_p(__self) + 216) = l14;
  *(dream_ptr *)((char *)dream_p(__self) + 224) = l15;
  *(dream_ptr *)((char *)dream_p(__self) + 232) = l16;
  *(dream_ptr *)((char *)dream_p(__self) + 240) = l17;
  *(int32_t *)((char *)dream_p(__self) + 248) = l18;
  *(int32_t *)((char *)dream_p(__self) + 256) = l19;
  *(dream_ptr *)((char *)dream_p(__self) + 264) = l20;
  *(int32_t *)((char *)dream_p(__self) + 272) = l21;
  *(int32_t *)((char *)dream_p(__self) + 280) = l22;
  *(dream_ptr *)((char *)dream_p(__self) + 288) = l23;
  *(dream_ptr *)((char *)dream_p(__self) + 296) = l24;
  *(int32_t *)((char *)dream_p(__self) + 304) = l25;
  *(dream_ptr *)((char *)dream_p(__self) + 312) = l26;
  *(float *)((char *)dream_p(__self) + 320) = l27;
  *(float *)((char *)dream_p(__self) + 328) = l28;
  *(float *)((char *)dream_p(__self) + 344) = l30;
  *(float *)((char *)dream_p(__self) + 360) = l32;
  *(dream_ptr *)((char *)dream_p(__self) + 376) = l34;
  *(int32_t *)((char *)dream_p(__self) + 384) = l35;
  *(int32_t *)((char *)dream_p(__self) + 392) = l36;
  *(int32_t *)((char *)dream_p(__self) + 400) = l37;
  *(int32_t *)((char *)dream_p(__self) + 408) = l38;
  *(int64_t *)((char *)dream_p(__self) + 416) = l39;
  *(float *)((char *)dream_p(__self) + 424) = l40;
  *(dream_ptr *)((char *)dream_p(__self) + 432) = l41;
  *(dream_ptr *)((char *)dream_p(__self) + 440) = l42;
  *(dream_ptr *)((char *)dream_p(__self) + 448) = l43;
  *(dream_ptr *)((char *)dream_p(__self) + 456) = l44;
  *(dream_ptr *)((char *)dream_p(__self) + 464) = l45;
  *(dream_ptr *)((char *)dream_p(__self) + 472) = l46;
  goto L7;
L7:;
  release_GpuSurface(l20);
  l20 = (({ dream_ptr __o = dream_malloc(4, 17); memset(dream_p(__o), 0, 4); __o; }));
  release_GpuSurface(l2);
  l2 = (Result_GpuSurface_GpuError_unwrap_or(l1, l20));
  l20 = (0);
  l3 = (GpuSurface_width(l2));
  l4 = (GpuSurface_height(l2));
  l21 = ((l3) < (1));
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = l1;
  *(dream_ptr *)((char *)dream_p(__self) + 80) = l2;
  *(int64_t *)((char *)dream_p(__self) + 88) = l3;
  *(int64_t *)((char *)dream_p(__self) + 96) = l4;
  *(dream_ptr *)((char *)dream_p(__self) + 136) = l6;
  *(dream_ptr *)((char *)dream_p(__self) + 144) = l7;
  *(int64_t *)((char *)dream_p(__self) + 184) = l10;
  *(float *)((char *)dream_p(__self) + 192) = l11;
  *(float *)((char *)dream_p(__self) + 200) = l12;
  *(int64_t *)((char *)dream_p(__self) + 208) = l13;
  *(float *)((char *)dream_p(__self) + 216) = l14;
  *(dream_ptr *)((char *)dream_p(__self) + 224) = l15;
  *(dream_ptr *)((char *)dream_p(__self) + 232) = l16;
  *(dream_ptr *)((char *)dream_p(__self) + 240) = l17;
  *(int32_t *)((char *)dream_p(__self) + 248) = l18;
  *(int32_t *)((char *)dream_p(__self) + 256) = l19;
  *(dream_ptr *)((char *)dream_p(__self) + 264) = l20;
  *(int32_t *)((char *)dream_p(__self) + 272) = l21;
  *(int32_t *)((char *)dream_p(__self) + 280) = l22;
  *(dream_ptr *)((char *)dream_p(__self) + 288) = l23;
  *(dream_ptr *)((char *)dream_p(__self) + 296) = l24;
  *(int32_t *)((char *)dream_p(__self) + 304) = l25;
  *(dream_ptr *)((char *)dream_p(__self) + 312) = l26;
  *(float *)((char *)dream_p(__self) + 320) = l27;
  *(float *)((char *)dream_p(__self) + 328) = l28;
  *(float *)((char *)dream_p(__self) + 344) = l30;
  *(float *)((char *)dream_p(__self) + 360) = l32;
  *(dream_ptr *)((char *)dream_p(__self) + 376) = l34;
  *(int32_t *)((char *)dream_p(__self) + 384) = l35;
  *(int32_t *)((char *)dream_p(__self) + 392) = l36;
  *(int32_t *)((char *)dream_p(__self) + 400) = l37;
  *(int32_t *)((char *)dream_p(__self) + 408) = l38;
  *(int64_t *)((char *)dream_p(__self) + 416) = l39;
  *(float *)((char *)dream_p(__self) + 424) = l40;
  *(dream_ptr *)((char *)dream_p(__self) + 432) = l41;
  *(dream_ptr *)((char *)dream_p(__self) + 440) = l42;
  *(dream_ptr *)((char *)dream_p(__self) + 448) = l43;
  *(dream_ptr *)((char *)dream_p(__self) + 456) = l44;
  *(dream_ptr *)((char *)dream_p(__self) + 464) = l45;
  *(dream_ptr *)((char *)dream_p(__self) + 472) = l46;
  if (l21) goto L8; else goto L9;
L8:;
  l3 = (1280);
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = l1;
  *(dream_ptr *)((char *)dream_p(__self) + 80) = l2;
  *(int64_t *)((char *)dream_p(__self) + 88) = l3;
  *(int64_t *)((char *)dream_p(__self) + 96) = l4;
  *(dream_ptr *)((char *)dream_p(__self) + 136) = l6;
  *(dream_ptr *)((char *)dream_p(__self) + 144) = l7;
  *(int64_t *)((char *)dream_p(__self) + 184) = l10;
  *(float *)((char *)dream_p(__self) + 192) = l11;
  *(float *)((char *)dream_p(__self) + 200) = l12;
  *(int64_t *)((char *)dream_p(__self) + 208) = l13;
  *(float *)((char *)dream_p(__self) + 216) = l14;
  *(dream_ptr *)((char *)dream_p(__self) + 224) = l15;
  *(dream_ptr *)((char *)dream_p(__self) + 232) = l16;
  *(dream_ptr *)((char *)dream_p(__self) + 240) = l17;
  *(int32_t *)((char *)dream_p(__self) + 248) = l18;
  *(int32_t *)((char *)dream_p(__self) + 256) = l19;
  *(dream_ptr *)((char *)dream_p(__self) + 264) = l20;
  *(int32_t *)((char *)dream_p(__self) + 272) = l21;
  *(int32_t *)((char *)dream_p(__self) + 280) = l22;
  *(dream_ptr *)((char *)dream_p(__self) + 288) = l23;
  *(dream_ptr *)((char *)dream_p(__self) + 296) = l24;
  *(int32_t *)((char *)dream_p(__self) + 304) = l25;
  *(dream_ptr *)((char *)dream_p(__self) + 312) = l26;
  *(float *)((char *)dream_p(__self) + 320) = l27;
  *(float *)((char *)dream_p(__self) + 328) = l28;
  *(float *)((char *)dream_p(__self) + 344) = l30;
  *(float *)((char *)dream_p(__self) + 360) = l32;
  *(dream_ptr *)((char *)dream_p(__self) + 376) = l34;
  *(int32_t *)((char *)dream_p(__self) + 384) = l35;
  *(int32_t *)((char *)dream_p(__self) + 392) = l36;
  *(int32_t *)((char *)dream_p(__self) + 400) = l37;
  *(int32_t *)((char *)dream_p(__self) + 408) = l38;
  *(int64_t *)((char *)dream_p(__self) + 416) = l39;
  *(float *)((char *)dream_p(__self) + 424) = l40;
  *(dream_ptr *)((char *)dream_p(__self) + 432) = l41;
  *(dream_ptr *)((char *)dream_p(__self) + 440) = l42;
  *(dream_ptr *)((char *)dream_p(__self) + 448) = l43;
  *(dream_ptr *)((char *)dream_p(__self) + 456) = l44;
  *(dream_ptr *)((char *)dream_p(__self) + 464) = l45;
  *(dream_ptr *)((char *)dream_p(__self) + 472) = l46;
  goto L10;
L9:;
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = l1;
  *(dream_ptr *)((char *)dream_p(__self) + 80) = l2;
  *(int64_t *)((char *)dream_p(__self) + 88) = l3;
  *(int64_t *)((char *)dream_p(__self) + 96) = l4;
  *(dream_ptr *)((char *)dream_p(__self) + 136) = l6;
  *(dream_ptr *)((char *)dream_p(__self) + 144) = l7;
  *(int64_t *)((char *)dream_p(__self) + 184) = l10;
  *(float *)((char *)dream_p(__self) + 192) = l11;
  *(float *)((char *)dream_p(__self) + 200) = l12;
  *(int64_t *)((char *)dream_p(__self) + 208) = l13;
  *(float *)((char *)dream_p(__self) + 216) = l14;
  *(dream_ptr *)((char *)dream_p(__self) + 224) = l15;
  *(dream_ptr *)((char *)dream_p(__self) + 232) = l16;
  *(dream_ptr *)((char *)dream_p(__self) + 240) = l17;
  *(int32_t *)((char *)dream_p(__self) + 248) = l18;
  *(int32_t *)((char *)dream_p(__self) + 256) = l19;
  *(dream_ptr *)((char *)dream_p(__self) + 264) = l20;
  *(int32_t *)((char *)dream_p(__self) + 272) = l21;
  *(int32_t *)((char *)dream_p(__self) + 280) = l22;
  *(dream_ptr *)((char *)dream_p(__self) + 288) = l23;
  *(dream_ptr *)((char *)dream_p(__self) + 296) = l24;
  *(int32_t *)((char *)dream_p(__self) + 304) = l25;
  *(dream_ptr *)((char *)dream_p(__self) + 312) = l26;
  *(float *)((char *)dream_p(__self) + 320) = l27;
  *(float *)((char *)dream_p(__self) + 328) = l28;
  *(float *)((char *)dream_p(__self) + 344) = l30;
  *(float *)((char *)dream_p(__self) + 360) = l32;
  *(dream_ptr *)((char *)dream_p(__self) + 376) = l34;
  *(int32_t *)((char *)dream_p(__self) + 384) = l35;
  *(int32_t *)((char *)dream_p(__self) + 392) = l36;
  *(int32_t *)((char *)dream_p(__self) + 400) = l37;
  *(int32_t *)((char *)dream_p(__self) + 408) = l38;
  *(int64_t *)((char *)dream_p(__self) + 416) = l39;
  *(float *)((char *)dream_p(__self) + 424) = l40;
  *(dream_ptr *)((char *)dream_p(__self) + 432) = l41;
  *(dream_ptr *)((char *)dream_p(__self) + 440) = l42;
  *(dream_ptr *)((char *)dream_p(__self) + 448) = l43;
  *(dream_ptr *)((char *)dream_p(__self) + 456) = l44;
  *(dream_ptr *)((char *)dream_p(__self) + 464) = l45;
  *(dream_ptr *)((char *)dream_p(__self) + 472) = l46;
  goto L10;
L10:;
  l22 = ((l4) < (1));
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = l1;
  *(dream_ptr *)((char *)dream_p(__self) + 80) = l2;
  *(int64_t *)((char *)dream_p(__self) + 88) = l3;
  *(int64_t *)((char *)dream_p(__self) + 96) = l4;
  *(dream_ptr *)((char *)dream_p(__self) + 136) = l6;
  *(dream_ptr *)((char *)dream_p(__self) + 144) = l7;
  *(int64_t *)((char *)dream_p(__self) + 184) = l10;
  *(float *)((char *)dream_p(__self) + 192) = l11;
  *(float *)((char *)dream_p(__self) + 200) = l12;
  *(int64_t *)((char *)dream_p(__self) + 208) = l13;
  *(float *)((char *)dream_p(__self) + 216) = l14;
  *(dream_ptr *)((char *)dream_p(__self) + 224) = l15;
  *(dream_ptr *)((char *)dream_p(__self) + 232) = l16;
  *(dream_ptr *)((char *)dream_p(__self) + 240) = l17;
  *(int32_t *)((char *)dream_p(__self) + 248) = l18;
  *(int32_t *)((char *)dream_p(__self) + 256) = l19;
  *(dream_ptr *)((char *)dream_p(__self) + 264) = l20;
  *(int32_t *)((char *)dream_p(__self) + 272) = l21;
  *(int32_t *)((char *)dream_p(__self) + 280) = l22;
  *(dream_ptr *)((char *)dream_p(__self) + 288) = l23;
  *(dream_ptr *)((char *)dream_p(__self) + 296) = l24;
  *(int32_t *)((char *)dream_p(__self) + 304) = l25;
  *(dream_ptr *)((char *)dream_p(__self) + 312) = l26;
  *(float *)((char *)dream_p(__self) + 320) = l27;
  *(float *)((char *)dream_p(__self) + 328) = l28;
  *(float *)((char *)dream_p(__self) + 344) = l30;
  *(float *)((char *)dream_p(__self) + 360) = l32;
  *(dream_ptr *)((char *)dream_p(__self) + 376) = l34;
  *(int32_t *)((char *)dream_p(__self) + 384) = l35;
  *(int32_t *)((char *)dream_p(__self) + 392) = l36;
  *(int32_t *)((char *)dream_p(__self) + 400) = l37;
  *(int32_t *)((char *)dream_p(__self) + 408) = l38;
  *(int64_t *)((char *)dream_p(__self) + 416) = l39;
  *(float *)((char *)dream_p(__self) + 424) = l40;
  *(dream_ptr *)((char *)dream_p(__self) + 432) = l41;
  *(dream_ptr *)((char *)dream_p(__self) + 440) = l42;
  *(dream_ptr *)((char *)dream_p(__self) + 448) = l43;
  *(dream_ptr *)((char *)dream_p(__self) + 456) = l44;
  *(dream_ptr *)((char *)dream_p(__self) + 464) = l45;
  *(dream_ptr *)((char *)dream_p(__self) + 472) = l46;
  if (l22) goto L11; else goto L12;
L11:;
  l4 = (720);
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = l1;
  *(dream_ptr *)((char *)dream_p(__self) + 80) = l2;
  *(int64_t *)((char *)dream_p(__self) + 88) = l3;
  *(int64_t *)((char *)dream_p(__self) + 96) = l4;
  *(dream_ptr *)((char *)dream_p(__self) + 136) = l6;
  *(dream_ptr *)((char *)dream_p(__self) + 144) = l7;
  *(int64_t *)((char *)dream_p(__self) + 184) = l10;
  *(float *)((char *)dream_p(__self) + 192) = l11;
  *(float *)((char *)dream_p(__self) + 200) = l12;
  *(int64_t *)((char *)dream_p(__self) + 208) = l13;
  *(float *)((char *)dream_p(__self) + 216) = l14;
  *(dream_ptr *)((char *)dream_p(__self) + 224) = l15;
  *(dream_ptr *)((char *)dream_p(__self) + 232) = l16;
  *(dream_ptr *)((char *)dream_p(__self) + 240) = l17;
  *(int32_t *)((char *)dream_p(__self) + 248) = l18;
  *(int32_t *)((char *)dream_p(__self) + 256) = l19;
  *(dream_ptr *)((char *)dream_p(__self) + 264) = l20;
  *(int32_t *)((char *)dream_p(__self) + 272) = l21;
  *(int32_t *)((char *)dream_p(__self) + 280) = l22;
  *(dream_ptr *)((char *)dream_p(__self) + 288) = l23;
  *(dream_ptr *)((char *)dream_p(__self) + 296) = l24;
  *(int32_t *)((char *)dream_p(__self) + 304) = l25;
  *(dream_ptr *)((char *)dream_p(__self) + 312) = l26;
  *(float *)((char *)dream_p(__self) + 320) = l27;
  *(float *)((char *)dream_p(__self) + 328) = l28;
  *(float *)((char *)dream_p(__self) + 344) = l30;
  *(float *)((char *)dream_p(__self) + 360) = l32;
  *(dream_ptr *)((char *)dream_p(__self) + 376) = l34;
  *(int32_t *)((char *)dream_p(__self) + 384) = l35;
  *(int32_t *)((char *)dream_p(__self) + 392) = l36;
  *(int32_t *)((char *)dream_p(__self) + 400) = l37;
  *(int32_t *)((char *)dream_p(__self) + 408) = l38;
  *(int64_t *)((char *)dream_p(__self) + 416) = l39;
  *(float *)((char *)dream_p(__self) + 424) = l40;
  *(dream_ptr *)((char *)dream_p(__self) + 432) = l41;
  *(dream_ptr *)((char *)dream_p(__self) + 440) = l42;
  *(dream_ptr *)((char *)dream_p(__self) + 448) = l43;
  *(dream_ptr *)((char *)dream_p(__self) + 456) = l44;
  *(dream_ptr *)((char *)dream_p(__self) + 464) = l45;
  *(dream_ptr *)((char *)dream_p(__self) + 472) = l46;
  goto L13;
L12:;
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = l1;
  *(dream_ptr *)((char *)dream_p(__self) + 80) = l2;
  *(int64_t *)((char *)dream_p(__self) + 88) = l3;
  *(int64_t *)((char *)dream_p(__self) + 96) = l4;
  *(dream_ptr *)((char *)dream_p(__self) + 136) = l6;
  *(dream_ptr *)((char *)dream_p(__self) + 144) = l7;
  *(int64_t *)((char *)dream_p(__self) + 184) = l10;
  *(float *)((char *)dream_p(__self) + 192) = l11;
  *(float *)((char *)dream_p(__self) + 200) = l12;
  *(int64_t *)((char *)dream_p(__self) + 208) = l13;
  *(float *)((char *)dream_p(__self) + 216) = l14;
  *(dream_ptr *)((char *)dream_p(__self) + 224) = l15;
  *(dream_ptr *)((char *)dream_p(__self) + 232) = l16;
  *(dream_ptr *)((char *)dream_p(__self) + 240) = l17;
  *(int32_t *)((char *)dream_p(__self) + 248) = l18;
  *(int32_t *)((char *)dream_p(__self) + 256) = l19;
  *(dream_ptr *)((char *)dream_p(__self) + 264) = l20;
  *(int32_t *)((char *)dream_p(__self) + 272) = l21;
  *(int32_t *)((char *)dream_p(__self) + 280) = l22;
  *(dream_ptr *)((char *)dream_p(__self) + 288) = l23;
  *(dream_ptr *)((char *)dream_p(__self) + 296) = l24;
  *(int32_t *)((char *)dream_p(__self) + 304) = l25;
  *(dream_ptr *)((char *)dream_p(__self) + 312) = l26;
  *(float *)((char *)dream_p(__self) + 320) = l27;
  *(float *)((char *)dream_p(__self) + 328) = l28;
  *(float *)((char *)dream_p(__self) + 344) = l30;
  *(float *)((char *)dream_p(__self) + 360) = l32;
  *(dream_ptr *)((char *)dream_p(__self) + 376) = l34;
  *(int32_t *)((char *)dream_p(__self) + 384) = l35;
  *(int32_t *)((char *)dream_p(__self) + 392) = l36;
  *(int32_t *)((char *)dream_p(__self) + 400) = l37;
  *(int32_t *)((char *)dream_p(__self) + 408) = l38;
  *(int64_t *)((char *)dream_p(__self) + 416) = l39;
  *(float *)((char *)dream_p(__self) + 424) = l40;
  *(dream_ptr *)((char *)dream_p(__self) + 432) = l41;
  *(dream_ptr *)((char *)dream_p(__self) + 440) = l42;
  *(dream_ptr *)((char *)dream_p(__self) + 448) = l43;
  *(dream_ptr *)((char *)dream_p(__self) + 456) = l44;
  *(dream_ptr *)((char *)dream_p(__self) + 464) = l45;
  *(dream_ptr *)((char *)dream_p(__self) + 472) = l46;
  goto L13;
L13:;
  ({ dream_ptr __v = GpuRenderPipelineDesc_overlay(); memcpy(dream_p(l5), dream_p(__v), 28); dream_free(__v); });
  dream_release(l23);
  dream_retain(__ds3);
  dream_retain(__ds4);
  l23 = (GpuRenderPipeline_create_ex(__ds3, __ds4, l5));
  memset(dream_p(l5), 0, 28);
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = l1;
  *(dream_ptr *)((char *)dream_p(__self) + 80) = l2;
  *(int64_t *)((char *)dream_p(__self) + 88) = l3;
  *(int64_t *)((char *)dream_p(__self) + 96) = l4;
  *(dream_ptr *)((char *)dream_p(__self) + 136) = l6;
  *(dream_ptr *)((char *)dream_p(__self) + 144) = l7;
  *(int64_t *)((char *)dream_p(__self) + 184) = l10;
  *(float *)((char *)dream_p(__self) + 192) = l11;
  *(float *)((char *)dream_p(__self) + 200) = l12;
  *(int64_t *)((char *)dream_p(__self) + 208) = l13;
  *(float *)((char *)dream_p(__self) + 216) = l14;
  *(dream_ptr *)((char *)dream_p(__self) + 224) = l15;
  *(dream_ptr *)((char *)dream_p(__self) + 232) = l16;
  *(dream_ptr *)((char *)dream_p(__self) + 240) = l17;
  *(int32_t *)((char *)dream_p(__self) + 248) = l18;
  *(int32_t *)((char *)dream_p(__self) + 256) = l19;
  *(dream_ptr *)((char *)dream_p(__self) + 264) = l20;
  *(int32_t *)((char *)dream_p(__self) + 272) = l21;
  *(int32_t *)((char *)dream_p(__self) + 280) = l22;
  *(dream_ptr *)((char *)dream_p(__self) + 288) = l23;
  *(dream_ptr *)((char *)dream_p(__self) + 296) = l24;
  *(int32_t *)((char *)dream_p(__self) + 304) = l25;
  *(dream_ptr *)((char *)dream_p(__self) + 312) = l26;
  *(float *)((char *)dream_p(__self) + 320) = l27;
  *(float *)((char *)dream_p(__self) + 328) = l28;
  *(float *)((char *)dream_p(__self) + 344) = l30;
  *(float *)((char *)dream_p(__self) + 360) = l32;
  *(dream_ptr *)((char *)dream_p(__self) + 376) = l34;
  *(int32_t *)((char *)dream_p(__self) + 384) = l35;
  *(int32_t *)((char *)dream_p(__self) + 392) = l36;
  *(int32_t *)((char *)dream_p(__self) + 400) = l37;
  *(int32_t *)((char *)dream_p(__self) + 408) = l38;
  *(int64_t *)((char *)dream_p(__self) + 416) = l39;
  *(float *)((char *)dream_p(__self) + 424) = l40;
  *(dream_ptr *)((char *)dream_p(__self) + 432) = l41;
  *(dream_ptr *)((char *)dream_p(__self) + 440) = l42;
  *(dream_ptr *)((char *)dream_p(__self) + 448) = l43;
  *(dream_ptr *)((char *)dream_p(__self) + 456) = l44;
  *(dream_ptr *)((char *)dream_p(__self) + 464) = l45;
  *(dream_ptr *)((char *)dream_p(__self) + 472) = l46;
  *(int32_t *)dream_p(__self) = 14;
  dream_await(__self, l23);
  return 0;
L14:;
  {
    dream_ptr __ch = *(dream_ptr *)((char *)dream_p(__self) + 36);
    l24 = (dream_ptr)*(dream_ptr *)((char *)dream_p(__ch) + 8);
  }
  l6 = (l24);
  l25 = (Result_GpuRenderPipeline_GpuError_is_err(l6));
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = l1;
  *(dream_ptr *)((char *)dream_p(__self) + 80) = l2;
  *(int64_t *)((char *)dream_p(__self) + 88) = l3;
  *(int64_t *)((char *)dream_p(__self) + 96) = l4;
  *(dream_ptr *)((char *)dream_p(__self) + 136) = l6;
  *(dream_ptr *)((char *)dream_p(__self) + 144) = l7;
  *(int64_t *)((char *)dream_p(__self) + 184) = l10;
  *(float *)((char *)dream_p(__self) + 192) = l11;
  *(float *)((char *)dream_p(__self) + 200) = l12;
  *(int64_t *)((char *)dream_p(__self) + 208) = l13;
  *(float *)((char *)dream_p(__self) + 216) = l14;
  *(dream_ptr *)((char *)dream_p(__self) + 224) = l15;
  *(dream_ptr *)((char *)dream_p(__self) + 232) = l16;
  *(dream_ptr *)((char *)dream_p(__self) + 240) = l17;
  *(int32_t *)((char *)dream_p(__self) + 248) = l18;
  *(int32_t *)((char *)dream_p(__self) + 256) = l19;
  *(dream_ptr *)((char *)dream_p(__self) + 264) = l20;
  *(int32_t *)((char *)dream_p(__self) + 272) = l21;
  *(int32_t *)((char *)dream_p(__self) + 280) = l22;
  *(dream_ptr *)((char *)dream_p(__self) + 288) = l23;
  *(dream_ptr *)((char *)dream_p(__self) + 296) = l24;
  *(int32_t *)((char *)dream_p(__self) + 304) = l25;
  *(dream_ptr *)((char *)dream_p(__self) + 312) = l26;
  *(float *)((char *)dream_p(__self) + 320) = l27;
  *(float *)((char *)dream_p(__self) + 328) = l28;
  *(float *)((char *)dream_p(__self) + 344) = l30;
  *(float *)((char *)dream_p(__self) + 360) = l32;
  *(dream_ptr *)((char *)dream_p(__self) + 376) = l34;
  *(int32_t *)((char *)dream_p(__self) + 384) = l35;
  *(int32_t *)((char *)dream_p(__self) + 392) = l36;
  *(int32_t *)((char *)dream_p(__self) + 400) = l37;
  *(int32_t *)((char *)dream_p(__self) + 408) = l38;
  *(int64_t *)((char *)dream_p(__self) + 416) = l39;
  *(float *)((char *)dream_p(__self) + 424) = l40;
  *(dream_ptr *)((char *)dream_p(__self) + 432) = l41;
  *(dream_ptr *)((char *)dream_p(__self) + 440) = l42;
  *(dream_ptr *)((char *)dream_p(__self) + 448) = l43;
  *(dream_ptr *)((char *)dream_p(__self) + 456) = l44;
  *(dream_ptr *)((char *)dream_p(__self) + 464) = l45;
  *(dream_ptr *)((char *)dream_p(__self) + 472) = l46;
  if (l25) goto L15; else goto L16;
L15:;
  print_string(__ds5);
  print_char(10);
  release_Result_GpuSurface_GpuError(l1);
  release_GpuSurface(l2);
  release_GpuRenderPipeline(l7);
  dream_release(l15);
  dream_release(l16);
  release_Result_bool_GpuError(l17);
  release_GpuSurface(l20);
  dream_release(l23);
  release_Result_GpuRenderPipeline_GpuError(l24);
  release_GpuRenderPipeline(l26);
  release_array_t87(l34);
  dream_release(l41);
  dream_release(l42);
  release_Result_bool_GpuError(l43);
  dream_release(l44);
  release_Result_bool_GpuError(l45);
  dream_release(l46);
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = l1;
  *(dream_ptr *)((char *)dream_p(__self) + 80) = l2;
  *(int64_t *)((char *)dream_p(__self) + 88) = l3;
  *(int64_t *)((char *)dream_p(__self) + 96) = l4;
  *(dream_ptr *)((char *)dream_p(__self) + 136) = l6;
  *(dream_ptr *)((char *)dream_p(__self) + 144) = l7;
  *(int64_t *)((char *)dream_p(__self) + 184) = l10;
  *(float *)((char *)dream_p(__self) + 192) = l11;
  *(float *)((char *)dream_p(__self) + 200) = l12;
  *(int64_t *)((char *)dream_p(__self) + 208) = l13;
  *(float *)((char *)dream_p(__self) + 216) = l14;
  *(dream_ptr *)((char *)dream_p(__self) + 224) = l15;
  *(dream_ptr *)((char *)dream_p(__self) + 232) = l16;
  *(dream_ptr *)((char *)dream_p(__self) + 240) = l17;
  *(int32_t *)((char *)dream_p(__self) + 248) = l18;
  *(int32_t *)((char *)dream_p(__self) + 256) = l19;
  *(dream_ptr *)((char *)dream_p(__self) + 264) = l20;
  *(int32_t *)((char *)dream_p(__self) + 272) = l21;
  *(int32_t *)((char *)dream_p(__self) + 280) = l22;
  *(dream_ptr *)((char *)dream_p(__self) + 288) = l23;
  *(dream_ptr *)((char *)dream_p(__self) + 296) = l24;
  *(int32_t *)((char *)dream_p(__self) + 304) = l25;
  *(dream_ptr *)((char *)dream_p(__self) + 312) = l26;
  *(float *)((char *)dream_p(__self) + 320) = l27;
  *(float *)((char *)dream_p(__self) + 328) = l28;
  *(float *)((char *)dream_p(__self) + 344) = l30;
  *(float *)((char *)dream_p(__self) + 360) = l32;
  *(dream_ptr *)((char *)dream_p(__self) + 376) = l34;
  *(int32_t *)((char *)dream_p(__self) + 384) = l35;
  *(int32_t *)((char *)dream_p(__self) + 392) = l36;
  *(int32_t *)((char *)dream_p(__self) + 400) = l37;
  *(int32_t *)((char *)dream_p(__self) + 408) = l38;
  *(int64_t *)((char *)dream_p(__self) + 416) = l39;
  *(float *)((char *)dream_p(__self) + 424) = l40;
  *(dream_ptr *)((char *)dream_p(__self) + 432) = l41;
  *(dream_ptr *)((char *)dream_p(__self) + 440) = l42;
  *(dream_ptr *)((char *)dream_p(__self) + 448) = l43;
  *(dream_ptr *)((char *)dream_p(__self) + 456) = l44;
  *(dream_ptr *)((char *)dream_p(__self) + 464) = l45;
  *(dream_ptr *)((char *)dream_p(__self) + 472) = l46;
  dream_async_complete(__self, 0); return 0;
L16:;
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = l1;
  *(dream_ptr *)((char *)dream_p(__self) + 80) = l2;
  *(int64_t *)((char *)dream_p(__self) + 88) = l3;
  *(int64_t *)((char *)dream_p(__self) + 96) = l4;
  *(dream_ptr *)((char *)dream_p(__self) + 136) = l6;
  *(dream_ptr *)((char *)dream_p(__self) + 144) = l7;
  *(int64_t *)((char *)dream_p(__self) + 184) = l10;
  *(float *)((char *)dream_p(__self) + 192) = l11;
  *(float *)((char *)dream_p(__self) + 200) = l12;
  *(int64_t *)((char *)dream_p(__self) + 208) = l13;
  *(float *)((char *)dream_p(__self) + 216) = l14;
  *(dream_ptr *)((char *)dream_p(__self) + 224) = l15;
  *(dream_ptr *)((char *)dream_p(__self) + 232) = l16;
  *(dream_ptr *)((char *)dream_p(__self) + 240) = l17;
  *(int32_t *)((char *)dream_p(__self) + 248) = l18;
  *(int32_t *)((char *)dream_p(__self) + 256) = l19;
  *(dream_ptr *)((char *)dream_p(__self) + 264) = l20;
  *(int32_t *)((char *)dream_p(__self) + 272) = l21;
  *(int32_t *)((char *)dream_p(__self) + 280) = l22;
  *(dream_ptr *)((char *)dream_p(__self) + 288) = l23;
  *(dream_ptr *)((char *)dream_p(__self) + 296) = l24;
  *(int32_t *)((char *)dream_p(__self) + 304) = l25;
  *(dream_ptr *)((char *)dream_p(__self) + 312) = l26;
  *(float *)((char *)dream_p(__self) + 320) = l27;
  *(float *)((char *)dream_p(__self) + 328) = l28;
  *(float *)((char *)dream_p(__self) + 344) = l30;
  *(float *)((char *)dream_p(__self) + 360) = l32;
  *(dream_ptr *)((char *)dream_p(__self) + 376) = l34;
  *(int32_t *)((char *)dream_p(__self) + 384) = l35;
  *(int32_t *)((char *)dream_p(__self) + 392) = l36;
  *(int32_t *)((char *)dream_p(__self) + 400) = l37;
  *(int32_t *)((char *)dream_p(__self) + 408) = l38;
  *(int64_t *)((char *)dream_p(__self) + 416) = l39;
  *(float *)((char *)dream_p(__self) + 424) = l40;
  *(dream_ptr *)((char *)dream_p(__self) + 432) = l41;
  *(dream_ptr *)((char *)dream_p(__self) + 440) = l42;
  *(dream_ptr *)((char *)dream_p(__self) + 448) = l43;
  *(dream_ptr *)((char *)dream_p(__self) + 456) = l44;
  *(dream_ptr *)((char *)dream_p(__self) + 464) = l45;
  *(dream_ptr *)((char *)dream_p(__self) + 472) = l46;
  goto L17;
L17:;
  release_GpuRenderPipeline(l26);
  l26 = (({ dream_ptr __o = dream_malloc(4, 19); memset(dream_p(__o), 0, 4); __o; }));
  release_GpuRenderPipeline(l7);
  l7 = (Result_GpuRenderPipeline_GpuError_unwrap_or(l6, l26));
  l26 = (0);
  l27 = (-(1.0f));
  l28 = (-(1.0f));
  ({ dream_ptr __v = make_vert(l27, l28); memcpy(dream_p(l29), dream_p(__v), 8); dream_free(__v); });
  l30 = (-(1.0f));
  ({ dream_ptr __v = make_vert(3.0f, l30); memcpy(dream_p(l31), dream_p(__v), 8); dream_free(__v); });
  l32 = (-(1.0f));
  ({ dream_ptr __v = make_vert(l32, 3.0f); memcpy(dream_p(l33), dream_p(__v), 8); dream_free(__v); });
  release_array_t87(l34);
  l34 = (({ dream_ptr __o = dream_malloc(28, 6); memset(dream_p(__o), 0, 28); *(int32_t*)dream_p(__o) = 3; memcpy((char*)dream_p(__o) + 4 + 0*8, dream_p(l29), 8); memcpy((char*)dream_p(__o) + 4 + 1*8, dream_p(l31), 8); memcpy((char*)dream_p(__o) + 4 + 2*8, dream_p(l33), 8); __o; }));
  ({ dream_ptr __v = GpuBuffer_Vertex_vertex_from(l34); memcpy(dream_p(l8), dream_p(__v), 12); dream_free(__v); });
  l34 = (0);
  ({ dream_ptr __v = GpuVec4_of(0.44f, 0.77f, 0.99f, 1.0f); memcpy(dream_p(l9), dream_p(__v), 16); dream_free(__v); });
  l10 = (timeNowNanos());
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = l1;
  *(dream_ptr *)((char *)dream_p(__self) + 80) = l2;
  *(int64_t *)((char *)dream_p(__self) + 88) = l3;
  *(int64_t *)((char *)dream_p(__self) + 96) = l4;
  *(dream_ptr *)((char *)dream_p(__self) + 136) = l6;
  *(dream_ptr *)((char *)dream_p(__self) + 144) = l7;
  *(int64_t *)((char *)dream_p(__self) + 184) = l10;
  *(float *)((char *)dream_p(__self) + 192) = l11;
  *(float *)((char *)dream_p(__self) + 200) = l12;
  *(int64_t *)((char *)dream_p(__self) + 208) = l13;
  *(float *)((char *)dream_p(__self) + 216) = l14;
  *(dream_ptr *)((char *)dream_p(__self) + 224) = l15;
  *(dream_ptr *)((char *)dream_p(__self) + 232) = l16;
  *(dream_ptr *)((char *)dream_p(__self) + 240) = l17;
  *(int32_t *)((char *)dream_p(__self) + 248) = l18;
  *(int32_t *)((char *)dream_p(__self) + 256) = l19;
  *(dream_ptr *)((char *)dream_p(__self) + 264) = l20;
  *(int32_t *)((char *)dream_p(__self) + 272) = l21;
  *(int32_t *)((char *)dream_p(__self) + 280) = l22;
  *(dream_ptr *)((char *)dream_p(__self) + 288) = l23;
  *(dream_ptr *)((char *)dream_p(__self) + 296) = l24;
  *(int32_t *)((char *)dream_p(__self) + 304) = l25;
  *(dream_ptr *)((char *)dream_p(__self) + 312) = l26;
  *(float *)((char *)dream_p(__self) + 320) = l27;
  *(float *)((char *)dream_p(__self) + 328) = l28;
  *(float *)((char *)dream_p(__self) + 344) = l30;
  *(float *)((char *)dream_p(__self) + 360) = l32;
  *(dream_ptr *)((char *)dream_p(__self) + 376) = l34;
  *(int32_t *)((char *)dream_p(__self) + 384) = l35;
  *(int32_t *)((char *)dream_p(__self) + 392) = l36;
  *(int32_t *)((char *)dream_p(__self) + 400) = l37;
  *(int32_t *)((char *)dream_p(__self) + 408) = l38;
  *(int64_t *)((char *)dream_p(__self) + 416) = l39;
  *(float *)((char *)dream_p(__self) + 424) = l40;
  *(dream_ptr *)((char *)dream_p(__self) + 432) = l41;
  *(dream_ptr *)((char *)dream_p(__self) + 440) = l42;
  *(dream_ptr *)((char *)dream_p(__self) + 448) = l43;
  *(dream_ptr *)((char *)dream_p(__self) + 456) = l44;
  *(dream_ptr *)((char *)dream_p(__self) + 464) = l45;
  *(dream_ptr *)((char *)dream_p(__self) + 472) = l46;
  goto L18;
L18:;
  l35 = (GpuSurface_close_requested(l2));
  l36 = (!(l35));
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = l1;
  *(dream_ptr *)((char *)dream_p(__self) + 80) = l2;
  *(int64_t *)((char *)dream_p(__self) + 88) = l3;
  *(int64_t *)((char *)dream_p(__self) + 96) = l4;
  *(dream_ptr *)((char *)dream_p(__self) + 136) = l6;
  *(dream_ptr *)((char *)dream_p(__self) + 144) = l7;
  *(int64_t *)((char *)dream_p(__self) + 184) = l10;
  *(float *)((char *)dream_p(__self) + 192) = l11;
  *(float *)((char *)dream_p(__self) + 200) = l12;
  *(int64_t *)((char *)dream_p(__self) + 208) = l13;
  *(float *)((char *)dream_p(__self) + 216) = l14;
  *(dream_ptr *)((char *)dream_p(__self) + 224) = l15;
  *(dream_ptr *)((char *)dream_p(__self) + 232) = l16;
  *(dream_ptr *)((char *)dream_p(__self) + 240) = l17;
  *(int32_t *)((char *)dream_p(__self) + 248) = l18;
  *(int32_t *)((char *)dream_p(__self) + 256) = l19;
  *(dream_ptr *)((char *)dream_p(__self) + 264) = l20;
  *(int32_t *)((char *)dream_p(__self) + 272) = l21;
  *(int32_t *)((char *)dream_p(__self) + 280) = l22;
  *(dream_ptr *)((char *)dream_p(__self) + 288) = l23;
  *(dream_ptr *)((char *)dream_p(__self) + 296) = l24;
  *(int32_t *)((char *)dream_p(__self) + 304) = l25;
  *(dream_ptr *)((char *)dream_p(__self) + 312) = l26;
  *(float *)((char *)dream_p(__self) + 320) = l27;
  *(float *)((char *)dream_p(__self) + 328) = l28;
  *(float *)((char *)dream_p(__self) + 344) = l30;
  *(float *)((char *)dream_p(__self) + 360) = l32;
  *(dream_ptr *)((char *)dream_p(__self) + 376) = l34;
  *(int32_t *)((char *)dream_p(__self) + 384) = l35;
  *(int32_t *)((char *)dream_p(__self) + 392) = l36;
  *(int32_t *)((char *)dream_p(__self) + 400) = l37;
  *(int32_t *)((char *)dream_p(__self) + 408) = l38;
  *(int64_t *)((char *)dream_p(__self) + 416) = l39;
  *(float *)((char *)dream_p(__self) + 424) = l40;
  *(dream_ptr *)((char *)dream_p(__self) + 432) = l41;
  *(dream_ptr *)((char *)dream_p(__self) + 440) = l42;
  *(dream_ptr *)((char *)dream_p(__self) + 448) = l43;
  *(dream_ptr *)((char *)dream_p(__self) + 456) = l44;
  *(dream_ptr *)((char *)dream_p(__self) + 464) = l45;
  *(dream_ptr *)((char *)dream_p(__self) + 472) = l46;
  if (l36) goto L19; else goto L20;
L19:;
  l3 = (GpuSurface_width(l2));
  l4 = (GpuSurface_height(l2));
  l37 = ((l3) < (1));
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = l1;
  *(dream_ptr *)((char *)dream_p(__self) + 80) = l2;
  *(int64_t *)((char *)dream_p(__self) + 88) = l3;
  *(int64_t *)((char *)dream_p(__self) + 96) = l4;
  *(dream_ptr *)((char *)dream_p(__self) + 136) = l6;
  *(dream_ptr *)((char *)dream_p(__self) + 144) = l7;
  *(int64_t *)((char *)dream_p(__self) + 184) = l10;
  *(float *)((char *)dream_p(__self) + 192) = l11;
  *(float *)((char *)dream_p(__self) + 200) = l12;
  *(int64_t *)((char *)dream_p(__self) + 208) = l13;
  *(float *)((char *)dream_p(__self) + 216) = l14;
  *(dream_ptr *)((char *)dream_p(__self) + 224) = l15;
  *(dream_ptr *)((char *)dream_p(__self) + 232) = l16;
  *(dream_ptr *)((char *)dream_p(__self) + 240) = l17;
  *(int32_t *)((char *)dream_p(__self) + 248) = l18;
  *(int32_t *)((char *)dream_p(__self) + 256) = l19;
  *(dream_ptr *)((char *)dream_p(__self) + 264) = l20;
  *(int32_t *)((char *)dream_p(__self) + 272) = l21;
  *(int32_t *)((char *)dream_p(__self) + 280) = l22;
  *(dream_ptr *)((char *)dream_p(__self) + 288) = l23;
  *(dream_ptr *)((char *)dream_p(__self) + 296) = l24;
  *(int32_t *)((char *)dream_p(__self) + 304) = l25;
  *(dream_ptr *)((char *)dream_p(__self) + 312) = l26;
  *(float *)((char *)dream_p(__self) + 320) = l27;
  *(float *)((char *)dream_p(__self) + 328) = l28;
  *(float *)((char *)dream_p(__self) + 344) = l30;
  *(float *)((char *)dream_p(__self) + 360) = l32;
  *(dream_ptr *)((char *)dream_p(__self) + 376) = l34;
  *(int32_t *)((char *)dream_p(__self) + 384) = l35;
  *(int32_t *)((char *)dream_p(__self) + 392) = l36;
  *(int32_t *)((char *)dream_p(__self) + 400) = l37;
  *(int32_t *)((char *)dream_p(__self) + 408) = l38;
  *(int64_t *)((char *)dream_p(__self) + 416) = l39;
  *(float *)((char *)dream_p(__self) + 424) = l40;
  *(dream_ptr *)((char *)dream_p(__self) + 432) = l41;
  *(dream_ptr *)((char *)dream_p(__self) + 440) = l42;
  *(dream_ptr *)((char *)dream_p(__self) + 448) = l43;
  *(dream_ptr *)((char *)dream_p(__self) + 456) = l44;
  *(dream_ptr *)((char *)dream_p(__self) + 464) = l45;
  *(dream_ptr *)((char *)dream_p(__self) + 472) = l46;
  if (l37) goto L21; else goto L22;
L20:;
  release_Result_GpuSurface_GpuError(l1);
  release_GpuSurface(l2);
  release_GpuRenderPipeline(l7);
  dream_release(l15);
  dream_release(l16);
  release_Result_bool_GpuError(l17);
  release_GpuSurface(l20);
  dream_release(l23);
  release_Result_GpuRenderPipeline_GpuError(l24);
  release_GpuRenderPipeline(l26);
  release_array_t87(l34);
  dream_release(l41);
  dream_release(l42);
  release_Result_bool_GpuError(l43);
  dream_release(l44);
  release_Result_bool_GpuError(l45);
  dream_release(l46);
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = l1;
  *(dream_ptr *)((char *)dream_p(__self) + 80) = l2;
  *(int64_t *)((char *)dream_p(__self) + 88) = l3;
  *(int64_t *)((char *)dream_p(__self) + 96) = l4;
  *(dream_ptr *)((char *)dream_p(__self) + 136) = l6;
  *(dream_ptr *)((char *)dream_p(__self) + 144) = l7;
  *(int64_t *)((char *)dream_p(__self) + 184) = l10;
  *(float *)((char *)dream_p(__self) + 192) = l11;
  *(float *)((char *)dream_p(__self) + 200) = l12;
  *(int64_t *)((char *)dream_p(__self) + 208) = l13;
  *(float *)((char *)dream_p(__self) + 216) = l14;
  *(dream_ptr *)((char *)dream_p(__self) + 224) = l15;
  *(dream_ptr *)((char *)dream_p(__self) + 232) = l16;
  *(dream_ptr *)((char *)dream_p(__self) + 240) = l17;
  *(int32_t *)((char *)dream_p(__self) + 248) = l18;
  *(int32_t *)((char *)dream_p(__self) + 256) = l19;
  *(dream_ptr *)((char *)dream_p(__self) + 264) = l20;
  *(int32_t *)((char *)dream_p(__self) + 272) = l21;
  *(int32_t *)((char *)dream_p(__self) + 280) = l22;
  *(dream_ptr *)((char *)dream_p(__self) + 288) = l23;
  *(dream_ptr *)((char *)dream_p(__self) + 296) = l24;
  *(int32_t *)((char *)dream_p(__self) + 304) = l25;
  *(dream_ptr *)((char *)dream_p(__self) + 312) = l26;
  *(float *)((char *)dream_p(__self) + 320) = l27;
  *(float *)((char *)dream_p(__self) + 328) = l28;
  *(float *)((char *)dream_p(__self) + 344) = l30;
  *(float *)((char *)dream_p(__self) + 360) = l32;
  *(dream_ptr *)((char *)dream_p(__self) + 376) = l34;
  *(int32_t *)((char *)dream_p(__self) + 384) = l35;
  *(int32_t *)((char *)dream_p(__self) + 392) = l36;
  *(int32_t *)((char *)dream_p(__self) + 400) = l37;
  *(int32_t *)((char *)dream_p(__self) + 408) = l38;
  *(int64_t *)((char *)dream_p(__self) + 416) = l39;
  *(float *)((char *)dream_p(__self) + 424) = l40;
  *(dream_ptr *)((char *)dream_p(__self) + 432) = l41;
  *(dream_ptr *)((char *)dream_p(__self) + 440) = l42;
  *(dream_ptr *)((char *)dream_p(__self) + 448) = l43;
  *(dream_ptr *)((char *)dream_p(__self) + 456) = l44;
  *(dream_ptr *)((char *)dream_p(__self) + 464) = l45;
  *(dream_ptr *)((char *)dream_p(__self) + 472) = l46;
  dream_async_complete(__self, 0); return 0;
L21:;
  l3 = (1280);
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = l1;
  *(dream_ptr *)((char *)dream_p(__self) + 80) = l2;
  *(int64_t *)((char *)dream_p(__self) + 88) = l3;
  *(int64_t *)((char *)dream_p(__self) + 96) = l4;
  *(dream_ptr *)((char *)dream_p(__self) + 136) = l6;
  *(dream_ptr *)((char *)dream_p(__self) + 144) = l7;
  *(int64_t *)((char *)dream_p(__self) + 184) = l10;
  *(float *)((char *)dream_p(__self) + 192) = l11;
  *(float *)((char *)dream_p(__self) + 200) = l12;
  *(int64_t *)((char *)dream_p(__self) + 208) = l13;
  *(float *)((char *)dream_p(__self) + 216) = l14;
  *(dream_ptr *)((char *)dream_p(__self) + 224) = l15;
  *(dream_ptr *)((char *)dream_p(__self) + 232) = l16;
  *(dream_ptr *)((char *)dream_p(__self) + 240) = l17;
  *(int32_t *)((char *)dream_p(__self) + 248) = l18;
  *(int32_t *)((char *)dream_p(__self) + 256) = l19;
  *(dream_ptr *)((char *)dream_p(__self) + 264) = l20;
  *(int32_t *)((char *)dream_p(__self) + 272) = l21;
  *(int32_t *)((char *)dream_p(__self) + 280) = l22;
  *(dream_ptr *)((char *)dream_p(__self) + 288) = l23;
  *(dream_ptr *)((char *)dream_p(__self) + 296) = l24;
  *(int32_t *)((char *)dream_p(__self) + 304) = l25;
  *(dream_ptr *)((char *)dream_p(__self) + 312) = l26;
  *(float *)((char *)dream_p(__self) + 320) = l27;
  *(float *)((char *)dream_p(__self) + 328) = l28;
  *(float *)((char *)dream_p(__self) + 344) = l30;
  *(float *)((char *)dream_p(__self) + 360) = l32;
  *(dream_ptr *)((char *)dream_p(__self) + 376) = l34;
  *(int32_t *)((char *)dream_p(__self) + 384) = l35;
  *(int32_t *)((char *)dream_p(__self) + 392) = l36;
  *(int32_t *)((char *)dream_p(__self) + 400) = l37;
  *(int32_t *)((char *)dream_p(__self) + 408) = l38;
  *(int64_t *)((char *)dream_p(__self) + 416) = l39;
  *(float *)((char *)dream_p(__self) + 424) = l40;
  *(dream_ptr *)((char *)dream_p(__self) + 432) = l41;
  *(dream_ptr *)((char *)dream_p(__self) + 440) = l42;
  *(dream_ptr *)((char *)dream_p(__self) + 448) = l43;
  *(dream_ptr *)((char *)dream_p(__self) + 456) = l44;
  *(dream_ptr *)((char *)dream_p(__self) + 464) = l45;
  *(dream_ptr *)((char *)dream_p(__self) + 472) = l46;
  goto L23;
L22:;
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = l1;
  *(dream_ptr *)((char *)dream_p(__self) + 80) = l2;
  *(int64_t *)((char *)dream_p(__self) + 88) = l3;
  *(int64_t *)((char *)dream_p(__self) + 96) = l4;
  *(dream_ptr *)((char *)dream_p(__self) + 136) = l6;
  *(dream_ptr *)((char *)dream_p(__self) + 144) = l7;
  *(int64_t *)((char *)dream_p(__self) + 184) = l10;
  *(float *)((char *)dream_p(__self) + 192) = l11;
  *(float *)((char *)dream_p(__self) + 200) = l12;
  *(int64_t *)((char *)dream_p(__self) + 208) = l13;
  *(float *)((char *)dream_p(__self) + 216) = l14;
  *(dream_ptr *)((char *)dream_p(__self) + 224) = l15;
  *(dream_ptr *)((char *)dream_p(__self) + 232) = l16;
  *(dream_ptr *)((char *)dream_p(__self) + 240) = l17;
  *(int32_t *)((char *)dream_p(__self) + 248) = l18;
  *(int32_t *)((char *)dream_p(__self) + 256) = l19;
  *(dream_ptr *)((char *)dream_p(__self) + 264) = l20;
  *(int32_t *)((char *)dream_p(__self) + 272) = l21;
  *(int32_t *)((char *)dream_p(__self) + 280) = l22;
  *(dream_ptr *)((char *)dream_p(__self) + 288) = l23;
  *(dream_ptr *)((char *)dream_p(__self) + 296) = l24;
  *(int32_t *)((char *)dream_p(__self) + 304) = l25;
  *(dream_ptr *)((char *)dream_p(__self) + 312) = l26;
  *(float *)((char *)dream_p(__self) + 320) = l27;
  *(float *)((char *)dream_p(__self) + 328) = l28;
  *(float *)((char *)dream_p(__self) + 344) = l30;
  *(float *)((char *)dream_p(__self) + 360) = l32;
  *(dream_ptr *)((char *)dream_p(__self) + 376) = l34;
  *(int32_t *)((char *)dream_p(__self) + 384) = l35;
  *(int32_t *)((char *)dream_p(__self) + 392) = l36;
  *(int32_t *)((char *)dream_p(__self) + 400) = l37;
  *(int32_t *)((char *)dream_p(__self) + 408) = l38;
  *(int64_t *)((char *)dream_p(__self) + 416) = l39;
  *(float *)((char *)dream_p(__self) + 424) = l40;
  *(dream_ptr *)((char *)dream_p(__self) + 432) = l41;
  *(dream_ptr *)((char *)dream_p(__self) + 440) = l42;
  *(dream_ptr *)((char *)dream_p(__self) + 448) = l43;
  *(dream_ptr *)((char *)dream_p(__self) + 456) = l44;
  *(dream_ptr *)((char *)dream_p(__self) + 464) = l45;
  *(dream_ptr *)((char *)dream_p(__self) + 472) = l46;
  goto L23;
L23:;
  l38 = ((l4) < (1));
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = l1;
  *(dream_ptr *)((char *)dream_p(__self) + 80) = l2;
  *(int64_t *)((char *)dream_p(__self) + 88) = l3;
  *(int64_t *)((char *)dream_p(__self) + 96) = l4;
  *(dream_ptr *)((char *)dream_p(__self) + 136) = l6;
  *(dream_ptr *)((char *)dream_p(__self) + 144) = l7;
  *(int64_t *)((char *)dream_p(__self) + 184) = l10;
  *(float *)((char *)dream_p(__self) + 192) = l11;
  *(float *)((char *)dream_p(__self) + 200) = l12;
  *(int64_t *)((char *)dream_p(__self) + 208) = l13;
  *(float *)((char *)dream_p(__self) + 216) = l14;
  *(dream_ptr *)((char *)dream_p(__self) + 224) = l15;
  *(dream_ptr *)((char *)dream_p(__self) + 232) = l16;
  *(dream_ptr *)((char *)dream_p(__self) + 240) = l17;
  *(int32_t *)((char *)dream_p(__self) + 248) = l18;
  *(int32_t *)((char *)dream_p(__self) + 256) = l19;
  *(dream_ptr *)((char *)dream_p(__self) + 264) = l20;
  *(int32_t *)((char *)dream_p(__self) + 272) = l21;
  *(int32_t *)((char *)dream_p(__self) + 280) = l22;
  *(dream_ptr *)((char *)dream_p(__self) + 288) = l23;
  *(dream_ptr *)((char *)dream_p(__self) + 296) = l24;
  *(int32_t *)((char *)dream_p(__self) + 304) = l25;
  *(dream_ptr *)((char *)dream_p(__self) + 312) = l26;
  *(float *)((char *)dream_p(__self) + 320) = l27;
  *(float *)((char *)dream_p(__self) + 328) = l28;
  *(float *)((char *)dream_p(__self) + 344) = l30;
  *(float *)((char *)dream_p(__self) + 360) = l32;
  *(dream_ptr *)((char *)dream_p(__self) + 376) = l34;
  *(int32_t *)((char *)dream_p(__self) + 384) = l35;
  *(int32_t *)((char *)dream_p(__self) + 392) = l36;
  *(int32_t *)((char *)dream_p(__self) + 400) = l37;
  *(int32_t *)((char *)dream_p(__self) + 408) = l38;
  *(int64_t *)((char *)dream_p(__self) + 416) = l39;
  *(float *)((char *)dream_p(__self) + 424) = l40;
  *(dream_ptr *)((char *)dream_p(__self) + 432) = l41;
  *(dream_ptr *)((char *)dream_p(__self) + 440) = l42;
  *(dream_ptr *)((char *)dream_p(__self) + 448) = l43;
  *(dream_ptr *)((char *)dream_p(__self) + 456) = l44;
  *(dream_ptr *)((char *)dream_p(__self) + 464) = l45;
  *(dream_ptr *)((char *)dream_p(__self) + 472) = l46;
  if (l38) goto L24; else goto L25;
L24:;
  l4 = (720);
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = l1;
  *(dream_ptr *)((char *)dream_p(__self) + 80) = l2;
  *(int64_t *)((char *)dream_p(__self) + 88) = l3;
  *(int64_t *)((char *)dream_p(__self) + 96) = l4;
  *(dream_ptr *)((char *)dream_p(__self) + 136) = l6;
  *(dream_ptr *)((char *)dream_p(__self) + 144) = l7;
  *(int64_t *)((char *)dream_p(__self) + 184) = l10;
  *(float *)((char *)dream_p(__self) + 192) = l11;
  *(float *)((char *)dream_p(__self) + 200) = l12;
  *(int64_t *)((char *)dream_p(__self) + 208) = l13;
  *(float *)((char *)dream_p(__self) + 216) = l14;
  *(dream_ptr *)((char *)dream_p(__self) + 224) = l15;
  *(dream_ptr *)((char *)dream_p(__self) + 232) = l16;
  *(dream_ptr *)((char *)dream_p(__self) + 240) = l17;
  *(int32_t *)((char *)dream_p(__self) + 248) = l18;
  *(int32_t *)((char *)dream_p(__self) + 256) = l19;
  *(dream_ptr *)((char *)dream_p(__self) + 264) = l20;
  *(int32_t *)((char *)dream_p(__self) + 272) = l21;
  *(int32_t *)((char *)dream_p(__self) + 280) = l22;
  *(dream_ptr *)((char *)dream_p(__self) + 288) = l23;
  *(dream_ptr *)((char *)dream_p(__self) + 296) = l24;
  *(int32_t *)((char *)dream_p(__self) + 304) = l25;
  *(dream_ptr *)((char *)dream_p(__self) + 312) = l26;
  *(float *)((char *)dream_p(__self) + 320) = l27;
  *(float *)((char *)dream_p(__self) + 328) = l28;
  *(float *)((char *)dream_p(__self) + 344) = l30;
  *(float *)((char *)dream_p(__self) + 360) = l32;
  *(dream_ptr *)((char *)dream_p(__self) + 376) = l34;
  *(int32_t *)((char *)dream_p(__self) + 384) = l35;
  *(int32_t *)((char *)dream_p(__self) + 392) = l36;
  *(int32_t *)((char *)dream_p(__self) + 400) = l37;
  *(int32_t *)((char *)dream_p(__self) + 408) = l38;
  *(int64_t *)((char *)dream_p(__self) + 416) = l39;
  *(float *)((char *)dream_p(__self) + 424) = l40;
  *(dream_ptr *)((char *)dream_p(__self) + 432) = l41;
  *(dream_ptr *)((char *)dream_p(__self) + 440) = l42;
  *(dream_ptr *)((char *)dream_p(__self) + 448) = l43;
  *(dream_ptr *)((char *)dream_p(__self) + 456) = l44;
  *(dream_ptr *)((char *)dream_p(__self) + 464) = l45;
  *(dream_ptr *)((char *)dream_p(__self) + 472) = l46;
  goto L26;
L25:;
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = l1;
  *(dream_ptr *)((char *)dream_p(__self) + 80) = l2;
  *(int64_t *)((char *)dream_p(__self) + 88) = l3;
  *(int64_t *)((char *)dream_p(__self) + 96) = l4;
  *(dream_ptr *)((char *)dream_p(__self) + 136) = l6;
  *(dream_ptr *)((char *)dream_p(__self) + 144) = l7;
  *(int64_t *)((char *)dream_p(__self) + 184) = l10;
  *(float *)((char *)dream_p(__self) + 192) = l11;
  *(float *)((char *)dream_p(__self) + 200) = l12;
  *(int64_t *)((char *)dream_p(__self) + 208) = l13;
  *(float *)((char *)dream_p(__self) + 216) = l14;
  *(dream_ptr *)((char *)dream_p(__self) + 224) = l15;
  *(dream_ptr *)((char *)dream_p(__self) + 232) = l16;
  *(dream_ptr *)((char *)dream_p(__self) + 240) = l17;
  *(int32_t *)((char *)dream_p(__self) + 248) = l18;
  *(int32_t *)((char *)dream_p(__self) + 256) = l19;
  *(dream_ptr *)((char *)dream_p(__self) + 264) = l20;
  *(int32_t *)((char *)dream_p(__self) + 272) = l21;
  *(int32_t *)((char *)dream_p(__self) + 280) = l22;
  *(dream_ptr *)((char *)dream_p(__self) + 288) = l23;
  *(dream_ptr *)((char *)dream_p(__self) + 296) = l24;
  *(int32_t *)((char *)dream_p(__self) + 304) = l25;
  *(dream_ptr *)((char *)dream_p(__self) + 312) = l26;
  *(float *)((char *)dream_p(__self) + 320) = l27;
  *(float *)((char *)dream_p(__self) + 328) = l28;
  *(float *)((char *)dream_p(__self) + 344) = l30;
  *(float *)((char *)dream_p(__self) + 360) = l32;
  *(dream_ptr *)((char *)dream_p(__self) + 376) = l34;
  *(int32_t *)((char *)dream_p(__self) + 384) = l35;
  *(int32_t *)((char *)dream_p(__self) + 392) = l36;
  *(int32_t *)((char *)dream_p(__self) + 400) = l37;
  *(int32_t *)((char *)dream_p(__self) + 408) = l38;
  *(int64_t *)((char *)dream_p(__self) + 416) = l39;
  *(float *)((char *)dream_p(__self) + 424) = l40;
  *(dream_ptr *)((char *)dream_p(__self) + 432) = l41;
  *(dream_ptr *)((char *)dream_p(__self) + 440) = l42;
  *(dream_ptr *)((char *)dream_p(__self) + 448) = l43;
  *(dream_ptr *)((char *)dream_p(__self) + 456) = l44;
  *(dream_ptr *)((char *)dream_p(__self) + 464) = l45;
  *(dream_ptr *)((char *)dream_p(__self) + 472) = l46;
  goto L26;
L26:;
  l11 = (((float)(int32_t)(l3)));
  l12 = (((float)(int32_t)(l4)));
  l13 = (timeNowNanos());
  l39 = ((l13) - (l10));
  l40 = ((l39));
  l14 = ((l40) / (1000000000.0f));
  dream_release(l41);
  l41 = (({ dream_ptr __o = dream_malloc(16, 6); memset(dream_p(__o), 0, 16); *(int32_t*)dream_p(__o) = 3; *(float*)((char*)dream_p(__o) + 4 + 0*4) = (float)(l14); *(float*)((char*)dream_p(__o) + 4 + 1*4) = (float)(l11); *(float*)((char*)dream_p(__o) + 4 + 2*4) = (float)(l12); __o; }));
  dream_release(l15);
  dream_retain(l41);
  l15 = (Uniforms_pack_f32(l41));
  dream_release(l42);
  dream_retain(l2);
  dream_retain(l7);
  dream_retain(l15);
  l42 = (GpuRenderPass_draw_ex__87(l2, l7, l8, 3, l15, l9));
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = l1;
  *(dream_ptr *)((char *)dream_p(__self) + 80) = l2;
  *(int64_t *)((char *)dream_p(__self) + 88) = l3;
  *(int64_t *)((char *)dream_p(__self) + 96) = l4;
  *(dream_ptr *)((char *)dream_p(__self) + 136) = l6;
  *(dream_ptr *)((char *)dream_p(__self) + 144) = l7;
  *(int64_t *)((char *)dream_p(__self) + 184) = l10;
  *(float *)((char *)dream_p(__self) + 192) = l11;
  *(float *)((char *)dream_p(__self) + 200) = l12;
  *(int64_t *)((char *)dream_p(__self) + 208) = l13;
  *(float *)((char *)dream_p(__self) + 216) = l14;
  *(dream_ptr *)((char *)dream_p(__self) + 224) = l15;
  *(dream_ptr *)((char *)dream_p(__self) + 232) = l16;
  *(dream_ptr *)((char *)dream_p(__self) + 240) = l17;
  *(int32_t *)((char *)dream_p(__self) + 248) = l18;
  *(int32_t *)((char *)dream_p(__self) + 256) = l19;
  *(dream_ptr *)((char *)dream_p(__self) + 264) = l20;
  *(int32_t *)((char *)dream_p(__self) + 272) = l21;
  *(int32_t *)((char *)dream_p(__self) + 280) = l22;
  *(dream_ptr *)((char *)dream_p(__self) + 288) = l23;
  *(dream_ptr *)((char *)dream_p(__self) + 296) = l24;
  *(int32_t *)((char *)dream_p(__self) + 304) = l25;
  *(dream_ptr *)((char *)dream_p(__self) + 312) = l26;
  *(float *)((char *)dream_p(__self) + 320) = l27;
  *(float *)((char *)dream_p(__self) + 328) = l28;
  *(float *)((char *)dream_p(__self) + 344) = l30;
  *(float *)((char *)dream_p(__self) + 360) = l32;
  *(dream_ptr *)((char *)dream_p(__self) + 376) = l34;
  *(int32_t *)((char *)dream_p(__self) + 384) = l35;
  *(int32_t *)((char *)dream_p(__self) + 392) = l36;
  *(int32_t *)((char *)dream_p(__self) + 400) = l37;
  *(int32_t *)((char *)dream_p(__self) + 408) = l38;
  *(int64_t *)((char *)dream_p(__self) + 416) = l39;
  *(float *)((char *)dream_p(__self) + 424) = l40;
  *(dream_ptr *)((char *)dream_p(__self) + 432) = l41;
  *(dream_ptr *)((char *)dream_p(__self) + 440) = l42;
  *(dream_ptr *)((char *)dream_p(__self) + 448) = l43;
  *(dream_ptr *)((char *)dream_p(__self) + 456) = l44;
  *(dream_ptr *)((char *)dream_p(__self) + 464) = l45;
  *(dream_ptr *)((char *)dream_p(__self) + 472) = l46;
  *(int32_t *)dream_p(__self) = 27;
  dream_await(__self, l42);
  return 0;
L27:;
  {
    dream_ptr __ch = *(dream_ptr *)((char *)dream_p(__self) + 36);
    l43 = (dream_ptr)*(dream_ptr *)((char *)dream_p(__ch) + 8);
  }
  dream_release(l44);
  l44 = (GpuSurface_present(l2));
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = l1;
  *(dream_ptr *)((char *)dream_p(__self) + 80) = l2;
  *(int64_t *)((char *)dream_p(__self) + 88) = l3;
  *(int64_t *)((char *)dream_p(__self) + 96) = l4;
  *(dream_ptr *)((char *)dream_p(__self) + 136) = l6;
  *(dream_ptr *)((char *)dream_p(__self) + 144) = l7;
  *(int64_t *)((char *)dream_p(__self) + 184) = l10;
  *(float *)((char *)dream_p(__self) + 192) = l11;
  *(float *)((char *)dream_p(__self) + 200) = l12;
  *(int64_t *)((char *)dream_p(__self) + 208) = l13;
  *(float *)((char *)dream_p(__self) + 216) = l14;
  *(dream_ptr *)((char *)dream_p(__self) + 224) = l15;
  *(dream_ptr *)((char *)dream_p(__self) + 232) = l16;
  *(dream_ptr *)((char *)dream_p(__self) + 240) = l17;
  *(int32_t *)((char *)dream_p(__self) + 248) = l18;
  *(int32_t *)((char *)dream_p(__self) + 256) = l19;
  *(dream_ptr *)((char *)dream_p(__self) + 264) = l20;
  *(int32_t *)((char *)dream_p(__self) + 272) = l21;
  *(int32_t *)((char *)dream_p(__self) + 280) = l22;
  *(dream_ptr *)((char *)dream_p(__self) + 288) = l23;
  *(dream_ptr *)((char *)dream_p(__self) + 296) = l24;
  *(int32_t *)((char *)dream_p(__self) + 304) = l25;
  *(dream_ptr *)((char *)dream_p(__self) + 312) = l26;
  *(float *)((char *)dream_p(__self) + 320) = l27;
  *(float *)((char *)dream_p(__self) + 328) = l28;
  *(float *)((char *)dream_p(__self) + 344) = l30;
  *(float *)((char *)dream_p(__self) + 360) = l32;
  *(dream_ptr *)((char *)dream_p(__self) + 376) = l34;
  *(int32_t *)((char *)dream_p(__self) + 384) = l35;
  *(int32_t *)((char *)dream_p(__self) + 392) = l36;
  *(int32_t *)((char *)dream_p(__self) + 400) = l37;
  *(int32_t *)((char *)dream_p(__self) + 408) = l38;
  *(int64_t *)((char *)dream_p(__self) + 416) = l39;
  *(float *)((char *)dream_p(__self) + 424) = l40;
  *(dream_ptr *)((char *)dream_p(__self) + 432) = l41;
  *(dream_ptr *)((char *)dream_p(__self) + 440) = l42;
  *(dream_ptr *)((char *)dream_p(__self) + 448) = l43;
  *(dream_ptr *)((char *)dream_p(__self) + 456) = l44;
  *(dream_ptr *)((char *)dream_p(__self) + 464) = l45;
  *(dream_ptr *)((char *)dream_p(__self) + 472) = l46;
  *(int32_t *)dream_p(__self) = 28;
  dream_await(__self, l44);
  return 0;
L28:;
  {
    dream_ptr __ch = *(dream_ptr *)((char *)dream_p(__self) + 36);
    l45 = (dream_ptr)*(dream_ptr *)((char *)dream_p(__ch) + 8);
  }
  dream_release(l46);
  l46 = (Gpu_frame());
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = l1;
  *(dream_ptr *)((char *)dream_p(__self) + 80) = l2;
  *(int64_t *)((char *)dream_p(__self) + 88) = l3;
  *(int64_t *)((char *)dream_p(__self) + 96) = l4;
  *(dream_ptr *)((char *)dream_p(__self) + 136) = l6;
  *(dream_ptr *)((char *)dream_p(__self) + 144) = l7;
  *(int64_t *)((char *)dream_p(__self) + 184) = l10;
  *(float *)((char *)dream_p(__self) + 192) = l11;
  *(float *)((char *)dream_p(__self) + 200) = l12;
  *(int64_t *)((char *)dream_p(__self) + 208) = l13;
  *(float *)((char *)dream_p(__self) + 216) = l14;
  *(dream_ptr *)((char *)dream_p(__self) + 224) = l15;
  *(dream_ptr *)((char *)dream_p(__self) + 232) = l16;
  *(dream_ptr *)((char *)dream_p(__self) + 240) = l17;
  *(int32_t *)((char *)dream_p(__self) + 248) = l18;
  *(int32_t *)((char *)dream_p(__self) + 256) = l19;
  *(dream_ptr *)((char *)dream_p(__self) + 264) = l20;
  *(int32_t *)((char *)dream_p(__self) + 272) = l21;
  *(int32_t *)((char *)dream_p(__self) + 280) = l22;
  *(dream_ptr *)((char *)dream_p(__self) + 288) = l23;
  *(dream_ptr *)((char *)dream_p(__self) + 296) = l24;
  *(int32_t *)((char *)dream_p(__self) + 304) = l25;
  *(dream_ptr *)((char *)dream_p(__self) + 312) = l26;
  *(float *)((char *)dream_p(__self) + 320) = l27;
  *(float *)((char *)dream_p(__self) + 328) = l28;
  *(float *)((char *)dream_p(__self) + 344) = l30;
  *(float *)((char *)dream_p(__self) + 360) = l32;
  *(dream_ptr *)((char *)dream_p(__self) + 376) = l34;
  *(int32_t *)((char *)dream_p(__self) + 384) = l35;
  *(int32_t *)((char *)dream_p(__self) + 392) = l36;
  *(int32_t *)((char *)dream_p(__self) + 400) = l37;
  *(int32_t *)((char *)dream_p(__self) + 408) = l38;
  *(int64_t *)((char *)dream_p(__self) + 416) = l39;
  *(float *)((char *)dream_p(__self) + 424) = l40;
  *(dream_ptr *)((char *)dream_p(__self) + 432) = l41;
  *(dream_ptr *)((char *)dream_p(__self) + 440) = l42;
  *(dream_ptr *)((char *)dream_p(__self) + 448) = l43;
  *(dream_ptr *)((char *)dream_p(__self) + 456) = l44;
  *(dream_ptr *)((char *)dream_p(__self) + 464) = l45;
  *(dream_ptr *)((char *)dream_p(__self) + 472) = l46;
  *(int32_t *)dream_p(__self) = 29;
  dream_await(__self, l46);
  return 0;
L29:;
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = l1;
  *(dream_ptr *)((char *)dream_p(__self) + 80) = l2;
  *(int64_t *)((char *)dream_p(__self) + 88) = l3;
  *(int64_t *)((char *)dream_p(__self) + 96) = l4;
  *(dream_ptr *)((char *)dream_p(__self) + 136) = l6;
  *(dream_ptr *)((char *)dream_p(__self) + 144) = l7;
  *(int64_t *)((char *)dream_p(__self) + 184) = l10;
  *(float *)((char *)dream_p(__self) + 192) = l11;
  *(float *)((char *)dream_p(__self) + 200) = l12;
  *(int64_t *)((char *)dream_p(__self) + 208) = l13;
  *(float *)((char *)dream_p(__self) + 216) = l14;
  *(dream_ptr *)((char *)dream_p(__self) + 224) = l15;
  *(dream_ptr *)((char *)dream_p(__self) + 232) = l16;
  *(dream_ptr *)((char *)dream_p(__self) + 240) = l17;
  *(int32_t *)((char *)dream_p(__self) + 248) = l18;
  *(int32_t *)((char *)dream_p(__self) + 256) = l19;
  *(dream_ptr *)((char *)dream_p(__self) + 264) = l20;
  *(int32_t *)((char *)dream_p(__self) + 272) = l21;
  *(int32_t *)((char *)dream_p(__self) + 280) = l22;
  *(dream_ptr *)((char *)dream_p(__self) + 288) = l23;
  *(dream_ptr *)((char *)dream_p(__self) + 296) = l24;
  *(int32_t *)((char *)dream_p(__self) + 304) = l25;
  *(dream_ptr *)((char *)dream_p(__self) + 312) = l26;
  *(float *)((char *)dream_p(__self) + 320) = l27;
  *(float *)((char *)dream_p(__self) + 328) = l28;
  *(float *)((char *)dream_p(__self) + 344) = l30;
  *(float *)((char *)dream_p(__self) + 360) = l32;
  *(dream_ptr *)((char *)dream_p(__self) + 376) = l34;
  *(int32_t *)((char *)dream_p(__self) + 384) = l35;
  *(int32_t *)((char *)dream_p(__self) + 392) = l36;
  *(int32_t *)((char *)dream_p(__self) + 400) = l37;
  *(int32_t *)((char *)dream_p(__self) + 408) = l38;
  *(int64_t *)((char *)dream_p(__self) + 416) = l39;
  *(float *)((char *)dream_p(__self) + 424) = l40;
  *(dream_ptr *)((char *)dream_p(__self) + 432) = l41;
  *(dream_ptr *)((char *)dream_p(__self) + 440) = l42;
  *(dream_ptr *)((char *)dream_p(__self) + 448) = l43;
  *(dream_ptr *)((char *)dream_p(__self) + 456) = l44;
  *(dream_ptr *)((char *)dream_p(__self) + 464) = l45;
  *(dream_ptr *)((char *)dream_p(__self) + 472) = l46;
  goto L18;
  return 0;
}

dream_ptr GpuRenderPass_draw_ex__87(dream_ptr l0, dream_ptr l1, dream_ptr l2, int32_t l3, dream_ptr l4, dream_ptr l5) {
  dream_ptr __self = dream_new_future(152, 27, 0);
  *(dream_ptr *)((char *)dream_p(__self) + 64) = (dream_ptr)l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = (dream_ptr)l1;
  memcpy((char *)dream_p(__self) + 80, dream_p(l2), 12);
  *(int64_t *)((char *)dream_p(__self) + 96) = (int64_t)l3;
  *(dream_ptr *)((char *)dream_p(__self) + 104) = (dream_ptr)l4;
  memcpy((char *)dream_p(__self) + 112, dream_p(l5), 16);
  dream_enqueue(__self);
  return __self;
}

int32_t poll_GpuRenderPass_draw_ex__87(dream_ptr __self) {
  dream_ptr l0 = *(dream_ptr *)((char *)dream_p(__self) + 64);
  dream_ptr l1 = *(dream_ptr *)((char *)dream_p(__self) + 72);
  dream_ptr l2 = (dream_ptr)((char *)dream_p(__self) + 80);
  int64_t l3 = *(int64_t *)((char *)dream_p(__self) + 96);
  dream_ptr l4 = *(dream_ptr *)((char *)dream_p(__self) + 104);
  dream_ptr l5 = (dream_ptr)((char *)dream_p(__self) + 112);
  dream_ptr l6 = *(dream_ptr *)((char *)dream_p(__self) + 128);
  dream_ptr l7 = *(dream_ptr *)((char *)dream_p(__self) + 136);
  dream_ptr l8 = *(dream_ptr *)((char *)dream_p(__self) + 144);
  int32_t __st = *(int32_t *)dream_p(__self);
  switch (__st) {
    case 1: goto L1;
    default: break;
  }
  goto L0;
L0:;
  release_Option_GpuTexture(l6);
  l6 = (({ dream_ptr __o = dream_malloc(16, 24); memset(dream_p(__o), 0, 16); *(int32_t*)dream_p(__o) = 1;  __o; }));
  dream_release(l7);
  l7 = (GpuRenderPass_draw_instanced__87(l0, l1, l2, l3, 1, l4, l5, l6, 0));
  memset(dream_p(l2), 0, 12);
  memset(dream_p(l5), 0, 16);
  l0 = (0);
  l1 = (0);
  l4 = (0);
  l6 = (0);
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = l1;
  *(int64_t *)((char *)dream_p(__self) + 96) = l3;
  *(dream_ptr *)((char *)dream_p(__self) + 104) = l4;
  *(dream_ptr *)((char *)dream_p(__self) + 128) = l6;
  *(dream_ptr *)((char *)dream_p(__self) + 136) = l7;
  *(dream_ptr *)((char *)dream_p(__self) + 144) = l8;
  *(int32_t *)dream_p(__self) = 1;
  dream_await(__self, l7);
  return 0;
L1:;
  {
    dream_ptr __ch = *(dream_ptr *)((char *)dream_p(__self) + 36);
    l8 = (dream_ptr)*(dream_ptr *)((char *)dream_p(__ch) + 8);
  }
  release_GpuSurface(l0);
  release_GpuRenderPipeline(l1);
  dream_release(l4);
  release_Option_GpuTexture(l6);
  dream_release(l7);
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = l1;
  *(int64_t *)((char *)dream_p(__self) + 96) = l3;
  *(dream_ptr *)((char *)dream_p(__self) + 104) = l4;
  *(dream_ptr *)((char *)dream_p(__self) + 128) = l6;
  *(dream_ptr *)((char *)dream_p(__self) + 136) = l7;
  *(dream_ptr *)((char *)dream_p(__self) + 144) = l8;
  dream_async_complete(__self, (dream_ptr)l8); return 0;
  return 0;
}

void GpuError_constructor(dream_ptr l0, dream_ptr l1, dream_ptr l2) {
  goto L0;
L0:;
  ({ dream_ptr __old = *(dream_ptr*)((char*)dream_p(l0) + 0); dream_ptr __v = (dream_ptr)(l1); if (__old != __v) { dream_retain(__v); *(dream_ptr*)((char*)dream_p(l0) + 0) = (dream_ptr)__v; dream_release(__old); } });
  ({ dream_ptr __old = *(dream_ptr*)((char*)dream_p(l0) + 8); dream_ptr __v = (dream_ptr)(l2); if (__old != __v) { dream_retain(__v); *(dream_ptr*)((char*)dream_p(l0) + 8) = (dream_ptr)__v; dream_release(__old); } });
  dream_release(l1);
  dream_release(l2);
  return;
}

dream_ptr GpuError_from_code(int32_t l0, dream_ptr l1) {
  dream_ptr l2 = 0;
  dream_ptr l3 = 0;
  int64_t l4 = 0;
  int32_t l5 = 0;
  int32_t l6 = 0;
  dream_ptr l7 = 0;
  int32_t l8 = 0;
  dream_ptr l9 = 0;
  int32_t l10 = 0;
  dream_ptr l11 = 0;
  dream_ptr l12 = 0;
  dream_ptr l13 = 0;
  dream_ptr l14 = 0;
  dream_ptr l15 = 0;
  dream_ptr l16 = 0;
  dream_ptr l17 = 0;
  dream_ptr l18 = 0;
  dream_ptr l19 = 0;
  dream_ptr l20 = 0;
  goto L0;
L0:;
  dream_release(l2);
  l2 = (gpuLastError());
  dream_release(l3);
  l3 = (l1);
  dream_retain(l1);
  l4 = (dream_str_len(l2));
  l5 = ((l4) > (0));
  if (l5) goto L1; else goto L3;
L1:;
  dream_release(l1);
  l3 = (({ dream_ptr __r = dream_concat_strings(l1, __ds6); dream_ptr __n; __n = dream_concat_strings(__r, l2); dream_release(__r); __r = __n; __r; }));
  goto L3;
L2:;
  abort();
L3:;
  l6 = ((l0) == (1));
  if (l6) goto L4; else goto L6;
L4:;
  release_GpuError(l7);
  l14 = (0);
  dream_release(0);
  dream_retain(__ds7);
  l14 = (({ dream_ptr __o = dream_malloc(16, 13); memset(dream_p(__o), 0, 16); GpuError_constructor(__o, __ds7, l3); __o; }));
  dream_release(0);
  l7 = (l14);
  l3 = (0);
  dream_release(l1);
  dream_release(l2);
  dream_release(0);
  release_GpuError(l9);
  release_GpuError(l11);
  release_GpuError(l12);
  return l14;
L5:;
  abort();
L6:;
  l8 = ((l0) == (2));
  if (l8) goto L7; else goto L9;
L7:;
  release_GpuError(l9);
  l16 = (0);
  dream_release(0);
  dream_retain(__ds8);
  l16 = (({ dream_ptr __o = dream_malloc(16, 13); memset(dream_p(__o), 0, 16); GpuError_constructor(__o, __ds8, l3); __o; }));
  dream_release(0);
  l9 = (l16);
  l3 = (0);
  dream_release(l1);
  dream_release(l2);
  dream_release(0);
  release_GpuError(l7);
  release_GpuError(l11);
  release_GpuError(l12);
  return l16;
L8:;
  abort();
L9:;
  l10 = ((l0) == (3));
  if (l10) goto L10; else goto L12;
L10:;
  release_GpuError(l11);
  l18 = (0);
  dream_release(0);
  dream_retain(__ds9);
  l18 = (({ dream_ptr __o = dream_malloc(16, 13); memset(dream_p(__o), 0, 16); GpuError_constructor(__o, __ds9, l3); __o; }));
  dream_release(0);
  l11 = (l18);
  l3 = (0);
  dream_release(l1);
  dream_release(l2);
  dream_release(0);
  release_GpuError(l7);
  release_GpuError(l9);
  release_GpuError(l12);
  return l18;
L11:;
  abort();
L12:;
  release_GpuError(l12);
  l20 = (0);
  dream_release(0);
  dream_retain(__ds10);
  l20 = (({ dream_ptr __o = dream_malloc(16, 13); memset(dream_p(__o), 0, 16); GpuError_constructor(__o, __ds10, l3); __o; }));
  dream_release(0);
  l12 = (l20);
  l3 = (0);
  dream_release(l1);
  dream_release(l2);
  dream_release(0);
  release_GpuError(l7);
  release_GpuError(l9);
  release_GpuError(l11);
  return l20;
L13:;
  abort();
L14:;
  abort();
L15:;
  abort();
L16:;
  abort();
L17:;
  abort();
L18:;
  abort();
L19:;
  abort();
L20:;
  abort();
}

dream_ptr GpuVec4_of(float l0, float l1, float l2, float l3) {
  _Alignas(8) unsigned char __vs4[16] = {0};
  dream_ptr l4 = (dream_ptr)(uintptr_t)__vs4;
  goto L0;
L0:;
  ({ dream_ptr __v = ({ dream_ptr __o = dream_malloc(16, 15); memset(dream_p(__o), 0, 16); __o; }); memcpy(dream_p(l4), dream_p(__v), 16); dream_free(__v); });
  *(float*)((char*)dream_p(l4) + 0) = (float)(l0);
  *(float*)((char*)dream_p(l4) + 4) = (float)(l1);
  *(float*)((char*)dream_p(l4) + 8) = (float)(l2);
  *(float*)((char*)dream_p(l4) + 12) = (float)(l3);
  { dream_ptr __r = dream_malloc(16, 15); memcpy(dream_p(__r), dream_p(l4), 16); return __r; }
}

dream_ptr Uniforms_pack_f32(dream_ptr l0) {
  dream_ptr l1 = 0;
  int64_t l2 = 0;
  dream_ptr l3 = 0;
  int64_t l4 = 0;
  int64_t l5 = 0;
  int64_t l6 = 0;
  int64_t l7 = 0;
  int32_t l8 = 0;
  float l9 = 0;
  int32_t l10 = 0;
  int64_t l11 = 0;
  int64_t l12 = 0;
  goto L0;
L0:;
  l5 = (*(int32_t*)dream_p(l0));
  l6 = ((l5) << (2));
  dream_release(l1);
  l1 = (dream_array_new(l6, 1));
  l2 = (0);
  goto L1;
L1:;
  l7 = (*(int32_t*)dream_p(l0));
  l8 = ((l2) < (l7));
  if (l8) goto L2; else goto L4;
L2:;
  l9 = ((*(float*)(((char*)dream_p(l0) + 4 + (l2)*4))));
  dream_release(l3);
  l3 = (dream_to_bytes((dream_ptr)(uintptr_t)&(float){(float)(l9)}, 4));
  l4 = (0);
  l11 = ((l2) << (2));
  l12 = (l11);
  *(uint8_t*)(({ int32_t __idx = (int32_t)(l11); int32_t __len = l1 ? *(int32_t*)dream_p(l1) : 0; if ((int64_t)(l11) == (int64_t)__idx && (uint32_t)__idx >= (uint32_t)__len) dream_panic(__ds65); (char*)dream_p(l1) + 4 + (int64_t)(l11)*1; })) = (uint8_t)((*(uint8_t*)(({ int32_t __idx = (int32_t)(l4); int32_t __len = l3 ? *(int32_t*)dream_p(l3) : 0; if ((int64_t)(l4) == (int64_t)__idx && (uint32_t)__idx >= (uint32_t)__len) dream_panic(__ds65); (char*)dream_p(l3) + 4 + (int64_t)(l4)*1; }))));
  l4 = (1);
  l12 = ((l11) + (1));
  *(uint8_t*)(({ int32_t __idx = (int32_t)(l12); int32_t __len = l1 ? *(int32_t*)dream_p(l1) : 0; if ((int64_t)(l12) == (int64_t)__idx && (uint32_t)__idx >= (uint32_t)__len) dream_panic(__ds65); (char*)dream_p(l1) + 4 + (int64_t)(l12)*1; })) = (uint8_t)((*(uint8_t*)(({ int32_t __idx = (int32_t)(l4); int32_t __len = l3 ? *(int32_t*)dream_p(l3) : 0; if ((int64_t)(l4) == (int64_t)__idx && (uint32_t)__idx >= (uint32_t)__len) dream_panic(__ds65); (char*)dream_p(l3) + 4 + (int64_t)(l4)*1; }))));
  l4 = (2);
  l12 = ((l11) + (2));
  *(uint8_t*)(({ int32_t __idx = (int32_t)(l12); int32_t __len = l1 ? *(int32_t*)dream_p(l1) : 0; if ((int64_t)(l12) == (int64_t)__idx && (uint32_t)__idx >= (uint32_t)__len) dream_panic(__ds65); (char*)dream_p(l1) + 4 + (int64_t)(l12)*1; })) = (uint8_t)((*(uint8_t*)(({ int32_t __idx = (int32_t)(l4); int32_t __len = l3 ? *(int32_t*)dream_p(l3) : 0; if ((int64_t)(l4) == (int64_t)__idx && (uint32_t)__idx >= (uint32_t)__len) dream_panic(__ds65); (char*)dream_p(l3) + 4 + (int64_t)(l4)*1; }))));
  l4 = (3);
  l12 = ((l11) + (3));
  *(uint8_t*)(({ int32_t __idx = (int32_t)(l12); int32_t __len = l1 ? *(int32_t*)dream_p(l1) : 0; if ((int64_t)(l12) == (int64_t)__idx && (uint32_t)__idx >= (uint32_t)__len) dream_panic(__ds65); (char*)dream_p(l1) + 4 + (int64_t)(l12)*1; })) = (uint8_t)((*(uint8_t*)(({ int32_t __idx = (int32_t)(l4); int32_t __len = l3 ? *(int32_t*)dream_p(l3) : 0; if ((int64_t)(l4) == (int64_t)__idx && (uint32_t)__idx >= (uint32_t)__len) dream_panic(__ds65); (char*)dream_p(l3) + 4 + (int64_t)(l4)*1; }))));
  l4 = (4);
  l2 = ((l2) + (1));
  goto L1;
L3:;
  abort();
L4:;
  dream_release(l0);
  dream_release(l3);
  return l1;
L5:;
  abort();
L6:;
  abort();
L7:;
  abort();
L8:;
  abort();
L9:;
  abort();
L10:;
  abort();
}

dream_ptr GpuSurface_create(dream_ptr l0, int32_t l1, int32_t l2) {
  dream_ptr l3 = 0;
  dream_ptr l4 = 0;
  int64_t l5 = 0;
  int64_t l6 = 0;
  int64_t l7 = 0;
  dream_ptr l8 = 0;
  int32_t l9 = 0;
  dream_ptr l10 = 0;
  dream_ptr l11 = 0;
  dream_ptr l12 = 0;
  dream_ptr l13 = 0;
  dream_ptr l14 = 0;
  goto L0;
L0:;
  release_Result_GpuSurface_GpuError(l3);
  l8 = (0);
  l11 = (0);
  l12 = (0);
  l14 = (0);
  l7 = (gpuSurfaceCreate(l0, l1, l2));
  l9 = ((l7) < (0));
  if (l9) goto L2; else goto L4;
L1:;
  abort();
L2:;
  dream_release(0);
  dream_retain(__ds11);
  l14 = (0);
  dream_release(0);
  dream_retain(__ds7);
  l14 = (({ dream_ptr __o = dream_malloc(16, 13); memset(dream_p(__o), 0, 16); GpuError_constructor(__o, __ds7, __ds11); __o; }));
  dream_release(0);
  dream_release(0);
  l11 = (({ dream_ptr __o = dream_malloc(16, 22); memset(dream_p(__o), 0, 16); *(int32_t*)dream_p(__o) = 1; *(dream_ptr*)((char*)dream_p(__o) + 8) = (dream_ptr)(l14); dream_retain(*(dream_ptr*)((char*)dream_p(__o) + 8));  __o; }));
  dream_release(0);
  dream_release(0);
  release_GpuError(l14);
  dream_release(0);
  l3 = (l11);
  goto L7;
L3:;
  abort();
L4:;
  dream_release(0);
  l8 = (({ dream_ptr __o = dream_malloc(4, 17); memset(dream_p(__o), 0, 4); __o; }));
  *(int32_t*)((char*)dream_p(l8) + 0) = (int32_t)(l7);
  dream_release(0);
  l12 = (({ dream_ptr __o = dream_malloc(16, 22); memset(dream_p(__o), 0, 16); *(int32_t*)dream_p(__o) = 0; *(dream_ptr*)((char*)dream_p(__o) + 8) = (dream_ptr)(l8); dream_retain(*(dream_ptr*)((char*)dream_p(__o) + 8));  __o; }));
  dream_release(0);
  release_GpuSurface(l8);
  dream_release(0);
  dream_release(0);
  l3 = (l12);
  goto L7;
L5:;
  abort();
L6:;
  abort();
L7:;
  l0 = (0);
  dream_release(0);
  return l3;
}

int32_t GpuSurface_width(dream_ptr l0) {
  int64_t l1 = 0;
  int64_t l2 = 0;
  goto L0;
L0:;
  l1 = ((*(int32_t*)((char*)dream_p(l0) + 0)));
  return gpuSurfaceWidth(l1);
}

int32_t GpuSurface_height(dream_ptr l0) {
  int64_t l1 = 0;
  int64_t l2 = 0;
  goto L0;
L0:;
  l1 = ((*(int32_t*)((char*)dream_p(l0) + 0)));
  return gpuSurfaceHeight(l1);
}

dream_ptr GpuSurface_present(dream_ptr l0) {
  dream_ptr __self = dream_new_future(136, 28, 0);
  *(dream_ptr *)((char *)dream_p(__self) + 64) = (dream_ptr)l0;
  dream_enqueue(__self);
  return __self;
}

int32_t poll_GpuSurface_present(dream_ptr __self) {
  dream_ptr l0 = *(dream_ptr *)((char *)dream_p(__self) + 64);
  int64_t l1 = *(int64_t *)((char *)dream_p(__self) + 72);
  int64_t l2 = *(int64_t *)((char *)dream_p(__self) + 80);
  dream_ptr l3 = *(dream_ptr *)((char *)dream_p(__self) + 88);
  int64_t l4 = *(int64_t *)((char *)dream_p(__self) + 96);
  int32_t l5 = *(int32_t *)((char *)dream_p(__self) + 104);
  dream_ptr l6 = *(dream_ptr *)((char *)dream_p(__self) + 112);
  dream_ptr l7 = *(dream_ptr *)((char *)dream_p(__self) + 120);
  dream_ptr l8 = *(dream_ptr *)((char *)dream_p(__self) + 128);
  int32_t __st = *(int32_t *)dream_p(__self);
  switch (__st) {
    case 1: goto L1;
    case 2: goto L2;
    case 3: goto L3;
    case 4: goto L4;
    default: break;
  }
  goto L0;
L0:;
  l2 = ((*(int32_t*)((char*)dream_p(l0) + 0)));
  dream_release(l3);
  l3 = (__async_gpuSurfacePresent(l2));
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(int64_t *)((char *)dream_p(__self) + 72) = l1;
  *(int64_t *)((char *)dream_p(__self) + 80) = l2;
  *(dream_ptr *)((char *)dream_p(__self) + 88) = l3;
  *(int64_t *)((char *)dream_p(__self) + 96) = l4;
  *(int32_t *)((char *)dream_p(__self) + 104) = l5;
  *(dream_ptr *)((char *)dream_p(__self) + 112) = l6;
  *(dream_ptr *)((char *)dream_p(__self) + 120) = l7;
  *(dream_ptr *)((char *)dream_p(__self) + 128) = l8;
  *(int32_t *)dream_p(__self) = 1;
  dream_await(__self, l3);
  return 0;
L1:;
  {
    dream_ptr __ch = *(dream_ptr *)((char *)dream_p(__self) + 36);
    l4 = (int64_t)*(dream_ptr *)((char *)dream_p(__ch) + 8);
  }
  l1 = (l4);
  l5 = ((l1) == (0));
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(int64_t *)((char *)dream_p(__self) + 72) = l1;
  *(int64_t *)((char *)dream_p(__self) + 80) = l2;
  *(dream_ptr *)((char *)dream_p(__self) + 88) = l3;
  *(int64_t *)((char *)dream_p(__self) + 96) = l4;
  *(int32_t *)((char *)dream_p(__self) + 104) = l5;
  *(dream_ptr *)((char *)dream_p(__self) + 112) = l6;
  *(dream_ptr *)((char *)dream_p(__self) + 120) = l7;
  *(dream_ptr *)((char *)dream_p(__self) + 128) = l8;
  if (l5) goto L2; else goto L3;
L2:;
  release_Result_bool_GpuError(l6);
  l6 = (({ dream_ptr __o = dream_malloc(16, 21); memset(dream_p(__o), 0, 16); *(int32_t*)dream_p(__o) = 0; *(uint8_t*)((char*)dream_p(__o) + 4) = (uint8_t)(1);  __o; }));
  dream_release(l3);
  release_GpuError(l7);
  release_Result_bool_GpuError(l8);
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(int64_t *)((char *)dream_p(__self) + 72) = l1;
  *(int64_t *)((char *)dream_p(__self) + 80) = l2;
  *(dream_ptr *)((char *)dream_p(__self) + 88) = l3;
  *(int64_t *)((char *)dream_p(__self) + 96) = l4;
  *(int32_t *)((char *)dream_p(__self) + 104) = l5;
  *(dream_ptr *)((char *)dream_p(__self) + 112) = l6;
  *(dream_ptr *)((char *)dream_p(__self) + 120) = l7;
  *(dream_ptr *)((char *)dream_p(__self) + 128) = l8;
  dream_async_complete(__self, (dream_ptr)l6); return 0;
L3:;
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(int64_t *)((char *)dream_p(__self) + 72) = l1;
  *(int64_t *)((char *)dream_p(__self) + 80) = l2;
  *(dream_ptr *)((char *)dream_p(__self) + 88) = l3;
  *(int64_t *)((char *)dream_p(__self) + 96) = l4;
  *(int32_t *)((char *)dream_p(__self) + 104) = l5;
  *(dream_ptr *)((char *)dream_p(__self) + 112) = l6;
  *(dream_ptr *)((char *)dream_p(__self) + 120) = l7;
  *(dream_ptr *)((char *)dream_p(__self) + 128) = l8;
  goto L4;
L4:;
  release_GpuError(l7);
  dream_retain(__ds12);
  l7 = (GpuError_from_code(l1, __ds12));
  release_Result_bool_GpuError(l8);
  l8 = (({ dream_ptr __o = dream_malloc(16, 21); memset(dream_p(__o), 0, 16); *(int32_t*)dream_p(__o) = 1; *(dream_ptr*)((char*)dream_p(__o) + 8) = (dream_ptr)(l7); dream_retain(*(dream_ptr*)((char*)dream_p(__o) + 8));  __o; }));
  dream_release(l3);
  release_Result_bool_GpuError(l6);
  release_GpuError(l7);
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(int64_t *)((char *)dream_p(__self) + 72) = l1;
  *(int64_t *)((char *)dream_p(__self) + 80) = l2;
  *(dream_ptr *)((char *)dream_p(__self) + 88) = l3;
  *(int64_t *)((char *)dream_p(__self) + 96) = l4;
  *(int32_t *)((char *)dream_p(__self) + 104) = l5;
  *(dream_ptr *)((char *)dream_p(__self) + 112) = l6;
  *(dream_ptr *)((char *)dream_p(__self) + 120) = l7;
  *(dream_ptr *)((char *)dream_p(__self) + 128) = l8;
  dream_async_complete(__self, (dream_ptr)l8); return 0;
  return 0;
}

int32_t GpuSurface_close_requested(dream_ptr l0) {
  int64_t l1 = 0;
  int32_t l2 = 0;
  goto L0;
L0:;
  l1 = ((*(int32_t*)((char*)dream_p(l0) + 0)));
  return gpuSurfaceCloseRequested(l1);
}

dream_ptr GpuRenderPipelineDesc_overlay(void) {
  _Alignas(8) unsigned char __vs0[28] = {0};
  dream_ptr l0 = (dream_ptr)(uintptr_t)__vs0;
  goto L0;
L0:;
  ({ dream_ptr __v = ({ dream_ptr __o = dream_malloc(28, 18); memset(dream_p(__o), 0, 28); __o; }); memcpy(dream_p(l0), dream_p(__v), 28); dream_free(__v); });
  *(int32_t*)((char*)dream_p(l0) + 0) = (int32_t)(0);
  *(int32_t*)((char*)dream_p(l0) + 4) = (int32_t)(0);
  *(int32_t*)((char*)dream_p(l0) + 8) = (int32_t)(0);
  *(uint8_t*)((char*)dream_p(l0) + 12) = (uint8_t)(0);
  *(uint8_t*)((char*)dream_p(l0) + 13) = (uint8_t)(0);
  *(int32_t*)((char*)dream_p(l0) + 16) = (int32_t)(0);
  *(uint8_t*)((char*)dream_p(l0) + 20) = (uint8_t)(1);
  *(int32_t*)((char*)dream_p(l0) + 24) = (int32_t)(1);
  { dream_ptr __r = dream_malloc(28, 18); memcpy(dream_p(__r), dream_p(l0), 28); return __r; }
}

dream_ptr GpuRenderPipeline_create_ex(dream_ptr l0, dream_ptr l1, dream_ptr l2) {
  dream_ptr __self = dream_new_future(272, 29, 0);
  *(dream_ptr *)((char *)dream_p(__self) + 64) = (dream_ptr)l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = (dream_ptr)l1;
  memcpy((char *)dream_p(__self) + 80, dream_p(l2), 28);
  dream_enqueue(__self);
  return __self;
}

int32_t poll_GpuRenderPipeline_create_ex(dream_ptr __self) {
  dream_ptr l0 = *(dream_ptr *)((char *)dream_p(__self) + 64);
  dream_ptr l1 = *(dream_ptr *)((char *)dream_p(__self) + 72);
  dream_ptr l2 = (dream_ptr)((char *)dream_p(__self) + 80);
  int64_t l3 = *(int64_t *)((char *)dream_p(__self) + 112);
  int64_t l4 = *(int64_t *)((char *)dream_p(__self) + 120);
  int64_t l5 = *(int64_t *)((char *)dream_p(__self) + 128);
  int64_t l6 = *(int64_t *)((char *)dream_p(__self) + 136);
  dream_ptr l7 = *(dream_ptr *)((char *)dream_p(__self) + 144);
  int32_t l8 = *(int32_t *)((char *)dream_p(__self) + 152);
  int32_t l9 = *(int32_t *)((char *)dream_p(__self) + 160);
  int32_t l10 = *(int32_t *)((char *)dream_p(__self) + 168);
  int64_t l11 = *(int64_t *)((char *)dream_p(__self) + 176);
  int64_t l12 = *(int64_t *)((char *)dream_p(__self) + 184);
  int64_t l13 = *(int64_t *)((char *)dream_p(__self) + 192);
  int64_t l14 = *(int64_t *)((char *)dream_p(__self) + 200);
  int64_t l15 = *(int64_t *)((char *)dream_p(__self) + 208);
  dream_ptr l16 = *(dream_ptr *)((char *)dream_p(__self) + 216);
  int64_t l17 = *(int64_t *)((char *)dream_p(__self) + 224);
  int32_t l18 = *(int32_t *)((char *)dream_p(__self) + 232);
  int64_t l19 = *(int64_t *)((char *)dream_p(__self) + 240);
  dream_ptr l20 = *(dream_ptr *)((char *)dream_p(__self) + 248);
  dream_ptr l21 = *(dream_ptr *)((char *)dream_p(__self) + 256);
  dream_ptr l22 = *(dream_ptr *)((char *)dream_p(__self) + 264);
  int32_t __st = *(int32_t *)dream_p(__self);
  switch (__st) {
    case 1: goto L1;
    case 2: goto L2;
    case 3: goto L3;
    case 4: goto L4;
    case 5: goto L5;
    case 6: goto L6;
    case 7: goto L7;
    case 8: goto L8;
    case 9: goto L9;
    case 10: goto L10;
    case 11: goto L11;
    case 12: goto L12;
    case 13: goto L13;
    default: break;
  }
  goto L0;
L0:;
  l3 = (0);
  l8 = ((*(uint8_t*)((char*)dream_p(l2) + 12)));
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = l1;
  *(int64_t *)((char *)dream_p(__self) + 112) = l3;
  *(int64_t *)((char *)dream_p(__self) + 120) = l4;
  *(int64_t *)((char *)dream_p(__self) + 128) = l5;
  *(int64_t *)((char *)dream_p(__self) + 136) = l6;
  *(dream_ptr *)((char *)dream_p(__self) + 144) = l7;
  *(int32_t *)((char *)dream_p(__self) + 152) = l8;
  *(int32_t *)((char *)dream_p(__self) + 160) = l9;
  *(int32_t *)((char *)dream_p(__self) + 168) = l10;
  *(int64_t *)((char *)dream_p(__self) + 176) = l11;
  *(int64_t *)((char *)dream_p(__self) + 184) = l12;
  *(int64_t *)((char *)dream_p(__self) + 192) = l13;
  *(int64_t *)((char *)dream_p(__self) + 200) = l14;
  *(int64_t *)((char *)dream_p(__self) + 208) = l15;
  *(dream_ptr *)((char *)dream_p(__self) + 216) = l16;
  *(int64_t *)((char *)dream_p(__self) + 224) = l17;
  *(int32_t *)((char *)dream_p(__self) + 232) = l18;
  *(int64_t *)((char *)dream_p(__self) + 240) = l19;
  *(dream_ptr *)((char *)dream_p(__self) + 248) = l20;
  *(dream_ptr *)((char *)dream_p(__self) + 256) = l21;
  *(dream_ptr *)((char *)dream_p(__self) + 264) = l22;
  if (l8) goto L1; else goto L2;
L1:;
  l3 = (1);
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = l1;
  *(int64_t *)((char *)dream_p(__self) + 112) = l3;
  *(int64_t *)((char *)dream_p(__self) + 120) = l4;
  *(int64_t *)((char *)dream_p(__self) + 128) = l5;
  *(int64_t *)((char *)dream_p(__self) + 136) = l6;
  *(dream_ptr *)((char *)dream_p(__self) + 144) = l7;
  *(int32_t *)((char *)dream_p(__self) + 152) = l8;
  *(int32_t *)((char *)dream_p(__self) + 160) = l9;
  *(int32_t *)((char *)dream_p(__self) + 168) = l10;
  *(int64_t *)((char *)dream_p(__self) + 176) = l11;
  *(int64_t *)((char *)dream_p(__self) + 184) = l12;
  *(int64_t *)((char *)dream_p(__self) + 192) = l13;
  *(int64_t *)((char *)dream_p(__self) + 200) = l14;
  *(int64_t *)((char *)dream_p(__self) + 208) = l15;
  *(dream_ptr *)((char *)dream_p(__self) + 216) = l16;
  *(int64_t *)((char *)dream_p(__self) + 224) = l17;
  *(int32_t *)((char *)dream_p(__self) + 232) = l18;
  *(int64_t *)((char *)dream_p(__self) + 240) = l19;
  *(dream_ptr *)((char *)dream_p(__self) + 248) = l20;
  *(dream_ptr *)((char *)dream_p(__self) + 256) = l21;
  *(dream_ptr *)((char *)dream_p(__self) + 264) = l22;
  goto L3;
L2:;
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = l1;
  *(int64_t *)((char *)dream_p(__self) + 112) = l3;
  *(int64_t *)((char *)dream_p(__self) + 120) = l4;
  *(int64_t *)((char *)dream_p(__self) + 128) = l5;
  *(int64_t *)((char *)dream_p(__self) + 136) = l6;
  *(dream_ptr *)((char *)dream_p(__self) + 144) = l7;
  *(int32_t *)((char *)dream_p(__self) + 152) = l8;
  *(int32_t *)((char *)dream_p(__self) + 160) = l9;
  *(int32_t *)((char *)dream_p(__self) + 168) = l10;
  *(int64_t *)((char *)dream_p(__self) + 176) = l11;
  *(int64_t *)((char *)dream_p(__self) + 184) = l12;
  *(int64_t *)((char *)dream_p(__self) + 192) = l13;
  *(int64_t *)((char *)dream_p(__self) + 200) = l14;
  *(int64_t *)((char *)dream_p(__self) + 208) = l15;
  *(dream_ptr *)((char *)dream_p(__self) + 216) = l16;
  *(int64_t *)((char *)dream_p(__self) + 224) = l17;
  *(int32_t *)((char *)dream_p(__self) + 232) = l18;
  *(int64_t *)((char *)dream_p(__self) + 240) = l19;
  *(dream_ptr *)((char *)dream_p(__self) + 248) = l20;
  *(dream_ptr *)((char *)dream_p(__self) + 256) = l21;
  *(dream_ptr *)((char *)dream_p(__self) + 264) = l22;
  goto L3;
L3:;
  l4 = (0);
  l9 = ((*(uint8_t*)((char*)dream_p(l2) + 13)));
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = l1;
  *(int64_t *)((char *)dream_p(__self) + 112) = l3;
  *(int64_t *)((char *)dream_p(__self) + 120) = l4;
  *(int64_t *)((char *)dream_p(__self) + 128) = l5;
  *(int64_t *)((char *)dream_p(__self) + 136) = l6;
  *(dream_ptr *)((char *)dream_p(__self) + 144) = l7;
  *(int32_t *)((char *)dream_p(__self) + 152) = l8;
  *(int32_t *)((char *)dream_p(__self) + 160) = l9;
  *(int32_t *)((char *)dream_p(__self) + 168) = l10;
  *(int64_t *)((char *)dream_p(__self) + 176) = l11;
  *(int64_t *)((char *)dream_p(__self) + 184) = l12;
  *(int64_t *)((char *)dream_p(__self) + 192) = l13;
  *(int64_t *)((char *)dream_p(__self) + 200) = l14;
  *(int64_t *)((char *)dream_p(__self) + 208) = l15;
  *(dream_ptr *)((char *)dream_p(__self) + 216) = l16;
  *(int64_t *)((char *)dream_p(__self) + 224) = l17;
  *(int32_t *)((char *)dream_p(__self) + 232) = l18;
  *(int64_t *)((char *)dream_p(__self) + 240) = l19;
  *(dream_ptr *)((char *)dream_p(__self) + 248) = l20;
  *(dream_ptr *)((char *)dream_p(__self) + 256) = l21;
  *(dream_ptr *)((char *)dream_p(__self) + 264) = l22;
  if (l9) goto L4; else goto L5;
L4:;
  l4 = (1);
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = l1;
  *(int64_t *)((char *)dream_p(__self) + 112) = l3;
  *(int64_t *)((char *)dream_p(__self) + 120) = l4;
  *(int64_t *)((char *)dream_p(__self) + 128) = l5;
  *(int64_t *)((char *)dream_p(__self) + 136) = l6;
  *(dream_ptr *)((char *)dream_p(__self) + 144) = l7;
  *(int32_t *)((char *)dream_p(__self) + 152) = l8;
  *(int32_t *)((char *)dream_p(__self) + 160) = l9;
  *(int32_t *)((char *)dream_p(__self) + 168) = l10;
  *(int64_t *)((char *)dream_p(__self) + 176) = l11;
  *(int64_t *)((char *)dream_p(__self) + 184) = l12;
  *(int64_t *)((char *)dream_p(__self) + 192) = l13;
  *(int64_t *)((char *)dream_p(__self) + 200) = l14;
  *(int64_t *)((char *)dream_p(__self) + 208) = l15;
  *(dream_ptr *)((char *)dream_p(__self) + 216) = l16;
  *(int64_t *)((char *)dream_p(__self) + 224) = l17;
  *(int32_t *)((char *)dream_p(__self) + 232) = l18;
  *(int64_t *)((char *)dream_p(__self) + 240) = l19;
  *(dream_ptr *)((char *)dream_p(__self) + 248) = l20;
  *(dream_ptr *)((char *)dream_p(__self) + 256) = l21;
  *(dream_ptr *)((char *)dream_p(__self) + 264) = l22;
  goto L6;
L5:;
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = l1;
  *(int64_t *)((char *)dream_p(__self) + 112) = l3;
  *(int64_t *)((char *)dream_p(__self) + 120) = l4;
  *(int64_t *)((char *)dream_p(__self) + 128) = l5;
  *(int64_t *)((char *)dream_p(__self) + 136) = l6;
  *(dream_ptr *)((char *)dream_p(__self) + 144) = l7;
  *(int32_t *)((char *)dream_p(__self) + 152) = l8;
  *(int32_t *)((char *)dream_p(__self) + 160) = l9;
  *(int32_t *)((char *)dream_p(__self) + 168) = l10;
  *(int64_t *)((char *)dream_p(__self) + 176) = l11;
  *(int64_t *)((char *)dream_p(__self) + 184) = l12;
  *(int64_t *)((char *)dream_p(__self) + 192) = l13;
  *(int64_t *)((char *)dream_p(__self) + 200) = l14;
  *(int64_t *)((char *)dream_p(__self) + 208) = l15;
  *(dream_ptr *)((char *)dream_p(__self) + 216) = l16;
  *(int64_t *)((char *)dream_p(__self) + 224) = l17;
  *(int32_t *)((char *)dream_p(__self) + 232) = l18;
  *(int64_t *)((char *)dream_p(__self) + 240) = l19;
  *(dream_ptr *)((char *)dream_p(__self) + 248) = l20;
  *(dream_ptr *)((char *)dream_p(__self) + 256) = l21;
  *(dream_ptr *)((char *)dream_p(__self) + 264) = l22;
  goto L6;
L6:;
  l5 = (0);
  l10 = ((*(uint8_t*)((char*)dream_p(l2) + 20)));
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = l1;
  *(int64_t *)((char *)dream_p(__self) + 112) = l3;
  *(int64_t *)((char *)dream_p(__self) + 120) = l4;
  *(int64_t *)((char *)dream_p(__self) + 128) = l5;
  *(int64_t *)((char *)dream_p(__self) + 136) = l6;
  *(dream_ptr *)((char *)dream_p(__self) + 144) = l7;
  *(int32_t *)((char *)dream_p(__self) + 152) = l8;
  *(int32_t *)((char *)dream_p(__self) + 160) = l9;
  *(int32_t *)((char *)dream_p(__self) + 168) = l10;
  *(int64_t *)((char *)dream_p(__self) + 176) = l11;
  *(int64_t *)((char *)dream_p(__self) + 184) = l12;
  *(int64_t *)((char *)dream_p(__self) + 192) = l13;
  *(int64_t *)((char *)dream_p(__self) + 200) = l14;
  *(int64_t *)((char *)dream_p(__self) + 208) = l15;
  *(dream_ptr *)((char *)dream_p(__self) + 216) = l16;
  *(int64_t *)((char *)dream_p(__self) + 224) = l17;
  *(int32_t *)((char *)dream_p(__self) + 232) = l18;
  *(int64_t *)((char *)dream_p(__self) + 240) = l19;
  *(dream_ptr *)((char *)dream_p(__self) + 248) = l20;
  *(dream_ptr *)((char *)dream_p(__self) + 256) = l21;
  *(dream_ptr *)((char *)dream_p(__self) + 264) = l22;
  if (l10) goto L7; else goto L8;
L7:;
  l5 = (1);
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = l1;
  *(int64_t *)((char *)dream_p(__self) + 112) = l3;
  *(int64_t *)((char *)dream_p(__self) + 120) = l4;
  *(int64_t *)((char *)dream_p(__self) + 128) = l5;
  *(int64_t *)((char *)dream_p(__self) + 136) = l6;
  *(dream_ptr *)((char *)dream_p(__self) + 144) = l7;
  *(int32_t *)((char *)dream_p(__self) + 152) = l8;
  *(int32_t *)((char *)dream_p(__self) + 160) = l9;
  *(int32_t *)((char *)dream_p(__self) + 168) = l10;
  *(int64_t *)((char *)dream_p(__self) + 176) = l11;
  *(int64_t *)((char *)dream_p(__self) + 184) = l12;
  *(int64_t *)((char *)dream_p(__self) + 192) = l13;
  *(int64_t *)((char *)dream_p(__self) + 200) = l14;
  *(int64_t *)((char *)dream_p(__self) + 208) = l15;
  *(dream_ptr *)((char *)dream_p(__self) + 216) = l16;
  *(int64_t *)((char *)dream_p(__self) + 224) = l17;
  *(int32_t *)((char *)dream_p(__self) + 232) = l18;
  *(int64_t *)((char *)dream_p(__self) + 240) = l19;
  *(dream_ptr *)((char *)dream_p(__self) + 248) = l20;
  *(dream_ptr *)((char *)dream_p(__self) + 256) = l21;
  *(dream_ptr *)((char *)dream_p(__self) + 264) = l22;
  goto L9;
L8:;
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = l1;
  *(int64_t *)((char *)dream_p(__self) + 112) = l3;
  *(int64_t *)((char *)dream_p(__self) + 120) = l4;
  *(int64_t *)((char *)dream_p(__self) + 128) = l5;
  *(int64_t *)((char *)dream_p(__self) + 136) = l6;
  *(dream_ptr *)((char *)dream_p(__self) + 144) = l7;
  *(int32_t *)((char *)dream_p(__self) + 152) = l8;
  *(int32_t *)((char *)dream_p(__self) + 160) = l9;
  *(int32_t *)((char *)dream_p(__self) + 168) = l10;
  *(int64_t *)((char *)dream_p(__self) + 176) = l11;
  *(int64_t *)((char *)dream_p(__self) + 184) = l12;
  *(int64_t *)((char *)dream_p(__self) + 192) = l13;
  *(int64_t *)((char *)dream_p(__self) + 200) = l14;
  *(int64_t *)((char *)dream_p(__self) + 208) = l15;
  *(dream_ptr *)((char *)dream_p(__self) + 216) = l16;
  *(int64_t *)((char *)dream_p(__self) + 224) = l17;
  *(int32_t *)((char *)dream_p(__self) + 232) = l18;
  *(int64_t *)((char *)dream_p(__self) + 240) = l19;
  *(dream_ptr *)((char *)dream_p(__self) + 248) = l20;
  *(dream_ptr *)((char *)dream_p(__self) + 256) = l21;
  *(dream_ptr *)((char *)dream_p(__self) + 264) = l22;
  goto L9;
L9:;
  l11 = ((*(int32_t*)((char*)dream_p(l2) + 0)));
  l12 = ((*(int32_t*)((char*)dream_p(l2) + 4)));
  l13 = ((*(int32_t*)((char*)dream_p(l2) + 8)));
  l14 = ((*(int32_t*)((char*)dream_p(l2) + 16)));
  l15 = ((*(int32_t*)((char*)dream_p(l2) + 24)));
  dream_release(l16);
  l16 = (__async_gpuRenderPipelineCreateEx(l0, l1, l11, l12, l13, l3, l4, l14, l5, l15));
  l0 = (0);
  l1 = (0);
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = l1;
  *(int64_t *)((char *)dream_p(__self) + 112) = l3;
  *(int64_t *)((char *)dream_p(__self) + 120) = l4;
  *(int64_t *)((char *)dream_p(__self) + 128) = l5;
  *(int64_t *)((char *)dream_p(__self) + 136) = l6;
  *(dream_ptr *)((char *)dream_p(__self) + 144) = l7;
  *(int32_t *)((char *)dream_p(__self) + 152) = l8;
  *(int32_t *)((char *)dream_p(__self) + 160) = l9;
  *(int32_t *)((char *)dream_p(__self) + 168) = l10;
  *(int64_t *)((char *)dream_p(__self) + 176) = l11;
  *(int64_t *)((char *)dream_p(__self) + 184) = l12;
  *(int64_t *)((char *)dream_p(__self) + 192) = l13;
  *(int64_t *)((char *)dream_p(__self) + 200) = l14;
  *(int64_t *)((char *)dream_p(__self) + 208) = l15;
  *(dream_ptr *)((char *)dream_p(__self) + 216) = l16;
  *(int64_t *)((char *)dream_p(__self) + 224) = l17;
  *(int32_t *)((char *)dream_p(__self) + 232) = l18;
  *(int64_t *)((char *)dream_p(__self) + 240) = l19;
  *(dream_ptr *)((char *)dream_p(__self) + 248) = l20;
  *(dream_ptr *)((char *)dream_p(__self) + 256) = l21;
  *(dream_ptr *)((char *)dream_p(__self) + 264) = l22;
  *(int32_t *)dream_p(__self) = 10;
  dream_await(__self, l16);
  return 0;
L10:;
  {
    dream_ptr __ch = *(dream_ptr *)((char *)dream_p(__self) + 36);
    l17 = (int64_t)*(dream_ptr *)((char *)dream_p(__ch) + 8);
  }
  l6 = (l17);
  l18 = ((l6) < (0));
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = l1;
  *(int64_t *)((char *)dream_p(__self) + 112) = l3;
  *(int64_t *)((char *)dream_p(__self) + 120) = l4;
  *(int64_t *)((char *)dream_p(__self) + 128) = l5;
  *(int64_t *)((char *)dream_p(__self) + 136) = l6;
  *(dream_ptr *)((char *)dream_p(__self) + 144) = l7;
  *(int32_t *)((char *)dream_p(__self) + 152) = l8;
  *(int32_t *)((char *)dream_p(__self) + 160) = l9;
  *(int32_t *)((char *)dream_p(__self) + 168) = l10;
  *(int64_t *)((char *)dream_p(__self) + 176) = l11;
  *(int64_t *)((char *)dream_p(__self) + 184) = l12;
  *(int64_t *)((char *)dream_p(__self) + 192) = l13;
  *(int64_t *)((char *)dream_p(__self) + 200) = l14;
  *(int64_t *)((char *)dream_p(__self) + 208) = l15;
  *(dream_ptr *)((char *)dream_p(__self) + 216) = l16;
  *(int64_t *)((char *)dream_p(__self) + 224) = l17;
  *(int32_t *)((char *)dream_p(__self) + 232) = l18;
  *(int64_t *)((char *)dream_p(__self) + 240) = l19;
  *(dream_ptr *)((char *)dream_p(__self) + 248) = l20;
  *(dream_ptr *)((char *)dream_p(__self) + 256) = l21;
  *(dream_ptr *)((char *)dream_p(__self) + 264) = l22;
  if (l18) goto L11; else goto L12;
L11:;
  l19 = (-(l6));
  release_GpuError(l20);
  dream_retain(__ds13);
  l20 = (GpuError_from_code(l19, __ds13));
  release_Result_GpuRenderPipeline_GpuError(l21);
  l21 = (({ dream_ptr __o = dream_malloc(16, 23); memset(dream_p(__o), 0, 16); *(int32_t*)dream_p(__o) = 1; *(dream_ptr*)((char*)dream_p(__o) + 8) = (dream_ptr)(l20); dream_retain(*(dream_ptr*)((char*)dream_p(__o) + 8));  __o; }));
  dream_release(l0);
  dream_release(l1);
  release_GpuRenderPipeline(l7);
  dream_release(l16);
  release_GpuError(l20);
  release_Result_GpuRenderPipeline_GpuError(l22);
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = l1;
  *(int64_t *)((char *)dream_p(__self) + 112) = l3;
  *(int64_t *)((char *)dream_p(__self) + 120) = l4;
  *(int64_t *)((char *)dream_p(__self) + 128) = l5;
  *(int64_t *)((char *)dream_p(__self) + 136) = l6;
  *(dream_ptr *)((char *)dream_p(__self) + 144) = l7;
  *(int32_t *)((char *)dream_p(__self) + 152) = l8;
  *(int32_t *)((char *)dream_p(__self) + 160) = l9;
  *(int32_t *)((char *)dream_p(__self) + 168) = l10;
  *(int64_t *)((char *)dream_p(__self) + 176) = l11;
  *(int64_t *)((char *)dream_p(__self) + 184) = l12;
  *(int64_t *)((char *)dream_p(__self) + 192) = l13;
  *(int64_t *)((char *)dream_p(__self) + 200) = l14;
  *(int64_t *)((char *)dream_p(__self) + 208) = l15;
  *(dream_ptr *)((char *)dream_p(__self) + 216) = l16;
  *(int64_t *)((char *)dream_p(__self) + 224) = l17;
  *(int32_t *)((char *)dream_p(__self) + 232) = l18;
  *(int64_t *)((char *)dream_p(__self) + 240) = l19;
  *(dream_ptr *)((char *)dream_p(__self) + 248) = l20;
  *(dream_ptr *)((char *)dream_p(__self) + 256) = l21;
  *(dream_ptr *)((char *)dream_p(__self) + 264) = l22;
  dream_async_complete(__self, (dream_ptr)l21); return 0;
L12:;
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = l1;
  *(int64_t *)((char *)dream_p(__self) + 112) = l3;
  *(int64_t *)((char *)dream_p(__self) + 120) = l4;
  *(int64_t *)((char *)dream_p(__self) + 128) = l5;
  *(int64_t *)((char *)dream_p(__self) + 136) = l6;
  *(dream_ptr *)((char *)dream_p(__self) + 144) = l7;
  *(int32_t *)((char *)dream_p(__self) + 152) = l8;
  *(int32_t *)((char *)dream_p(__self) + 160) = l9;
  *(int32_t *)((char *)dream_p(__self) + 168) = l10;
  *(int64_t *)((char *)dream_p(__self) + 176) = l11;
  *(int64_t *)((char *)dream_p(__self) + 184) = l12;
  *(int64_t *)((char *)dream_p(__self) + 192) = l13;
  *(int64_t *)((char *)dream_p(__self) + 200) = l14;
  *(int64_t *)((char *)dream_p(__self) + 208) = l15;
  *(dream_ptr *)((char *)dream_p(__self) + 216) = l16;
  *(int64_t *)((char *)dream_p(__self) + 224) = l17;
  *(int32_t *)((char *)dream_p(__self) + 232) = l18;
  *(int64_t *)((char *)dream_p(__self) + 240) = l19;
  *(dream_ptr *)((char *)dream_p(__self) + 248) = l20;
  *(dream_ptr *)((char *)dream_p(__self) + 256) = l21;
  *(dream_ptr *)((char *)dream_p(__self) + 264) = l22;
  goto L13;
L13:;
  release_GpuRenderPipeline(l7);
  l7 = (({ dream_ptr __o = dream_malloc(4, 19); memset(dream_p(__o), 0, 4); __o; }));
  *(int32_t*)((char*)dream_p(l7) + 0) = (int32_t)(l6);
  release_Result_GpuRenderPipeline_GpuError(l22);
  l22 = (({ dream_ptr __o = dream_malloc(16, 23); memset(dream_p(__o), 0, 16); *(int32_t*)dream_p(__o) = 0; *(dream_ptr*)((char*)dream_p(__o) + 8) = (dream_ptr)(l7); dream_retain(*(dream_ptr*)((char*)dream_p(__o) + 8));  __o; }));
  dream_release(l0);
  dream_release(l1);
  release_GpuRenderPipeline(l7);
  dream_release(l16);
  release_GpuError(l20);
  release_Result_GpuRenderPipeline_GpuError(l21);
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = l1;
  *(int64_t *)((char *)dream_p(__self) + 112) = l3;
  *(int64_t *)((char *)dream_p(__self) + 120) = l4;
  *(int64_t *)((char *)dream_p(__self) + 128) = l5;
  *(int64_t *)((char *)dream_p(__self) + 136) = l6;
  *(dream_ptr *)((char *)dream_p(__self) + 144) = l7;
  *(int32_t *)((char *)dream_p(__self) + 152) = l8;
  *(int32_t *)((char *)dream_p(__self) + 160) = l9;
  *(int32_t *)((char *)dream_p(__self) + 168) = l10;
  *(int64_t *)((char *)dream_p(__self) + 176) = l11;
  *(int64_t *)((char *)dream_p(__self) + 184) = l12;
  *(int64_t *)((char *)dream_p(__self) + 192) = l13;
  *(int64_t *)((char *)dream_p(__self) + 200) = l14;
  *(int64_t *)((char *)dream_p(__self) + 208) = l15;
  *(dream_ptr *)((char *)dream_p(__self) + 216) = l16;
  *(int64_t *)((char *)dream_p(__self) + 224) = l17;
  *(int32_t *)((char *)dream_p(__self) + 232) = l18;
  *(int64_t *)((char *)dream_p(__self) + 240) = l19;
  *(dream_ptr *)((char *)dream_p(__self) + 248) = l20;
  *(dream_ptr *)((char *)dream_p(__self) + 256) = l21;
  *(dream_ptr *)((char *)dream_p(__self) + 264) = l22;
  dream_async_complete(__self, (dream_ptr)l22); return 0;
  return 0;
}

dream_ptr Gpu_try_init(void) {
  dream_ptr __self = dream_new_future(120, 30, 0);
  dream_enqueue(__self);
  return __self;
}

int32_t poll_Gpu_try_init(dream_ptr __self) {
  int64_t l0 = *(int64_t *)((char *)dream_p(__self) + 64);
  dream_ptr l1 = *(dream_ptr *)((char *)dream_p(__self) + 72);
  int64_t l2 = *(int64_t *)((char *)dream_p(__self) + 80);
  int32_t l3 = *(int32_t *)((char *)dream_p(__self) + 88);
  dream_ptr l4 = *(dream_ptr *)((char *)dream_p(__self) + 96);
  dream_ptr l5 = *(dream_ptr *)((char *)dream_p(__self) + 104);
  dream_ptr l6 = *(dream_ptr *)((char *)dream_p(__self) + 112);
  int32_t __st = *(int32_t *)dream_p(__self);
  switch (__st) {
    case 1: goto L1;
    case 2: goto L2;
    case 3: goto L3;
    case 4: goto L4;
    default: break;
  }
  goto L0;
L0:;
  dream_release(l1);
  l1 = (__async_gpuTryInit());
  *(int64_t *)((char *)dream_p(__self) + 64) = l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = l1;
  *(int64_t *)((char *)dream_p(__self) + 80) = l2;
  *(int32_t *)((char *)dream_p(__self) + 88) = l3;
  *(dream_ptr *)((char *)dream_p(__self) + 96) = l4;
  *(dream_ptr *)((char *)dream_p(__self) + 104) = l5;
  *(dream_ptr *)((char *)dream_p(__self) + 112) = l6;
  *(int32_t *)dream_p(__self) = 1;
  dream_await(__self, l1);
  return 0;
L1:;
  {
    dream_ptr __ch = *(dream_ptr *)((char *)dream_p(__self) + 36);
    l2 = (int64_t)*(dream_ptr *)((char *)dream_p(__ch) + 8);
  }
  l0 = (l2);
  l3 = ((l0) == (0));
  *(int64_t *)((char *)dream_p(__self) + 64) = l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = l1;
  *(int64_t *)((char *)dream_p(__self) + 80) = l2;
  *(int32_t *)((char *)dream_p(__self) + 88) = l3;
  *(dream_ptr *)((char *)dream_p(__self) + 96) = l4;
  *(dream_ptr *)((char *)dream_p(__self) + 104) = l5;
  *(dream_ptr *)((char *)dream_p(__self) + 112) = l6;
  if (l3) goto L2; else goto L3;
L2:;
  release_Result_bool_GpuError(l4);
  l4 = (({ dream_ptr __o = dream_malloc(16, 21); memset(dream_p(__o), 0, 16); *(int32_t*)dream_p(__o) = 0; *(uint8_t*)((char*)dream_p(__o) + 4) = (uint8_t)(1);  __o; }));
  dream_release(l1);
  release_GpuError(l5);
  release_Result_bool_GpuError(l6);
  *(int64_t *)((char *)dream_p(__self) + 64) = l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = l1;
  *(int64_t *)((char *)dream_p(__self) + 80) = l2;
  *(int32_t *)((char *)dream_p(__self) + 88) = l3;
  *(dream_ptr *)((char *)dream_p(__self) + 96) = l4;
  *(dream_ptr *)((char *)dream_p(__self) + 104) = l5;
  *(dream_ptr *)((char *)dream_p(__self) + 112) = l6;
  dream_async_complete(__self, (dream_ptr)l4); return 0;
L3:;
  *(int64_t *)((char *)dream_p(__self) + 64) = l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = l1;
  *(int64_t *)((char *)dream_p(__self) + 80) = l2;
  *(int32_t *)((char *)dream_p(__self) + 88) = l3;
  *(dream_ptr *)((char *)dream_p(__self) + 96) = l4;
  *(dream_ptr *)((char *)dream_p(__self) + 104) = l5;
  *(dream_ptr *)((char *)dream_p(__self) + 112) = l6;
  goto L4;
L4:;
  release_GpuError(l5);
  dream_retain(__ds14);
  l5 = (GpuError_from_code(l0, __ds14));
  release_Result_bool_GpuError(l6);
  l6 = (({ dream_ptr __o = dream_malloc(16, 21); memset(dream_p(__o), 0, 16); *(int32_t*)dream_p(__o) = 1; *(dream_ptr*)((char*)dream_p(__o) + 8) = (dream_ptr)(l5); dream_retain(*(dream_ptr*)((char*)dream_p(__o) + 8));  __o; }));
  dream_release(l1);
  release_Result_bool_GpuError(l4);
  release_GpuError(l5);
  *(int64_t *)((char *)dream_p(__self) + 64) = l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = l1;
  *(int64_t *)((char *)dream_p(__self) + 80) = l2;
  *(int32_t *)((char *)dream_p(__self) + 88) = l3;
  *(dream_ptr *)((char *)dream_p(__self) + 96) = l4;
  *(dream_ptr *)((char *)dream_p(__self) + 104) = l5;
  *(dream_ptr *)((char *)dream_p(__self) + 112) = l6;
  dream_async_complete(__self, (dream_ptr)l6); return 0;
  return 0;
}

dream_ptr Gpu_frame(void) {
  dream_ptr __self = dream_new_future(72, 31, 0);
  dream_enqueue(__self);
  return __self;
}

int32_t poll_Gpu_frame(dream_ptr __self) {
  dream_ptr l0 = *(dream_ptr *)((char *)dream_p(__self) + 64);
  int32_t __st = *(int32_t *)dream_p(__self);
  switch (__st) {
    case 1: goto L1;
    default: break;
  }
  goto L0;
L0:;
  dream_release(l0);
  l0 = (__async_gpuFrame());
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(int32_t *)dream_p(__self) = 1;
  dream_await(__self, l0);
  return 0;
L1:;
  dream_release(l0);
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  dream_async_complete(__self, 0); return 0;
  return 0;
}

int32_t Result_bool_GpuError_is_err(dream_ptr l0) {
  int32_t l1 = 0;
  int32_t l2 = 0;
  dream_ptr l3 = 0;
  int64_t l4 = 0;
  goto L0;
L0:;
  l4 = (*(int32_t*)dream_p(l0));
  {
    static void *const __jt[] = {
      &&L3,
      &&L4,
    };
    unsigned __k = (unsigned)(l4); if (__k < 2) goto *__jt[__k]; goto L1;
  }
L1:;
  return l2;
L2:;
  abort();
L3:;
  l2 = (0);
  return 0;
L4:;
  l2 = (1);
  return 1;
}

int32_t Result_GpuSurface_GpuError_is_err(dream_ptr l0) {
  dream_ptr l1 = 0;
  int32_t l2 = 0;
  dream_ptr l3 = 0;
  int64_t l4 = 0;
  goto L0;
L0:;
  l4 = (*(int32_t*)dream_p(l0));
  {
    static void *const __jt[] = {
      &&L3,
      &&L4,
    };
    unsigned __k = (unsigned)(l4); if (__k < 2) goto *__jt[__k]; goto L1;
  }
L1:;
  return l2;
L2:;
  abort();
L3:;
  l2 = (0);
  return 0;
L4:;
  l2 = (1);
  return 1;
}

dream_ptr Result_GpuSurface_GpuError_unwrap_or(dream_ptr l0, dream_ptr l1) {
  dream_ptr l2 = 0;
  dream_ptr l3 = 0;
  dream_ptr l4 = 0;
  int64_t l5 = 0;
  goto L0;
L0:;
  l5 = (*(int32_t*)dream_p(l0));
  {
    static void *const __jt[] = {
      &&L3,
      &&L4,
    };
    unsigned __k = (unsigned)(l5); if (__k < 2) goto *__jt[__k]; goto L2;
  }
L1:;
  abort();
L2:;
  release_GpuSurface(l1);
  return l3;
L3:;
  l2 = ((*(dream_ptr*)((char*)dream_p(l0) + 8)));
  release_GpuSurface(l3);
  l3 = (l2);
  dream_retain(l2);
  goto L2;
L4:;
  release_GpuSurface(l3);
  l3 = (l1);
  l1 = (0);
  goto L2;
}

int32_t Result_GpuRenderPipeline_GpuError_is_err(dream_ptr l0) {
  dream_ptr l1 = 0;
  int32_t l2 = 0;
  dream_ptr l3 = 0;
  int64_t l4 = 0;
  goto L0;
L0:;
  l4 = (*(int32_t*)dream_p(l0));
  {
    static void *const __jt[] = {
      &&L3,
      &&L4,
    };
    unsigned __k = (unsigned)(l4); if (__k < 2) goto *__jt[__k]; goto L1;
  }
L1:;
  return l2;
L2:;
  abort();
L3:;
  l2 = (0);
  return 0;
L4:;
  l2 = (1);
  return 1;
}

dream_ptr Result_GpuRenderPipeline_GpuError_unwrap_or(dream_ptr l0, dream_ptr l1) {
  dream_ptr l2 = 0;
  dream_ptr l3 = 0;
  dream_ptr l4 = 0;
  int64_t l5 = 0;
  goto L0;
L0:;
  l5 = (*(int32_t*)dream_p(l0));
  {
    static void *const __jt[] = {
      &&L3,
      &&L4,
    };
    unsigned __k = (unsigned)(l5); if (__k < 2) goto *__jt[__k]; goto L2;
  }
L1:;
  abort();
L2:;
  release_GpuRenderPipeline(l1);
  return l3;
L3:;
  l2 = ((*(dream_ptr*)((char*)dream_p(l0) + 8)));
  release_GpuRenderPipeline(l3);
  l3 = (l2);
  dream_retain(l2);
  goto L2;
L4:;
  release_GpuRenderPipeline(l3);
  l3 = (l1);
  l1 = (0);
  goto L2;
}

int32_t GpuBuffer_Vertex_get_id(dream_ptr l0) {
  int64_t l1 = 0;
  goto L0;
L0:;
  l1 = ((*(int32_t*)((char*)dream_p(l0) + 0)));
  return l1;
}

dream_ptr GpuBuffer_Vertex_vertex_from(dream_ptr l0) {
  _Alignas(8) unsigned char __vs1[12] = {0};
  dream_ptr l1 = (dream_ptr)(uintptr_t)__vs1;
  int64_t l2 = 0;
  int64_t l3 = 0;
  dream_ptr l4 = 0;
  int64_t l5 = 0;
  _Alignas(8) unsigned char __vs6[12] = {0};
  dream_ptr l6 = (dream_ptr)(uintptr_t)__vs6;
  _Alignas(8) unsigned char __vs7[8] = {0};
  dream_ptr l7 = (dream_ptr)(uintptr_t)__vs7;
  dream_ptr l8 = 0;
  int64_t l9 = 0;
  goto L0;
L0:;
  l2 = (*(int32_t*)dream_p(l0));
  l3 = (l2);
  l4 = (0);
  l8 = (0);
  dream_release(0);
  l4 = (dream_array_new(1, 8));
  l7 = (dream_ptr)(((dream_ptr)(({ int32_t __idx = (int32_t)(0); int32_t __len = l4 ? *(int32_t*)dream_p(l4) : 0; if ((int64_t)(0) == (int64_t)__idx && (uint32_t)__idx >= (uint32_t)__len) dream_panic(__ds65); (char*)dream_p(l4) + 4 + (int64_t)(0)*8; }))));
  dream_release(0);
  l8 = (dream_to_bytes(l7, 8));
  l5 = (*(int32_t*)dream_p(l8));
  ({ dream_ptr __v = ({ dream_ptr __o = dream_malloc(12, 20); memset(dream_p(__o), 0, 12); __o; }); memcpy(dream_p(l6), dream_p(__v), 12); dream_free(__v); });
  l9 = ((l2) * (l5));
  *(int32_t*)((char*)dream_p(l6) + 0) = (int32_t)(gpuBufferAllocVertexBytes(l9));
  *(int32_t*)((char*)dream_p(l6) + 4) = (int32_t)(l3);
  *(int32_t*)((char*)dream_p(l6) + 8) = (int32_t)(l5);
  release_array_t87(l4);
  dream_release(l8);
  memcpy(dream_p(l1), dream_p(l6), 12); ;
  GpuBuffer_Vertex_write(l1, l0);
  l0 = (0);
  dream_release(0);
  { dream_ptr __r = dream_malloc(12, 20); memcpy(dream_p(__r), dream_p(l1), 12); return __r; }
L1:;
  abort();
L2:;
  abort();
}

void GpuBuffer_Vertex_write(dream_ptr l0, dream_ptr l1) {
  int64_t l2 = 0;
  int64_t l3 = 0;
  int64_t l4 = 0;
  dream_ptr l5 = 0;
  int64_t l6 = 0;
  dream_ptr l7 = 0;
  int64_t l8 = 0;
  int64_t l9 = 0;
  int32_t l10 = 0;
  _Alignas(8) unsigned char __vs11[8] = {0};
  dream_ptr l11 = (dream_ptr)(uintptr_t)__vs11;
  int32_t l12 = 0;
  int64_t l13 = 0;
  int64_t l14 = 0;
  goto L0;
L0:;
  l2 = ((*(int32_t*)((char*)dream_p(l0) + 0)));
  l3 = ((*(int32_t*)((char*)dream_p(l0) + 8)));
  l4 = (*(int32_t*)dream_p(l1));
  l9 = ((l4) * (l3));
  dream_release(l5);
  l5 = (dream_array_new(l9, 1));
  l6 = (0);
  goto L1;
L1:;
  l10 = ((l6) < (l4));
  if (l10) goto L2; else goto L4;
L2:;
  l11 = (dream_ptr)(((dream_ptr)(((char*)dream_p(l1) + 4 + (l6)*8))));
  dream_release(l7);
  l7 = (dream_to_bytes(l11, 8));
  l8 = (0);
  l13 = ((l6) * (l3));
  goto L5;
L3:;
  l6 = ((l6) + (1));
  goto L1;
L4:;
  gpuBufferWriteBytes(l2, l5);
  l5 = (0);
  release_array_t87(l1);
  dream_release(0);
  dream_release(l7);
  return;
L5:;
  l12 = ((l8) < (l3));
  if (l12) goto L6; else goto L3;
L6:;
  l14 = ((l13) + (l8));
  *(uint8_t*)(({ int32_t __idx = (int32_t)(l14); int32_t __len = l5 ? *(int32_t*)dream_p(l5) : 0; if ((int64_t)(l14) == (int64_t)__idx && (uint32_t)__idx >= (uint32_t)__len) dream_panic(__ds65); (char*)dream_p(l5) + 4 + (int64_t)(l14)*1; })) = (uint8_t)((*(uint8_t*)(({ int32_t __idx = (int32_t)(l8); int32_t __len = l7 ? *(int32_t*)dream_p(l7) : 0; if ((int64_t)(l8) == (int64_t)__idx && (uint32_t)__idx >= (uint32_t)__len) dream_panic(__ds65); (char*)dream_p(l7) + 4 + (int64_t)(l8)*1; }))));
  l8 = ((l8) + (1));
  goto L5;
L7:;
  abort();
L8:;
  abort();
L9:;
  abort();
}

dream_ptr GpuRenderPass_draw_instanced__87(dream_ptr l0, dream_ptr l1, dream_ptr l2, int32_t l3, int32_t l4, dream_ptr l5, dream_ptr l6, dream_ptr l7, int32_t l8) {
  dream_ptr __self = dream_new_future(304, 32, 0);
  *(dream_ptr *)((char *)dream_p(__self) + 64) = (dream_ptr)l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = (dream_ptr)l1;
  memcpy((char *)dream_p(__self) + 80, dream_p(l2), 12);
  *(int64_t *)((char *)dream_p(__self) + 96) = (int64_t)l3;
  *(int64_t *)((char *)dream_p(__self) + 104) = (int64_t)l4;
  *(dream_ptr *)((char *)dream_p(__self) + 112) = (dream_ptr)l5;
  memcpy((char *)dream_p(__self) + 120, dream_p(l6), 16);
  *(dream_ptr *)((char *)dream_p(__self) + 136) = (dream_ptr)l7;
  *(int32_t *)((char *)dream_p(__self) + 144) = (int32_t)l8;
  dream_enqueue(__self);
  return __self;
}

int32_t poll_GpuRenderPass_draw_instanced__87(dream_ptr __self) {
  dream_ptr l0 = *(dream_ptr *)((char *)dream_p(__self) + 64);
  dream_ptr l1 = *(dream_ptr *)((char *)dream_p(__self) + 72);
  dream_ptr l2 = (dream_ptr)((char *)dream_p(__self) + 80);
  int64_t l3 = *(int64_t *)((char *)dream_p(__self) + 96);
  int64_t l4 = *(int64_t *)((char *)dream_p(__self) + 104);
  dream_ptr l5 = *(dream_ptr *)((char *)dream_p(__self) + 112);
  dream_ptr l6 = (dream_ptr)((char *)dream_p(__self) + 120);
  dream_ptr l7 = *(dream_ptr *)((char *)dream_p(__self) + 136);
  int32_t l8 = *(int32_t *)((char *)dream_p(__self) + 144);
  dream_ptr l9 = *(dream_ptr *)((char *)dream_p(__self) + 152);
  int64_t l10 = *(int64_t *)((char *)dream_p(__self) + 160);
  int64_t l11 = *(int64_t *)((char *)dream_p(__self) + 168);
  int64_t l12 = *(int64_t *)((char *)dream_p(__self) + 176);
  int64_t l13 = *(int64_t *)((char *)dream_p(__self) + 184);
  int64_t l14 = *(int64_t *)((char *)dream_p(__self) + 192);
  int64_t l15 = *(int64_t *)((char *)dream_p(__self) + 200);
  int64_t l16 = *(int64_t *)((char *)dream_p(__self) + 208);
  int64_t l17 = *(int64_t *)((char *)dream_p(__self) + 216);
  float l18 = *(float *)((char *)dream_p(__self) + 224);
  float l19 = *(float *)((char *)dream_p(__self) + 232);
  float l20 = *(float *)((char *)dream_p(__self) + 240);
  float l21 = *(float *)((char *)dream_p(__self) + 248);
  dream_ptr l22 = *(dream_ptr *)((char *)dream_p(__self) + 256);
  int64_t l23 = *(int64_t *)((char *)dream_p(__self) + 264);
  int32_t l24 = *(int32_t *)((char *)dream_p(__self) + 272);
  dream_ptr l25 = *(dream_ptr *)((char *)dream_p(__self) + 280);
  dream_ptr l26 = *(dream_ptr *)((char *)dream_p(__self) + 288);
  dream_ptr l27 = *(dream_ptr *)((char *)dream_p(__self) + 296);
  int32_t __st = *(int32_t *)dream_p(__self);
  switch (__st) {
    case 1: goto L1;
    case 2: goto L2;
    case 3: goto L3;
    case 4: goto L4;
    case 5: goto L5;
    case 6: goto L6;
    case 7: goto L7;
    case 8: goto L8;
    default: break;
  }
  goto L0;
L0:;
  l14 = (*(int32_t*)dream_p(l7));
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = l1;
  *(int64_t *)((char *)dream_p(__self) + 96) = l3;
  *(int64_t *)((char *)dream_p(__self) + 104) = l4;
  *(dream_ptr *)((char *)dream_p(__self) + 112) = l5;
  *(dream_ptr *)((char *)dream_p(__self) + 136) = l7;
  *(int32_t *)((char *)dream_p(__self) + 144) = l8;
  *(dream_ptr *)((char *)dream_p(__self) + 152) = l9;
  *(int64_t *)((char *)dream_p(__self) + 160) = l10;
  *(int64_t *)((char *)dream_p(__self) + 168) = l11;
  *(int64_t *)((char *)dream_p(__self) + 176) = l12;
  *(int64_t *)((char *)dream_p(__self) + 184) = l13;
  *(int64_t *)((char *)dream_p(__self) + 192) = l14;
  *(int64_t *)((char *)dream_p(__self) + 200) = l15;
  *(int64_t *)((char *)dream_p(__self) + 208) = l16;
  *(int64_t *)((char *)dream_p(__self) + 216) = l17;
  *(float *)((char *)dream_p(__self) + 224) = l18;
  *(float *)((char *)dream_p(__self) + 232) = l19;
  *(float *)((char *)dream_p(__self) + 240) = l20;
  *(float *)((char *)dream_p(__self) + 248) = l21;
  *(dream_ptr *)((char *)dream_p(__self) + 256) = l22;
  *(int64_t *)((char *)dream_p(__self) + 264) = l23;
  *(int32_t *)((char *)dream_p(__self) + 272) = l24;
  *(dream_ptr *)((char *)dream_p(__self) + 280) = l25;
  *(dream_ptr *)((char *)dream_p(__self) + 288) = l26;
  *(dream_ptr *)((char *)dream_p(__self) + 296) = l27;
  {
    static void *const __jt[] = {
      &&L3,
      &&L4,
    };
    unsigned __k = (unsigned)(l14); if (__k < 2) goto *__jt[__k]; goto L1;
  }
L1:;
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = l1;
  *(int64_t *)((char *)dream_p(__self) + 96) = l3;
  *(int64_t *)((char *)dream_p(__self) + 104) = l4;
  *(dream_ptr *)((char *)dream_p(__self) + 112) = l5;
  *(dream_ptr *)((char *)dream_p(__self) + 136) = l7;
  *(int32_t *)((char *)dream_p(__self) + 144) = l8;
  *(dream_ptr *)((char *)dream_p(__self) + 152) = l9;
  *(int64_t *)((char *)dream_p(__self) + 160) = l10;
  *(int64_t *)((char *)dream_p(__self) + 168) = l11;
  *(int64_t *)((char *)dream_p(__self) + 176) = l12;
  *(int64_t *)((char *)dream_p(__self) + 184) = l13;
  *(int64_t *)((char *)dream_p(__self) + 192) = l14;
  *(int64_t *)((char *)dream_p(__self) + 200) = l15;
  *(int64_t *)((char *)dream_p(__self) + 208) = l16;
  *(int64_t *)((char *)dream_p(__self) + 216) = l17;
  *(float *)((char *)dream_p(__self) + 224) = l18;
  *(float *)((char *)dream_p(__self) + 232) = l19;
  *(float *)((char *)dream_p(__self) + 240) = l20;
  *(float *)((char *)dream_p(__self) + 248) = l21;
  *(dream_ptr *)((char *)dream_p(__self) + 256) = l22;
  *(int64_t *)((char *)dream_p(__self) + 264) = l23;
  *(int32_t *)((char *)dream_p(__self) + 272) = l24;
  *(dream_ptr *)((char *)dream_p(__self) + 280) = l25;
  *(dream_ptr *)((char *)dream_p(__self) + 288) = l26;
  *(dream_ptr *)((char *)dream_p(__self) + 296) = l27;
  goto L2;
L2:;
  l11 = (l10);
  l12 = (l8);
  l15 = ((*(int32_t*)((char*)dream_p(l0) + 0)));
  l16 = ((*(int32_t*)((char*)dream_p(l1) + 0)));
  l17 = (GpuBuffer_Vertex_get_id(l2));
  l18 = ((*(float*)((char*)dream_p(l6) + 0)));
  l19 = ((*(float*)((char*)dream_p(l6) + 4)));
  l20 = ((*(float*)((char*)dream_p(l6) + 8)));
  l21 = ((*(float*)((char*)dream_p(l6) + 12)));
  dream_release(l22);
  l22 = (__async_gpuRenderDrawEx(l15, l16, l17, l3, l4, l5, l18, l19, l20, l21, l11, l12));
  l5 = (0);
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = l1;
  *(int64_t *)((char *)dream_p(__self) + 96) = l3;
  *(int64_t *)((char *)dream_p(__self) + 104) = l4;
  *(dream_ptr *)((char *)dream_p(__self) + 112) = l5;
  *(dream_ptr *)((char *)dream_p(__self) + 136) = l7;
  *(int32_t *)((char *)dream_p(__self) + 144) = l8;
  *(dream_ptr *)((char *)dream_p(__self) + 152) = l9;
  *(int64_t *)((char *)dream_p(__self) + 160) = l10;
  *(int64_t *)((char *)dream_p(__self) + 168) = l11;
  *(int64_t *)((char *)dream_p(__self) + 176) = l12;
  *(int64_t *)((char *)dream_p(__self) + 184) = l13;
  *(int64_t *)((char *)dream_p(__self) + 192) = l14;
  *(int64_t *)((char *)dream_p(__self) + 200) = l15;
  *(int64_t *)((char *)dream_p(__self) + 208) = l16;
  *(int64_t *)((char *)dream_p(__self) + 216) = l17;
  *(float *)((char *)dream_p(__self) + 224) = l18;
  *(float *)((char *)dream_p(__self) + 232) = l19;
  *(float *)((char *)dream_p(__self) + 240) = l20;
  *(float *)((char *)dream_p(__self) + 248) = l21;
  *(dream_ptr *)((char *)dream_p(__self) + 256) = l22;
  *(int64_t *)((char *)dream_p(__self) + 264) = l23;
  *(int32_t *)((char *)dream_p(__self) + 272) = l24;
  *(dream_ptr *)((char *)dream_p(__self) + 280) = l25;
  *(dream_ptr *)((char *)dream_p(__self) + 288) = l26;
  *(dream_ptr *)((char *)dream_p(__self) + 296) = l27;
  *(int32_t *)dream_p(__self) = 5;
  dream_await(__self, l22);
  return 0;
L3:;
  l9 = ((*(dream_ptr*)((char*)dream_p(l7) + 8)));
  l10 = ((*(int32_t*)((char*)dream_p(l9) + 0)));
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = l1;
  *(int64_t *)((char *)dream_p(__self) + 96) = l3;
  *(int64_t *)((char *)dream_p(__self) + 104) = l4;
  *(dream_ptr *)((char *)dream_p(__self) + 112) = l5;
  *(dream_ptr *)((char *)dream_p(__self) + 136) = l7;
  *(int32_t *)((char *)dream_p(__self) + 144) = l8;
  *(dream_ptr *)((char *)dream_p(__self) + 152) = l9;
  *(int64_t *)((char *)dream_p(__self) + 160) = l10;
  *(int64_t *)((char *)dream_p(__self) + 168) = l11;
  *(int64_t *)((char *)dream_p(__self) + 176) = l12;
  *(int64_t *)((char *)dream_p(__self) + 184) = l13;
  *(int64_t *)((char *)dream_p(__self) + 192) = l14;
  *(int64_t *)((char *)dream_p(__self) + 200) = l15;
  *(int64_t *)((char *)dream_p(__self) + 208) = l16;
  *(int64_t *)((char *)dream_p(__self) + 216) = l17;
  *(float *)((char *)dream_p(__self) + 224) = l18;
  *(float *)((char *)dream_p(__self) + 232) = l19;
  *(float *)((char *)dream_p(__self) + 240) = l20;
  *(float *)((char *)dream_p(__self) + 248) = l21;
  *(dream_ptr *)((char *)dream_p(__self) + 256) = l22;
  *(int64_t *)((char *)dream_p(__self) + 264) = l23;
  *(int32_t *)((char *)dream_p(__self) + 272) = l24;
  *(dream_ptr *)((char *)dream_p(__self) + 280) = l25;
  *(dream_ptr *)((char *)dream_p(__self) + 288) = l26;
  *(dream_ptr *)((char *)dream_p(__self) + 296) = l27;
  goto L2;
L4:;
  l10 = (-(1));
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = l1;
  *(int64_t *)((char *)dream_p(__self) + 96) = l3;
  *(int64_t *)((char *)dream_p(__self) + 104) = l4;
  *(dream_ptr *)((char *)dream_p(__self) + 112) = l5;
  *(dream_ptr *)((char *)dream_p(__self) + 136) = l7;
  *(int32_t *)((char *)dream_p(__self) + 144) = l8;
  *(dream_ptr *)((char *)dream_p(__self) + 152) = l9;
  *(int64_t *)((char *)dream_p(__self) + 160) = l10;
  *(int64_t *)((char *)dream_p(__self) + 168) = l11;
  *(int64_t *)((char *)dream_p(__self) + 176) = l12;
  *(int64_t *)((char *)dream_p(__self) + 184) = l13;
  *(int64_t *)((char *)dream_p(__self) + 192) = l14;
  *(int64_t *)((char *)dream_p(__self) + 200) = l15;
  *(int64_t *)((char *)dream_p(__self) + 208) = l16;
  *(int64_t *)((char *)dream_p(__self) + 216) = l17;
  *(float *)((char *)dream_p(__self) + 224) = l18;
  *(float *)((char *)dream_p(__self) + 232) = l19;
  *(float *)((char *)dream_p(__self) + 240) = l20;
  *(float *)((char *)dream_p(__self) + 248) = l21;
  *(dream_ptr *)((char *)dream_p(__self) + 256) = l22;
  *(int64_t *)((char *)dream_p(__self) + 264) = l23;
  *(int32_t *)((char *)dream_p(__self) + 272) = l24;
  *(dream_ptr *)((char *)dream_p(__self) + 280) = l25;
  *(dream_ptr *)((char *)dream_p(__self) + 288) = l26;
  *(dream_ptr *)((char *)dream_p(__self) + 296) = l27;
  goto L2;
L5:;
  {
    dream_ptr __ch = *(dream_ptr *)((char *)dream_p(__self) + 36);
    l23 = (int64_t)*(dream_ptr *)((char *)dream_p(__ch) + 8);
  }
  l13 = (l23);
  l24 = ((l13) == (0));
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = l1;
  *(int64_t *)((char *)dream_p(__self) + 96) = l3;
  *(int64_t *)((char *)dream_p(__self) + 104) = l4;
  *(dream_ptr *)((char *)dream_p(__self) + 112) = l5;
  *(dream_ptr *)((char *)dream_p(__self) + 136) = l7;
  *(int32_t *)((char *)dream_p(__self) + 144) = l8;
  *(dream_ptr *)((char *)dream_p(__self) + 152) = l9;
  *(int64_t *)((char *)dream_p(__self) + 160) = l10;
  *(int64_t *)((char *)dream_p(__self) + 168) = l11;
  *(int64_t *)((char *)dream_p(__self) + 176) = l12;
  *(int64_t *)((char *)dream_p(__self) + 184) = l13;
  *(int64_t *)((char *)dream_p(__self) + 192) = l14;
  *(int64_t *)((char *)dream_p(__self) + 200) = l15;
  *(int64_t *)((char *)dream_p(__self) + 208) = l16;
  *(int64_t *)((char *)dream_p(__self) + 216) = l17;
  *(float *)((char *)dream_p(__self) + 224) = l18;
  *(float *)((char *)dream_p(__self) + 232) = l19;
  *(float *)((char *)dream_p(__self) + 240) = l20;
  *(float *)((char *)dream_p(__self) + 248) = l21;
  *(dream_ptr *)((char *)dream_p(__self) + 256) = l22;
  *(int64_t *)((char *)dream_p(__self) + 264) = l23;
  *(int32_t *)((char *)dream_p(__self) + 272) = l24;
  *(dream_ptr *)((char *)dream_p(__self) + 280) = l25;
  *(dream_ptr *)((char *)dream_p(__self) + 288) = l26;
  *(dream_ptr *)((char *)dream_p(__self) + 296) = l27;
  if (l24) goto L6; else goto L7;
L6:;
  release_Result_bool_GpuError(l25);
  l25 = (({ dream_ptr __o = dream_malloc(16, 21); memset(dream_p(__o), 0, 16); *(int32_t*)dream_p(__o) = 0; *(uint8_t*)((char*)dream_p(__o) + 4) = (uint8_t)(1);  __o; }));
  release_GpuSurface(l0);
  release_GpuRenderPipeline(l1);
  dream_release(l5);
  release_Option_GpuTexture(l7);
  dream_release(l22);
  release_GpuError(l26);
  release_Result_bool_GpuError(l27);
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = l1;
  *(int64_t *)((char *)dream_p(__self) + 96) = l3;
  *(int64_t *)((char *)dream_p(__self) + 104) = l4;
  *(dream_ptr *)((char *)dream_p(__self) + 112) = l5;
  *(dream_ptr *)((char *)dream_p(__self) + 136) = l7;
  *(int32_t *)((char *)dream_p(__self) + 144) = l8;
  *(dream_ptr *)((char *)dream_p(__self) + 152) = l9;
  *(int64_t *)((char *)dream_p(__self) + 160) = l10;
  *(int64_t *)((char *)dream_p(__self) + 168) = l11;
  *(int64_t *)((char *)dream_p(__self) + 176) = l12;
  *(int64_t *)((char *)dream_p(__self) + 184) = l13;
  *(int64_t *)((char *)dream_p(__self) + 192) = l14;
  *(int64_t *)((char *)dream_p(__self) + 200) = l15;
  *(int64_t *)((char *)dream_p(__self) + 208) = l16;
  *(int64_t *)((char *)dream_p(__self) + 216) = l17;
  *(float *)((char *)dream_p(__self) + 224) = l18;
  *(float *)((char *)dream_p(__self) + 232) = l19;
  *(float *)((char *)dream_p(__self) + 240) = l20;
  *(float *)((char *)dream_p(__self) + 248) = l21;
  *(dream_ptr *)((char *)dream_p(__self) + 256) = l22;
  *(int64_t *)((char *)dream_p(__self) + 264) = l23;
  *(int32_t *)((char *)dream_p(__self) + 272) = l24;
  *(dream_ptr *)((char *)dream_p(__self) + 280) = l25;
  *(dream_ptr *)((char *)dream_p(__self) + 288) = l26;
  *(dream_ptr *)((char *)dream_p(__self) + 296) = l27;
  dream_async_complete(__self, (dream_ptr)l25); return 0;
L7:;
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = l1;
  *(int64_t *)((char *)dream_p(__self) + 96) = l3;
  *(int64_t *)((char *)dream_p(__self) + 104) = l4;
  *(dream_ptr *)((char *)dream_p(__self) + 112) = l5;
  *(dream_ptr *)((char *)dream_p(__self) + 136) = l7;
  *(int32_t *)((char *)dream_p(__self) + 144) = l8;
  *(dream_ptr *)((char *)dream_p(__self) + 152) = l9;
  *(int64_t *)((char *)dream_p(__self) + 160) = l10;
  *(int64_t *)((char *)dream_p(__self) + 168) = l11;
  *(int64_t *)((char *)dream_p(__self) + 176) = l12;
  *(int64_t *)((char *)dream_p(__self) + 184) = l13;
  *(int64_t *)((char *)dream_p(__self) + 192) = l14;
  *(int64_t *)((char *)dream_p(__self) + 200) = l15;
  *(int64_t *)((char *)dream_p(__self) + 208) = l16;
  *(int64_t *)((char *)dream_p(__self) + 216) = l17;
  *(float *)((char *)dream_p(__self) + 224) = l18;
  *(float *)((char *)dream_p(__self) + 232) = l19;
  *(float *)((char *)dream_p(__self) + 240) = l20;
  *(float *)((char *)dream_p(__self) + 248) = l21;
  *(dream_ptr *)((char *)dream_p(__self) + 256) = l22;
  *(int64_t *)((char *)dream_p(__self) + 264) = l23;
  *(int32_t *)((char *)dream_p(__self) + 272) = l24;
  *(dream_ptr *)((char *)dream_p(__self) + 280) = l25;
  *(dream_ptr *)((char *)dream_p(__self) + 288) = l26;
  *(dream_ptr *)((char *)dream_p(__self) + 296) = l27;
  goto L8;
L8:;
  release_GpuError(l26);
  dream_retain(__ds15);
  l26 = (GpuError_from_code(l13, __ds15));
  release_Result_bool_GpuError(l27);
  l27 = (({ dream_ptr __o = dream_malloc(16, 21); memset(dream_p(__o), 0, 16); *(int32_t*)dream_p(__o) = 1; *(dream_ptr*)((char*)dream_p(__o) + 8) = (dream_ptr)(l26); dream_retain(*(dream_ptr*)((char*)dream_p(__o) + 8));  __o; }));
  release_GpuSurface(l0);
  release_GpuRenderPipeline(l1);
  dream_release(l5);
  release_Option_GpuTexture(l7);
  dream_release(l22);
  release_Result_bool_GpuError(l25);
  release_GpuError(l26);
  *(dream_ptr *)((char *)dream_p(__self) + 64) = l0;
  *(dream_ptr *)((char *)dream_p(__self) + 72) = l1;
  *(int64_t *)((char *)dream_p(__self) + 96) = l3;
  *(int64_t *)((char *)dream_p(__self) + 104) = l4;
  *(dream_ptr *)((char *)dream_p(__self) + 112) = l5;
  *(dream_ptr *)((char *)dream_p(__self) + 136) = l7;
  *(int32_t *)((char *)dream_p(__self) + 144) = l8;
  *(dream_ptr *)((char *)dream_p(__self) + 152) = l9;
  *(int64_t *)((char *)dream_p(__self) + 160) = l10;
  *(int64_t *)((char *)dream_p(__self) + 168) = l11;
  *(int64_t *)((char *)dream_p(__self) + 176) = l12;
  *(int64_t *)((char *)dream_p(__self) + 184) = l13;
  *(int64_t *)((char *)dream_p(__self) + 192) = l14;
  *(int64_t *)((char *)dream_p(__self) + 200) = l15;
  *(int64_t *)((char *)dream_p(__self) + 208) = l16;
  *(int64_t *)((char *)dream_p(__self) + 216) = l17;
  *(float *)((char *)dream_p(__self) + 224) = l18;
  *(float *)((char *)dream_p(__self) + 232) = l19;
  *(float *)((char *)dream_p(__self) + 240) = l20;
  *(float *)((char *)dream_p(__self) + 248) = l21;
  *(dream_ptr *)((char *)dream_p(__self) + 256) = l22;
  *(int64_t *)((char *)dream_p(__self) + 264) = l23;
  *(int32_t *)((char *)dream_p(__self) + 272) = l24;
  *(dream_ptr *)((char *)dream_p(__self) + 280) = l25;
  *(dream_ptr *)((char *)dream_p(__self) + 288) = l26;
  *(dream_ptr *)((char *)dream_p(__self) + 296) = l27;
  dream_async_complete(__self, (dream_ptr)l27); return 0;
  return 0;
}

static void dream_init_ft(void) {
  dream_ft[1] = (void *)make_vert;
  dream_ft[2] = (void *)main_dream;
  dream_ft[3] = (void *)GpuRenderPass_draw_ex__87;
  dream_ft[4] = (void *)GpuError_constructor;
  dream_ft[5] = (void *)GpuError_from_code;
  dream_ft[6] = (void *)GpuVec4_of;
  dream_ft[7] = (void *)Uniforms_pack_f32;
  dream_ft[8] = (void *)GpuSurface_create;
  dream_ft[9] = (void *)GpuSurface_width;
  dream_ft[10] = (void *)GpuSurface_height;
  dream_ft[11] = (void *)GpuSurface_present;
  dream_ft[12] = (void *)GpuSurface_close_requested;
  dream_ft[13] = (void *)GpuRenderPipelineDesc_overlay;
  dream_ft[14] = (void *)GpuRenderPipeline_create_ex;
  dream_ft[15] = (void *)Gpu_try_init;
  dream_ft[16] = (void *)Gpu_frame;
  dream_ft[17] = (void *)Result_bool_GpuError_is_err;
  dream_ft[18] = (void *)Result_GpuSurface_GpuError_is_err;
  dream_ft[19] = (void *)Result_GpuSurface_GpuError_unwrap_or;
  dream_ft[20] = (void *)Result_GpuRenderPipeline_GpuError_is_err;
  dream_ft[21] = (void *)Result_GpuRenderPipeline_GpuError_unwrap_or;
  dream_ft[22] = (void *)GpuBuffer_Vertex_get_id;
  dream_ft[23] = (void *)GpuBuffer_Vertex_vertex_from;
  dream_ft[24] = (void *)GpuBuffer_Vertex_write;
  dream_ft[25] = (void *)GpuRenderPass_draw_instanced__87;
  dream_ft[26] = (void *)poll_main_dream;
  dream_ft[27] = (void *)poll_GpuRenderPass_draw_ex__87;
  dream_ft[28] = (void *)poll_GpuSurface_present;
  dream_ft[29] = (void *)poll_GpuRenderPipeline_create_ex;
  dream_ft[30] = (void *)poll_Gpu_try_init;
  dream_ft[31] = (void *)poll_Gpu_frame;
  dream_ft[32] = (void *)poll_GpuRenderPass_draw_instanced__87;
}

void *dream_ft_get(int32_t i) {
  return (i > 0 && i < 33) ? dream_ft[i] : 0;
}

static void dream_init_itables(void) {
}

dream_ptr dream_worker_invoke(int32_t fn, dream_ptr env, dream_ptr arg) {
  if (fn <= 0) return 0;
  g0 = env;
  dream_ptr result = ((dream_fn)dream_ft[fn])(arg, 0, 0, 0, 0, 0, 0, 0);
  switch (fn) {
    case 2: dream_run_loop(); return *(dream_ptr *)((char *)dream_p(result) + 8);
    case 3: dream_run_loop(); return *(dream_ptr *)((char *)dream_p(result) + 8);
    case 11: dream_run_loop(); return *(dream_ptr *)((char *)dream_p(result) + 8);
    case 14: dream_run_loop(); return *(dream_ptr *)((char *)dream_p(result) + 8);
    case 15: dream_run_loop(); return *(dream_ptr *)((char *)dream_p(result) + 8);
    case 16: dream_run_loop(); return *(dream_ptr *)((char *)dream_p(result) + 8);
    case 25: dream_run_loop(); return *(dream_ptr *)((char *)dream_p(result) + 8);
    default: break;
  }
  return result;
}

int dream_guest_entry(void) {
  dream_init_ft();
  dream_init_itables();
  dream_host_bind(dream_string_alloc, dream_array_new);
  main_dream();
  dream_run_loop();
  return 0;
}
int main(void) { return dream_guest_entry(); }
