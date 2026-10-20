"""Seeded campsite, quarry and orchard connected by open combat circuits."""
import json
import math
import random

import bpy


def bake_world(root, bake_objects, part):
    bpy.ops.wm.read_factory_settings(use_empty=True)
    rng = random.Random(44119)
    objects, boxes = [], []

    def p(c, s, col, kind='box', collision=False, material=None):
        obj = part(c, s, col, kind)
        if kind == 'rock' and c[1] < s[1] and s[1] > 1.5:
            obj.location.z = min(c[1], s[1] * .30)
        if material is not None:
            obj['material_id'] = material
        objects.append(obj)
        if collision:
            boxes.append(dict(center=list(c), half=list(s), material=40 + col, collision=True))
        return obj

    def face(points, color):
        mesh = bpy.data.meshes.new('terrain facet')
        mesh.from_pydata([(x, -z, y) for x, y, z in points], [], [list(range(len(points)))])
        obj = bpy.data.objects.new('terrain facet', mesh)
        bpy.context.collection.objects.link(obj)
        obj.data.materials.append(part_material(color))
        obj['material_id'] = 40 + color
        objects.append(obj)

    def part_material(color):
        # Reuse the authoring palette without leaving a disposable object in the bake.
        from procedural import material
        return material(color)

    def pine(x, z, size=1):
        if int(abs(x * 3 + z)) % 3 == 0:
            p((x, 1.5 * size, z), (.17 * size, 1.5 * size, .17 * size), 5, collision=True)
            for side in [-1, 1]:
                branch = p((x + side * .35 * size, 2.0 * size, z), (.65 * size, .10 * size, .12 * size), 5)
                branch.rotation_euler.y = side * .45
            return
        p((x, 1.65 * size, z), (.30 * size, 1.65 * size, .30 * size), 10, 'sphere', True)
        for side, h in [(-1, 1.7), (1, 2.2)]:
            p((x + side * .55 * size, h * size, z), (.60 * size, .19 * size, .19 * size), 10, 'sphere')
            p((x + side * .95 * size, (h + .45) * size, z), (.20 * size, .65 * size, .20 * size), 23, 'sphere')

    def cabin(x, z, color):
        p((x, 1.25, z), (2.7, 1.25, 2), 17, collision=True)
        for side in [-1, 1]:
            roof = p((x + side * 1.35, 2.95, z), (1.65, .12, 2.4), color)
            roof.rotation_euler.y = side * .43
            p((x + side * 1.65, 1.5, z - 2.025), (.48, .5, .025), 12, material=68)
        p((x, 1.0, z - 2.035), (.47, 1., .035), 5)
        for y in [.3, .65, 1., 1.35, 1.7, 2.05]:
            p((x, y, z - 2.045), (2.65, .025, .025), 5)
        p((x, .08, z - 2.7), (3.1, .08, .65), 18)
        p((x + 1.8, 3.5, z + .6), (.3, .9, .3), 3)

    # Walkable geometry stays at zero; surrounding ridges provide relief without
    # making the fixed-height controller disagree with collision.
    p((0, -.4, -2.2), (54, .4, 23.8), 0)
    p((0, -.4, 58.2), (54, .4, 31.8), 0)
    p((0, -1.4, 24), (54, .2, 2.4), 18)
    for x in [-53, 53]:
        boxes.append(dict(center=[x, 1, 32], half=[1, 4, 58], material=43, collision=True))
    for z in [-25, 89]:
        boxes.append(dict(center=[0, 1, z], half=[54, 4, 1], material=43, collision=True))
    for x in range(-52, 52, 8):
        for z in range(-24, 88, 8):
            colors = [rng.choice([0, 0, 21, 1]), rng.choice([0, 21, 0])]
            spans = [(z, min(z + 8, 21.6)), (max(z, 26.4), z + 8)] if z < 26.4 and z + 8 > 21.6 else [(z, z + 8)]
            for near, far in spans:
                if far <= near:
                    continue
                corners = [(x, .002, near), (x + 8, .002, near), (x + 8, .002, far), (x, .002, far)]
                face([corners[0], corners[3], corners[2]], colors[0])
                face([corners[0], corners[2], corners[1]], colors[1])
    for side in [-1, 1]:
        for z in range(-25, 94, 5):
            height = 3.0 + rng.random() * 2.5
            p((side * 54, height * .36, z), (4.5, height, 4.0), 3 if z % 2 else 4, 'rock')
        for x in range(-50, 55, 5):
            height = 3.0 + rng.random() * 2.0
            p((x, height * .35, -26 if side < 0 else 90), (4., height, 4.0), 3 if x % 2 else 4, 'rock')
    # Cross trails at both ends turn the outer routes into circuits rather than cul-de-sacs.
    for z in [-17, 82]:
        for x in range(-42, 44, 3):
            p((x, .021, z), (1.6, .012, 2.), 2)
    for i in range(60):
        angle = i / 60 * math.tau
        x, z = math.sin(angle) * 66, 32 + math.cos(angle) * 72
        p((x, rng.uniform(2, 6), z), (rng.uniform(6, 12), rng.uniform(5, 15), rng.uniform(6, 12)), [2, 3, 4, 18][i % 4], 'rock')
    # Winding trails meet at the river bridge, the orchard and the quarry.
    for i in range(58):
        z = -22 + i * 1.9
        if abs(z - 24) < 3.5:
            continue
        x = math.sin(z * .075) * 3.5
        p((x, .017, z), (3.2, .012, 1.05), 2)
    for side in [-1, 1]:
        for i in range(32):
            z = -10 + i * 2.7
            if abs(z - 24) < 4.0:
                continue
            x = side * (25 + math.sin(z * .065) * 10)
            p((x, .019, z), (2.5, .012, 1.5), 2)
        for i in range(20):
            x = side * i * 2
            p((x, .020, 47 + math.sin(x * .1) * 3), (1.2, .012, 2.3), 2)
    for i in range(72):
        x, z = rng.uniform(-50, 50), rng.uniform(-22, 86)
        if abs(x) < 13 or 20 < z < 28 or (x > 22 and z > 43):
            continue
        pine(x, z, rng.uniform(.8, 1.65))
    for x, z, s in [(-6, 9, 1.5), (7, 16, 1.4), (-8, 30, 1.7), (8, 38, 1.8), (-16, 19, 2), (16, 27, 2), (-33, 59, 3), (-25, 70, 3.5), (-39, 44, 2.5), (32, 75, 2)]:
        p((x, s * .5, z), (s, s * .7, s * .8), 3, 'rock', True)
        p((x - .2, s, z + .1), (s * .6, s * .22, s * .6), 4, 'rock')
    for i in range(160):
        x, z = rng.uniform(-50, 50), rng.uniform(-22, 86)
        if abs(x) < 6 or 20 < z < 28:
            continue
        for side in [-1, 1]:
            p((x + side * .12, .20, z), (.12, .26, .12), 21, 'cone')
        if i % 3 == 0:
            p((x, .4, z), (.14, .12, .14), [7, 12, 15][i % 3], 'rock')
    p((0, .025, 24), (52, .02, 2.4), 19)
    for z in [21.3, 26.7]:
        for x in range(-50, 52, 3):
            p((x, .03, z), (1.7, .055, .35), 18, 'rock')
    for z in [21.2 + i * .36 for i in range(17)]:
        p((0, .12, z), (3, .10, .14), 17)
    for x in [-2.9, 2.9]:
        for z in [21.4, 24, 26.6]:
            p((x, .6, z), (.1, .6, .1), 5, collision=True)
        p((x, 1., 24), (.07, .07, 2.7), 18, collision=True)
    cabin(-17, 4, 6)
    cabin(17, 38, 9)
    cabin(-18, 65, 6)
    # Quarry machinery and orchard make the far routes visually identifiable.
    for x, z in [(-35, 54), (-30, 63), (-39, 72)]:
        p((x, .8, z), (1.7, .8, 1.1), 7, collision=True)
        p((x, 1.8, z), (.85, .65, .9), 9)
        for dx in [-1.8, 1.8]:
            p((x + dx, .4, z), (.28, .4, 1.1), 13, 'rock')
    for x in [26, 36, 46]:
        for z in [52, 64, 76]:
            p((x, .45, z), (1.2, .45, .9), 17)
            for dx in [-1.5, 1.5]:
                p((x + dx, 1.35, z), (.09, 1.35, .09), 5, collision=True)
            p((x, 2.6, z), (1.9, .09, 1.3), 6 if int(x + z) % 2 else 9)
            for dx in [-.65, .3, .9]:
                p((x + dx, 1.05, z), (.25, .16, .23), 7, 'rock')
    for x, z in [(-12, 15), (13, 8), (-23, 34), (22, 57), (-36, 80)]:
        for i in range(5):
            p((x + i * .9, .65, z), (.07, .65, .07), 5, collision=True)
        for h in [.45, 1.0]:
            p((x + 1.8, h, z), (2, .055, .08), 17, collision=True)
    for i in range(26):
        x, z = rng.uniform(-48, 48), rng.uniform(-20, 84)
        if abs(x) < 8 or 20 < z < 29 or (x > 22 and z > 44):
            continue
        p((x, .45, z), (rng.uniform(.5, 1.2), .6, rng.uniform(.5, 1.2)), 3, 'rock', collision=True)
        p((x + .35, .13, z - .6), (.4, .22, .4), 4, 'rock')
    for i in range(100):
        x, z = rng.uniform(-48, 48), rng.uniform(-20, 84)
        if abs(x) > 5 and not 21 < z < 27:
            p((x, .15, z), (.28, .2, .28), 18, 'rock')
    # A landmark tower is visible across all three combat circuits.
    for x in [-2, 2]:
        for z in [72, 76]:
            p((x, 3.5, z), (.22, 3.5, .22), 5, collision=True)
    p((0, 6.9, 74), (2.7, .2, 2.7), 18)
    p((0, 8.2, 74), (2, 1.1, 2), 17)
    p((0, 9.4, 74), (3, .22, 3), 6)
    p((0, 8.3, 71.97), (1.5, .45, .03), 12, material=68)
    for z in range(69, 78):
        p((0, .03, z), (3, .02, .5), 2)
    # Shelter clusters and broken carts make the side circuits useful combat cover.
    for x,z in [(-11,-5),(12,12),(-25,25),(24,36),(-34,48),(14,67)]:
        p((x,.55,z),(1.5,.55,.8),17,collision=True)
        for dx in [-1.2,1.2]:
            p((x+dx,.38,z),(.15,.40,.65),13,'rock')
        for dz in [-.8,.8]:
            p((x,.9,z+dz),(1.6,.15,.10),5)
        for dx in [-.8,.5]:
            p((x+dx,1.25,z),(.4,.3,.4),18,'rock')
    for x,z in [(-12,-12),(16,-7),(-23,13),(27,21),(-13,42),(11,54),(-23,75),(22,81)]:
        p((x,1.5,z),(.20,1.5,.20),5,collision=True)
        for y,r in [(1.5,1.4),(2.3,1.1),(3.0,.8)]:
            p((x,y,z),(r,.95,r),10,'cone')
    for x,z in [(-9,2),(11,17),(-14,31),(22,42),(-27,61),(12,73),(-40,34),(38,70)]:
        for dx,dz,scale in [(0,0,1),(1.2,.4,.7),(-1.1,-.3,.65)]:
            p((x+dx,.12,z+dz),(1.8*scale,.55*scale,1.3*scale),20,'rock',material=76)
    # Explosive barrels share their authored centers with CPU hits and GPU state.
    hazards = [[-5, .6, 16], [-2, .6, 20], [-20, .6, 51], [-16, .6, 55], [28, .6, 48], [31, .6, 53]]
    for index, (x, y, z) in enumerate(hazards):
        for height, radius, col in [(.6, .42, 6), (.18, .45, 13), (1.02, .45, 13), (.61, .46, 7)]:
            p((x, height, z), (radius, .48 if height == .6 else .08, radius), col, 'sphere', material=70 + index)
        p((x, 1.17, z), (.10, .04, .10), 12, material=70 + index)
    supplies = [[-2.5, 1.65, 4], [-28, 1.65, 51], [33, 1.65, 62]]
    for x, _, z in supplies:
        p((x, .24, z), (.9, .24, .9), 4)
        p((x, .8, z), (.5, .4, .5), 7)
        p((x, 1.35, z), (.18, .18, .18), 12, 'rock', material=68)
    for x, z in [(0, 4), (-17, 1), (17, 35), (-18, 62)]:
        for i in range(7):
            a = i * math.tau / 7
            p((x + math.sin(a) * .65, .15, z + math.cos(a) * .65), (.22, .18, .22), 3, 'rock')
        for i in range(3):
            p((x + (i - 1) * .12, .32 + i * .16, z), (.16, .35 - i * .07, .16), 7, 'rock', material=68)
    for i in range(16):
        p((rng.uniform(-60, 60), 22 + rng.random() * 9, rng.uniform(-15, 95)), (5 + rng.random() * 4, 1., 2.5), 20, 'rock')
    (root / 'assets/level.json').write_text(json.dumps(dict(boxes=boxes, hazards=hazards, supplies=supplies, bounds=[-51, 51, -23, 87]), indent=2) + '\n')
    bake_objects('town', objects)
    print('PROCEDURAL WORLD', len(objects), 'objects', len(boxes), 'colliders', flush=True)
