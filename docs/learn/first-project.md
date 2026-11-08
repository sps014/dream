---
title: Build a small project
summary: "Combine variables, lists, loops, and functions into a shopping-total app."
icon: material/cart-outline
---

# Build a shopping total

Combine variables, lists, loops, and functions into a small project that adds item prices and reports whether they fit your budget.

## Create the project

```sh
dreamer init console shopping
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

`List<int>` holds the prices, `push(10)` adds one, and `total` returns their sum. `const budget` stays fixed, and the `if` chooses which message to print.

The prices are whole numbers to keep this example small. For money that includes fractions, store the smallest currency unit as an integer; for example, store 125 cents rather than 1.25 dollars.

## Try a change

Set the budget to 30 and run again: the total stays 35 and the message becomes `Over budget`. Add another price and check that the total changes.

## Add a test

Create `tests/total.dream`:

```dream
import system.testing;

@test
fun four_prices(): void {
    Assert.eq(12 + 8 + 5 + 10, 35);
}
```

Run `dreamer test` to check the example's arithmetic. As the project grows, put reusable functions in their own [module](../language/rules/imports.md) and test those functions.

Continue with [Lists](../library/collections/list.md), [Functions](../language/functions/declaring.md), or [project testing](../tooling/testing.md).
