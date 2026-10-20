"""Deterministic flat shaded art; no downloaded models or image textures."""
from pathlib import Path
import math
import random
import json
import bpy
import numpy as np
from mathutils import Vector

PALETTE = json.loads(Path(__file__).with_name('palette.json').read_text())

def reset():
    bpy.ops.wm.read_factory_settings(use_empty=True)

def material(index):
    name='palette_'+str(index)
    if name not in bpy.data.materials:
        m=bpy.data.materials.new(name)
        m.diffuse_color=(*PALETTE[index],1)
    return bpy.data.materials[name]

def part(center, scale, color, kind='box', rotation=(0,0,0)):
    # Author in the same Y-up coordinates consumed by collision and the renderer.
    if kind=='box': bpy.ops.mesh.primitive_cube_add(size=2)
    elif kind=='rock': bpy.ops.mesh.primitive_ico_sphere_add(subdivisions=1,radius=1)
    elif kind=='cone': bpy.ops.mesh.primitive_cone_add(vertices=7,radius1=1,radius2=0,depth=2)
    else: bpy.ops.mesh.primitive_uv_sphere_add(segments=8,ring_count=4,radius=1)
    o=bpy.context.object;o.location=(center[0],-center[2],center[1]);o.scale=(scale[0],scale[2],scale[1]);o.rotation_euler=rotation
    o.data.materials.append(material(color));o['material_id']=40+color
    for f in o.data.polygons:f.use_smooth=False
    return o

def limb(a,b,r,color):
    a=Vector((a[0],-a[2],a[1]));b=Vector((b[0],-b[2],b[1]));d=b-a
    bpy.ops.mesh.primitive_cone_add(vertices=7,radius1=r,radius2=r*.78,depth=d.length,location=(a+b)*.5)
    o=bpy.context.object;o.rotation_euler=d.to_track_quat('Z','Y').to_euler();o.data.materials.append(material(color));o['material_id']=40+color
    return o

def emit(objects,mesh_arrays):
    vs=[];ts=[];offset=0
    for o in objects:
        v,t,_=mesh_arrays(o);v[:,3]=(o['material_id']-40+.5)/32;v[:,7]=.5
        vs.append(v);ts.append(t+offset);offset+=len(v)
    return np.concatenate(vs),np.concatenate(ts)

def bake_viewmodel(mesh_arrays,write_model):
    reset();objects=[]
    for weapon in [0,1,2]:
        def p(c,s,col,kind='box',mat=None):
            o=part(c,s,col,kind);o['material_id']=mat if mat else (17 if weapon==0 else (20 if weapon==1 else 80));objects.append(o)
            if kind=='box':
                bevel=o.modifiers.new('Readable machined silhouette','BEVEL');bevel.width=.18;bevel.segments=1
        if weapon == 0:
            p((.28,-.35,.94),(.075,.075,.28),16)
            p((.28,-.33,1.35),(.035,.035,.23),13)
            p((.32,-.46,.50),(.065,.055,.20),13)
            p((.28,-.48,.88),(.055,.14,.08),9,mat=18)
            p((.28,-.47,.70),(.055,.11,.055),13)
            p((.28,-.25,.91),(.018,.025,.15),13)
            p((.28,-.235,.83),(.026,.025,.023),7)
            p((.28,-.285,1.50),(.012,.04,.015),13)
            p((.355,-.33,.96),(.009,.023,.06),7)
        elif weapon == 1:
            p((.28,-.35,1.16),(.035,.035,.43),13)
            p((.28,-.43,1.21),(.038,.035,.32),9)
            p((.28,-.37,.82),(.072,.075,.14),13)
            p((.32,-.44,.51),(.075,.055,.23),17)
            p((.28,-.46,.70),(.06,.10,.05),17)
            p((.28,-.41,1.17),(.085,.07,.17),17,mat=23)
            p((.28,-.29,1.52),(.013,.025,.015),7)
        else:
            p((.28,-.35,1.12),(.065,.065,.34),13)
            p((.28,-.34,1.65),(.026,.026,.28),13,mat=81)
            p((.31,-.42,.60),(.075,.065,.24),5)
            p((.28,-.48,.93),(.05,.12,.08),13,mat=84)
            p((.28,-.235,1.12),(.065,.065,.20),13,'sphere',82)
            p((.28,-.235,1.33),(.06,.06,.025),9,'sphere',82)
            p((.37,-.33,.95),(.06,.02,.02),13,mat=83)
            p((.43,-.36,.95),(.03,.03,.03),13,'sphere',83)
        def arm_segment(a,b,radius,color,mat):
            o=limb(a,b,radius,color);o['material_id']=mat;objects.append(o)
        for support in [False, True]:
            hand_mat=(15 if support else 14) if weapon==0 else ((38 if support else 35) if weapon==1 else (88 if support else 86))
            arm_mat=(16 if support else 5) if weapon==0 else ((39 if support else 36) if weapon==1 else (89 if support else 87))
            wrist=(.23,-.43,1.03) if support else (.30,-.51,.69)
            elbow=(-.24,-.65,.57) if support else (.62,-.66,.37)
            shoulder=(-.52,-.92,.09) if support else (.78,-.91,.06)
            arm_segment(shoulder,elbow,.125,16,arm_mat)
            arm_segment(elbow,wrist,.09,16,arm_mat)
            p(wrist,(.073,.065,.09),13,'sphere',hand_mat)
            for finger in range(4):
                x=wrist[0]-.058+finger*.035
                arm_segment((x,wrist[1]+.035,wrist[2]+.02),(x,wrist[1]+.07,wrist[2]+.09),.018,17,hand_mat)
                arm_segment((x,wrist[1]+.07,wrist[2]+.09),(x,wrist[1]+.025,wrist[2]+.125),.017,17,hand_mat)
            arm_segment((wrist[0]-.065,wrist[1],wrist[2]),(wrist[0]-.09,wrist[1]+.04,wrist[2]+.075),.025,17,hand_mat)
        p((.28,-.33,1.55),(.08,.08,.16),7,'rock',19 if weapon==0 else (24 if weapon==1 else 85))
    vs=[];ts=[];mats=[];offset=0
    for o in objects:
        v,t,_=mesh_arrays(o);col=int(o.data.materials[0].name.split('_')[1]);v[:,3]=(col+.5)/32;v[:,7]=.5
        vs.append(v);ts.append(t+offset);mats.extend([o['material_id']]*(len(t)//3));offset+=len(v)
    write_model('rifle',[np.concatenate(vs)],np.concatenate(ts),mats,[])
    print('PROCEDURAL VIEWMODEL',sum(len(t)//3 for t in ts),flush=True)
