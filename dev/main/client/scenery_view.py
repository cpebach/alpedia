"""景色の箱（main/scenery.py の Scenery）を描く。シナリオ・場所の MOD の画面側が使う。本体は Rust（alpedia_core.SceneView）。"""
from .. import native
from .render import DynamicMesh, quad_tris, shade  # noqa: F401（MOD がここから使うことがある）

_FACES = [(0, 1), (0, -1), (1, 1), (1, -1), (2, 1), (2, -1)]

# 岩はブロックと同じドット絵（px_rock、0.5 m で1枚＝ブロック1個分）にそろえる。建物・地面は今までの模様
TEXTURES = {"rock": ("px_rock.png", 0.5), "metal": ("metal.png", 2.0), "ground": ("ground.png", 2.5)}
PIXEL = {"rock"}                 # ドット絵：にじませない
EDGE_RGBA = (0.04, 0.045, 0.06, 0.55)   # 箱のふちの線（ブロックの輪郭の線と同じ考え方）

_c = native.require()
box_quads = _c.scenery_box_quads      # 箱 (B) → 各面の4頂点 (B,6,4,3)・法線 (B,6,3)。外から見て表になる回り順
tex_kind = _c.scenery_tex_kind        # 景色の箱の見た目の種類：rock・ground・metal・plain
SceneView = _c.SceneView              # 景色の箱1か所の描画：SceneView(parent, scene)・update(origin, lights_on, t)・remove()
