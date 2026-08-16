#include "dream_rt.h"

__attribute__((weak)) void dream_user_main(void) {}

int main(int argc, char **argv) {
    dream_rt_set_args(argc, argv);
    dream_rt_init();
    dream_user_main();
    return 0;
}
