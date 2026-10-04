# Hello, World

Print a line to check that Dream is installed and working. This is the smallest useful program to start with.

```dream
import system;

fun main() {
    System.println("Hello, world!");
}
```

Run:

```bash
dreamer init hello && cd hello
# paste into src/main.dream
dreamer run
```

`import system;` is required for `System.println`.
