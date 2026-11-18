"""レールと車輪（本体の機能。MOD ではない）。

- 線路（Track）：惑星ローカルの点の並び（レールの上面）。長さ s で位置・向きを引ける。
  プレイヤーが置いたレールのブロック（機器 "rail"）からも作るし、MOD（大都市圏など）が add_track で足すこともできる。
- 車輪（機器 "wheel"）のある構築物が線路の上（車輪の下 1 m ほどに線路）に来ると、線路に乗る。乗っている間は物理で動かさず、
  線路に沿った 1 次元の運動（重力の坂の成分・車輪の駆動・転がり抵抗）で動かす（構築物の hold_in_frame / move_with_frame）。
  座って W で前（操縦席の向き）・S で後ろ。何も押さずにゆっくりなら止まって動かない（ブレーキ）。線路の端では止まる。
  端と端がつながっている線路へは乗り継ぐ。車輪を E で触ると、線路から降ろす・また乗せる。
- 駆動には電気を使う（構築物のバッテリーから。力 × 速さ）。
"""
import numpy as np

from main import log
from main.hooks import Addon

L = log.get("rail")

ATTACH_UP = 1.6         # 車輪の真ん中が、レールの上面からこの高さまでなら乗る（m）
ATTACH_SIDE = 1.0       # 横のずれ（m）
ROLL = 0.003            # 転がり抵抗（重さ × 重力 × これ）
HOLD_V = 0.4            # 何も押していないとき、これより遅ければ止める（m/s）
LINK = 1.2              # 端と端がこれより近い線路は、つながっているとみなす（m）
SCAN = 0.5              # 乗れる構築物・レールのブロックを調べる間隔（秒）


def _unit(v):
    n = np.linalg.norm(v, axis=-1, keepdims=True)
    return v / np.maximum(n, 1e-12)


class Track:
    """線路 1 本。pts：惑星ローカルのレールの上面の点 (n,3)、ups：上向き（無ければ惑星の中心から外向き）"""

    def __init__(self, key, planet, pts, ups=None, name=""):
        P = np.asarray(pts, float)
        if len(P) < 2:
            raise ValueError("線路には 2 点以上が要ります")
        self.key = key
        self.planet = planet
        self.name = name
        self.P = P
        self.U = _unit(np.asarray(ups, float) if ups is not None else P)
        seg = np.linalg.norm(np.diff(P, axis=0), axis=1)
        self.S = np.concatenate([[0.0], np.cumsum(seg)])
        self.length = float(self.S[-1])
        self.T = _unit(np.diff(P, axis=0))          # 区間ごとの向き

    def _seg(self, s):
        s = float(np.clip(s, 0.0, self.length))
        i = int(np.clip(np.searchsorted(self.S, s, side="right") - 1, 0, len(self.P) - 2))
        L_ = max(self.S[i + 1] - self.S[i], 1e-9)
        return i, (s - self.S[i]) / L_

    def at(self, s):
        """長さ s の所の (点, 向き, 上向き)（惑星ローカル）"""
        i, f = self._seg(s)
        p = self.P[i] * (1 - f) + self.P[i + 1] * f
        u = self.U[i] * (1 - f) + self.U[i + 1] * f
        # 向き：区間のつなぎ目の近く（前後 3 m）はなめらかに
        t = self.T[i]
        L_ = self.S[i + 1] - self.S[i]
        d0, d1 = f * L_, (1 - f) * L_
        if d0 < 3.0 and i > 0:
            k = 0.5 + 0.5 * d0 / 3.0
            t = _unit(self.T[i - 1] * (1 - k) + t * k)
        elif d1 < 3.0 and i < len(self.T) - 1:
            k = 0.5 + 0.5 * d1 / 3.0
            t = _unit(self.T[i + 1] * (1 - k) + t * k)
        u = _unit(u - t * (u @ t))
        return p, t, u

    def frame(self, s):
        """長さ s の所の (向き R（列が 右・上・前）, 点（世界）, 向き t（世界）)"""
        p, t, u = self.at(s)
        r = _unit(np.cross(u, t))
        R = np.stack([r, u, t], axis=1)
        return R, np.asarray(self.planet.center, float) + p, t

    def nearest(self, q):
        """世界の点 q にいちばん近い所の (s, 上へのずれ, 横のずれ)"""
        ql = np.asarray(q, float) - np.asarray(self.planet.center, float)
        A, B = self.P[:-1], self.P[1:]
        AB = B - A
        t = np.clip(np.einsum("ij,ij->i", ql - A, AB) / np.maximum(np.einsum("ij,ij->i", AB, AB), 1e-12), 0.0, 1.0)
        C = A + AB * t[:, None]
        d = np.linalg.norm(ql - C, axis=1)
        i = int(np.argmin(d))
        s = float(self.S[i] + t[i] * (self.S[i + 1] - self.S[i]))
        _, tt, u = self.at(s)
        off = ql - C[i]
        up = float(off @ u)
        side = float(np.linalg.norm(off - u * up - tt * (off @ tt)))
        return s, up, side

    def ends(self):
        c = np.asarray(self.planet.center, float)
        return c + self.P[0], c + self.P[-1]


