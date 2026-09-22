# Comments as documentation

Dream uses ordinary contiguous `//` comments as API documentation. There is no `///` syntax and no block-comment syntax — a short sentence above each public type, constructor, and method is the convention.

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
