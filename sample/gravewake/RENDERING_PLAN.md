# Rendering choices

The demo prioritizes responsive combat and a coherent original low-poly style.
It uses flat shaded meshes, a small authored palette, exaggerated zombie faces,
procedural articulated poses, warm sunlight and cool atmospheric depth. Photorealism, downloaded
assets, high-poly scans, material atlases and a mandatory RT renderer are retired.

Raster is the default. Frustum culling, spatial chunks, instanced static draws,
front-to-back actor sorting and an optional RT depth prepass bound rendering cost. Internal
resolution preserves aspect ratio: High uses the drawable resolution up to 4K,
Standard caps at 960x540 and Low at 640x360. Ultra renders at twice the drawable
width and height, capped at 3840x2160, then downsamples to the surface. The HUD
stays at surface resolution.

`anti_alias.dream` implements a bounded FXAA-style directional edge filter,
based on the screen-space approach described in [NVIDIA's FXAA paper](https://developer.download.nvidia.com/assets/gamedev/files/sdk/11/FXAA_WhitePaper.pdf).
Contrast detection uses perceptual brightness after tone mapping; low-contrast
pixels retain their center color. Directional taps stay within four scene pixels.
The filter operates before the HUD and final sRGB conversion. It requires no
motion vectors or history, so it does not introduce temporal trails. It does not
provide temporal reconstruction of subpixel details. Ultra's supersampling helps
thin geometry, while Standard/Low remain limited by their source resolution.

Raster sun shadows use a 2048x2048 RGBA16Float map with a depth attachment and four
manual comparison taps. Shared projection math and a receiver bias keep shadows
stable. The map includes moving enemies, barrels and foliage. Hemisphere ambient
lighting, material highlights, emissive windows, muzzle illumination and procedural
water give the scene depth without an acceleration structure.

T enables optional hardware RT on supported devices. Its purpose is additional
static ray-query occlusion. The river uses planar reflections in both modes. It does not affect enemy AI,
weapon visibility or collision. Dynamic characters retain raster shadows but have no RT reflections. Wind bends raster foliage slightly while its RT geometry remains
at rest. RT is an optional visual comparison, not a gameplay requirement.

Avoid adding rendering complexity unless a measured visual or performance problem
justifies it. Better encounters, enemy silhouettes, reactions and route choices take
priority over tessellation, path tracing or a realistic asset import pipeline.

The creek uses a 960x540 planar reflection pass on native and WebGPU, including
animated actors. Refraction samples a copy of opaque scene color and depth, rejects foreground
samples and applies depth-varying absorption. Bullet impacts add splashes and
expanding ripples; winter replaces moving water with a fractured ice surface. The playable creek remains shallow;
this is visual depth, not swimming physics. Planar reflections add one bounded
scene pass and do not require ray-query support.
