"""ミサイル・砲塔のビーム・爆発、燃料ポンプ（ワールド側）。

ミサイルは構築物ではなく軽い「飛ぶ物」（位置・速度だけ）。目標（構築物か地点）へ曲がりながら飛び、
構築物・地面・目標の地点に近づくと爆発する。爆発は近くのブロックを壊す（protected に入れた構築物は壊さない）。
迎撃砲塔は state["data"]["aim"]（向き）と ["fire"]（撃つ頼み）で動き、ビームの線に近いミサイルを壊す。

ほかの MOD から：
    w = world.addon("weapons")
    w.spawn_missile(位置, 速度, target=構築物の ID か None, point=地点か None, owner=撃った構築物の ID, speed=, turn=)
    w.protected.add(構築物の ID)       爆発で壊れない（シナリオの船など）
    w.listeners.append(fn)            fn(種類, 情報)：種類は "intercept"（迎撃）・"hit"（構築物に当たった）・
                                      "ground"（景色の箱に当たった）・"miss"（時間切れ）
"""
import itertools

import numpy as np

from main import log
from main.hooks import Addon

L = log.get("weapons")

MAX_MISSILES = 60
EFFECT_LIFE = 1.6


class WeaponsAddon(Addon):
    name = "weapons"

    def __init__(self):
        self.world = None
        self.missiles = []           # dict(id, p, v, speed, turn, target, point, owner, t0, life, blast)
        self.effects = []            # (種類, 位置, 終点か None, 時刻) 種類: beam / boom / spark / launch
        self.protected = set()
        self.ignore = set()             # ミサイルが当たらない構築物（遠くの艦隊の張りぼてなど）
        self.listeners = []
        self._ids = itertools.count(1)

    def on_load(self, world):
        self.world = world

    # ------------------------------------------------------------------
    # 撃つ
    # ------------------------------------------------------------------
    def spawn_missile(self, pos, vel, target=None, point=None, owner=None, speed=None, turn=1.5, life=25.0,
                      blast=1.2, tag=None):
        if len(self.missiles) >= MAX_MISSILES:
            return None
        v = np.asarray(vel, float)
        sp = float(speed if speed is not None else max(np.linalg.norm(v), 1.0))
        m = {"id": next(self._ids), "p": np.asarray(pos, float).copy(), "v": v.copy(), "speed": sp, "turn": float(turn),
             "target": target, "point": None if point is None else np.asarray(point, float), "owner": owner,
             "t0": self.world.time, "life": float(life), "blast": float(blast), "tag": tag}
        self.missiles.append(m)
        self.effects.append(("launch", m["p"].copy(), None, self.world.time))
        return m["id"]

    def _dev_world(self, c, i):
        """機器のワールドの位置と、向いている方向。"""
        from main.devices import world_axis
        p = c.voxel_center(i)
        try:
            f = world_axis(c, i, c.devs[i]["dir"], c.voxel_frames())
        except Exception:  # noqa: BLE001
            f = np.array([0.0, 1.0, 0.0])
        return p, f / max(np.linalg.norm(f), 1e-9)

    def launch_from(self, c, i, target_point):
        d = c.devs[i]
        if not d or not d["state"].get("active"):
            return False
        dd = c.reg.device(d["def"])
        prm = dd.get("params", {})
        data = d["state"].setdefault("data", {})
        if self.world.time - float(data.get("last", -99.0)) < float(prm.get("cooldown", 1.2)):
            return False
        data["last"] = self.world.time
        p, f = self._dev_world(c, i)
        sp = float(prm.get("speed", 55.0))
        self.spawn_missile(p + f * c.size * 1.2, f * sp * 0.6 + c.velocity(), point=target_point, owner=c.id,
                           speed=sp, turn=float(prm.get("turn", 1.5)), life=float(prm.get("life", 25.0)),
                           blast=float(prm.get("blast", 1.2)))
        return True

    def turret_ready(self, c, i):
        d = c.devs[i]
        dd = c.reg.device(d["def"])
        data = d["state"].get("data", {})
        return bool(d["state"].get("active")) and \
            self.world.time - float(data.get("shot", -99.0)) >= float(dd["params"].get("cooldown", 0.3))

    # ------------------------------------------------------------------
    # レーダー
    # ------------------------------------------------------------------
    def radar_contacts(self, c):
        """構築物 c のレーダー（電気が来ていて ON）に映るミサイル：[(位置, 速度, 距離)]（近い順）。自分の撃った物は除く。"""
        rng = 0.0
        for d in c.devs:
            if d and d["state"].get("active"):
                dd = c.reg.device(d["def"])
                if dd and dd.get("behavior") == "radar":
                    rng = max(rng, float(dd["params"].get("range", 1500.0)))
        if rng <= 0:
            return []
        com = c.center_of_mass()
        out = []
        for m in self.missiles:
            if m["owner"] == c.id:
                continue
            dist = float(np.linalg.norm(m["p"] - com))
            if dist <= rng:
                out.append((m["p"].tolist(), m["v"].tolist(), dist))
        return sorted(out, key=lambda x: x[2])

    # ------------------------------------------------------------------
    # 毎ティック
    # ------------------------------------------------------------------
    def before_physics(self, world, dt):
        if world.net is not None and getattr(world.net, "role", "") == "client":
            return set()
        self._turrets(world)
        self._pumps(world, dt)
        self._fly(world, dt)
        t = world.time
        self.effects = [e for e in self.effects if t - e[3] < EFFECT_LIFE]
        return set()

    def _pumps(self, world, dt):
        """燃料ポンプ（ON の間）：後ろの面の配管の区画のタンクから、前（向き）の面の区画のタンクへ流体を送る。"""
        from main import devices
        from main.construct import DIR_TUPLES, link_key
        for c in list(world.constructs.values()):
            if getattr(c, "_pump_ver", None) != c.version:
                c._pump_ver = c.version
                c._pumps = [i for i, d in enumerate(c.devs)
                            if d and (c.reg.device(d["def"]) or {}).get("behavior") == "pump"]
            if not c._pumps:
                continue
            if c.dirty or not hasattr(c, "_net"):
                devices.rebuild_networks(c)
            net = c._net
            for i in c._pumps:
                d = c.devs[i]
                if not d or not d["state"].get("enabled", True):
                    continue
                cell, dv = c.cells[i], DIR_TUPLES[d["dir"]]
                segs = []
                for sgn in (-1, 1):
                    nb = (cell[0] + sgn * dv[0], cell[1] + sgn * dv[1], cell[2] + sgn * dv[2])
                    j = c.index.get(nb)
                    ok = j is not None and (link_key(cell, nb) in c.links)
                    segs.append(net.pipe_of.get(j) if ok else None)
                src, dst = segs
                data = d["state"].setdefault("data", {})
                if src is None or dst is None or src == dst:
                    data["flow"] = 0.0
                    continue
                rate = float((c.reg.device(d["def"]) or {}).get("params", {}).get("rate", 60.0))
                moved = self._pump_move(c, net.pipes[src], net.pipes[dst], rate * dt)
                data["flow"] = moved / max(dt, 1e-6)
                data["total"] = float(data.get("total", 0.0)) + moved

    @staticmethod
    def _pump_move(c, src_ids, dst_ids, amount):
        from main import devices

        def tanks(ids):
            out = []
            for k in ids:
                dd = devices.dev_def(c, k)
                if dd and dd["type"] == "tank":
                    out.append((c.devs[k]["state"].setdefault("contents", {}), float(dd["params"].get("capacity", 0))))
            return out
        src, dst = tanks(src_ids), tanks(dst_ids)
        have = {}
        for cont, _ in src:
            for r, v in cont.items():
                if v > 1e-9 and devices.is_fluid(c.reg, r):
                    have[r] = have.get(r, 0.0) + v
        if not have:
            return 0.0
        res = "fuel" if "fuel" in have else max(have, key=have.get)
        free = sum(max(cap - sum(cont.values()), 0.0) for cont, cap in dst)
        n = min(amount, have[res], free)
        if n <= 1e-9:
            return 0.0
        left = n
        for cont, _ in src:
            t = min(cont.get(res, 0.0), left)
            cont[res] = cont.get(res, 0.0) - t
            left -= t
        left = n
        for cont, cap in dst:
            t = min(max(cap - sum(cont.values()), 0.0), left)
            cont[res] = cont.get(res, 0.0) + t
            left -= t
        return n

    def _turrets(self, world):
        t = world.time
        for c in list(world.constructs.values()):
            for i, d in enumerate(c.devs):
                if not d:
                    continue
                dd = c.reg.device(d["def"])
                if not dd or dd.get("behavior") != "turret":
                    continue
                data = d["state"].setdefault("data", {})
                req = data.get("fire")
                if req is None or t - float(req) > 0.2 or not self.turret_ready(c, i):
                    continue
                data.pop("fire", None)
                data["shot"] = t
                prm = dd["params"]
                p, f = self._dev_world(c, i)
                aim = np.asarray(data.get("aim") or f, float)
                n = float(np.linalg.norm(aim))
                aim = aim / n if n > 1e-6 else f
                rng = float(prm.get("range", 450.0))
                start = p + aim * c.size * 0.8
                end = start + aim * rng
                hit_r = float(prm.get("hit_radius", 3.0))
                best = None
                for m in self.missiles:
                    if m["owner"] == c.id:
                        continue
                    rel = m["p"] - start
                    s = float(rel @ aim)
                    if not 0 <= s <= rng:
                        continue
                    if float(np.linalg.norm(rel - aim * s)) <= hit_r and (best is None or s < best[0]):
                        best = (s, m)
                if best is not None:
                    end = start + aim * best[0]
                    self.missiles.remove(best[1])
                    self.effects.append(("boom", best[1]["p"].copy(), None, t))
                    self._tell("intercept", {"by": c.id, "missile": best[1]})
                self.effects.append(("beam", start, end, t))

    def _fly(self, world, dt):
        t = world.time
        keep = []
        for m in self.missiles:
            if t - m["t0"] > m["life"]:
                self.effects.append(("spark", m["p"].copy(), None, t))
                self._tell("miss", {"missile": m})
                continue
            goal = None
            if m["target"] is not None:
                tc = world.constructs.get(m["target"])
                if tc is not None and tc.n_vox:
                    goal = tc.center_of_mass()
            if goal is None and m["point"] is not None:
                goal = m["point"]
            v = m["v"]
            sp = float(np.linalg.norm(v))
            want = m["speed"]
            fwd = v / sp if sp > 1e-6 else np.array([0.0, 1.0, 0.0])
            if goal is not None:
                to = goal - m["p"]
                L_ = float(np.linalg.norm(to))
                if L_ > 1e-6:
                    tgt = to / L_
                    ang = float(np.arccos(np.clip(fwd @ tgt, -1, 1)))
                    step = m["turn"] * dt
                    if ang > 1e-6:
                        k = min(1.0, step / ang)
                        fwd = fwd * (1 - k) + tgt * k
                        fwd /= np.linalg.norm(fwd)
            sp = sp + (want - sp) * min(1.0, dt * 1.5)
            m["v"] = fwd * sp
            m["p"] = m["p"] + m["v"] * dt
            if self._impact(world, m, goal):
                continue
            keep.append(m)
        self.missiles = keep

    def _impact(self, world, m, goal):
        """当たったら爆発して True。"""
        p = m["p"]
        t = world.time
        for pl in world.planets:
            if np.linalg.norm(p - pl.center) < pl.bound and float(pl.sdf(p[None])[0]) < 0.3:
                self._explode(world, m, None)
                return True
        armed = t - m["t0"] > 0.4
        if armed and world.push_sphere(p, 0.6):         # 景色の箱（街・甲板など）
            self._explode(world, m, None)
            self._tell("ground", {"missile": m})
            return True
        near = world.constructs_near(p, 3.0) if hasattr(world, "constructs_near") else world.constructs.values()
        for c in near:
            if not c.n_vox or (c.id == m["owner"] and not armed) or c.id in self.ignore:
                continue
            P = c.centers_cached() if hasattr(c, "centers_cached") else c.voxel_centers()
            dd = np.linalg.norm(P - p, axis=1)
            if float(dd.min()) < c.size * 0.9 + 0.3:
                self._explode(world, m, c)
                return True
        if m["point"] is not None and m["target"] is None and float(np.linalg.norm(p - m["point"])) < 2.0:
            self._explode(world, m, None)
            return True
        return False

    def _explode(self, world, m, c):
        t = world.time
        self.effects.append(("boom", m["p"].copy(), None, t))
        if c is not None:
            self._tell("hit", {"construct": c.id, "missile": m})
        if c is None or c.id in self.protected or m["blast"] <= 0:
            return
        P = c.voxel_centers()
        idx = [int(k) for k in np.nonzero(np.linalg.norm(P - m["p"], axis=1) < m["blast"] + c.size * 0.5)[0]]
        if not idx:
            return
        for pl in world.all_players():
            if pl.seated and pl.seated[0] == c.id and any(tuple(c.cells[k]) == tuple(pl.seated[1]) for k in idx):
                pl.seated = None
        c.remove_voxels(idx)
        if not c.n_vox:
            del world.constructs[c.id]
            return
        c.wake()
        for part in c.split():
            world.add_construct(part)

    def _tell(self, kind, info):
        for fn in list(self.listeners):
            try:
                fn(kind, info)
            except Exception:  # noqa: BLE001
                L.exception("ミサイルの知らせの処理に失敗")

    # ------------------------------------------------------------------
    # マルチプレイ
    # ------------------------------------------------------------------
    def net_state(self, world):
        if not self.missiles and not self.effects:
            return {"m": [], "e": []}
        return {"m": [[m["id"]] + np.round(m["p"], 2).tolist() + np.round(m["v"], 2).tolist() for m in self.missiles],
                "e": [[k, np.round(p, 2).tolist(), None if q is None else np.round(q, 2).tolist(), round(t, 2)]
                      for k, p, q, t in self.effects]}

    def net_apply(self, world, state):
        if not isinstance(state, dict):
            return
        self.missiles = [{"id": r[0], "p": np.array(r[1:4], float), "v": np.array(r[4:7], float), "owner": None}
                         for r in state.get("m", [])]
        self.effects = [(k, np.asarray(p, float), None if q is None else np.asarray(q, float), t)
                        for k, p, q, t in state.get("e", [])]

    def net_client_tick(self, world, dt):
        for m in self.missiles:
            m["p"] = m["p"] + m["v"] * dt
