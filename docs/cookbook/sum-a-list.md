# Sum a list

Build a `List<int>`, walk it, and print the total.

```dream
import system;
import system.collections;

fun main() {
    let xs = List<int>();
    xs.push(10);
    xs.push(20);
    xs.push(30);

    let total = 0;
    for (let n in xs) {
        total = total + n;
    }
    System.println(total);   // 60
}
```

`List` needs `import system.collections;`. `for (let n in xs)` sets `n` to each element.
