# Greet by name

A function takes a `string` and returns a `string`.

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
