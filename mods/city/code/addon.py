"""街パック（ワールド側）：サイバーパンクの街・経済・政治をコアにつなぐ。

コアからは hooks.Addon の口だけで呼ばれる。画面側は query 用のメソッド（下の「問い合わせ」）と
client.py の CityClient を通して使う。
"""
import numpy as np

from main.geom import box_sdf
from main.hooks import Addon


class CityAddon(Addon):
    name = "city"
    settings_keys = ("city_state", "economy", "politics")

    def __init__(self):
        from .cities import NoCities
        self.world = None
        self.cities = NoCities()
        self.economy = None
        self.politics = None

    # ------------------------------------------------------------------
    # 読み込み・保存
    # ------------------------------------------------------------------
    def on_load(self, world):
        from .cities import CityManager
        from .citylife import Economy
        from .politics import Politics
        self.world = world
        self.cities = CityManager(world)
        if self.cities.enabled:
            self.cities.clear_overlaps(world.constructs.values())
        self.economy = Economy(world, self)
        if self.cities.enabled:
            self.economy.give_starter()
        self.politics = Politics(world, self)

    def on_save(self, world):
        if self.cities.enabled:
            world.settings["city_state"] = self.cities.to_state()

    # ------------------------------------------------------------------
    # 1ティックごと
    # ------------------------------------------------------------------
    def before_physics(self, world, dt):
        if self.politics is not None:
            self.politics.update(dt)
        # 軌道上の街：くっついた構築物の推進機の力を街に伝えて、街ごと軌道を進める
        anchored = self._anchored(world)
        if self.cities.enabled:
            for cid, lst in anchored.items():
                city = self.cities.by_id[cid]
                for c in lst:
                    c.hold_in_frame(city.R, city.x, city.v, city.w)     # 街が動く前の位置関係で固定する
                    if city.kind == "orbit" and np.any(c.force):
                        city.force += c.force.sum(0)
                        city.torque += np.cross(c.pos - city.x, c.force).sum(0)
            self.cities.step(dt, anchored)
            for cid, lst in anchored.items():
                city = self.cities.by_id[cid]
                for c in lst:
                    c.move_with_frame(city.R, city.x, city.v, city.w)
        return {c.id for lst in anchored.values() for c in lst}

    def _anchored(self, world):
        """軌道上の街に固定された構築物：{街ID: [構築物]}"""
        out = {}
        for c in world.constructs.values():
            a = c.anchor
            if not a:
                continue
            if not self.cities.enabled or a["to"] not in self.cities.by_id:
                c.anchor = None
                c.release_frame()
                continue
            out.setdefault(a["to"], []).append(c)
        return out

    def colliders(self, world, lo, hi, margin):
        if not self.cities.enabled or not self.cities.cities:
            return []
        return self.cities.boxes_for(lo, hi, margin)

    def after_physics(self, world, cons):
        """建物の損傷と、動いている街への反動をまとめて反映する。"""
        if not self.cities.enabled:
            return
        mine = set(map(id, self.cities.cities))
        for c in cons:
            for city, best, J, pts in c.ext_events:
                if id(city) not in mine:
                    continue
                if best:
                    before = city.alive()
                    for part, v in best.items():
                        city.hit(part, v)
                    if before and not city.alive():
                        world.message(f"{city.name} は壊滅しました")
                if J is not None and city.is_moving():
                    city.impulse += J.sum(0)
                    city.ang_impulse += np.cross(pts - city.x, J).sum(0)

    # ------------------------------------------------------------------
    # 問い合わせ・操作（コアから）
    # ------------------------------------------------------------------
    def raycast(self, world, o, dvec, max_t):
        if not self.cities.enabled:
            return None
        r = self.cities.raycast(o, dvec, max_t)
        if r and r["kind"] == "city":
            r["placeable"] = True
        return r

    def push_sphere(self, world, center, r):
        if not self.cities.enabled:
            return []
        return self.cities.push_sphere(center, r)

    def interact(self, world, c, i):
        return self.anchor_toggle(c)

    def anchor_toggle(self, c):
        """E：軌道上の街に固定する／切り離す。"""
        if c.anchor:
            city = self.cities.by_id.get(c.anchor["to"])
            c.anchor = None
            c.release_frame()
            c.wake()
            return f"{c.name} を {city.name if city else '街'} から切り離しました"
        if not self.cities.enabled:
            return None
        K = c.contact_cores()
        K = K[~np.isnan(K).any(1)]
        for city, C, A, E, idx in self.cities.boxes_for(c.pos.min(0), c.pos.max(0), 1.0):
            if city.kind not in ("orbit", "wreck") or not len(K):
                continue
            d, _, _ = box_sdf(K, C, A, E)
            if float(d.min()) < c.size * 0.5 + 0.35:
                c.anchor = {"to": city.id}
                return f"{c.name} を {city.name} に固定しました（推進機で街ごと動かせます。もう一度 E で切り離し）"
        return None

    def on_placed(self, world, hit, c):
        if hit.get("kind") != "city":
            return
        city = hit["city"]
        c.vel[:] = city.point_velocity(c.pos)
        if city.kind in ("orbit", "wreck"):
            c.anchor = {"to": city.id}      # ステーションに直接置いたものは街に固定

    # ------------------------------------------------------------------
    # マルチプレイ（LAN）
    # ------------------------------------------------------------------
    def on_player_join(self, world, player):
        if self.economy is not None and self.cities.enabled:
            self.economy.give_starter()             # world.player は参加した人

    def net_state(self, world):
        """参加者へ：街の状態（動くステーションの位置・建物の壊れ具合）と、経済・政治の状態。"""
        if not self.cities.enabled:
            return None
        return {"cities": self.cities.to_state(), "eco": world.settings.get("economy"),
                "pol": world.settings.get("politics")}

    def net_apply(self, world, st):
        if not self.cities.enabled:
            return
        for cid, d in (st.get("cities") or {}).items():
            c = self.cities.by_id.get(cid)
            if c is None:
                continue
            b = d.get("b") or {}
            want = [0] * len(c.state)
            for k, v in b.items():
                if int(k) < len(want):
                    want[int(k)] = int(v)
            if list(map(int, c.state)) != want:
                c.load_state({"b": b})              # 壊れ具合が変わった（描き直す）
            if "x" in d:
                c.kind = d.get("kind", c.kind)
                c.x, c.v = np.asarray(d["x"], float), np.asarray(d["v"], float)
                c.R, c.w = np.asarray(d["R"], float), np.asarray(d["w"], float)
        for key, new in (("economy", st.get("eco")), ("politics", st.get("pol"))):
            cur = world.settings.get(key)
            if isinstance(cur, dict) and isinstance(new, dict):
                for k, v in new.items():            # 中身を入れ替える（経済・政治は同じ辞書を持ち続けている）
                    if isinstance(cur.get(k), dict) and isinstance(v, dict):
                        cur[k].clear()
                        cur[k].update(v)
                    elif isinstance(cur.get(k), list) and isinstance(v, list):
                        cur[k][:] = v
                    else:
                        cur[k] = v

    def net_client_tick(self, world, dt):
        """参加者：次の状態が届くまで、軌道上の街を速度のまま進める。"""
        if not self.cities.enabled:
            return
        for c in self.cities.cities:
            if c.kind != "orbit":
                continue
            c.x = c.x + c.v * dt
            wn = float(np.linalg.norm(c.w))
            if wn > 1e-12:
                from .cities import _rot
                c.R = _rot(c.w, wn * dt) @ c.R

    def net_ref_of(self, obj):
        from .cities import City
        if isinstance(obj, City):
            return ("city", obj.id)
        if isinstance(obj, dict) and "id" in obj and "bld" in obj:
            return ("home", obj["id"])
        return None

    def net_ref(self, world, kind, id_):
        if kind == "city":
            return self.cities.by_id.get(id_)
        if kind == "home":
            for c in self.cities.cities:
                for h in c.homes:
                    if h["id"] == id_:
                        return h
        return None

    # ------------------------------------------------------------------
    # 問い合わせ（画面側から。街パックの中身を直接触らずに使う）
    # ------------------------------------------------------------------
    @property
    def enabled(self):
        return bool(self.cities.enabled)

    def city_list(self):
        return list(self.cities.cities)

    def marker_target(self, p):
        return self.cities.marker_target(p)

    def home_id(self):
        return self.economy.home_id() if self.economy else None

    def home(self):
        """自宅：(街, 家) か (None, None)"""
        hid = self.home_id()
        if hid:
            for c in self.cities.cities:
                for h in c.homes:
                    if h["id"] == hid:
                        return c, h
        return None, None

    def can_use_home(self, hid):
        return bool(self.economy and self.economy.can_use(hid))

    def set_home(self, hid):
        self.economy.set_home(hid)
