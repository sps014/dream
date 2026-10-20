"""Rigid joint animation baked into portable, flat-shaded vertex poses."""
import math

import numpy as np
from mathutils import Euler, Matrix, Vector


CLIPS = [('idle', 8, 2.4), ('walk', 16, 1.05), ('attack', 12, .8),
         ('death', 20, 1.25), ('hit', 8, .35)]


def smooth(value):
    value = max(0, min(1, value))
    return value * value * (3 - 2 * value)


def skeleton():
    bones = [('hips', None, (0, .88, 0)), ('spine', 'hips', (0, .23, 0)),
             ('chest', 'spine', (0, .23, .015)), ('neck', 'chest', (0, .13, .025)),
             ('head', 'neck', (0, .12, .035))]
    for side, sign in [('left', -1), ('right', 1)]:
        bones.extend([(side + '_thigh', 'hips', (sign * .17, -.02, 0)),
                      (side + '_shin', side + '_thigh', (0, -.39, .025)),
                      (side + '_foot', side + '_shin', (0, -.35, .015)),
                      (side + '_arm', 'chest', (sign * .29, -.015, .015)),
                      (side + '_forearm', side + '_arm', (sign * .10, -.285, .045)),
                      (side + '_hand', side + '_forearm', (sign * .02, -.12, .255))])
    return bones


def matrices(bones, rotations=None, translation=(0, 0, 0)):
    rotations = rotations or {}
    result = {}
    for name, parent, offset in bones:
        local = Matrix.Translation(Vector(offset)) @ Euler(rotations.get(name, (0, 0, 0)), 'XYZ').to_matrix().to_4x4()
        result[name] = result[parent] @ local if parent else Matrix.Translation(Vector(translation)) @ local
    return result


def pose(label, phase):
    cycle = math.sin(phase * math.tau)
    r = {'spine': (.10, 0, .025), 'neck': (-.09, 0, -.055), 'head': (.02, 0, .035)}
    translation = [0, 0, 0]
    if label == 'idle':
        r['chest'] = (math.sin(phase * math.tau) * .025, 0, .025)
        r['left_arm'] = (-.08 + cycle * .025, 0, -.05)
        r['right_arm'] = (.08 - cycle * .02, 0, .06)
    if label == 'walk':
        translation[1] = abs(cycle) * .025
        r['hips'] = (0, cycle * .055, cycle * .035)
        r['spine'] = (.15, -cycle * .06, -.025)
        r['head'] = (-.06, cycle * .07, -.05)
        for side, sign in [('left', -1), ('right', 1)]:
            swing = cycle * sign
            r[side + '_thigh'] = (swing * .34, 0, sign * .025)
            r[side + '_shin'] = (-max(0, swing) * .58 - .04, 0, 0)
            r[side + '_foot'] = (max(0, swing) * .16, 0, 0)
            r[side + '_arm'] = (-swing * .22 - .12, 0, sign * .055)
            r[side + '_forearm'] = (-.08 - max(0, -swing) * .15, 0, 0)
    if label == 'attack':
        windup = smooth(phase / .30)
        strike = smooth((phase - .30) / .15)
        recovery = smooth((phase - .48) / .52)
        reach = (windup * .9 - strike * 1.65) * (1 - recovery)
        r['spine'] = (-windup * .15 + strike * .46 * (1 - recovery), .08 * (1 - recovery), 0)
        r['left_arm'] = (reach - .15, -.16, -.20 * (1 - recovery))
        r['right_arm'] = (reach * .8 - .22, .12, .12 * (1 - recovery))
        r['left_forearm'] = (-strike * .38 * (1 - recovery), 0, 0)
        r['right_forearm'] = (-strike * .22 * (1 - recovery), 0, 0)
        r['head'] = (-strike * .18 * (1 - recovery), 0, -.04)
    if label == 'hit':
        kick = smooth(phase / .18) * (1 - smooth((phase - .18) / .82))
        r['spine'] = (.10 - kick * .36, kick * .16, -kick * .12)
        r['chest'] = (-kick * .18, -kick * .10, 0)
        r['head'] = (kick * .32, -kick * .22, kick * .15)
        r['left_arm'] = (kick * .24, 0, -kick * .15)
        r['right_arm'] = (kick * .42, 0, kick * .08)
        translation[2] = -kick * .045
    if label == 'death':
        buckle = smooth(phase / .30)
        fall = smooth((phase - .18) / .57)
        settle = math.sin(max(0, phase - .70) / .30 * math.pi) * .055 * (1 - smooth((phase - .70) / .30))
        translation = [-fall * .16, -buckle * .33 - fall * .20, -fall * .22]
        r['hips'] = (-fall * 1.62, fall * .12, fall * .20)
        r['spine'] = (.10 + buckle * .40 - fall * .47, 0, -fall * .18)
        r['chest'] = (-fall * .12, fall * .12, settle)
        r['neck'] = (.05 + fall * .15, 0, -fall * .15)
        r['head'] = (fall * .25 + settle, -.15 * fall, fall * .12)
        leg_fold = buckle * (1 - .8 * smooth((phase - .55) / .45))
        r['left_thigh'] = (.65 * leg_fold, 0, -.18 * fall)
        r['right_thigh'] = (.40 * leg_fold, 0, .12 * fall)
        r['left_shin'] = (-1.12 * leg_fold, 0, 0)
        r['right_shin'] = (-.83 * leg_fold, 0, 0)
        r['left_arm'] = (-.35 * buckle * (1 - fall) + .20 * fall, 0, -.55 * fall)
        r['right_arm'] = (-.45 * buckle * (1 - fall) - .12 * fall, .20 * fall, .34 * fall)
        r['left_forearm'] = (.98 * fall, 0, -.16 * fall)
        r['right_forearm'] = (1.08 * fall, 0, 0)
    return r, translation


