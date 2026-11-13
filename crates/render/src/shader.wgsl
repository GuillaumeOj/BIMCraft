// The opaque pass of the renderer (VP-02, VP-04): `clip = P · (MV · v)` in f32, where MV is the
// camera-relative model-view matrix computed in f64 on the CPU (docs/dev/adr/0020-camera-relative-rendering.md),
// so no value here is a world coordinate. MV and the colour come per instance: objects sharing a
// mesh are one instanced draw (docs/dev/adr/0024-instancing-culling-and-level-of-detail.md). Lambert shading plus ambient, lit on both sides because
// back faces are drawn (BIM solids are not always consistently oriented).

struct Frame {
    projection: mat4x4<f32>,
    // Unit direction towards the light, in camera coordinates; w is the ambient share.
    light: vec4<f32>,
}

// One instance record (INSTANCE_FLOATS in world.rs): the columns of the model-view, then the colour.
struct Instance {
    @location(2) model_view_0: vec4<f32>,
    @location(3) model_view_1: vec4<f32>,
    @location(4) model_view_2: vec4<f32>,
    @location(5) model_view_3: vec4<f32>,
    @location(6) color: vec4<f32>,
}

@group(0) @binding(0) var<uniform> frame: Frame;

struct Varyings {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) color: vec4<f32>,
}

@vertex
fn vs_main(
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    instance: Instance,
) -> Varyings {
    let model_view = mat4x4<f32>(
        instance.model_view_0,
        instance.model_view_1,
        instance.model_view_2,
        instance.model_view_3,
    );
    var out: Varyings;
    out.clip = frame.projection * (model_view * vec4<f32>(position, 1.0));
    // The upper 3×3 of the model-view is a rotation, so it maps normals too.
    out.normal = (model_view * vec4<f32>(normal, 0.0)).xyz;
    out.color = instance.color;
    return out;
}

@fragment
fn fs_main(in: Varyings) -> @location(0) vec4<f32> {
    let ambient = frame.light.w;
    // Interpolated normals can cancel out between opposite vertex normals: no diffuse light then,
    // rather than the NaN of normalising a zero vector.
    let norm = length(in.normal);
    let diffuse = select(0.0, abs(dot(in.normal, frame.light.xyz)) / norm, norm > 1e-6);
    return vec4<f32>(in.color.rgb * (ambient + (1.0 - ambient) * diffuse), in.color.a);
}
