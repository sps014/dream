# Yield and lazy sequences

Use `yield value;` to produce one element and pause a function. The next request resumes
immediately after that statement. `yield break;` ends the sequence; reaching the end does too.

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

Calling `numbers` captures its arguments without running its body. Each loop creates a fresh
cursor, so the sequence can be enumerated repeatedly or by independent consumers. Reference
arguments keep their usual shared identity; enumeration does not snapshot their contents.
Value struct receivers get an independent copy for each cursor.

`continue;` skips a loop iteration. It does not produce a value. There is no `yield continue`
or `yield return` spelling.

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

An async iterator declares `AsyncIterable<T>` and may await between elements. Calling it returns
the sequence directly. Use `await for` inside an async function to consume it:

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

`Iterable<T>.iterator()` returns an `Iterator<T>`; its `next()` returns `Option<T>`.
`AsyncIterable<T>.iterator()` returns an `AsyncIterator<T>` whose `next()` returns
`Future<Option<T>>`. Both cursors have `close(): void`, which ends iteration and releases suspended
producer state. Repeated close is harmless; advancing a closed cursor returns `None`.

```dream
let cursor = numbers(3).iterator();
let first = cursor.next(); // Some(0)
cursor.close();
```

Loops close their cursors when exhausted or exited early, including `return`. Manual cursor
consumers should close cursors when finished. Cancelled consumers close their active cursor when
the Future frame is released. An async cursor permits one pending advancement
at a time. Closing it cancels its producer and wakes pending advancement with `None`.

## Rules

- Named functions and methods, including generic methods, support yield.
- Synchronous yield functions declare `Iterable<T>`; async yield functions declare `AsyncIterable<T>`.
- Yielded values must be assignable to `T`.
- Use `yield break` instead of an ordinary `return` inside a yield function.
- Yield is not supported in lambdas or property accessors.
- Iterator parameters cannot be `ref` or `borrow`; stack-only values cannot escape into suspended state.
- Suspension inside `lock` or `defer` is rejected.

Run the complete example with `dream run sample/iterators/yield.dream`.
