# InputFrame

**Import:** `import system.window;`

Read the [usage guide](../../library/system/window.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `InputFrame`

Everything `Window.poll` collected since the previous poll.  The states are snapshots: keeping an older frame does not change it.

```dream
public class InputFrame
```

## `events`

The events since the previous poll, in the order they happened.

```dream
public get events(): List<WindowEvent>
```

## `keyboard`

The keyboard after applying this frame's events.

```dream
public get keyboard(): KeyboardState
```

## `mouse`

The mouse after applying this frame's events.

```dream
public get mouse(): MouseState
```

## `gamepads`

The connected gamepads, sampled during this poll.

```dream
public get gamepads(): Gamepads
```

## `delta_seconds`

Seconds since the previous poll on the monotonic clock; `0.0` for the first poll. Multiply speeds by this value to move at the same rate at any frame rate.

```dream
public get delta_seconds(): float
```

## `time_seconds`

Seconds since the window was created, on the monotonic clock.

```dream
public get time_seconds(): double
```
