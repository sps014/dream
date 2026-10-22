#include "dream_core.h"
#include "dream_guest_contract.h"

uint32_t dream_guest_length(dream_ptr reference);
uint32_t dream_guest_truncate(dream_ptr reference, uint32_t length);
void *dream_guest_data(dream_ptr reference, uint32_t kind);
int32_t dream_async_host_status(dream_ptr future);
void dream_sched_set_park(void (*park)(int64_t, void *), void (*wake)(void *), void *context);
void dream_capability_bind(uint32_t (*bind)(const dream_guest_context *)) {
    const dream_guest_context context = {
        DREAM_GUEST_ABI_VERSION, sizeof(dream_guest_context),
        (void *(*)(int32_t))dream_string_alloc,
        (void *(*)(int32_t, int32_t))dream_array_new_shared,
        (void *(*)(void *, uint32_t))dream_guest_data,
        (uint32_t (*)(void *))dream_guest_length,
        (uint32_t (*)(void *, uint32_t))dream_guest_truncate,
        (void (*)(void *, uint64_t))dream_complete_foreign,
        (int32_t (*)(void *))dream_async_host_status,
        (void (*)(void *))dream_retain,
        (void (*)(void *))dream_release,
        dream_sched_set_park,
    };
    if (bind(&context) != 0) { dream_platform_abort(); }
}
