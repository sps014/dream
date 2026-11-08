---
title: Yield and iterators
summary: "Produce lazy sequences one element at a time with `yield`."
icon: material/repeat
status: stable
targets: [native, web, node]
---

# Yield and lazy sequences

`yield value;` produces one element and pauses until the next request, which resumes just after that statement. `yield break;` ends the sequence, and so does reaching the end.

!!! glance "At a glance"

    ```dream
    fun positives(values: int[]): Iterable<int> {
        for (let value in values) {
            if value > 0 {
                yield value;
            }
        }
    }
    ```

    - `yield value;` produces one element and pauses the function until the next one is requested.
    - `yield break;` ends the sequence early.

```dream
import system;

fun numbers(count: int): Iterable<int> {
    for (let i = 0; i < count; i++) {
        yield i;
    }
}

fun main(): void {
    let values = numbers(3);
    for (let value in values) {
        System.println(value);
    } // 0, 1, 2
    for (let value in values) {
        System.println(value);
    } // 0, 1, 2 again
}
```

Calling `numbers` saves its arguments and does not run the body yet. Each loop gets a fresh cursor, so you can walk the sequence more than once.

Reference arguments stay shared, and the sequence does not copy their contents. A value-struct receiver is copied for each cursor.

`continue;` skips a loop iteration and does not produce a value. There is no `yield continue` or `yield return`.

```dream
fun positives(values: int[]): Iterable<int> {
    for (let value in values) {
        if value == 0 {
            yield break;
        }
        if value < 0 {
            continue;
        }
        yield value;
    }
}
```

## Asynchronous sequences

An async iterator returns `AsyncIterable<T>` and may await between elements. Consume it with `await for` inside an async function:

```dream
import system;

async fun delayed_numbers(count: int): AsyncIterable<int> {
    for (let i = 0; i < count; i++) {
        Time.sleep(100).await;
        yield i;
    }
}

async fun main(): void {
    await for (let value in delayed_numbers(3)) {
        System.println(value);
    }
}
```

Only requested elements are produced. Ordinary async functions still return `Future<T>`.

## Cursor access and cleanup

`Iterable<T>.iterator()` returns an `Iterator<T>` whose `next()` is `Option<T>`. `AsyncIterable<T>.iterator()` returns an `AsyncIterator<T>` whose `next()` is `Future<Option<T>>`.

Both cursors have `close(): void`, which ends iteration and drops the paused producer. Closing again is harmless, and a closed cursor's `next` is `None`.

| Type | Member | Returns |
|------|--------|---------|
| `Iterable<T>` | `iterator()` | A fresh `Iterator<T>` |
| `Iterator<T>` | `next()` | `Option<T>`; `None` when the sequence has ended |
| `Iterator<T>` | `close()` | — |
| `AsyncIterable<T>` | `iterator()` | A fresh `AsyncIterator<T>` |
| `AsyncIterator<T>` | `next()` | `Future<Option<T>>`; `None` when the sequence has ended |
| `AsyncIterator<T>` | `close()` | — |

All four types are available without an import.

```dream
let cursor = numbers(3).iterator();
let first = cursor.next(); // Some(0)
cursor.close();
```

Loops close their cursors when they finish or leave early, including on `return`. Close a cursor yourself when you drive it by hand.

A cancelled consumer closes its active cursor. An async cursor allows one pending `next` at a time, and closing it cancels the producer and completes that `next` with `None`.

## Statements at a glance

| Statement | Where | Effect |
|-----------|-------|--------|
| `yield value;` | Yield function | Produces one element and pauses until the next is requested |
| `yield break;` | Yield function | Ends the sequence |
| `continue;` | Loop in a yield function | Skips to the next loop iteration without producing a value |
| `for (let x in seq) { … }` | Anywhere | Consumes an `Iterable<T>` and closes its cursor when the loop exits |
| `await for (let x in seq) { … }` | `async` function | Consumes an `AsyncIterable<T>` and closes its cursor when the loop exits |

## Rules

- Named functions and methods, including generic methods, support yield.
- Synchronous yield functions declare `Iterable<T>`; async yield functions declare `AsyncIterable<T>`.
- Yielded values must be assignable to `T`.
- Use `yield break` instead of an ordinary `return` inside a yield function.
- Yield is not supported in lambdas or property accessors.
- Iterator parameters cannot be `ref` or `borrow`; stack-only values cannot escape into suspended state.
- Suspension inside `lock` or `defer` is rejected.

Run the complete example with `dream run sample/iterators/yield.dream`.
