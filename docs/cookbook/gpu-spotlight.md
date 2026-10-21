---
title: A spotlight that follows the mouse
summary: Open a window and draw a glowing circle under the mouse with a fragment shader.
icon: material/cursor-default-click-outline
---

# A spotlight that follows the mouse

This program opens a window and draws a soft-edged circle wherever the mouse is. Clicking switches
the color between orange and blue, and Escape closes the window. A fragment shader decides the
color of every pixel on its own, so the whole picture is one draw call.

Building it needs the shader compiler, which you install once with
`dreamer toolchain install slang`. Run it on a desktop with a display.

```dream
import system;
import system.gpu;
import system.window;

public struct Pixel {
    @builtin("position")
    public position: GpuVec4;
}

@vertex
fun cover_vertex(): Pixel {
    let x = -1.0f;
    let y = -1.0f;
    if vertex_id() == 1 {
        x = 3.0f;
    } else if vertex_id() == 2 {
        y = 3.0f;
    }
    let pixel = Pixel();
    pixel.position = GpuVec4(x, y, 0.0f, 1.0f);
    return pixel;
}

@fragment
fun spotlight_fragment(pixel: Pixel, center: GpuVec2, radius: float, color: GpuVec4): GpuVec4 {
    let dx = pixel.position.x - center.x;
    let dy = pixel.position.y - center.y;
    let distance = GpuMath.sqrt(dx * dx + dy * dy);
    let inside = 1.0f - GpuMath.smoothstep(radius - 2.0f, radius, distance);
    return color * inside;
}

async fun run(window: Window): Result<Unit, GpuError> {
    let device = GpuDevice.create(GpuBackend.Auto).await?;
    let surface = device.create_surface(window)?;
    let pipeline = device.render_pipeline(
        GpuShaders.cover_vertex,
        GpuShaders.spotlight_fragment,
        GpuRenderPipelineDesc.defaults(surface.color_format)
    )?;

    let warm = GpuVec4(1.0f, 0.6f, 0.2f, 1.0f);
    let cool = GpuVec4(0.2f, 0.6f, 1.0f, 1.0f);
    let color = warm;
    while window.is_open {
        let input = window.poll();
        for (let event in input.events) {
            switch (event) {
                WindowEvent.Resized(size) => {
                    surface.resize(size.width, size.height)?;
                }
                _ => {}
            }
        }
        if input.keyboard.pressed(KeyCode.Escape) {
            window.close();
        }
        if !window.is_open {
            break;
        }
        if input.mouse.pressed(MouseButton.Left) {
            if color == warm {
                color = cool;
            } else {
                color = warm;
            }
        }

        let scale = window.scale_factor;
        let center = GpuVec2(input.mouse.position.x * scale, input.mouse.position.y * scale);

        let frame = surface.begin_frame(GpuVec4(0.0f, 0.0f, 0.0f, 1.0f)).await?;
        let pass = frame.pass;
        pass.set_pipeline(pipeline);
        pass.bind_value(GpuShaders.spotlight_fragment.center, center);
        pass.bind_value(GpuShaders.spotlight_fragment.radius, 40.0f * scale);
        pass.bind_value(GpuShaders.spotlight_fragment.color, color);
        pass.draw(3);
        frame.present().await?;
    }
    return Result.Ok(Unit());
}

async fun main(): void {
    let opened = Window.open(WindowDesc("Spotlight", 800, 600).resizable(true).vsync(true));
    if opened.is_err() {
        System.println("no window: " + opened.unwrap_err().message());
        return;
    }
    switch (run(opened.unwrap()).await) {
        Ok(_) => {}
        Err(e) => {
            System.println("GPU error: " + e.message());
        }
    }
}
```

Run it with `dream run main.dream`. On a machine without a display, such as a server, it prints
`no window:` and the reason instead.

How it works:

- `cover_vertex` draws one triangle large enough to cover the whole window: corners at
  (-1, -1), (3, -1) and (-1, 3) in clip space. The window shows only the part from -1 to 1 on
  each axis, so every pixel runs `spotlight_fragment` once.
- In a fragment shader, the `@builtin("position")` field holds the pixel's position in physical
  pixels, with (0, 0) at the top-left corner.
- Mouse positions are in logical pixels, which differ from physical pixels on high-DPI displays.
  Multiplying by `window.scale_factor` converts between them, so the circle stays under the
  cursor and keeps its size.
- `smoothstep` fades the last two pixels of the edge, which looks smoother than a hard cut.
- `bind_value` passes each frame's center, radius and color to the shader's value parameters.
- `begin_frame` waits for the display when vsync is on, so the loop runs once per screen
  refresh.

Things to try:

- Draw a second light with its own center and add the two results together.
- Make `radius` grow while the right mouse button is held, using `input.mouse.is_down`.

[Rendering](../library/gpu/rendering.md) covers render pipelines and window surfaces, and
[Windows and game input](../library/system/window.md) covers input.
