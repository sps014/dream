"""Original articulated infected hound, sharing the game's five clip timings."""
import math
import numpy as np
from zombie_art import CLIPS, matrices, smooth


def bake_dog(mesh_arrays, write_model):
    from procedural import reset, part, limb
    reset()
    bones = [('body', None, (0, .55, 0)), ('head', 'body', (0, .15, .49)),
             ('jaw', 'head', (0, -.09, .20)), ('tail', 'body', (0, .03, -.50))]
    for name, x, z in [('fl', -.22, .34), ('fr', .22, .34), ('bl', -.20, -.36), ('br', .20, -.36)]:
        bones += [(name, 'body', (x, -.04, z)), (name + '_shin', name, (0, -.23, -.04))]
    bind = matrices(bones)
    objects = []
    def p(c, s, color, bone, kind='rock'):
        o = part(c, s, color, kind); objects.append((o, bone)); return o
    def link(a, b, radius, color, bone):
        objects.append((limb(a, b, radius, color), bone))
    p((0, .61, -.02), (.26, .27, .59), 13, 'body', 'sphere')
    p((0, .69, .30), (.30, .29, .28), 5, 'body')
    p((0, .58, -.39), (.22, .23, .25), 13, 'body')
    for sign in [-1, 1]:
        p((sign * .25, .61, -.03), (.022, .14, .24), 29, 'body')
        for z in [-.19, -.07, .05, .17]:
            link((sign * .268, .52, z), (sign * .276, .72, z - .035), .018, 30, 'body')
    p((0, .85, .56), (.23, .23, .26), 5, 'head', 'sphere')
    p((0, .79, .80), (.16, .13, .24), 13, 'head')
    p((0, .79, .993), (.14, .085, .04), 13, 'head')
    p((0, .65, .81), (.145, .065, .23), 29, 'jaw')
    for x in [-.12, -.055, .055, .12]:
        p((x, .70, .88), (.025, .065, .025), 30, 'jaw', 'cone')
    for sign in [-1, 1]:
        ear = p((sign * .16, 1.10, .53), (.10, .20, .10), 13, 'head', 'cone')
        p((sign * .19, .88, .735), (.058, .042, .034), 29, 'head')
        p((sign * .19, .885, .762), (.024, .026, .013), 7, 'head')
    link((0, .59, -.50), (0, .48, -.88), .07, 5, 'tail')
    link((0, .48, -.88), (0, .62, -1.03), .045, 13, 'tail')
    for name, x, z in [('fl', -.22, .34), ('fr', .22, .34), ('bl', -.20, -.36), ('br', .20, -.36)]:
        link((x, .51, z), (x, .28, z - .04), .085, 5, name)
        link((x, .28, z - .04), (x, .09, z + .02), .052, 13, name + '_shin')
        p((x, .065, z + .09), (.085, .065, .14), 13, name + '_shin')
        for toe in [-1, 0, 1]:
            p((x + toe * .047, .045, z + .205), (.018, .025, .04), 30, name + '_shin')
    base=[];indices=[];groups=[];offset=0
    for o, bone in objects:
        v,t,_=mesh_arrays(o);v[:,3]=(o['material_id']-40+.5)/32;v[:,7]=[b[0] for b in bones].index(bone)
        base.append(v);indices.append(t+offset);groups.append((offset,offset+len(v),bone));offset+=len(v)
    base=np.concatenate(base);indices=np.concatenate(indices);frames=[];clips=[]
    for label,count,duration in CLIPS:
        clips.append(dict(name=label,start=len(frames),count=count,duration=duration,loop=label in ['idle','walk']))
        for frame in range(count):
            phase=frame/(count-1);cycle=math.sin(phase*math.tau);r={};move=[0,0,0]
            r['head']=(.05,cycle*.04,0);r['tail']=(0,cycle*.25,0)
            if label=='walk':
                move[1]=abs(cycle)*.065
                for name,sign in [('fl',1),('br',1),('fr',-1),('bl',-1)]:
                    r[name]=(cycle*sign*.60,0,0);r[name+'_shin']=(-max(0,cycle*sign)*.62,0,0)
                r['body']=(cycle*.05,0,0)
            if label=='attack':
                launch=smooth(phase/.30)*(1-smooth((phase-.48)/.52))
                move=[0,math.sin(phase*math.pi)*.30,launch*.38];r['head']=(-launch*.22,0,0);r['jaw']=(launch*.65,0,0)
                for name in ['fl','fr']:r[name]=(-launch*.7,0,0)
            if label=='hit':r['body']=(-math.sin(phase*math.pi)*.22,0,math.sin(phase*math.pi)*.18)
            if label=='death':
                fall=smooth(phase/.70);r['body']=(0,0,fall*1.65);r['head']=(fall*.2,0,-fall*.15);move[1]=-fall*.35
                for name in ['fl','fr','bl','br']:r[name]=(fall*.5,0,0);r[name+'_shin']=(-fall*.8,0,0)
            animated=matrices(bones,r,move);v=base.copy()
            for a,b,bone in groups:
                transform=np.asarray(animated[bone]@bind[bone].inverted(),dtype=np.float32)
                v[a:b,:3]=base[a:b,:3]@transform[:3,:3].T+transform[:3,3]
                v[a:b,4:7]=base[a:b,4:7]@transform[:3,:3].T
            if label=='idle' and frame==0:
                from rig_export import export_rig
                targets={'body':'head','head':'jaw','jaw':'nose','tail':'tail_tip'}
                extra=[('jaw',(0,0,.22),'nose'),('tail',(0,-.05,-.45),'tail_tip')]
                for leg in ['fl','fr','bl','br']:
                    targets[leg]=leg+'_shin';targets[leg+'_shin']=leg+'_paw'
                    extra.append((leg+'_shin',(0,-.21,.14),leg+'_paw'))
                export_rig('hound',bones,animated,targets,extra,0)
            if label=='death':v[:,1]+=.015-v[:,1].min()
            frames.append(v)
    write_model('hound',frames,indices,[7]*(len(indices)//3),clips)
    print('INFECTED HOUND',len(indices)//3,'triangles',len(frames),'poses',flush=True)
