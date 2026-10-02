#include "include/dream_rt_native.h"

#include <math.h>

double dream_host_abs(double v) { return fabs(v); }
double dream_host_log(double v) { return log(v); }
double dream_host_log10(double v) { return log10(v); }
double dream_host_exp(double v) { return exp(v); }
double dream_host_hypot(double x, double y) { return hypot(x, y); }
