// Alpedia 陰影（フラグメントシェーダー）：CPU 版 render.shade と同じ式をピクセルごとに計算する
//   明るさ = AMBIENT + (1 - AMBIENT) * max(0, 法線・太陽)、噴射中はオレンジに光らせる

uniform sampler2D p3d_Texture0;
uniform vec3 sun_dir;
uniform float ambient;

in vec3 v_normal;
in vec4 v_color;
in vec2 v_uv;
in float v_fire;

out vec4 o_color;

void main() {
    float len = length(v_normal);
    vec3 n = len > 1e-12 ? v_normal / len : vec3(0.0);
    float lam = clamp(dot(n, sun_dir), 0.0, 1.0);
    vec3 c = v_color.rgb * (ambient + (1.0 - ambient) * lam);
    c = c * (1.0 - v_fire * 0.6) + vec3(1.0, 0.75, 0.25) * v_fire;
    o_color = vec4(c, 1.0) * texture(p3d_Texture0, v_uv);
}
