"""描画の下まわり：Rust の画面（native/alpedia_view）。本体は Rust（alpedia_core の gfx_*・RsNode など）。

render.py・scenery_view.py の描画（ノード・メッシュ・テクスチャ）は、ここの RsNode・RsDynamicMesh を使う。
RsNode は Panda3D の NodePath のうち、描画で使う分だけをまねる（親子・行列・表示・テクスチャ・半透明など）。
"""
from .. import native

_c = native.require()

VIEW = None            # alpedia_view.View（Rust の画面を使っているとき。gfx.start で入る）
EDGES = "edges"        # 2枚目のテクスチャ（輪郭の線）の段の目印

active = _c.gfx_active
start = _c.gfx_start                 # Rust の画面を開く（gfx.VIEW も入る）。使えなければ例外
set_view = _c.gfx_set_view
RsTex = _c.RsTex
tex = _c.gfx_tex
tex_rgba = _c.gfx_tex_rgba
RsNode = _c.RsNode                   # Rust の画面のノード（NodePath のまね）
RsDynamicMesh = _c.RsDynamicMesh     # 頂点 (N,12) と三角形を numpy 配列から設定するメッシュ
font = _c.gfx_font                   # フォントを読む（1回だけ）
quad = _c.gfx_quad                   # メッシュに色の四角を1つ（文字の下地など）
RsText = _c.RsText                   # 文字（と下地）
RsDisplayView = _c.RsDisplayView     # コンピューターの表示（看板）
box_vertices = _c.gfx_box_vertices   # 箱（-0.5〜0.5）の頂点 (24,12) と三角形
wire_box = _c.gfx_wire_box           # 枠だけの箱（12 本の細い棒）
set_box = _c.gfx_set_box             # ノードを (中心, 軸, 大きさ) に置く
