---
title: Rendering
summary: Draw with vertex and fragment shaders into textures and system.window windows.
icon: material/triangle-outline
api: reference/api/system-gpu.md
---

# Rendering

Rendering runs two shaders. The vertex shader places each vertex on screen, and the fragment shader
colors each covered pixel. A render pipeline pairs them with the format of the image they draw into.

## Vertex and fragment shaders

The vertex shader returns a struct. Its `@builtin("position")` field is the vertex position in clip
space, from -1 to 1 on each axis with +y pointing up. Its `@location(n)` fields are interpolated
across the triangle and passed to the fragment shader, which returns the pixel color:

```dream
import system;
import system.gpu;

public struct Varying {
    @builtin("position")
    public position: GpuVec4;

    @location(0)
    public color: GpuVec4;
}

@vertex
fun colored_vertex(@location(0) position: GpuVec2, @location(1) color: GpuVec4): Varying {
    let result = Varying();
    result.position = GpuVec4(position.x, position.y, 0.0f, 1.0f);
    result.color = color;
    return result;
}

@fragment
fun colored_fragment(input: Varying): GpuVec4 {
    return input.color;
}
```

The vertex shader's `@location` parameters are read from vertex buffers. A shader that computes its
corners from `vertex_id()` needs no vertex buffer at all; the window example below works that way.
Both shaders of one pipeline must come from the same module, and the vertex shader's `@location`
numbers must count up from 0.

## Drawing into a texture

This program draws a red triangle into a 64 by 64 texture and reads the pixels back. It uses the
shaders above:

```dream
public struct Vertex {
    public position: GpuVec2;
    public color: GpuVec4;

    public constructor(position: GpuVec2, color: GpuVec4) {
        this.position = position;
        this.color = color;
    }
}

async fun render(device: GpuDevice): Result<byte[], GpuError> {
    let size = 64;
    let target = device.texture(
        GpuTextureDesc(size, size, GpuTextureFormat.Rgba8Unorm).render_target(true)
    )?;

    let red = GpuVec4(1.0f, 0.0f, 0.0f, 1.0f);
    let vertices = device.vertex_buffer_from(
        [
            Vertex(GpuVec2(-1.0f, -1.0f), red),
            Vertex(GpuVec2(1.0f, -1.0f), red),
            Vertex(GpuVec2(0.0f, 1.0f), red)
        ]
    )?;
    let layout = GpuVertexLayout(sizeof(Vertex))
        .attribute(0, GpuVertexFormat.Float32x2, 0)
        .attribute(1, GpuVertexFormat.Float32x4, 8);
    let pipeline = device.render_pipeline(
        GpuShaders.colored_vertex,
        GpuShaders.colored_fragment,
        GpuRenderPipelineDesc.defaults(GpuTextureFormat.Rgba8Unorm).with_vertex_buffer(layout)
    )?;

    let encoder = device.command_encoder();
    let pass = encoder.begin_render_pass(
        GpuRenderPassDesc.color(target, GpuLoad.Clear(GpuVec4(0.0f, 0.0f, 0.0f, 1.0f)))
    );
    pass.set_pipeline(pipeline);
    pass.set_vertex_buffer(0, vertices);
    pass.draw(3);
    pass.end();
    device.submit(encoder.finish()).await?;

    return target.read(0).await;
}

async fun main(): void {
    switch (GpuDevice.create(GpuBackend.Auto).await) {
        Ok(device) => {
            switch (render(device).await) {
                Ok(pixels) => {
                    let centre = (32 * 64 + 32) * 4;
                    System.println("centre red: " + pixels[centre].to_string());
                    System.println("corner red: " + pixels[0].to_string());
                }
                Err(e) => {
                    System.println("GPU error: " + e.message());
                }
            }
        }
        Err(e) => {
            System.println("no GPU: " + e.message());
        }
    }
}
```

It prints `centre red: 255` and `corner red: 0`: the triangle covers the middle of the image, and
the top corners keep the black clear color.

The pieces:

- `GpuTextureDesc(...).render_target(true)` creates a texture that pipelines can draw into.
- `GpuVertexLayout` tells the pipeline where each `@location` input sits inside one `Vertex`: the
  position at byte 0 and the color at byte 8.
- `GpuRenderPipelineDesc.defaults(format)` draws triangle lists without blending or depth. Chain
  `with_topology`, `with_cull_mode`, `with_blend_mode`, `with_depth` or `with_samples` to change
  that.
