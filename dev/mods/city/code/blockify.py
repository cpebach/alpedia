"""A-03：街・集落の建物をブロックの構築物にする。

- 箱（"body"・"wall"）で作った建物：ビルは中が空いた殻（1〜2 m のブロック、窓はガラス）、店・住宅の壁は 0.5 m のブロック。
  箱は街から抜き出す（当たり判定・描画はブロックの構築物が受け持つ）。家具・看板・ネオン・舗装は今までどおり箱。
- Kit で作った建物（集落の家・納屋・工場・港のクレーンなど。settlements.py）：そのまま置く。
- どの建物もアンカーで固定（動かない・軽い）。近くにプレイヤーがいる街だけ作り（NEAR m）、離れたら（FAR m）片付ける。
  プレイヤーが手を加えた建物（形が変わった）は残す。壊されて無くなった建物は作り直さない。
- 作るのはホストだけ（構築物はふつうに参加者へ送られる）。作った物・残した物・無くなった物は settings["city_blocks"] に保存。
"""
import numpy as np

from main import buildkit, log
from main.construct import Construct

L = log.get("city.blocks")

NEAR = 1200.0           # これより近い街の建物を作る
FAR = 1800.0            # これより遠くなったら片付ける
CHECK = 1.0             # 何秒ごとに調べるか
CHECK_BUSY = 0.2        # 置く途中の建物があるときは、この間隔で続ける
PER_CHECK = 2           # 1 回に置く建物の数（一度にたくさん置くと固まる）
TOWER_MATS = ("hull_dark", "hull_navy", "hull_black", "concrete", "hull_teal")
WALL_MAT = "concrete"


def extract(city):
    """city.parts から建物の箱（"body"・"wall"）を抜き出して city.blocks に入れる（finish の前に呼ぶ）。地表の街だけ。"""
    if city.kind != "surface":
        return
    keep, groups = [], {}
    for p in city.parts:
        if p["kind"] in ("body", "wall", "glass") and p["bld"] >= 0:
            groups.setdefault((p["bld"], p["kind"]), []).append(p)
        else:
            keep.append(p)
    city.parts = keep
    blocks = getattr(city, "blocks", None) or []
    for (b, kind), parts in sorted(groups.items(), key=lambda kv: (kv[0][0], kv[0][1])):
        blocks.append({"bld": b, "kind": kind, "boxes": [(p["c"], p["A"], p["e"]) for p in parts]})
    city.blocks = blocks


def extent(city):
    """建物を含めた街の半径（街の中心から）"""
    r = 0.0
    for bl in getattr(city, "blocks", []):
        for c, A, e in bl.get("boxes", []):
            r = max(r, float(np.linalg.norm(c)) + float(np.linalg.norm(e)))
        if "origin" in bl:
            r = max(r, float(np.linalg.norm(bl["origin"])) + bl.get("radius", 10.0))
    return r


def _box_voxels(c, A, e, s, mat, shell, rng):
    """向きつきの箱 → (セル, 素材, None) のリスト・原点（セル (0,0,0) の角、街ローカル）・セルの数"""
    n = [max(1, int(round(2 * e[k] / s))) for k in range(3)]
    org = np.asarray(c, float) - sum(np.asarray(A[k], float) * (n[k] * s / 2) for k in range(3))
    vox = []
    nx, ny, nz = n
    win = "glass"
    for i in range(nx):
        for j in range(ny):
            for k in range(nz):
                edge = i in (0, nx - 1) or k in (0, nz - 1)
                if shell and not (edge or j in (0, ny - 1)):
                    continue
                m = mat
                # 窓：外壁の、床から上の段の一部をガラスに（2 段に 1 段・角は残す）
                if shell and edge and 0 < j < ny - 1 and j % 2 == 1 and not ((i in (0, nx - 1)) and (k in (0, nz - 1))):
                    m = win
                vox.append(((i, j, k), m, None))
    return vox, org, n


def plan(city, rng_seed):
    """街の建物ごとの (ID, ボクセル, 原点（世界）, 回転（列が軸）, セルの大きさ, 名前)。毎回同じ結果"""
    out = []
    R = city.R
    for k, bl in enumerate(getattr(city, "blocks", [])):
        cid = f"blk-{city.id}-{k}"
        if "lazy" in bl:              # 集落の建物：ここで形を作る
            from . import settlements
            from .cities import _ground
            r = settlements.realize(city, bl, _ground)
            if r is None:
                continue
            V, org, rot, s, name = r
            out.append((cid, bl["bld"], V, city.x + R @ org, R @ rot, s, name))
            continue
        if "voxels" in bl:            # Kit で作った建物
            org = city.x + R @ np.asarray(bl["origin"], float)
            out.append((cid, bl["bld"], bl["voxels"], org, R @ np.asarray(bl["rot"], float), bl["size"], bl.get("name", "建物")))
            continue
        rng = np.random.default_rng([rng_seed, k])
        if bl["kind"] == "body":
            big = max(2 * float(np.max(e)) for _, _, e in bl["boxes"])
            s = 1.0 if big <= 24 else 2.0
            mat = TOWER_MATS[int(rng.integers(len(TOWER_MATS)))]
            shell = True
        elif bl["kind"] == "glass":
            s, mat, shell = 0.5, "glass", False             # ガラスの壁（高層階の部屋。metro.py）
        else:
            s, mat, shell = 0.5, WALL_MAT, False
        # 1 つの建物の箱は同じ向き。最初の箱の向き・原点に合わせて、ほかの箱のセルを並べる
        c0, A0, _ = bl["boxes"][0]
        A0 = np.asarray(A0, float)
        vox, org0 = {}, None
        for c, A, e in bl["boxes"]:
            v, org, _ = _box_voxels(c, A0, e, s, mat, shell, rng)
            if org0 is None:
                org0 = org
            off = np.round(A0 @ (org - org0) / s).astype(int)
            for cell, m, d in v:
                vox[(cell[0] + off[0], cell[1] + off[1], cell[2] + off[2])] = (m, d)
        V = [(cell, m, d) for cell, (m, d) in vox.items()]
        V = buildkit.anchored(V, 4 if s < 1 else 3)
        name = {"body": "ビル", "glass": "ガラスの壁"}.get(bl["kind"], "建物")
        out.append((cid, bl["bld"], V, city.x + R @ org0, R @ A0.T, s, name))
    return out


