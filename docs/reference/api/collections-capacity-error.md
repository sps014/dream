# Capacity Error

**Import:** `import system.collections;`

## `CapacityError`

Invalid reserve request; existing contents and capacity are preserved.

```dream
public class CapacityError : Error
```

## `constructor`

Creates a capacity diagnostic without performing an allocation.

```dream
public constructor()
```

## `code`

Stable code for negative or unsupported capacity requests.

```dream
public fun code(): string
```

## `message`

Describes the accepted reserve domain.

```dream
public fun message(): string
```
