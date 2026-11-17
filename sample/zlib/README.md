# Compress bytes with zlib

Run this example on 64-bit macOS or Linux with the zlib headers and library installed. The macOS SDK includes them. On Debian or Ubuntu, install `zlib1g-dev`.

```sh
cd sample/zlib
dreamer run
```

The program compresses UTF-8 text in memory, restores it, and checks it against the original. It prints the input and compressed sizes, followed by `Round trip verified`.

`cimport "zlib.h" from "z" as zlib` gets the functions and status constants from the installed header and links `libz`. The example passes each C buffer by address and supplies its length separately. zlib writes the number of output bytes through the length pointer. The example copies those bytes into a Dream array before freeing its C buffers, and frees the buffers on error too. See the [zlib utility functions](https://www.zlib.net/manual.html#Utility).
