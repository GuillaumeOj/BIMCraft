// The opaque pass of the renderer (VP-02, VP-04): `clip = P · (MV · v)` in f32, where MV is the
// camera-relative model-view matrix computed in f64 on the CPU (docs/dev/adr/0020-camera-relative-rendering.md),
// so no value here is a world coordinate. MV and the colour come per instance: objects sharing a
// mesh are one instanced draw (docs/dev/adr/0024-instancing-culling-and-level-of-detail.md). Lambert shading plus ambient, lit on both sides because
// back faces are drawn (BIM solids are not always consistently oriented).
//
// The overlay pass (see overlay.rs) reuses vs_main for ghosts (fs_main, blended) and for
// translucent boxes (fs_flat), and draws lines and snap markers with vs_primitive / fs_primitive.

struct Frame {
    projection: mat4x4<f32>,
    // Unit direction towards the light, in camera coordinates; w is the ambient share.
    light: vec4<f32>,
    // The size of the target in pixels, for the overlay primitives; zw unused.
    viewport: vec4<f32>,
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

// Translucent boxes: the colour as it is, since the unit cube has no usable normals.
@fragment
fn fs_flat(in: Varyings) -> @location(0) vec4<f32> {
    return in.color;
}

// ---- Lines and snap markers: one instance per primitive, a quad built from the vertex index. ----

// One record (PRIMITIVE_FLOATS in overlay.rs). a and b are camera-space points: a line runs from a
// to b, a marker is at a. a.w is the shape (0 line, 1 square, 2 triangle, 3 short line, 4 diamond,
// 5 plus) and b.w the half size of a marker, in pixels. params: x the halo width added around the
// shape, y the layer (0 halo, 1 ink), z the stroke or line width in pixels.
struct Primitive {
    @location(0) a: vec4<f32>,
    @location(1) b: vec4<f32>,
    @location(2) color: vec4<f32>,
    @location(3) params: vec4<f32>,
}

struct PrimitiveVaryings {
    @builtin(position) clip: vec4<f32>,
    @location(0) @interpolate(flat) seg_a: vec2<f32>,
    @location(1) @interpolate(flat) seg_b: vec2<f32>,
    @location(2) @interpolate(flat) color: vec4<f32>,
    @location(3) @interpolate(flat) params: vec4<f32>,
    @location(4) @interpolate(flat) shape: f32,
    @location(5) @interpolate(flat) size: f32,
}

// A point of the camera space in pixels, from the top left, as @builtin(position) counts them.
fn to_pixels(camera: vec3<f32>) -> vec2<f32> {
    let clip = frame.projection * vec4<f32>(camera, 1.0);
    let ndc = clip.xy / clip.w;
    return vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5) * frame.viewport.xy;
}

@vertex
fn vs_primitive(@builtin(vertex_index) index: u32, primitive: Primitive) -> PrimitiveVaryings {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0),
    );
    let corner = corners[index];
    let seg_a = to_pixels(primitive.a.xyz);
    let seg_b = to_pixels(primitive.b.xyz);
    let shape = primitive.a.w;
    let stroke = primitive.params.z;
    // One pixel more than the shape reaches, for the antialiased edge.
    let reach = stroke * 0.5 + primitive.params.x + 1.0;
    var pixel: vec2<f32>;
    if shape < 0.5 {
        let along = seg_b - seg_a;
        let length_px = length(along);
        let dir = select(vec2<f32>(1.0, 0.0), along / length_px, length_px > 1e-4);
        let normal = vec2<f32>(-dir.y, dir.x);
        pixel = mix(seg_a, seg_b, (corner.x + 1.0) * 0.5) + dir * corner.x * reach + normal * corner.y * reach;
    } else {
        // The triangle reaches a quarter further than the other shapes.
        pixel = seg_a + corner * (primitive.b.w * 1.25 + reach);
    }
    var out: PrimitiveVaryings;
    out.clip = vec4<f32>(pixel.x / frame.viewport.x * 2.0 - 1.0, 1.0 - pixel.y / frame.viewport.y * 2.0, 0.0, 1.0);
    out.seg_a = seg_a;
    out.seg_b = seg_b;
    out.color = primitive.color;
    out.params = primitive.params;
    out.shape = shape;
    out.size = primitive.b.w;
    return out;
}

// The distance from p to the segment from a to b.
fn segment_distance(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
    let ab = b - a;
    let t = clamp(dot(p - a, ab) / max(dot(ab, ab), 1e-6), 0.0, 1.0);
    return length(p - a - ab * t);
}

// The signed distance from q, relative to the centre of a marker, to its shape: negative inside
// the shapes that have an inside. y points down the screen. s is the half size.
fn marker_distance(shape: f32, q: vec2<f32>, s: f32) -> f32 {
    if shape < 1.5 {
        // Square.
        return max(abs(q.x), abs(q.y)) - s;
    }
    if shape < 2.5 {
        // Triangle pointing up, after Inigo Quilez's equilateral triangle.
        let k = sqrt(3.0);
        var p = vec2<f32>(abs(q.x) - s, -q.y + s / k);
        if p.x + k * p.y > 0.0 {
            p = vec2<f32>(p.x - k * p.y, -k * p.x - p.y) * 0.5;
        }
        p.x = p.x - clamp(p.x, -2.0 * s, 0.0);
        return -length(p) * sign(p.y);
    }
    if shape < 3.5 {
        // Short line rising to the right.
        let end = vec2<f32>(s, -s) * 0.8;
        return segment_distance(q, -end, end);
    }
    if shape < 4.5 {
        // Diamond: s is the distance of its corners.
        return (abs(q.x) + abs(q.y) - s) * 0.70710678;
    }
    // Plus.
    return min(
        segment_distance(q, vec2<f32>(-s, 0.0), vec2<f32>(s, 0.0)),
        segment_distance(q, vec2<f32>(0.0, -s), vec2<f32>(0.0, s)),
    );
}

@fragment
fn fs_primitive(in: PrimitiveVaryings) -> @location(0) vec4<f32> {
    let p = in.clip.xy;
    var dist: f32;
    if in.shape < 0.5 {
        dist = segment_distance(p, in.seg_a, in.seg_b);
    } else {
        dist = marker_distance(in.shape, p - in.seg_a, in.size);
    }
    let half_stroke = in.params.z * 0.5;
    // The halo layer covers the shape, filled, and its halo; the ink layer only its stroke.
    let edge = select(abs(dist) - half_stroke, dist - half_stroke - in.params.x, in.params.y < 0.5);
    let coverage = clamp(0.5 - edge, 0.0, 1.0);
    return vec4<f32>(in.color.rgb, in.color.a * coverage);
}
