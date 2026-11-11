// The opaque pass of the renderer (VP-04): `clip = P · (MV · v)` in f32, where MV is the
// camera-relative model-view matrix computed in f64 on the CPU (docs/dev/adr/0020-camera-relative-rendering.md),
// so no value here is a world coordinate. Lambert shading plus ambient, lit on both sides because
// back faces are drawn (BIM solids are not always consistently oriented).

struct Frame {
    projection: mat4x4<f32>,
    // Unit direction towards the light, in camera coordinates; w is the ambient share.
    light: vec4<f32>,
}

struct Object {
    model_view: mat4x4<f32>,
    color: vec4<f32>,
}

@group(0) @binding(0) var<uniform> frame: Frame;
@group(1) @binding(0) var<uniform> object: Object;

struct Varyings {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
}

@vertex
fn vs_main(@location(0) position: vec3<f32>, @location(1) normal: vec3<f32>) -> Varyings {
    var out: Varyings;
    out.clip = frame.projection * (object.model_view * vec4<f32>(position, 1.0));
    // The upper 3×3 of the model-view is a rotation, so it maps normals too.
    out.normal = (object.model_view * vec4<f32>(normal, 0.0)).xyz;
    return out;
}

@fragment
fn fs_main(in: Varyings) -> @location(0) vec4<f32> {
    let ambient = frame.light.w;
    let diffuse = abs(dot(normalize(in.normal), frame.light.xyz));
    return vec4<f32>(object.color.rgb * (ambient + (1.0 - ambient) * diffuse), object.color.a);
}
