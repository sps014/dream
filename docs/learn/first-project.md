# Build a shopping total

You have run your first program. Now combine variables, lists, loops, and functions into a small project. This program adds item prices and reports whether they fit your budget.

## Create the project

```sh
dreamer init shopping
cd shopping
```

Replace `src/main.dream` with:

```dream
import system;
import system.collections;

fun total(borrow prices: List<int>): int {
    let sum = 0;
    for (let price in prices) {
        sum = sum + price;
    }
    return sum;
}

fun main() {
    let prices: List<int> = [12, 8, 5];
    prices.push(10);
    const budget = 40;
    let cost = total(prices);
    System.println(cost);
    if cost <= budget {
        System.println("Within budget");
    } else {
        System.println("Over budget");
    }
}
```

Run `dreamer run`. You should see:

```text
35
Within budget
```

## Understand each part

`List<int>` holds whole-number prices. `push(10)` adds another price. The function receives the list through a borrowed parameter, visits its items, and returns their sum. `const budget` keeps the budget fixed. The `if` statement chooses which message to print.

The prices are whole numbers to keep this example small. For money that includes fractions, store the smallest currency unit as an integer; for example, store 125 cents rather than 1.25 dollars.

## Try a change

Set the budget to 30 and run again. The total stays 35, but the message becomes `Over budget`. Add another price and check that the total changes.

## Add a test

Create `tests/total.dream`:

```dream
import system.testing;

@test
fun four_prices(): void {
    Assert.eq(12 + 8 + 5 + 10, 35);
}
```

Run `dreamer test`. This first test checks the example's arithmetic. As your project grows, put reusable functions in their own [module](../reference/language/imports.md) and test those functions directly.

Continue with [Lists](../reference/stdlib/collections/list.md), [Functions](../reference/language/functions.md), or [project testing](../reference/tooling/testing.md).
