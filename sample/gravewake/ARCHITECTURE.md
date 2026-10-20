# Architecture

`main.dream` enters `src/app.dream`, which owns window events, pointer capture,
pause/resume, input, fixed simulation stepping, quality controls and diagnostics.
Rendering never advances gameplay. Death creates a fresh game when R is pressed.

`src/game/simulation.dream` owns player combat, damage, pickups, score and abilities.
`waves.dream` owns spawn pacing, escalating budgets, safe spawn selection and
intermissions. `first_person.dream` owns movement, ADS, magazines and reloads.
`explosives.dream` owns barrel visibility, shot selection, bounded chain reactions
and cover-aware blast damage. Barrels, supply stations and world bounds are emitted
from the same authoring data as the environment.
`enemy.dream`, `pickup.dream` and `particles.dream` contain their respective state.
`hud_model.dream` produces full-resolution UI text independently of scene resolution.

The wave director has a fixed twelve-enemy active pool. Wave budgets continue to
grow to 48 while health grows each wave and speed growth is bounded. Vacated slots
wait for corpse lifetime completion. Spawn candidates are deterministic, checked
against shared collision boxes, and at least eleven meters from the player.
This is bounded steering around obstacles, not a navigation-mesh pathfinder.

`tools/procedural.py` authors the palette and viewmodels. `tools/zombie_art.py`
authors the joint hierarchy, body variants and rigid articulated animation poses.
`tools/world_art.py` authors the desert settlement, quarry, trading stalls,
connecting circuits, grounded canyon rim and collision placements. `tools/bake.py` evaluates flat-shaded triangles and animation poses.
It writes intermediates only into the repository build cache. `tools/pack.mjs`
spatially orders triangles, bounds chunks, emits the typed Dream catalogs and
packs runtime buffers. `assets/level.json` supplies collision from the same scene
authoring step. Imported art and image texture atlases are no longer in the runtime.

`src/renderer.dream` owns GPU resource lifetime, optional RT depth prepass, static chunk
culling, actor culling, quality scaling and composition. `render/raster.dream`
interpolates cached animation poses; `toon.dream` provides palette colors,
hemisphere ambient light, warm sun, material highlights, Fresnel water and fog.
`shadow_space.dream` shares the light projection between raster passes;
`shadows.dream` filters the shadow map. The sun pass renders animated geometry
once per frame into an RGBA16Float color target with a separate depth attachment. `atmosphere.dream` provides the sky and wind. `presentation.dream` adapts
game state into bounded actor and particle buffers. The renderer presents HDR scene
color through tone mapping and `anti_alias.dream` before a full-resolution HUD.
`quality.dream` owns aspect-preserving preset sizes and the Ultra supersampling
budget. Captures use the drawable dimensions too, so they include the same
downsampling as presentation rather than exporting the supersampled render target.

Raster pipelines contain no acceleration-structure binding. The optional RT
pipeline is created only on first activation; `assets.dream` builds the small
static BLAS/TLAS at that point. `hardware.dream` performs hardware ray queries.
`tracing.dream` adds static ray-query occlusion. Both modes use the shared river surface pass for planar reflections.
The shadow map remains active in RT mode for moving actors. Destructible barrels
and emissive geometry are excluded from the static acceleration structure.
Turning RT off selects the raster pipeline again. No per-frame AS rebuild is needed.

The current packed format retains a bounded offline BVH inventory and cached pose
catalogs for spatial ordering. These do not execute software ray traversal in the
render loop. Current geometry budgets are checked by `test_packed_assets.py`.
A future importer or runtime animation system should replace this format outright
if its needs differ; no compatibility layer is required for this demo.

`water.dream` combines a 960x540 planar reflection of scenery and visible actors
with a copy of opaque scene color and encoded depth. Refraction rejects foreground
samples, estimates optical thickness and applies Beer absorption and scattering.
The river's winter plane uses the same height for rendering and mirrored geometry;
ice uses a cellular fracture pattern rather than moving water normals.

`game/seasons.dream` owns season selection, a 384-event stamp queue and a 512-square
CPU compression field. `snow_imprint.dream` stamps a persistent 1024-square GPU
height field; `snow_terrain.dream` samples it in a 320x352-cell displaced surface
and computes normals from height differences. The CPU field controls movement and
eye/foot height. Both fields gradually restore fresh snow. `season_renderer.dream`
draws snowfall into a transparent layer so its opacity does not overwrite scene
depth. `render/seasonal_materials.dream` applies seasonal object coverage and color.

`tools/rig_export.py` exports rest joints, bone targets and distance constraints
alongside baked living animation poses. A bone index travels in each vertex's
unused normal.w component. `game/ragdoll.dream` advances fixed-step Verlet joints
with two substeps, six constraint solves, minimum bend spans, ground friction and world-box contacts.
`presentation.dream` uploads interpolated corpse joints, and `render/raster.dream`
rotates each rigid mesh segment from its rest bone into the simulated bone.
Ragdolls begin at the rest pose; full anatomical angular limits, self-collision and exact
transfer of the last living pose are not implemented. The same deformed geometry
appears in the main, shadow and reflection passes.

`game/grenades.dream` owns six pooled bouncing projectiles, fuses and cover-aware
blast damage. The sniper uses the shared hitscan system with its own magazine,
cooldown, scope projection and viewmodel pieces. `first_person.dream` handles its
reload state; the HUD draws the scope at full drawable resolution.

`game/sound.dream` queues bounded effect events and emits a music cue only when
wave number, living-enemy count, intermission or game state changes. The browser
launcher routes these records to `src/input/audio.js`. `audio-voices.js` constructs
bounded, automatically disconnected voices. `audio-effects.js` layers noise,
filtered pressure impulses and mechanical transients, with per-shot variation.

`audio-music.js` schedules an original eight-bar F# minor progression at 108 BPM,
using a 25 ms lookahead timer with a 120 ms audio scheduling horizon. Five buses
crossfade pads, bass, drums, arpeggios and restrained lead phrases according to
wave intensity. Intensity is capped, intermissions soften the arrangement, and
shots duck the music so cues remain audible. A shared compressor preserves
headroom. Suspending the AudioContext freezes the musical timeline; resume
continues the score without generating a backlog. M mutes the complete mix.
Native sound output remains unwired.

Browser mouse buttons use mousedown/mouseup in the shared window host. Unlike
pointerdown/pointerup, these events report every button transition inside a chord,
so holding right mouse to scope does not suppress left mouse fire. Pointer motion
and Pointer Lock still use the shared window input path.
