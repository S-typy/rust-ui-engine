struct Viewport { dimensions: vec2<f32>, unused: vec2<f32> }
@group(0) @binding(0) var<uniform> viewport: Viewport;
struct Fill {
    rect: vec4<f32>,
    color: vec4<f32>,
    count: u32,
    even_odd: u32,
    unused: vec2<u32>,
    segments: array<vec4<f32>, 256>,
}
@group(1) @binding(0) var<storage, read> paths: array<Fill>;
struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) @interpolate(flat) path: u32,
}
@vertex
fn vs_main(@builtin(vertex_index) vertex: u32, @builtin(instance_index) instance: u32) -> VertexOut {
    let corners = array<vec2<f32>, 6>(vec2(0.0,0.0),vec2(1.0,0.0),vec2(0.0,1.0),
        vec2(0.0,1.0),vec2(1.0,0.0),vec2(1.0,1.0));
    let rect = paths[instance].rect;
    let pixel = rect.xy + corners[vertex] * rect.zw;
    var out: VertexOut;
    out.position = vec4(pixel.x / viewport.dimensions.x * 2.0 - 1.0,
        1.0 - pixel.y / viewport.dimensions.y * 2.0, 0.0, 1.0);
    out.path = instance;
    return out;
}
fn crossing(segment: vec4<f32>, point: vec2<f32>) -> i32 {
    let delta = segment.zw - segment.xy;
    let relative = point - segment.xy;
    let cross = delta.x * relative.y - delta.y * relative.x;
    if segment.y <= point.y && segment.w > point.y && cross > 0.0 { return 1; }
    if segment.y > point.y && segment.w <= point.y && cross < 0.0 { return -1; }
    return 0;
}
fn interior(winding: i32, even_odd: u32) -> bool {
    return select(winding != 0, (abs(winding) % 2) != 0, even_odd != 0u);
}
@fragment
fn fs_main(input: VertexOut) -> @location(0) vec4<f32> {
    let count = min(paths[input.path].count, 256u);
    let rule = paths[input.path].even_odd;
    let color = paths[input.path].color;
    var distance = 3.0e18;
    var winding = 0;
    for (var i = 0u; i < count; i += 1u) {
        let s = paths[input.path].segments[i];
        winding += crossing(s, input.position.xy);
        let delta = s.zw - s.xy;
        let relative = input.position.xy - s.xy;
        let fraction = clamp(dot(relative,delta) / max(dot(delta,delta),1.0e-20),0.0,1.0);
        distance = min(distance,length(relative-fraction*delta));
    }
    var coverage = select(0.0,1.0,interior(winding,rule));
    if distance < 0.75 {
        // Evaluate the whole winding function at each subpixel. Internal or
        // cancelled edges cannot introduce seams or a second alpha blend.
        let offsets = array<vec2<f32>,4>(vec2(-0.125,-0.375),vec2(0.375,-0.125),
            vec2(-0.375,0.125),vec2(0.125,0.375));
        coverage = 0.0;
        for (var sample = 0u; sample < 4u; sample += 1u) {
            var value = 0;
            for (var i = 0u; i < count; i += 1u) {
                value += crossing(paths[input.path].segments[i],input.position.xy+offsets[sample]);
            }
            coverage += select(0.0,0.25,interior(value,rule));
        }
    }
    return vec4(color.rgb,color.a*coverage);
}
