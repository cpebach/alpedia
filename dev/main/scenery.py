"""景色の箱（ボクセルではない、軽い建物・崖・街）と、惑星の表面の地図。シナリオ・場所の MOD が使う（中身は Rust：game/scenery.rs）。

- 地図 Map：惑星の表面の座標 (x, z) [m]。原点の向き up0 と、その点での前 fwd0。
- Scenery：箱の集まり（1か所）。世界座標の箱 C (中心)・A (行が軸)・E (半分の大きさ)。
  惑星の地形（掘った跡は含まない）から毎回同じ形に作る（セーブには入らない）。プレイヤー・構築物は当たって上に乗れる。
- SceneryHost：Addon に混ぜると、self.scenes の箱に当たり判定・視線・プレイヤーの押し出しが付く。
  画面側は main/client/scenery_view.py の SceneView で描く。
"""
from . import native

# ---------------------------------------------------------------------------
# 色（景色の箱）
# ---------------------------------------------------------------------------
ROCK = (0.46, 0.42, 0.38)
ROCK2 = (0.40, 0.37, 0.34)
ROCK3 = (0.52, 0.47, 0.41)
CONC = (0.62, 0.62, 0.60)
DARK = (0.22, 0.23, 0.26)
METAL = (0.55, 0.58, 0.62)
WHITE = (0.88, 0.89, 0.90)
ORANGE = (0.95, 0.45, 0.12)
RED = (0.80, 0.16, 0.14)
NAVY = (0.12, 0.18, 0.35)
MOON_G = (0.50, 0.50, 0.53)
GLASS = (0.55, 0.78, 0.92)

WARM = (1.0, 0.82, 0.45)      # 明かり（窓・街灯）
CYAN = (0.25, 0.95, 1.0)
PINK = (1.0, 0.3, 0.78)
GREEN = (0.35, 1.0, 0.5)
LIME = (0.6, 1.0, 0.3)
REDL = (1.0, 0.2, 0.15)
OFF = (0.16, 0.16, 0.18)      # 消えている明かり


_C = native.require()
unit = _C.scenery_unit
_unit = unit
Map = _C.Map              # 惑星の表面の地図。原点の向き up0 と、その点での前 fwd0
Scenery = _C.Scenery      # 景色の箱の集まり（1つの惑星の1か所）。finish() で C・A・E・col・center・radius・tags を作る


class SceneryHost:
    """Addon に混ぜる：self.scenes（Scenery のリスト）の当たり判定・視線・押し出し（計算は Rust）。"""
    scenes = ()
    scene_kind = "scenery"           # raycast の結果の "kind"（画面側がどの MOD の物か見分ける）

    def colliders(self, world, lo, hi, margin):
        return _C.scenery_colliders(list(self.scenes), lo, hi, float(margin))

    def raycast(self, world, o, dvec, max_t):
        r = _C.scenery_raycast(list(self.scenes), [float(x) for x in o], [float(x) for x in dvec], float(max_t))
        if r is None:
            return None
        s, t, k, n = r
        return {"kind": self.scene_kind, "t": t, "scene": s, "part": k, "point": o + dvec * t,
                "normal": n, "placeable": True, "tag": s.parts[k]["tag"]}

    def push_sphere(self, world, center, r):
        return _C.scenery_push_sphere(self, world, center, float(r))
