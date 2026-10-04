# Greet by name

Write a function that builds a greeting from a name. This example shows how a parameter enters a function and how a result comes back.

```dream
import system;

fun greet(name: string): string {
    return "Hello, " + name + "!";
}

fun main() {
    System.println(greet("Ada"));
}
```

```
Hello, Ada!
```

Or with interpolation:

```dream
fun greet(name: string): string {
    return $"Hello, {name}!";
}
```