class Streamer:
    """近くの街の建物を作り、遠くなったら片付ける"""

    def __init__(self, world, manager):
        self.world = world
        self.m = manager
        st = world.settings.setdefault("city_blocks", {})
        self.kept = set(st.get("kept", []))          # 手を加えたので残す物
        self.gone = set(st.get("gone", []))          # 壊されて無くなった物（作り直さない）
        self.made = {}                               # ID → 作ったときの版（まだ手を加えていない物）
        self.t = 0.0
        self._plans = {}

    def save(self):
        self.world.settings["city_blocks"] = {"kept": sorted(self.kept), "gone": sorted(self.gone)}

    def _plan(self, city):
        p = self._plans.get(city.id)
        if p is None:
            p = self._plans[city.id] = plan(city, abs(hash(city.id)) % (2 ** 31))
        return p

    def players(self):
        w = self.world
        ps = [w.player] + list(getattr(w, "remote", {}).values())
        return [np.asarray(p.pos, float) for p in ps if p is not None]

    def update(self, dt=None):
        """dt を渡さなければワールドの時刻で CHECK 秒ごと（テストは dt を渡して強制できる）"""
        w = self.world
        if getattr(w, "net", None) is not None and getattr(w.net, "is_client", False):
            return
        if dt is None:
            last = getattr(self, "_last", None)
            if last is not None and w.time - last < (CHECK_BUSY if getattr(self, "_busy", False) else CHECK):
                return
            self._last = w.time
        else:
            self.t += dt
            if self.t < CHECK:
                return
            self.t = 0.0
        pp = self.players()
        if not pp:
            return
        cons = w.constructs
        if not getattr(self, "_adopted", False):
            # 保存してあった街の建物（作ったままの物）：これからも出し入れする
            self._adopted = True
            for cid, c in cons.items():
                if str(cid).startswith("blk-") and cid not in self.kept:
                    self.made[cid] = c.version
        # 手を加えた・壊された
        for cid, ver in list(self.made.items()):
            c = cons.get(cid)
            if c is None:
                self.gone.add(cid)
                del self.made[cid]
            elif ver is None:
                self.made[cid] = c.version          # 作った次の確認で版を覚える（最初のティックで作り直されるので）
            elif c.version != ver:
                self.kept.add(cid)
                del self.made[cid]
        by_id = {c.id: c for c in self.m.cities}

        def dist(city):
            return min(float(np.linalg.norm(p - city.x)) for p in pp) - getattr(city, "block_radius", city.radius)

        # 遠くなった街の、手を加えていない建物を片付ける（建物のある街だけ見る）
        for cid in [k for k in self.made]:
            city = by_id.get(self._city_of(cid))
            if city is not None and dist(city) > FAR and cid in cons and self.made[cid] in (None, cons[cid].version):
                del cons[cid]
                del self.made[cid]
        # 近い街から順に、まだ無い建物を少しずつ置く
        near = sorted((d, i) for i, c in enumerate(self.m.cities) if getattr(c, "blocks", None) and (d := dist(c)) < NEAR)
        left = PER_CHECK
        busy = False
        for _, i in near:
            city = self.m.cities[i]
            self.on_state(city)
            for cid, b, V, org, rot, s, name in self._plan(city):
                if cid in cons or cid in self.gone or cid in self.kept:
                    continue
                if b >= 0 and city.state is not None and b < len(city.state) and city.state[b] >= 2:
                    continue
                if left <= 0:
                    busy = True
                    break
                c = Construct.from_voxels(w.registry, V, org, rot, size=s, name=f"{city.name}の{name}", id_=cid)
                w.add_construct(c)
                self.made[cid] = None
                left -= 1
            if busy:
                break
        self._busy = busy

    @staticmethod
    def _city_of(cid):
        """建物の ID（blk-街の ID-番号）→ 街の ID"""
        s = str(cid)[4:]
        return s.rsplit("-", 1)[0]

    def on_state(self, city):
        """建物の状態が変わった（2 = 消えた）：その建物のブロックも片付ける"""
        cons = self.world.constructs
        for cid, b, *_ in self._plan(city):
            if b >= 0 and city.state is not None and b < len(city.state) and city.state[b] >= 2 and cid in cons \
                    and cid in self.made:
                del cons[cid]
                del self.made[cid]
                self.gone.add(cid)