- A render pass clears its target with `GpuLoad.Clear(color)`, or keeps what is there with
  `GpuLoad.Keep`.
- `target.read(0)` returns mip level 0 as tightly packed RGBA bytes, row 0 first.

## Shader parameters in a render pass

Parameters of render shaders are bound on the pass instead of being passed to a wrapper. Each
parameter has a slot on the generated class:

```dream
import system;
import system.gpu;

public struct WavePoint {
    @builtin("position")
    public position: GpuVec4;
}

@vertex
fun wave_vertex(time: float): WavePoint {
    let point = WavePoint();
    let x = GpuMath.sin(time + (float)vertex_id()) * 0.5f;
    point.position = GpuVec4(x, 0.0f, 0.0f, 1.0f);
    return point;
}

@fragment
fun wave_fragment(point: WavePoint): GpuVec4 {
    return GpuVec4(1.0f, 1.0f, 1.0f, 1.0f);
}

fun bind_time(pass: GpuRenderPass, time: float): void {
    pass.bind_value(GpuShaders.wave_vertex.time, time);
}
```

Use `bind_value` for values, `bind_buffer` for buffers, and `bind` for textures and samplers.

## Drawing into a window

A `GpuSurface` connects a device to a [`system.window`](../system/window.md) window. Each pass of the
game loop polls input, begins a frame, draws, and presents:

```dream
import system;
import system.gpu;
import system.window;

public struct SpinVarying {
    @builtin("position")
    public position: GpuVec4;

    @location(0)
    public color: GpuVec4;
}

@vertex
fun spin_vertex(angle: float): SpinVarying {
    let corner = 2.0943951f * (float)vertex_id();
    let x = GpuMath.cos(corner + angle) * 0.6f;
    let y = GpuMath.sin(corner + angle) * 0.6f;

    let result = SpinVarying();
    result.position = GpuVec4(x, y, 0.0f, 1.0f);
    result.color = GpuVec4(0.5f + 0.5f * x, 0.5f + 0.5f * y, 1.0f, 1.0f);
    return result;
}

@fragment
fun spin_fragment(input: SpinVarying): GpuVec4 {
    return input.color;
}

async fun run(window: Window): Result<bool, GpuError> {
    let device = GpuDevice.create(GpuBackend.Auto).await?;
    let surface = device.create_surface(window)?;
    let pipeline = device.render_pipeline(
        GpuShaders.spin_vertex,
        GpuShaders.spin_fragment,
        GpuRenderPipelineDesc.defaults(surface.color_format)
    )?;

    let angle = 0.0f;
    while window.is_open {
        let input = window.poll();
        for (let event in input.events) {
            switch (event) {
                WindowEvent.Resized(size) => {
                    surface.resize(size.width, size.height)?;
                }
                WindowEvent.KeyDown(key) => {
                    if key.code == KeyCode.Escape {
                        window.close();
                    }
                }
                _ => {}
            }
        }
        if !window.is_open {
            break;
        }

        angle = angle + input.delta_seconds;
        let frame = surface.begin_frame(GpuVec4(0.05f, 0.05f, 0.1f, 1.0f)).await?;
        let pass = frame.pass;
        pass.set_pipeline(pipeline);
        pass.bind_value(GpuShaders.spin_vertex.angle, angle);
        pass.draw(3);
        frame.present().await?;
    }
    return Result.Ok(true);
}

async fun main(): void {
    let opened = Window.open(
        WindowDesc.of("Spinning triangle", 800, 600).resizable(true).vsync(true)
    );
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

The triangle turns one radian per second until you press Escape or close the window.

- `create_surface` picks the window's color format; create pipelines with `surface.color_format`.
- `begin_frame` waits for the next image to draw into and clears it. With vsync on, this is where
  the loop waits for the display, so the loop runs at the monitor's refresh rate.
- `frame.pass` is the render pass for the frame. `frame.draw(pipeline, count)` is a shortcut for
  shaders without parameters, and `frame.encoder` records extra passes such as post-processing.
- Nothing appears until `present`.
- Pass the size from `WindowEvent.Resized` to `surface.resize`. It is in physical pixels, which is
  what the surface needs on high-DPI displays.

In a browser, open the window with `Window.canvas` or `Window.open` and await each frame, as the
loop above does, so the browser can deliver input between frames.
