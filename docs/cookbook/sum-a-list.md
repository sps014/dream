# Sum a list

Add several numbers stored in a list. This example combines creating a collection, visiting its items, and updating a running total.

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
