# Comments as documentation

Use `//` to leave a note for readers. A short group of comments directly above a public declaration also becomes its API documentation in the editor. Dream does not use `///` or block comments.

```dream
// Number of elements currently stored.
public get length(): int {
    return this.count;
}
```

Contiguous `//` lines directly above a declaration are the docs. A blank line ends that block:

```dream
// This is attached — hover shows it.
public fun ready(): bool {
    return true;
}

// This is NOT attached — a blank line sits above the declaration.


public fun orphan(): void { }
```

The editor shows that block on hover. Private helpers and non-public `fun`s may omit comments.

Prefer one concise sentence that states behavior and edge cases (`None` when empty, case-sensitivity, …), matching the style of `List` and `int.parse` in the stdlib.
