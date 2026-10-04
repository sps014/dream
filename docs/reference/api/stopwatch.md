# Stopwatch

**Import:** `import system;`

Read the [usage guide](../stdlib/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class Stopwatch`

A high-resolution timer for measuring elapsed time, useful for benchmarking.

```dream
public class Stopwatch
```

## `constructor`

Creates a new Stopwatch and starts measuring time immediately.

```dream
public constructor()
```

## `is_running`

True while the stopwatch is currently accumulating time.

```dream
public get is_running(): bool
```

## `start`

Starts or resumes measuring elapsed time.

```dream
public fun start(): void
```

## `stop`

Stops measuring elapsed time.

```dream
public fun stop(): void
```

## `reset`

Stops measuring elapsed time and resets the elapsed time to zero.

```dream
public fun reset(): void
```

## `restart`

Stops measuring, resets the elapsed time to zero, and starts measuring again.

```dream
public fun restart(): void
```

## `elapsed_nanos`

The total elapsed time measured by the current instance, in nanoseconds.

```dream
public get elapsed_nanos(): long
```

## `elapsed_ms`

The total elapsed time measured by the current instance, in milliseconds.

```dream
public get elapsed_ms(): long
```
