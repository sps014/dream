# Weak

No import is needed.

Read the [usage guide](../stdlib/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class Weak<T : class>`

A weak handle to a class instance: holding it does NOT keep the target alive. Use it to break reference cycles that static analysis cannot see — most commonly a callback stored on an object that must call back into that same object:  ```dream let wb = Weak<Engine>(engine); engine.on_tick = () => { switch (wb.get()) { Some(e) => e.tick(), None => {}, } }; ```  Capture the handle (`wb`) in closures, never the raw object — capturing the object directly recreates the cycle. When the target is freed, the slot is marked dead automatically: `is_dead()` flips to true and stays true. The registration is removed when either side dies (target free or handle drop), so nothing dangles.

```dream
public class Weak<T : class>
```

## `constructor`

```dream
public constructor(target_obj: T)
```

## `is_dead`

True once the tracked object has been freed.

```dream
public fun is_dead(): bool
```

## `get`

Atomically obtains an owning reference, or None if destruction won the race.

```dream
public fun get(): Option<T>
```

## `release`

Removes the registration early (optional — happens automatically when either side dies).

```dream
public fun release(): void
```