def bake_character(mesh_arrays, write_model, low=False):
    from procedural import reset, part, limb
    reset()
    bones = skeleton()
    bind = matrices(bones)
    objects = []

    def p(center, scale, color, bone, kind='box', bevel=False):
        obj = part(center, scale, color, kind)
        if bevel and not low:
            modifier = obj.modifiers.new('Broad silhouette facets', 'BEVEL')
            modifier.width = .18
            modifier.segments = 1
        objects.append((obj, bone))
        return obj

    def segment(a, b, radius, color, bone):
        objects.append((limb(a, b, radius, color), bone))

    p((0, .87, -.015), (.245, .14, .16), 28, 'hips', 'rock')
    p((0, 1.10, 0), (.245, .24, .18), 27, 'spine', 'sphere')
    p((0, 1.31, .01), (.30, .21, .19), 27, 'chest', 'sphere')
    p((0, 1.43, .10), (.12, .045, .10), 28, 'chest')
    p((.10, 1.30, .202), (.07, .09, .012), 28, 'chest')
    p((-.13, 1.10, .179), (.085, .06, .014), 29, 'spine', 'rock')
    for x, y in [(-.19, .98), (.01, .95), (.15, .99)]:
        p((x, y, .04), (.07, .11, .14), 27, 'spine', 'rock')
    p((0, 1.49, .04), (.095, .10, .09), 25, 'neck', 'sphere')
    p((0, 1.73, .07), (.25, .285, .22), 24, 'head', 'sphere')
    p((-.01, 1.54, .155), (.18, .09, .15), 25, 'head', 'rock')
    p((.0, 1.565, .291), (.12, .055, .02), 13, 'head', 'rock')
    p((-.105, 1.68, .268), (.095, .075, .028), 25, 'head', 'rock')
    p((.10, 1.68, .267), (.09, .07, .028), 25, 'head', 'rock')
    for x, y, size in [(-.103, 1.70, .052), (.095, 1.695, .046)]:
        p((x, y, .288), (size, size * .76, .016), 30, 'head', bevel=True)
        p((x + .009, y, .307), (.018, .026, .008), 13, 'head')
        p((x, y + .070, .273), (.091, .024, .027), 25, 'head', 'rock')
    p((-.005, 1.655, .294), (.042, .056, .04), 26, 'head', 'rock')
    for x, y in [(-.10, 1.596), (-.047, 1.602), (.028, 1.592), (.09, 1.599)]:
        p((x, y, .313), (.020, .028, .009), 30, 'head')
    for sign in [-1, 1]:
        p((sign * .237, 1.71, .04), (.035, .067, .055), 25, 'head', 'rock')
    if not low:
        p((.14, 1.835, .275), (.042, .055, .008), 29, 'head', 'rock')
        p((-.07, 1.935, .095), (.12, .022, .13), 25, 'head', 'rock')
    for side, sign in [('left', -1), ('right', 1)]:
        hip = (sign * .17, .86, 0)
        knee = (sign * .17, .47, .025)
        ankle = (sign * .17, .12, .04)
        segment(hip, knee, .125, 28, side + '_thigh')
        segment(knee, ankle, .095, 28, side + '_shin')
        p((sign * .17, .45, .112), (.09, .07, .025), 25 if sign == -1 else 27, side + '_shin', 'rock')
        p((sign * .17, .085, .10), (.12, .085, .21), 13, side + '_foot', 'rock')
        shoulder = (sign * .29, 1.325, .03)
        elbow = (sign * .39, 1.04, .075)
        wrist = (sign * .41, .92, .33)
        segment(shoulder, elbow, .115, 27, side + '_arm')
        segment(elbow, wrist, .080, 24, side + '_forearm')
        p(wrist, (.085, .055, .11), 24, side + '_hand', 'rock')
        for finger in range(2 if low else 4):
            x = wrist[0] + (finger - (0.5 if low else 1.5)) * .039
            segment((x, .91, .38), (x, .865, .46), .019, 26, side + '_hand')
        segment((wrist[0] - sign * .065, .935, .34),
                (wrist[0] - sign * .095, .90, .395), .025, 25, side + '_hand')

    base, triangles, groups, offset = [], [], [], 0
    for obj, bone in objects:
        vertices, indices, _ = mesh_arrays(obj)
        vertices[:, 3] = (obj['material_id'] - 40 + .5) / 32
        vertices[:, 7] = [b[0] for b in bones].index(bone)
        base.append(vertices)
        triangles.append(indices + offset)
        groups.append((offset, offset + len(vertices), bone))
        offset += len(vertices)
    base = np.concatenate(base)
    triangles = np.concatenate(triangles)
    frames, clips = [], []
    for label, count, duration in CLIPS:
        clips.append(dict(name=label, start=len(frames), count=count, duration=duration,
                          loop=label in ['idle', 'walk']))
        for frame in range(count):
            rotations, translation = pose(label, frame / (count - 1))
            animated = matrices(bones, rotations, translation)
            vertices = base.copy()
            for start, end, bone in groups:
                transform = np.asarray(animated[bone] @ bind[bone].inverted(), dtype=np.float32)
                vertices[start:end, :3] = base[start:end, :3] @ transform[:3, :3].T + transform[:3, 3]
                vertices[start:end, 4:7] = base[start:end, 4:7] @ transform[:3, :3].T
            if label == 'idle' and frame == 0:
                from rig_export import export_rig
                targets={'hips':'spine','spine':'chest','chest':'neck','neck':'head','head':'head_tip'}
                extra=[('head',(0,.22,0),'head_tip')]
                for side in ['left','right']:
                    targets[side+'_thigh']=side+'_shin';targets[side+'_shin']=side+'_foot';targets[side+'_foot']=side+'_toe'
                    targets[side+'_arm']=side+'_forearm';targets[side+'_forearm']=side+'_hand';targets[side+'_hand']=side+'_finger'
                    extra.extend([(side+'_foot',(0,0,.20),side+'_toe'),(side+'_hand',(0,-.04,.13),side+'_finger')])
                export_rig('human',bones,animated,targets,extra,max(0,.015-vertices[:,1].min()))
            if label == 'death':
                vertices[:, 1] += .015 - vertices[:, 1].min()
            else:
                vertices[:, 1] += max(0, .015 - vertices[:, 1].min())
            frames.append(vertices)
    original = list(frames)
    frames = []
    stride = len(original)
    for variant in range(3):
        for source in original:
            vertices = source.copy()
            if variant == 1:
                vertices[:, 0] *= 1.22
                vertices[:, 1] *= 1.07
                vertices[:, 2] *= 1.12
                colors = (vertices[:, 3] * 32).astype(int)
                vertices[colors == 27, 3] = (22 + .5) / 32
                vertices[colors == 28, 3] = (5 + .5) / 32
                vertices[colors == 24, 3] = (25 + .5) / 32
                missing_eye = (base[:, 1] > 1.65) & (base[:, 0] < 0) & ((base[:, 3] * 32).astype(int) == 30)
                vertices[missing_eye, 3] = (25 + .5) / 32
            elif variant == 2:
                vertices[:, 0] *= .87
                vertices[:, 1] *= .96
                colors = (vertices[:, 3] * 32).astype(int)
                vertices[colors == 27, 3] = (16 + .5) / 32
                vertices[colors == 24, 3] = (26 + .5) / 32
                vertices[colors == 29, 3] = (25 + .5) / 32
            frames.append(vertices)
    write_model('brute_lod' if low else 'brute', frames, triangles, [7] * (len(triangles) // 3), clips)
    print('JOINTED ZOMBIE', 'LOD' if low else 'NEAR', len(triangles) // 3, 'triangles', len(frames), 'poses / 3 variants', flush=True)
