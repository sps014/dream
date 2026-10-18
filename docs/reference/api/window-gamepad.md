# Gamepad

**Import:** `import system.window;`

Read the [usage guide](../../library/system/window.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `Gamepad`

A connected gamepad as of one `Window.poll`, using the standard controller layout.  Native programs map controllers with the SDL game controller database bundled in GLFW; browsers use the Gamepad API's standard mapping. `pressed` and `released` compare this poll with the previous one, so a button tapped entirely between two polls can go unnoticed.

```dream
public class Gamepad
```

## `slot`

The platform slot the gamepad occupies; it matches the slot in `WindowEvent.GamepadConnected` and stays the same while the pad is connected.

```dream
public get slot(): int
```

## `name`

The controller's name as reported by the mapping database or the browser.

```dream
public get name(): string
```

## `is_down`

True while `button` is held down.

```dream
public fun is_down(button: GamepadButton): bool
```

## `pressed`

True when `button` went down since the previous poll.

```dream
public fun pressed(button: GamepadButton): bool
```

## `released`

True when `button` was released since the previous poll.

```dream
public fun released(button: GamepadButton): bool
```

## `axis`

The current value of `axis`: `-1.0` to `1.0` for sticks, `0.0` to `1.0` for triggers.

```dream
public fun axis(axis: GamepadAxis): float
```

## `rumble`

Vibrates the controller for `seconds`. `low` drives the strong low-frequency motor and `high` the weak high-frequency motor, each from `0.0` to `1.0`. Returns false when the platform cannot vibrate this controller; native programs built on GLFW currently always return false.

```dream
public fun rumble(low: float, high: float, seconds: float): bool
```
