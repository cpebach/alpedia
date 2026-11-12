// Alpedia の画面：頂点の色（陰影は焼き込み済み）× テクスチャ（2枚目は輪郭の線などを掛け合わせる）

struct Camera {
    view_proj: mat4x4<f32>,
};
@group(0) @binding(0) var<uniform> cam: Camera;

struct Model {
    model: mat4x4<f32>,
    tint: vec4<f32>,      // 色を掛ける（ノードの色）
    flags: vec4<f32>,     // x：2枚目のテクスチャを使う、y：テクスチャを使う、z：不透明さがこれ未満は描かない
};
@group(1) @binding(0) var<uniform> mdl: Model;

@group(2) @binding(0) var tex0: texture_2d<f32>;
@group(2) @binding(1) var tex1: texture_2d<f32>;
@group(2) @binding(2) var samp: sampler;

struct VIn {
    @location(0) pos: vec3<f32>,
    @location(1) nrm: vec3<f32>,
    @location(2) col: vec3<f32>,
    @location(3) alpha: f32,
    @location(4) uv: vec2<f32>,
};

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) col: vec4<f32>,
    @location(1) uv: vec2<f32>,
};

@vertex
fn vs_main(v: VIn) -> VOut {
    var o: VOut;
    o.clip = cam.view_proj * mdl.model * vec4<f32>(v.pos, 1.0);
    o.col = vec4<f32>(v.col, v.alpha) * mdl.tint;
    o.uv = v.uv;
    return o;
}

@fragment
fn fs_main(i: VOut) -> @location(0) vec4<f32> {
    var c = i.col;
    if (mdl.flags.y > 0.5) {
        c = c * textureSample(tex0, samp, i.uv);
    }
    if (mdl.flags.x > 0.5) {
        c = c * textureSample(tex1, samp, i.uv);
    }
    if (c.a < max(mdl.flags.z, 0.01)) {
        discard;
    }
    return c;
}
