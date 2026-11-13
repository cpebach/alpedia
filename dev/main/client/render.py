"""描画の材料：構築物・惑星・地表・看板などの頂点を作り、Rust の画面（gfx）のメッシュに渡す。本体は Rust（alpedia_core）。

浮動原点：全ての描画位置は「カメラ（プレイヤーの目）からの相対座標」にしてから
float32 に変換するので、世界がどれだけ広くても精度が落ちない。

MOD が使う物：DynamicMesh(parent, name)・quad_tris(四角の数)・shade(法線, 色)。
"""
import numpy as np

from .. import native

SUN = np.array([0.35, 0.85, 0.4])
SUN /= np.linalg.norm(SUN)
AMBIENT = 0.38

# 見た目の設定（画面が config の graphics から入れる）
STYLE = {"edges": True, "ao": True}
EDGE_TEXTURE = None          # ブロックの輪郭の線のテクスチャ（画面が sauce/textures/px_edge.png を入れる）
DEVICE_TEXTURE = None        # テクスチャの無い機器の外板

_c = native.require()
load_texture = _c.render_load_texture         # 画像ファイルのテクスチャ（loader は昔の名残り。使わない）
DynamicMesh = _c.render_dynamic_mesh          # 頂点 (N,12) float32 と三角形を numpy 配列から設定できるメッシュ
quad_tris = _c.render_quad_tris               # 四角 nq 個の三角形 (2nq,3)
shade = _c.render_shade                       # 陰影：色 × (AMBIENT + (1−AMBIENT)·clip(法線·SUN))
project_uv = _c.render_project_uv
block_ao = _c.render_block_ao                 # 角の暗さ（環境光の遮り）
lod_vertices = _c.render_lod_vertices         # 構築物の遠景の箱の頂点
light_ids = _c.render_light_ids               # 構築物のライトの番号
preload = _c.render_preload                   # 定義が使うテクスチャを全部読み込んでおく（読めない物はエラーのリスト）
display_faces = _c.render_display_faces       # 看板を表示する面
face_frame_p = _c.render_face_frame_p
face_frame = _c.render_face_frame
_frames8 = _c.render_frames8
_edge_stage = _c.render_edge_stage

LightLayer = _c.LightLayer                    # ライトの光る箱
LodLayer = _c.LodLayer                        # 読み込み距離より遠い構築物の箱
ConstructView = _c.ConstructView              # 1つの構築物の描画（形・記号・看板）
DecalView = _c.DecalView                      # 機器の記号
PlanetView = _c.PlanetView                    # 遠景の惑星
TerrainPatch = _c.TerrainPatch                # プレイヤー周辺の詳細な地表と水面
CarveView = _c.CarveView                      # 洞窟・掘削跡の詳細なメッシュ


def _native():
    return _c
