import { readFile, writeFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
const require = createRequire(new URL('../../../target/gravewake-assets/package.json', import.meta.url));
const { BufferGeometry, BufferAttribute } = require('three');
const { MeshBVH, SAH } = require('three-mesh-bvh');

const root = new URL('../', import.meta.url);
const input = new URL('../../target/gravewake-assets/intermediate/', root);
const output = new URL('assets/packed/', root);
const palette = JSON.parse(await readFile(new URL('tools/palette.json', root)));
const paletteSource = `import system.gpu;\n\nfun palette_encoded(index: int): GpuVec3 {\n${palette.map((c, i) => `    if index == ${i} { return GpuVec3(${c.map(v => Number(v).toFixed(4) + 'f').join(', ')}); }`).join('\n')}\n    return GpuVec3(0.3f, 0.4f, 0.3f);\n}\n\npublic fun palette(index: int): GpuVec3 {\n    return GpuMath.pow(palette_encoded(index), GpuVec3(2.2f));\n}\n`;
await writeFile(new URL('src/render/palette.dream', root), paletteSource);
const models = JSON.parse(await readFile(new URL('models.json', input)));
const vertices = [];
const triangles = [];
const nodes = [];
const catalog = [];
const poseCatalog = [];
const chunks = [];

for (const model of models) {
  const vb = await readFile(new URL(`${model.name}.vertices`, input));
  const tb = await readFile(new URL(`${model.name}.triangles`, input));
  const v = new Float32Array(vb.buffer, vb.byteOffset, vb.length / 4);
  const t = new Uint32Array(tb.buffer, tb.byteOffset, tb.length / 4);
  const baseVertex = vertices.length / 8;
  for (const value of v) vertices.push(value);
  for (let pose = 0; pose < model.frames; pose++) {
  const positions = new Float32Array(model.vertices * 3);
  const lo = new Float32Array(model.vertices * 3).fill(Infinity);
  const hi = new Float32Array(model.vertices * 3).fill(-Infinity);
  for (let i = 0; i < model.vertices; i++) {
    for (let axis = 0; axis < 3; axis++) {
      positions[i * 3 + axis] = v[(pose * model.vertices + i) * 8 + axis];
      for (let f = pose; f <= Math.min(model.frames - 1, pose + 1); f++) {
        const p = v[(f * model.vertices + i) * 8 + axis];
        lo[i * 3 + axis] = Math.min(lo[i * 3 + axis], p);
        hi[i * 3 + axis] = Math.max(hi[i * 3 + axis], p);
      }
    }
  }
  const indices = new Uint32Array(model.triangles * 3);
  for (let i = 0; i < model.triangles; i++) indices.set(t.subarray(i * 4, i * 4 + 3), i * 3);
  const geometry = new BufferGeometry();
  geometry.setAttribute('position', new BufferAttribute(positions, 3));
  geometry.setIndex(new BufferAttribute(indices, 1));
  const bvh = new MeshBVH(geometry, { strategy: SAH, maxLeafTris: 8, indirect: true });
  const baseNode = nodes.length;
  const triangleFirst = triangles.length / 4;
  const stack = [];
  bvh.traverse((depth, leaf, bounds, offset, count) => {
    const at = nodes.length;
    while (stack.length && nodes[stack.at(-1)].depth >= depth) nodes[stack.pop()].escape = at;
    const node = { depth, lo: [Infinity, Infinity, Infinity], hi: [-Infinity, -Infinity, -Infinity], escape: -1, first: 0, count: 0 };
    nodes.push(node);
    stack.push(at);
    if (leaf) {
      node.first = triangles.length / 4;
      if (count > 64) throw new Error('BVH leaf exceeds shader bound');
      node.count = count;
      for (let i = offset; i < offset + count; i++) {
        const original = bvh.resolveTriangleIndex(i);
        triangles.push(...t.subarray(original * 4, original * 4 + 4));
        for (let corner = 0; corner < 3; corner++) {
          const id = t[original * 4 + corner];
          for (let axis = 0; axis < 3; axis++) {
            node.lo[axis] = Math.min(node.lo[axis], lo[id * 3 + axis] - 0.002);
            node.hi[axis] = Math.max(node.hi[axis], hi[id * 3 + axis] + 0.002);
          }
        }
      }
    }
  });
  while (stack.length) nodes[stack.pop()].escape = nodes.length;
  for (let i = nodes.length - 1; i >= baseNode; i--) {
    const node = nodes[i];
    if (!node.count) {
      const left = nodes[i + 1];
      const right = nodes[left.escape];
      for (let axis = 0; axis < 3; axis++) {
        node.lo[axis] = Math.min(left.lo[axis], right.lo[axis]);
        node.hi[axis] = Math.max(left.hi[axis], right.hi[axis]);
      }
    }
  }
  const entry = { ...model, vertexBase: baseVertex, root: baseNode, end: nodes.length, triangleFirst, bounds: [...nodes[baseNode].lo, ...nodes[baseNode].hi] };
  if (model.name === 'town') {
    for (let first = triangleFirst; first < triangles.length / 4; first += 8192) {
      const count = Math.min(8192, triangles.length / 4 - first);
      const min = [Infinity, Infinity, Infinity], max = [-Infinity, -Infinity, -Infinity];
      for (let tri = first; tri < first + count; tri++) for (let corner = 0; corner < 3; corner++) {
        const id = triangles[tri * 4 + corner];
        for (let axis = 0; axis < 3; axis++) {
          min[axis] = Math.min(min[axis], positions[id * 3 + axis]);
          max[axis] = Math.max(max[axis], positions[id * 3 + axis]);
        }
      }
      const center = min.map((value, axis) => (value + max[axis]) * .5);
      const radius = Math.hypot(...max.map((value, axis) => value - center[axis])) + .4;
      chunks.push([...center, radius], [first - triangleFirst, count, 0, 0]);
    }
  }
  if (pose === 0 && model.name !== 'brute_lod') catalog.push(entry);
  if (model.frames > 1) poseCatalog.push(entry);
  geometry.dispose();
  }
  console.log(`${model.name}: ${model.triangles} triangles, ${model.frames} poses, ${nodes.length} total BVH nodes`);
}

catalog.push(...poseCatalog);
const nodeOffset = triangles.length / 4;
const catalogOffset = nodeOffset + nodes.length * 2;
const acceleration = new Float32Array((catalogOffset + catalog.length * 3) * 4);
acceleration.set(triangles);
nodes.forEach((node, i) => acceleration.set([...node.lo, node.count ? -node.count : node.escape, ...node.hi, node.first], (nodeOffset + i * 2) * 4));
catalog.forEach((m, i) => acceleration.set([m.vertexBase, m.vertices, m.root, m.end, m.triangleFirst, m.triangles, ...m.bounds], (catalogOffset + i * 3) * 4));
if (catalogOffset + catalog.length * 3 >= 16777216) throw new Error('Packed scene indexes exceed exact float32 integer range');
await writeFile(new URL('vertices.bin', output), Buffer.from(new Float32Array(vertices).buffer));
await writeFile(new URL('acceleration.bin', output), Buffer.from(acceleration.buffer));
await writeFile(new URL('manifest.json', output), JSON.stringify({ version: 6, procedural: true, textures: {}, chunks, layout: { nodeOffset, catalogOffset, nodeCount: nodes.length, triangleCount: triangles.length / 4 }, models: catalog, files: ['vertices.bin', 'acceleration.bin'] }, null, 2) + '\n');
const dogIndex = catalog.findIndex(m => m.name === 'hound');
const distantIndex = catalog.findIndex((m, i) => i >= 4 && m.name === 'brute_lod');
const source = `import system.gpu;\n\npublic fun scene_layout(): GpuVec4 { return GpuVec4(${nodeOffset}.0f, ${catalogOffset}.0f, 0.0f, 0.0f); }\n\npublic fun model_triangle_count(index: int): int {\n if index == 1 { return ${catalog[1].triangles}; }\n if index == 2 { return ${catalog[2].triangles}; }\n if index == ${dogIndex} { return ${catalog[dogIndex].triangles}; }\n if index >= ${distantIndex} && index < ${distantIndex + models.find(m => m.name === 'brute_lod').frames} { return ${models.find(m => m.name === 'brute_lod').triangles}; }\n return ${catalog[0].triangles}; }\n\npublic fun enemy_model(kind: int, distant: bool = false): int {\n if kind == 2 { return ${dogIndex}; }\n if distant { return ${distantIndex}; }\n return 0; }\n\npublic fun animation_pose_count(): int { return ${catalog[0].frames}; }\n\npublic fun animation_clip(index: int): GpuVec4 {\n let clips = [${catalog[0].clips.map(c => `GpuVec4(${c.start}.0f, ${c.count}.0f, ${c.duration}f, ${c.loop ? '1.0f' : '0.0f'})`).join(',\n')}]; return clips[index]; }\n`;
const chunksSource = `\npublic fun static_chunks(): GpuVec4[] {\n return [${chunks.map(chunk => `GpuVec4(${chunk.map(value => Number(value).toFixed(5) + 'f').join(', ')})`).join(',\n')}];\n}\n`;
const clipStart = source.indexOf('public fun animation_clip');
await writeFile(new URL('src/render/catalog.dream', root), source.slice(0, clipStart) + chunksSource);
await writeFile(new URL('src/game/animation.dream', root), 'import system.gpu;\n\n' + source.slice(clipStart) + `\npublic fun animation_variant_stride(): int { return ${catalog[0].clips.reduce((n,c) => n+c.count, 0)}; }\npublic fun corpse_lifetime(): float { return 8.0f; }\n`);
const level = JSON.parse(await readFile(new URL('assets/level.json', root)));
const number = value => `${Number(value).toFixed(5)}f`;
const collision = `import system;\nimport system.gpu;\n\npublic struct WorldBox {\n    public lo: GpuVec4;\n    public hi: GpuVec4;\n    public constructor(x: float, y: float, z: float, sx: float, sy: float, sz: float, material: float) {\n        this.lo = GpuVec4(x - sx, y - sy, z - sz, material);\n        this.hi = GpuVec4(x + sx, y + sy, z + sz, 0.0f);\n    }\n}\n\npublic fun make_level(): WorldBox[] {\n    return [${level.boxes.filter(box => box.collision).map(box => `WorldBox(${[...box.center, ...box.half, box.material].map(number).join(', ')})`).join(',\n        ')}];\n}\n`;
const worldData = `\npublic fun world_bounds(): GpuVec4 { return GpuVec4(${level.bounds.map(number).join(', ')}); }\n\npublic fun hazard_positions(): GpuVec3[] { return [${level.hazards.map(p => `GpuVec3(${p.map(number).join(', ')})`).join(', ')}]; }\n`;
await writeFile(new URL('src/game/level_geometry.dream', root), collision + worldData + `\npublic fun supply_positions(): GpuVec3[] { return [${level.supplies.map(p => `GpuVec3(${p.map(number).join(', ')})`).join(', ')}]; }\n`);
console.log(`Packed ${(vertices.length * 4 / 1048576).toFixed(1)} MiB of vertices`);

const rigs = await Promise.all(['human','hound'].map(async name => JSON.parse(await readFile(new URL(`${name}.rig.json`, input)))));
const vec = p => `GpuVec4(${p.map(n => Number(n).toFixed(6)+'f').join(', ')})`;
const rigSource = 'import system.gpu;\n\n'+['nodes','links'].map(field => `public fun rig_${field}(kind: int): GpuVec4[] {\n if kind == 2 { return [${rigs[1][field].map(vec).join(',\n')}]; }\n return [${rigs[0][field].map(vec).join(',\n')}];\n}\n`).join('\n');
await writeFile(new URL('src/game/rigs.dream',root),rigSource);
