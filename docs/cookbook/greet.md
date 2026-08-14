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

Change the argument, or use interpolation: `return $"Hello, {name}!";`
