"""構築物の「形」：ボクセルを球として扱い、結合したボクセルの間を埋める。本体は Rust（alpedia_core の shape_*）。

形 = 次の図形を半径 r（ボクセルの半分）でなぞったものの和集合
  - 各ボクセルの中心（点）              → 球
  - 結合した2つのボクセルの中心を結ぶ線 → 両端が球の円柱（カプセル）
  - 2×2×2 の範囲で結合しているボクセル群の中心の凸包の面（三角形） → 斜面・平面
したがって、2つ並べればカプセル、L字や階段状に置けば内側の角が斜面で埋まる。

この形は描画（マーチングキューブでメッシュ化し、ボクセルの8頂点で変形させる）と
当たり判定（地面・プレイヤー・構築物どうし）の両方で使う。
"""
import itertools

from . import native

_c = native.require()

OFFS8 = [tuple(int(x) for x in o) for o in itertools.product((0, 1), repeat=3)]
# かたい素材の角ばった見た目：面の位置（丸い形の半径に対して。Rust の topo.rs と同じ）
FACET_SCALE = 0.97

marching_cubes = _c.shape_marching_cubes
Csr = _c.Csr
_hull_table = _c.shape_hull_table
block_kinds = _c.shape_block_kinds
topology = _c.shape_topology
facet_voxels = _c.shape_facet_voxels
build_mesh = _c.shape_build_mesh
cached_mesh = _c.shape_cached_mesh
mesh_ready = _c.shape_mesh_ready
request_mesh = _c.shape_request_mesh
mesh_for = _c.shape_mesh_for
_owner = _c.shape_owner
closest_on_shape = _c.shape_closest_on_shape
collision_proxies = _c.shape_collision_proxies


def skin(c, mesh):
    """現在の頂点位置（変形後）。"""
    return _c.skin(c.pos, mesh["vidx"], mesh["w"])


def vertex_normals(v, faces):
    import numpy as np
    return _c.vertex_normals(np.ascontiguousarray(v, dtype=np.float64), np.ascontiguousarray(faces, dtype=np.int64))
