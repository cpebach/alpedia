// Alpedia 柔らかい構築物の頂点シェーダー
// USE_COMPUTE あり（GL 4.3 以上）：コンピュートシェーダーが求めた頂点位置（skinned）を読む
// USE_COMPUTE なし（GL 3.3）     ：ここで直接スキニングする（隣の頂点の分も計算するので少し重い）
// 法線は隣り合う三角形の外積の和（CPU 版 shape.vertex_normals と同じ面積重み付き）

uniform mat4 p3d_ModelViewProjectionMatrix;
in vec4 p3d_Vertex;
in vec4 p3d_Color;
in vec2 p3d_MultiTexCoord0;
in vec4 skin_info;            // x: 頂点番号, y: 隣接リストの先頭, z: 隣接三角形の数, w: 持ち主ボクセル

uniform isamplerBuffer adj;   // 隣接三角形ごとに (j, k) の2つ（r32i を2つずつ）
uniform samplerBuffer firing; // ボクセルごとの噴射の強さ（r32f）

#ifdef USE_COMPUTE
uniform samplerBuffer skinned;
vec3 P(int g) { return texelFetch(skinned, g).xyz; }
#else
uniform samplerBuffer node_pos;
uniform isamplerBuffer v_owner;
uniform samplerBuffer v_w;
vec3 P(int g) {
    int o = texelFetch(v_owner, g).r * 8;
    vec4 w0 = texelFetch(v_w, 2 * g);
    vec4 w1 = texelFetch(v_w, 2 * g + 1);
    return w0.x * texelFetch(node_pos, o + 0).xyz + w0.y * texelFetch(node_pos, o + 1).xyz
         + w0.z * texelFetch(node_pos, o + 2).xyz + w0.w * texelFetch(node_pos, o + 3).xyz
         + w1.x * texelFetch(node_pos, o + 4).xyz + w1.y * texelFetch(node_pos, o + 5).xyz
         + w1.z * texelFetch(node_pos, o + 6).xyz + w1.w * texelFetch(node_pos, o + 7).xyz;
}
#endif

out vec3 v_normal;
out vec4 v_color;
out vec2 v_uv;
out float v_fire;

void main() {
    int g = int(skin_info.x + 0.5);
    int start = int(skin_info.y + 0.5);
    int count = int(skin_info.z + 0.5);
    vec3 p = P(g);
    vec3 n = vec3(0.0);
    for (int i = 0; i < count; ++i) {
        int j = texelFetch(adj, 2 * (start + i)).r;
        int k = texelFetch(adj, 2 * (start + i) + 1).r;
        n += cross(P(j) - p, P(k) - p);
    }
    v_normal = n;
    v_color = p3d_Color;
    v_uv = p3d_MultiTexCoord0;
    v_fire = texelFetch(firing, int(skin_info.w + 0.5)).r;
    gl_Position = p3d_ModelViewProjectionMatrix * vec4(p, 1.0);
}
