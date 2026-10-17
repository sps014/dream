---
title: Windows and game input
summary: Open a window or use a browser canvas, then read keyboard, mouse and gamepad input once per frame.
icon: material/gamepad-variant-outline
api: reference/api/system-window.md
---

# Windows and game input

Import `system.window` to open a window and read input for games and other interactive programs.
The package only creates windows and reports input; drawing into a window is the job of a GPU
surface.

A program calls `poll` once per frame. Each call returns an `InputFrame` with:

- the events since the previous poll, in the order they happened
- the keyboard, mouse and gamepad state after those events
- the time since the previous poll

## A game loop

This loop opens a native window, moves a player with the keyboard and a gamepad stick, and stops
when the user closes the window or presses Escape:

```dream
import system;
import system.window;

fun main(): void {
    let opened = Window.open(
        WindowDesc.of("Dream game", 1280, 720)
            .resizable(true)
            .vsync(true)
    );
    if opened.is_err() {
        System.println("no window: " + opened.unwrap_err().message());
        return;
    }
    let window = opened.unwrap();
    let x = 0.0f;

    while window.is_open {
        let input = window.poll();

        for (let event in input.events) {
            switch (event) {
                WindowEvent.KeyDown(key) => {
                    if key.code == KeyCode.Escape {
                        window.close();
                    }
                    if key.code == KeyCode.F11 {
                        window.set_fullscreen(!window.is_fullscreen);
                    }
                }
                WindowEvent.Resized(size) => {
                    System.println("drawable size: " + size.width.to_string());
                }
                _ => {}
            }
        }

        let speed = 240.0f * input.delta_seconds;
        if input.keyboard.is_down(KeyCode.A) {
            x = x - speed;
        }
        if input.keyboard.is_down(KeyCode.D) {
            x = x + speed;
        }

        switch (input.gamepads.first()) {
            Some(pad) => {
                x = x + pad.axis(GamepadAxis.LeftX) * speed;
            }
            None => {}
        }
    }

    System.println(x);
}
```

`Window.open` returns a `WindowError` instead of a window when there is no display, such as on a
server, under Node, or in a CI job without a desktop session. Check `WindowError.code()` for the
reason.

A real game also draws a frame on each pass through the loop. Without that work, this loop polls as
fast as the CPU allows.

## Levels and edges

`KeyboardState`, `MouseState` and `Gamepad` answer three different questions:

- `is_down` is true for as long as a key or button is held.
- `pressed` is true only in the frame where it went down.
- `released` is true only in the frame where it came up.

Use `is_down` for movement, and `pressed` for actions that should happen once, such as jumping or
firing. A key that the user taps between two polls reports both `pressed` and `released` in the
same frame, while `is_down` is false. Key repeats from the operating system appear as `KeyDown`
events with `repeat` set, but they never count as presses.

`KeyCode` names physical keys by their position on a US keyboard, so `KeyCode.W` is the same key on
QWERTY, AZERTY and Dvorak layouts. Use `KeyEvent.key` for the character the active layout prints,
and `WindowEvent.Text` for typed text, which includes dead keys and input-method composition.

## Mouse look

For first-person controls, capture the cursor and read the movement from `MouseState.delta`:

```dream
import system;
import system.window;

fun look(window: Window, input: InputFrame, yaw: float): float {
    window.set_cursor_mode(CursorMode.Captured);
    return yaw + input.mouse.delta.x * 0.002f;
}
```

While the cursor is captured it is hidden and locked to the window, and `delta` keeps reporting
movement without limit. Browsers only grant the lock after a user gesture, so the canvas captures
the cursor on its next click when the first request is refused.

## Testing input without a display

`Window.headless` creates a window that only exists in memory. Combined with `simulate`, it lets
tests and replays drive input-handling code on machines without a display:

```dream
import system;
import system.window;

fun main(): void {
    let window = Window.headless(WindowDesc.of("test", 320, 200));
    let space = KeyEvent(KeyCode.Space, " ", KeyModifiers.None, false);

    window.simulate(WindowEvent.KeyDown(space));
    let first = window.poll();
    System.println(first.keyboard.pressed(KeyCode.Space));

    let second = window.poll();
    System.println(second.keyboard.pressed(KeyCode.Space));
    System.println(second.keyboard.is_down(KeyCode.Space));
}
```

This prints `true`, `false` and `true`: the press is an edge in the first frame only, while the
key stays down.

Simulated events also reach real windows. They arrive after the platform's own events in the
next poll, which is useful for on-screen controls.

## Windows, threads and browsers

Native windows use GLFW and belong to the program's main thread. `main` and its `async` code run
on that thread, so a normal game loop needs no extra setup. Calls from a `Task` worker do not
reach the window: `Window.open` returns a `WindowError` with code `ETHREAD`, and the other methods
do nothing.

In a browser, use `Window.canvas` with the `id` of a `<canvas>` element on the page.
`Window.open` also works there: it adds a canvas of the requested size to the page. The browser
only delivers input while your program is waiting, so a browser frame loop must `await` something
on every frame, such as presenting a GPU frame or `Time.delay(16)`.

Platform differences:

- Gamepads use the SDL controller database natively and the standard Gamepad API mapping in
  browsers. `Gamepad.rumble` currently works only in browsers that support vibration.
- `WindowEvent.FileDropped` carries a full path natively and only the file name in browsers.
- Browsers never report `WindowEvent.Moved`, and they only allow fullscreen and cursor capture
  during a user gesture.
