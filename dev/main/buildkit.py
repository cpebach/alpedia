"""ブロックで建物・船・飾りを組み立てる道具（シナリオ・場所の MOD が使う）。本体は Rust（game/buildkit.rs）。

Kit にセル（整数の (x, y, z)。+Y が上、+Z が前）ごとに素材・機器を置いていき、voxels() で構築物のボクセルの
リスト [(セル, 素材, 機器)] にする。あとから置いた物が前の物を上書きする。clear で消す。

- 形：fill（詰まった箱）・shell（壁・床・天井だけの箱）・dome（半球）・ellipsoid（だ円体）・cylinder（円柱）
- 開口：door（壁に穴）・windows（壁の一部をガラスに）
- 家具：desk・chair・table・sofa・bed・shelf・console（表示つきのコンピューター）・planter・crates・lamp_post など
- anchored(V)：足もとの下にアンカーを足す（動かない物。sites.anchored と同じ規則）
- spawn(world, V, ...)：地図の場所・ワールドの姿勢に置く。ensure(world, key, items)：無ければ作る（MOD 用）
"""
import numpy as np

from . import native

ANCHOR = {"def": "anchor", "dir": 2, "name": "アンカー"}
_N6 = ((1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1))
# 機器の向き（dir）：+X 0, -X 1, +Y 2, -Y 3, +Z 4, -Z 5
FACING = {"+x": 0, "-x": 1, "+y": 2, "-y": 3, "+z": 4, "-z": 5}


_C = native.require()
Kit = _C.Kit                             # セルごとに素材・機器を置いていく（v：セル → (素材, 機器)）


def _rng(a, b):
    return range(int(min(a, b)), int(max(a, b)) + 1)


def _axes(f):
    """前の向き f → (横の向き, 前の向き)（セルの差）。"""
    fw = {"+z": (0, 0, 1), "-z": (0, 0, -1), "+x": (1, 0, 0), "-x": (-1, 0, 0)}[f]
    ax = (fw[2], 0, -fw[0]) if fw[0] else (1 if fw[2] > 0 else -1, 0, 0)
    return ax, fw


def _add(p, d, k):
    return (p[0] + d[0] * k, p[1] + d[1] * k, p[2] + d[2] * k)


# ---------------------------------------------------------------------------
# アンカー
# ---------------------------------------------------------------------------
root_cells = _C.bk_root_cells            # アクチュエーターに動かされない部分のセルの集合
anchor_cells = _C.bk_anchor_cells        # 足もとの下に置くアンカーのセル
anchored = _C.bk_anchored                # V にアンカーを足したもの（足もとの下に埋める）
anchored_inside = _C.bk_anchored_inside  # V の中のセル cells をアンカーに置き換えたもの
anchor_center = _C.bk_anchor_center      # 真ん中あたりの、機器の無いブロック1つをアンカーに


# ---------------------------------------------------------------------------
# 置く
# ---------------------------------------------------------------------------
def grounded(V, R, t, size, sdf, mat=None, step=2):
    """足もとと地面のすき間を土台のブロックで埋め、その下にアンカーを置き直した V（坂に置いても浮かない）。
    R, t：格子の座標（セル × 大きさ）→ ワールド。sdf(P)：点の地面からの高さ（中は負）。
    mat：土台の素材（None なら真上のブロックと同じ）。"""
    return _C.bk_grounded(V, np.ascontiguousarray(R, dtype=np.float64), np.ascontiguousarray(t, dtype=np.float64).ravel(),
                          float(size), sdf, mat, int(step))


spawn = _C.bk_spawn                      # ボクセルを置く（p：足もとの基準点、R：向き。lift_sdf があれば持ち上げる）
ensure = _C.bk_ensure                    # MOD の建物：まだ無ければ作る（ワールドに1回だけ）
ground_fn = _C.bk_ground_fn              # 地図の惑星の地面と景色の箱を合わせた「地面からの高さ」の関数
spawn_on_map = _C.bk_spawn_on_map        # 地図の場所 (x, z, 向き, 高さ) に置く（持ち上げ・土台・アンカーの置き直し）


# ---------------------------------------------------------------------------
# 明かりのまとまり
# ---------------------------------------------------------------------------
light_devices = _C.bk_light_devices      # 構築物 c のライト（group が名前ならそのまとまりだけ）の機器のリスト
set_light_group = _C.bk_set_light_group  # まとまり group のライトを全部点ける・消す
