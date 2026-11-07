# Subscription

**Import:** `import system.app;`

Read the [usage guide](../../library/system/app.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `Subscription`

A registered `App` hook. Keep it to remove the hook early with `cancel`; a subscription that is ignored stays registered until the program stops.

```dream
public class Subscription
```

## `cancel`

Removes the hook, so it no longer runs. For an `on_stop` cleanup, the cleanup is skipped at shutdown. Cancelling twice, or after the hook ran, does nothing.

```dream
public fun cancel(): void
```
