// Alpedia の画面：頂点の色（陰影は焼き込み済み）× テクスチャ（2枚目は輪郭の線などを掛け合わせる）

struct Camera {
    view_proj: mat4x4<f32>,
};
@group(0) @binding(0) var<uniform> cam: Camera;

struct Model {
    model: mat4x4<f32>,
    tint: vec4<f32>,      // 色を掛ける（ノードの色）
    flags: vec4<f32>,     // x：2枚目のテクスチャを使う、y：テクスチャを使う、z：不透明さがこれ未満は描かない
    extra: vec4<f32>,     // x：立体映像（ホログラム）の描き方、y：時刻
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
    @location(2) raw: vec4<f32>,      // 頂点の色そのまま（立体映像：縁の光り方・高さ・明るさの倍率・濃さ）
};

@vertex
fn vs_main(v: VIn) -> VOut {
    var o: VOut;
    o.clip = cam.view_proj * mdl.model * vec4<f32>(v.pos, 1.0);
    o.col = vec4<f32>(v.col, v.alpha) * mdl.tint;
    o.uv = v.uv;
    o.raw = vec4<f32>(v.col, v.alpha);
    return o;
}

@fragment
fn fs_main(i: VOut) -> @location(0) vec4<f32> {
    var t0 = vec4<f32>(1.0);
    if (mdl.flags.y > 0.5) {
        t0 = textureSample(tex0, samp, i.uv);
    }
    var c = i.col * t0;
    if (mdl.flags.x > 0.5) {
        c = c * textureSample(tex1, samp, i.uv);
    }
    if (mdl.extra.x > 0.5) {
        // 立体映像：単色・走査線・ちらつき・縁が光る（光の色はノードの色）
        if (t0.a < mdl.flags.z) {
            discard;
        }
        let lum = dot(t0.rgb, vec3<f32>(0.3, 0.59, 0.11)) * i.raw.b;
        let fres = i.raw.r;
        let vh = i.raw.g;
        let holo = i.raw.a;
        let time = mdl.extra.y;
        let scan = 0.6 + 0.4 * sin(vh * 260.0 - time * 12.0);
        let band = smoothstep(0.0, 0.04, fract(vh * 1.7 - time * 0.6)) * 0.25 + 0.75;
        let hc = mdl.tint.rgb * (0.1 + 0.4 * lum + 0.8 * fres) * scan * band;
        return vec4<f32>(hc * holo, holo * (0.3 + 0.4 * fres + 0.1 * scan));
    }
    if (c.a < max(mdl.flags.z, 0.01)) {
        discard;
    }
    return c;
}
