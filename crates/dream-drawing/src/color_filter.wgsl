struct Parameters { kind:f32, factor:f32, reserved0:f32, reserved1:f32 }
@group(0) @binding(0) var source:texture_2d<f32>;
@group(0) @binding(1) var<uniform> parameters:Parameters;
@vertex fn vertex(@builtin(vertex_index) id:u32)->@builtin(position) vec4<f32> {
    let x=f32((id & 1u)*4u)-1.;
    let y=f32((id >> 1u)*4u)-1.;
    return vec4<f32>(x,y,0.,1.);
}
@fragment fn fragment(@builtin(position) position:vec4<f32>)->@location(0) vec4<f32> {
    let pixel=textureLoad(source,vec2<i32>(position.xy),0);
    if pixel.a==0. {return vec4<f32>(0.);}
    let rgb=pixel.rgb/pixel.a;
    var adjusted:vec3<f32>;
    if parameters.kind==0. {adjusted=rgb*parameters.factor;}
    else if parameters.kind==1. {adjusted=(rgb-vec3<f32>(0.5))*parameters.factor+vec3<f32>(0.5);}
    else {let luminance=dot(rgb,vec3<f32>(0.2126,0.7152,0.0722));adjusted=vec3<f32>(luminance)+(rgb-vec3<f32>(luminance))*parameters.factor;}
    return vec4<f32>(clamp(adjusted,vec3<f32>(0.),vec3<f32>(1.))*pixel.a,pixel.a);
}