def tracks_from_voxels(c, reg, planet, rail_ids):
    """構築物 c のレールのブロック（番号 rail_ids）→ 線路の点の並び（世界）のリスト。

    隣り合うレール（26 近傍）は、どちらかのレールの向きの軸に沿ってずれていればつながる（曲がり角・段もつながる）。
    枝分かれ（3 本以上）の所で切る。"""
    cells = [tuple(int(v) for v in c.cells[i]) for i in rail_ids]
    if not cells:
        return []
    cen = np.asarray(c.voxel_centers(), float)[rail_ids]
    fr = np.asarray(c.voxel_frames(), float)[rail_ids]
    s = float(c.size)
    axis = [int(c.devs[i].get("dir", 0)) // 2 for i in rail_ids]
    at = {cell: k for k, cell in enumerate(cells)}
    nb = [[] for _ in cells]
    for k, cell in enumerate(cells):
        for dx in (-1, 0, 1):
            for dy in (-1, 0, 1):
                for dz in (-1, 0, 1):
                    if dx == dy == dz == 0:
                        continue
                    j = at.get((cell[0] + dx, cell[1] + dy, cell[2] + dz))
                    if j is None or j <= k:
                        continue
                    dd = (dx, dy, dz)
                    if dd[axis[k]] != 0 or dd[axis[j]] != 0:
                        # 同じ軸の真横に並んだ 2 本（複線）はつながない
                        if sum(abs(v) for v in dd) > 1 and dd[axis[k]] == 0 and dd[axis[j]] == 0:
                            continue
                        nb[k].append(j)
                        nb[j].append(k)
    pc = np.asarray(planet.center, float)

    def top(k):
        return cen[k] + _unit(cen[k] - pc) * (s / 2)

    seen_e = set()
    out = []

    def walk(a, b):
        path = [a, b]
        seen_e.add((min(a, b), max(a, b)))
        while len(nb[path[-1]]) == 2:
            nxt = [x for x in nb[path[-1]] if x != path[-2]][0]
            e = (min(path[-1], nxt), max(path[-1], nxt))
            if e in seen_e:
                break
            seen_e.add(e)
            path.append(nxt)
        return path

    starts = [k for k in range(len(cells)) if len(nb[k]) != 2] + list(range(len(cells)))
    for a in starts:
        for b in nb[a]:
            if (min(a, b), max(a, b)) in seen_e:
                continue
            path = walk(a, b)
            pts = [top(k) for k in path]
            # 端のブロックの外側の面まで延ばす
            d0 = _unit(pts[0] - pts[1])
            d1 = _unit(pts[-1] - pts[-2])
            pts = [pts[0] + d0 * (s / 2)] + pts + [pts[-1] + d1 * (s / 2)]
            out.append(np.array(pts))
    for k in range(len(cells)):
        if not nb[k]:
            a = fr[k][axis[k]]
            p = top(k)
            out.append(np.array([p - a * (s / 2), p + a * (s / 2)]))
    return out


class RailAddon(Addon):
    name = "rail"

    def __init__(self):
        self.world = None
        self.ext = {}            # MOD が足した線路：key → Track
        self.vox = {}            # レールのブロックから作った線路：key → Track
        self._vox_ver = {}       # 構築物 ID → 版（作り直すかどうか）
        self.riding = {}         # 構築物 ID → {"key", "s", "v", "fwd"}
        self.t = 0.0
        self._cache = {}         # 構築物 ID → (版, 車輪の番号, 駆動の力, 最高速度, 出力)

    # ------------------------------------------------------------------ 線路
    def add_track(self, key, planet, pts, ups=None, name=""):
        """線路を足す（pts は惑星ローカル）。同じ key は置き換える"""
        tr = Track(key, planet, pts, ups, name)
        self.ext[key] = tr
        return tr

    def remove_track(self, key):
        self.ext.pop(key, None)

    def tracks(self):
        return list(self.ext.values()) + list(self.vox.values())

    def track(self, key):
        return self.ext.get(key) or self.vox.get(key)

    def on_load(self, world):
        self.world = world

    def _scan_rails(self, world):
        """レールのブロックのある構築物から線路を作り直す（形が変わった物だけ）"""
        seen = set()
        for cid, c in world.constructs.items():
            try:
                ids = list(c.dev_ids("rail"))
            except Exception:  # noqa: BLE001
                continue
            if not ids:
                continue
            seen.add(cid)
            if self._vox_ver.get(cid) == c.version:
                continue
            self._vox_ver[cid] = c.version
            for k in [k for k in self.vox if k.startswith(f"v:{cid}:")]:
                del self.vox[k]
            pl = world.nearest_planet(np.asarray(c.voxel_centers(), float)[ids[0]])
            if pl is None:
                continue
            for n, pts in enumerate(tracks_from_voxels(c, world.registry, pl, ids)):
                key = f"v:{cid}:{n}"
                self.vox[key] = Track(key, pl, pts - np.asarray(pl.center, float))
        for cid in [k for k in self._vox_ver if k not in seen]:
            del self._vox_ver[cid]
            for k in [k for k in self.vox if k.startswith(f"v:{cid}:")]:
                del self.vox[k]

    def _wheels(self, c):
        """(車輪の番号, 駆動の力の和 N, 最高速度 m/s, 出力の和 W)。形が変わったときだけ数え直す"""
        e = self._cache.get(c.id)
        if e is not None and e[0] == c.version:
            return e[1:]
        try:
            ids = list(c.dev_ids("wheel"))
        except Exception:  # noqa: BLE001
            ids = []
        drive, vmax, power = 0.0, 1e9, 0.0
        for i in ids:
            p = (c.reg.device(c.devs[i]["def"]) or {}).get("params", {})
            drive += float(p.get("drive", 30000.0))
            vmax = min(vmax, float(p.get("max_speed", 30.0)))
            power += float(p.get("power", 120000.0))
        r = (ids, drive, (vmax if ids else 0.0), power)
        self._cache[c.id] = (c.version,) + r
        return r

    # ------------------------------------------------------------------ 乗る・降りる
    def try_attach(self, world, c):
        """車輪の下に線路があれば乗せる。乗せたら True"""
        ids, *_ = self._wheels(c)
        if not ids or c.anchored or c.id in self.riding:
            return False
        if any((c.devs[i].get("state") or {}).get("off") for i in ids):
            return False
        cen = np.asarray(c.voxel_centers(), float)[ids]
        best = None
        for tr in self.tracks():
            if tr.key.startswith(f"v:{c.id}:"):
                continue                        # 自分のレールには乗らない
            # だいたいの距離で先にふるい落とす
            lo = np.linalg.norm(cen[0] - (np.asarray(tr.planet.center, float) + tr.P), axis=1).min()
            if lo > tr.length + 5.0 and lo > 60.0:
                continue
            hits = []
            for w in cen:
                s, up, side = tr.nearest(w)
                if -0.4 < up < ATTACH_UP and side < ATTACH_SIDE and 0.0 <= s <= tr.length:
                    hits.append(s)
            if hits and (best is None or len(hits) > best[0]):
                best = (len(hits), tr, float(np.mean(hits)))
        if best is None:
            return False
        _, tr, s = best
        v0 = float(np.asarray(c.velocity(), float) @ tr.frame(s)[2]) if hasattr(c, "velocity") else 0.0
        # 前：操縦席の向き（無ければ線路の向き）
        fwd = 1.0
        try:
            seats = list(c.dev_ids("seat"))
        except Exception:  # noqa: BLE001
            seats = []
        if seats:
            i = seats[0]
            d = int(c.devs[i].get("dir", 4))
            a = np.asarray(c.voxel_frames(), float)[i][d // 2] * (1 if d % 2 == 0 else -1)
            fwd = 1.0 if float(a @ tr.frame(s)[2]) >= 0 else -1.0
        R, x, t = tr.frame(s)
        c.hold_in_frame(R, x, t * v0, np.zeros(3))
        self.riding[c.id] = {"key": tr.key, "s": s, "v": v0, "fwd": fwd}
        return True

    def detach(self, c):
        st = self.riding.pop(c.id, None)
        if st is None:
            return
        tr = self.track(st["key"])
        c.release_frame()
        if tr is not None:
            # 線路の向きの速さを残す
            _, _, t = tr.frame(st["s"])
            try:
                c.vel[:] = t * st["v"]
            except Exception:  # noqa: BLE001
                pass

    def interact_device(self, world, c, i, d, dd):
        t = (dd or {}).get("type")
        if t == "rail":
            for k, tr in self.vox.items():
                if k.startswith(f"v:{c.id}:"):
                    s, up, side = tr.nearest(np.asarray(c.voxel_centers(), float)[i])
                    if side < 1.0 and abs(up) < 1.0:
                        return f"{d.get('name') or 'レール'}: 線路 {tr.length:.0f} m"
            return f"{d.get('name') or 'レール'}: つながっていません（向きの矢印の方へ並べてください）"
        if t != "wheel":
            return None
        st = d.setdefault("state", {})
        if c.id in self.riding:
            self.detach(c)
            st["off"] = True
            return "車輪: 線路から降ろしました（E でまた乗せる）"
        st.pop("off", None)
        if self.try_attach(world, c):
            return "車輪: 線路に乗りました（座って W で前・S で後ろ）"
        return "車輪: 下に線路がありません"

    # ------------------------------------------------------------------ 毎ティック
    def before_physics(self, world, dt):
        dt = float(dt)
        self.t += dt
        cons = world.constructs
        if self.t >= SCAN:
            self.t = 0.0
            self._scan_rails(world)
            if self.tracks():
                for cid, c in list(cons.items()):
                    if cid in self.riding or c.anchored:
                        continue
                    ids, *_ = self._wheels(c)
                    if ids:
                        self.try_attach(world, c)
        out = set()
        for cid in list(self.riding):
            c = cons.get(cid)
            st = self.riding[cid]
            tr = self.track(st["key"])
            ids, drive, vmax, power = (self._wheels(c) if c is not None else ([], 0, 0, 0))
            if c is None or tr is None or not ids:
                if c is not None:
                    self.detach(c)
                else:
                    self.riding.pop(cid, None)
                continue
            self._drive(world, c, st, tr, dt, drive, vmax, power)
            out.add(cid)
        return out

    def _drive(self, world, c, st, tr, dt, drive, vmax, power):
        m = max(float(c.total_mass()), 1.0)
        R, x, t = tr.frame(st["s"])
        g = np.asarray(world.gravity(x[None]), float)[0]
        gn = float(np.linalg.norm(g))
        keys = set(world.keys_for(c)) if world.seated_in(c) else set()
        thr = (1.0 if "w" in keys else 0.0) - (1.0 if "s" in keys else 0.0)
        v = st["v"]
        F = m * float(g @ t)
        if thr and self._has_power(c):
            f = thr * st["fwd"] * drive
            if abs(v) > 0.5 and f * v > 0:
                f = float(np.sign(f)) * min(abs(f), power / abs(v))
            F += f
            self._use_power(c, abs(f * v) * dt + 50.0 * dt)
        elif not thr and abs(v) < HOLD_V:
            st["v"] = 0.0                                   # ブレーキ：止まっている
            self._move(c, tr, st, dt)
            return
        F -= np.sign(v) * ROLL * m * gn
        v += F / m * dt
        v = float(np.clip(v, -vmax, vmax))
        s = st["s"] + v * dt
        if s < 0.0 or s > tr.length:
            nxt = self._next(tr, 0 if s < 0.0 else 1)
            if nxt is None:
                s, v = float(np.clip(s, 0.0, tr.length)), 0.0
            else:
                tr2, s2, flip = nxt
                over = -s if s < 0.0 else s - tr.length
                st["key"] = tr2.key
                v = v * (-1 if flip else 1)
                st["fwd"] *= (-1 if flip else 1)
                s = s2 + over * (1 if s2 == 0.0 else -1)
                tr = tr2
        st["s"], st["v"] = s, v
        self._move(c, tr, st, dt)

    def _move(self, c, tr, st, dt):
        R, x, t = tr.frame(st["s"])
        c.move_with_frame(R, x, t * st["v"], np.zeros(3))

    def _next(self, tr, end):
        """線路 tr の端（0：始まり・1：終わり）につながる線路：(線路, 乗る所の s, 向きが逆か) か None"""
        p = tr.ends()[end]
        for o in self.tracks():
            if o is tr:
                continue
            a, b = o.ends()
            if np.linalg.norm(a - p) < LINK:
                return o, 0.0, end == 0
            if np.linalg.norm(b - p) < LINK:
                return o, o.length, end == 1
        return None

    # ------------------------------------------------------------------ 電気
    @staticmethod
    def _batteries(c):
        out = []
        for d in c.devs:
            if d and (c.reg.device(d["def"]) or {}).get("type") == "battery":
                out.append(d.setdefault("state", {}))
        return out

    def _has_power(self, c):
        bats = self._batteries(c)
        if bats:
            return any(float(s.get("charge", 0.0)) > 1.0 for s in bats)
        return bool(getattr(c, "powered", False))

    def _use_power(self, c, joules):
        for s in self._batteries(c):
            ch = float(s.get("charge", 0.0))
            take = min(ch, joules)
            s["charge"] = ch - take
            joules -= take
            if joules <= 0:
                break

    def on_save(self, world):
        pass
