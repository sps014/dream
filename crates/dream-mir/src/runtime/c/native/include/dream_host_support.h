#ifndef DREAM_HOST_SUPPORT_H
#define DREAM_HOST_SUPPORT_H

#include "dream_rt_native.h"

/* UTF-8 buffers belong to the caller; converted Dream strings use the guest heap. */
char *dream_str_utf8(dream_ptr s);
dream_ptr dream_str_from_utf8(const char *s);
int32_t dream_host_path_is_dir(const char *path);

/* Joining sorts the owned names but leaves their cleanup to the caller. */
void dream_host_names_free(char **names, size_t n);
int dream_host_names_push(char ***names, size_t *n, size_t *cap, const char *name);
dream_ptr dream_host_names_join_lines(char **names, size_t n);

#endif
