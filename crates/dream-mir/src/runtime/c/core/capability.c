#include "dream_core.h"
#include "dream_guest_contract.h"

uint32_t dream_guest_abi_version(void) { return DREAM_GUEST_ABI_VERSION; }

uint32_t dream_guest_length(dream_ptr reference) {
    return reference ? (uint32_t)dream_i32(reference)[0] : 0;
}

void *dream_guest_data(dream_ptr reference, uint32_t kind) {
    if (!reference) { return NULL; }
    if (kind == DREAM_GUEST_STRING) { return (void *)dream_str_units(reference); }
    if (kind == DREAM_GUEST_ARRAY) { return (char *)dream_p(reference) + LEN_PREFIX_SIZE; }
    return NULL;
}

uint32_t dream_guest_truncate(dream_ptr reference, uint32_t length) {
    if (!reference || length > dream_guest_length(reference)) { return DREAM_HOST_BOUNDS; }
    dream_i32(reference)[0] = (int32_t)length;
    return DREAM_HOST_OK;
}
