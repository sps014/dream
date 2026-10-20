"""Export rest joints for runtime articulated corpses."""
import json
from pathlib import Path

def export_rig(name,bones,rest,targets,extra,lift):
    names=[b[0] for b in bones];points=[list(rest[n].translation) for n in names]
    parents=[names.index(parent) if parent else -1 for _,parent,_ in bones]
    for parent,offset,label in extra:
        p=points[names.index(parent)]
        names.append(label);points.append([p[i]+offset[i] for i in range(3)]);parents.append(names.index(parent))
    nodes=[]
    for i,n in enumerate(names):
        child=targets.get(n)
        if child is None: child=next((names[j] for j,p in enumerate(parents) if p==i),None)
        target=names.index(child) if child else parents[i]
        nodes.append([points[i][0],points[i][1]+lift,points[i][2],max(0,target)])
    links=[[i,p,1,0] for i,p in enumerate(parents) if p>=0]
    if name=='human':
        for a,b in [('left_arm','right_arm'),('left_thigh','right_thigh'),('left_arm','hips'),('right_arm','hips'),('head','hips')]:
            links.append([names.index(a),names.index(b),.75,0])
    else:
        for a,b in [('fl','fr'),('bl','br'),('fl','bl'),('fr','br'),('body','tail'),('fl','br'),('fr','bl')]:
            links.append([names.index(a),names.index(b),.85,0])
    out=Path(__file__).resolve().parents[3]/'target/gravewake-assets/intermediate'
    (out/f'{name}.rig.json').write_text(json.dumps({'nodes':nodes,'links':links}))
