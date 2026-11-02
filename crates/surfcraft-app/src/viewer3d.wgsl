struct Uniforms {
    view_proj: mat4x4<f32>,
    eye: vec4<f32>,
    light: vec4<f32>,
    base: vec4<f32>,
    // x: mode (0 shaded, 1 zebra), y: zebra stripe frequency
    params: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;

struct VertexIn {
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    // 0 board, 1 fin
    @location(2) kind: f32,
};

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) kind: f32,
};

@vertex
fn vs_main(v: VertexIn) -> VertexOut {
    var o: VertexOut;
    o.clip = u.view_proj * vec4<f32>(v.pos, 1.0);
    o.world = v.pos;
    o.normal = v.normal;
    o.kind = v.kind;
    return o;
}

@fragment
fn fs_main(i: VertexOut, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    var n = normalize(i.normal);
    if (!front) {
        n = -n;
    }
    let view_dir = normalize(u.eye.xyz - i.world);
    let l = normalize(u.light.xyz);
    let diffuse = max(dot(n, l), 0.0);
    let fill = max(dot(n, normalize(vec3<f32>(-l.x, -l.y, 0.4))), 0.0) * 0.3;
    let h = normalize(l + view_dir);
    let spec = pow(max(dot(n, h), 0.0), 60.0) * 0.35;
    let rim = pow(1.0 - max(dot(n, view_dir), 0.0), 3.0) * 0.15;
    let fin = i.kind > 0.5;
    let base = select(u.base.rgb, vec3<f32>(0.32, 0.58, 0.88), fin);
    var col = base * (0.16 + 0.72 * diffuse + fill) + vec3<f32>(spec + rim);
    if (u.params.x > 0.5 && !fin) {
        // Zebra: reflect the view ray off the surface into a striped environment. Smooth stripes = fair surface.
        let r = reflect(-view_dir, n);
        let stripe = smoothstep(-0.08, 0.08, sin(r.z * u.params.y));
        col = mix(vec3<f32>(0.06, 0.06, 0.07), vec3<f32>(0.96, 0.96, 0.98), stripe) * (0.55 + 0.45 * diffuse);
    }
    return vec4<f32>(col, 1.0);
}
