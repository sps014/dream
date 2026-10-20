"""Bake original procedural low-poly assets with Blender into Dream GPU buffers."""
from pathlib import Path
import json
import sys
sys.dont_write_bytecode = True
sys.path.insert(0,str(Path(__file__).resolve().parent))
import bpy
import numpy as np
from asset_output import write_asset
from procedural import bake_viewmodel, part
from zombie_art import bake_character
from dog_art import bake_dog
from world_art import bake_world
ROOT=Path(__file__).resolve().parents[1]
CACHE=ROOT.parents[1]/'target/gravewake-assets'
OUT=CACHE/'intermediate'
OUT.mkdir(parents=True,exist_ok=True)
models=[]
def mesh_arrays(obj):
    evaluated = obj.evaluated_get(bpy.context.evaluated_depsgraph_get())
    mesh = evaluated.to_mesh()
    mesh.calc_loop_triangles()
    count = len(mesh.loops)
    positions = np.empty(len(mesh.vertices) * 3, dtype=np.float32)
    mesh.vertices.foreach_get('co', positions)
    loop_vertices = np.empty(count, dtype=np.int32)
    mesh.loops.foreach_get('vertex_index', loop_vertices)
    normals = np.empty(count * 3, dtype=np.float32)
    mesh.corner_normals.foreach_get('vector', normals)
    uv = np.zeros(count * 2, dtype=np.float32)
    if mesh.uv_layers.active:
        mesh.uv_layers.active.data.foreach_get('uv', uv)
    uv = uv.reshape(-1, 2)
    smooth = np.empty(len(mesh.polygons), dtype=bool)
    sizes = np.empty(len(mesh.polygons), dtype=np.int32)
    mesh.polygons.foreach_get('use_smooth', smooth)
    mesh.polygons.foreach_get('loop_total', sizes)
    groups = np.where(np.repeat(smooth, sizes), -1, np.arange(count, dtype=np.int32))
    # Animated position/normal values cannot determine the vertex inventory or its order.
    keys = np.empty(count, dtype=[('vertex', '<i4'), ('u', '<f4'), ('v', '<f4'), ('group', '<i4')])
    keys['vertex'] = loop_vertices
    keys['u'], keys['v'] = np.round(uv[:, 0], 6), np.round(uv[:, 1], 6)
    keys['group'] = groups
    _, first, inverse = np.unique(keys, return_index=True, return_inverse=True)
    world = np.asarray(obj.matrix_world, dtype=np.float32)
    normal_matrix = np.asarray(obj.matrix_world.to_3x3().inverted().transposed(), dtype=np.float32)
    points = positions.reshape(-1, 3)[loop_vertices[first]] @ world[:3, :3].T + world[:3, 3]
    normal = normals.reshape(-1, 3)[first] @ normal_matrix.T
    normal /= np.maximum(np.linalg.norm(normal, axis=1, keepdims=True), 1e-8)
    vertices = np.column_stack([points[:, 0], points[:, 2], -points[:, 1], uv[first, 0], normal[:, 0], normal[:, 2], -normal[:, 1], uv[first, 1]])
    triangle_loops = np.empty(len(mesh.loop_triangles) * 3, dtype=np.int32)
    mesh.loop_triangles.foreach_get('loops', triangle_loops)
    materials = np.zeros(len(mesh.loop_triangles), dtype=np.int32)
    if len(mesh.materials) > 1:
        mesh.loop_triangles.foreach_get('material_index', materials)
    indices = inverse[triangle_loops].astype('<u4')
    evaluated.to_mesh_clear()
    return vertices.astype('<f4'), indices, materials.tolist()

def write_model(name, frames, indices, materials, clips):
    vertices = np.stack(frames)
    write_asset(OUT / f"{name}.vertices", vertices.tobytes())
    triangles = np.column_stack([indices.reshape(-1, 3), np.asarray(materials, dtype='<u4')])
    write_asset(OUT / f"{name}.triangles", triangles.astype('<u4').tobytes())
    entry = {"name": name, "vertices": int(vertices.shape[1]), "frames": len(frames), "triangles": len(materials), "clips": clips}
    for index, previous in enumerate(models):
        if previous['name'] == name:
            models[index] = entry
            break
    else:
        models.append(entry)


def bake_objects(name, objects):
    vertices=[];indices=[];materials=[];offset=0
    for obj in objects:
        v,t,_=mesh_arrays(obj)
        vertices.append(v);indices.append(t+offset);offset+=len(v)
        materials.extend([obj['material_id']]*(len(t)//3))
    write_model(name,[np.concatenate(vertices)],np.concatenate(indices),materials,[])

if __name__=='__main__':
    bake_character(mesh_arrays,write_model)
    bake_world(ROOT,bake_objects,part)
    bake_viewmodel(mesh_arrays,write_model)
    bake_character(mesh_arrays,write_model,low=True)
    bake_dog(mesh_arrays,write_model)
    (OUT/'models.json').write_text(json.dumps(models,indent=2)+'\n')
    print('PROCEDURAL BAKE COMPLETE',flush=True)
