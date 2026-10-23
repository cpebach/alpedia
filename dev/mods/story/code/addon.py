"""ストーリー MOD（ワールド側）：舞台の景色、構築物を最初に作る、章と目標の進み方、無線のセリフ。

状態は world.settings["story"] に入れてセーブする：
    built  構築物・地面の跡・スクリプトを作ったか
    ch     いまの章（script.CHAPTERS の番号）　goals  済んだ目標 {ID: True}
    ids    構築物の ID {役: ID}　log  無線のセリフ [[番号, 話す人, 文, 出す時刻]]
    n      セリフの通し番号　dig  掘った回数　look  見回した角度　rod_t  棒がまっすぐな時間
    events 一度だけのセリフ（済んだもの）　lights  基地の明かり　met  ミオに会った　done  クリア
マルチプレイでは、ホストが進み方を決めて参加者へ送る（net_state）。
"""
import json

import numpy as np

from main import log
from main.construct import Construct
from main.geom import box_sdf, ray_boxes
from main.hooks import Addon

from . import script, sites

L = log.get("story")

CHECK_EVERY = 0.2          # 目標を調べる間隔（秒・ワールド時刻）
HINT_AFTER = 75.0          # 目標が済まないとき、この秒数でヒントのセリフ


def _say_time(text):
    return 1.6 + len(text) * 0.075


class StoryAddon(Addon):
    name = "story"
    settings_keys = ("story",)

    def __init__(self):
        self.world = None
        self.active = False
        self.scenes = []
        self._t = 0.0
        self._look_prev = {}

    # ------------------------------------------------------------------
    # 読み込み・保存
    # ------------------------------------------------------------------
    def on_load(self, world):
        self.world = world
        st = world.settings.get("story")
        if not isinstance(st, dict):
            return                       # ストーリーのワールドではない（MOD だけ入れた）
        self.terra = next((p for p in world.planets if p.defn["id"] == "terra"), None)
        self.luna = next((p for p in world.planets if p.defn["id"] == "luna"), None)
        if self.terra is None or self.luna is None:
            L.warning("ストーリーにはテラと月ルナが必要です")
            return
        self.active = True
        self.map = sites.terra_map(self.terra, sites.terra_craters(sites.terra_map(self.terra)))
        self.lmap = sites.luna_map(self.luna, self.terra.center)
        self.scenes = [sites.build_terra(self.terra), sites.build_luna(self.luna, self.terra)]
        self.points = {}
        for s in self.scenes:
            self.points.update(s.points)
        for k, v in {"built": False, "ch": 0, "goals": {}, "ids": {}, "log": [], "n": 0, "dig": 0, "look": 0.0,
                     "rod_t": 0.0, "events": [], "lights": False, "met": False, "done": False,
                     "ch_t": 0.0}.items():
            st.setdefault(k, v)
        self.st = st
        if not st["built"] and (world.net is None or getattr(world.net, "role", "") != "client"):
            self.build_world(world)

    def on_save(self, world):
        pass                             # world.settings["story"] をそのまま使っている

    # ------------------------------------------------------------------
    # 最初に1回：地面の跡・構築物・スクリプト・プレイヤーの位置
    # ------------------------------------------------------------------
    def build_world(self, world):
        from main import blocks as B
        from main import tutorial_drone as drone
        from main import tutorial_rod as rod
        from main.content import SCRIPT_README
        st = self.st
        reg = world.registry
        self.terra.craters += sites.terra_craters(sites.terra_map(self.terra))
        self.terra.version += 1
        size = float(world.settings.get("voxel_size", 0.5))
        ids = st["ids"]

        def put(role, name, vox, place, m=None):
            c = self._spawn(vox, place, m or self.map, name)
            ids[role] = c.id
            return c

        P = sites.PLACES
        put("pod", "脱出ポッド", sites.pod_voxels(), P["pod"])
        put("beacon", "救難ビーコン", sites.beacon_voxels(), P["beacon"])
        put("rocks", "落石", sites.rock_voxels(5, 13, 6, 3), (-3.25, 27.0, 0.0, 0.05))
        put("power", "観測基地の電源", sites.power_voxels(), P["power"])
        put("model", "お手本の装置", rod.apparatus("お手本", "otehon.blocks.json", "otehon.py"), P["model"])
        put("task", "課題の装置", rod.apparatus("課題", "kadai.blocks.json", "kadai.py"), P["task"])
        put("console", "ドローンの操縦台", drone.remote_voxels(), P["console"])
        put("drone", "ドローン", drone.drone_voxels(), P["drone"])
        put("gate", "格納庫の遮断機", sites.gate_voxels(), P["gate"])
        put("bike", "ホバーバイク", sites.bike_voxels(), P["bike"])
        put("ship", "ルミナス号", sites.ship_voxels(broken=True), P["ship"])
        put("sign_base", "看板", sites.sign_voxels("観測基地 ALP-7\n（無人・電源停止中）", width=2.4), P["sign_base"])
        put("sign_luna", "看板", sites.sign_voxels("ようこそ\nルナ・ポートへ", width=2.6, color=(0.6, 1.0, 1.0)),
            (-4.0, -14.0, 180.0, 0.05), self.lmap)
        st["rocks0"] = len(world.constructs[ids["rocks"]].cells)
        # スクリプト
        sd = world.dir / "scripts"
        sd.mkdir(parents=True, exist_ok=True)
        pad = self.points["pad"]
        up = self.points["pad_up"]
        files = {
            "beacon.py": sites.BEACON_SCRIPT, "power.py": sites.POWER_SCRIPT,
            "sway.py": rod.SWAY_SCRIPT,
            "README.txt": SCRIPT_README,
        }
        for fname, text in files.items():
            (sd / fname).write_text(text, encoding="utf-8")
        from . import programs
        progs = [("kadai", rod.starter_program()), ("otehon", rod.solution_program())]
        progs += [(name, make()) for name, make in programs.PROGRAMS.items()]   # 乗り物（ブロックプログラム）
        progs.append(("moon", programs.moon_program(pad, up)))                  # ルミナス号の航法
        for role, base in (("bike", "bike"), ("drone", "drone"), ("console", "drone_remote")):
            c, i = self.dev(role, "computer")
            if i is not None:
                c.devs[i]["state"].update(script=f"{base}.py", blocks=f"{base}.blocks.json")
        for base, prog in progs:
            B.save(sd / f"{base}.blocks.json", prog)
            (sd / f"{base}.py").write_text(B.to_python(prog, f"{base}.blocks.json"), encoding="utf-8")
        # プレイヤー：ポッドの手前、奥（+Z）を向いて
        p, up = self.map.ground(-1.5, -5.0, 1.2)
        pl = world.local_player
        pl.pos, pl.vel, pl.up = p, np.zeros(3), up
        pl.hotbar = [("material", "steel"), ("material", "hull_white"), ("material", "rock"), ("material", "glass"),
                     ("device", "thruster"), ("device", "tank"), ("material", "hull_red"), ("device", "computer"),
                     ("device", "seat")]
        st["built"] = True
        st["ch_t"] = world.time
        self._enter(0, intro=True)
        L.info("ストーリーのワールドを作りました（構築物 %d）", len(ids))

    def _spawn(self, vox, place, m, name, id_=None):
        """地図の場所に構築物を置く。地面（くぼ地の丸みなど）にめり込まないように、角がすべて地面より上になるまで持ち上げる。"""
        w = self.world
        x, z, yaw, lift = place
        p, _ = m.ground(x, z, lift)
        R = m.frame(x, z, yaw)
        c = Construct.from_voxels(w.registry, vox, p, R, size=float(w.settings.get("voxel_size", 0.5)), name=name,
                                  id_=id_)
        for _ in range(8):                 # 地面と景色の箱（床・台）の上に出るまで持ち上げる
            dmin = float(m.pl.sdf(c.pos).min())
            for sc in self.scenes:
                if np.linalg.norm(c.pos[0] - sc.center) < sc.radius + 10:
                    d, _, _ = box_sdf(c.pos, sc.C[sc.col], sc.A[sc.col], sc.E[sc.col])
                    dmin = min(dmin, float(d.min()))
            if dmin >= 0.03:
                break
            c.pos += R[:, 1] * (0.03 - dmin + 0.01)
        w.add_construct(c)
        return c

    # ------------------------------------------------------------------
    # 構築物・場所
    # ------------------------------------------------------------------
    def con(self, role):
        w = self.world
        cid = self.st["ids"].get(role)
        c = w.constructs.get(cid) if cid else None
        return c if c is not None and c.n_vox else None

    def dev(self, role, def_id=None, name=None):
        c = self.con(role)
        if c is None:
            return None, None
        for i, d in enumerate(c.devs):
            if d and (def_id is None or d["def"] == def_id) and (name is None or d["name"] == name):
                return c, i
        return c, None

    def where(self, key):
        """目印の場所（構築物なら今の位置）。"""
        if key is None:
            return None
        if key == "navi":
            c, i = self.dev("ship", "computer")
            return c.pos[8 * i:8 * i + 8].mean(0) if c is not None and i is not None else None
        if key in ("valve", "engine"):
            c, i = self.dev("power", "thruster" if key == "engine" else key)
            return c.pos[8 * i:8 * i + 8].mean(0) if c is not None and i is not None else None
        if key == "gate":
            c, i = self.dev("gate", "actuator")
            return c.pos[8 * i:8 * i + 8].mean(0) if c is not None and i is not None else None
        if key in self.points:
            return self.points[key]
        c = self.con(key)
        return c.center_of_mass() if c is not None else None

    def map_xz(self, p):
        """テラの地図の座標 (x, z)。"""
        d = np.asarray(p, float) - self.terra.center
        d /= np.linalg.norm(d)
        m = self.map
        k = m.pl.radius / max(float(d @ m.up0), 1e-6)
        return float(d @ m.right0) * k, float(d @ m.fwd0) * k

    def _players(self):
        return self.world.all_players()

    def _near(self, p, r):
        return p is not None and any(np.linalg.norm(pl.pos - p) < r for pl in self._players())

    def _seated(self, role):
        c = self.con(role)
        return c is not None and any(pl.seated and pl.seated[0] == c.id for pl in self._players())

    # ------------------------------------------------------------------
    # 目標の判定（目標の ID → 済んだら True）
    # ------------------------------------------------------------------
    def check_look(self):
        return self.st["look"] >= 150.0

    def check_walk(self):
        return self._near(self.where("beacon"), 3.6)

    def check_beacon(self):
        c, i = self.dev("beacon", "computer")
        return i is not None and bool(c.devs[i]["state"].get("running"))

    def check_rocks(self):
        """谷に、幅 1 m・高さ 1.9 m の通り道があるか（落石の間を抜けたらそれでも可）。"""
        if any(self.map_xz(p.pos)[1] > 33.0 and abs(self.map_xz(p.pos)[0]) < 20 for p in self._players()):
            return True
        bins = np.zeros(28, bool)                     # x: -3.5〜3.5 を 0.25 m ごと
        for c in self.world.constructs.values():
            if not c.n_vox:
                continue
            P = c.voxel_centers()
            for p in P:
                x, z = self.map_xz(p)
                if not (24.0 < z < 32.0 and abs(x) < 3.6):
                    continue
                g, up = self.map.ground(x, z)
                h = float((p - g) @ up)
                if h > 1.9:
                    continue
                lo, hi = int((x - 0.3 + 3.5) / 0.25), int((x + 0.3 + 3.5) / 0.25)
                bins[max(lo, 0):max(min(hi + 1, 28), 0)] = True
        free = 0
        for b in bins[1:-1]:
            free = 0 if b else free + 1
            if free >= 4:
                return True
        return False

    def check_dig(self):
        return self.st["dig"] >= 3

    def check_ledge(self):
        return any(self.map_xz(p.pos)[1] > sites.Z_LEDGE + 3.0 for p in self._players())

    def check_valve(self):
        c, i = self.dev("power", "valve")
        return i is not None and float(c.devs[i]["state"].get("open", 0.0)) > 0

    def _engines_lit(self):
        c = self.con("power")
        if c is None:
            return 0
        return sum(1 for d in c.devs if d and d["def"] == "thruster" and d["state"].get("lit"))

    def check_engine(self):
        return self._engines_lit() >= 2

    def check_lights(self):
        c = self.con("power")
        if c is None or not (c.status or {}).get("production", 0) > 0:
            return False
        self.st["lights"] = True
        return True

    def check_model(self):
        return self._near(self.where("model"), 4.5)

    def check_rod(self):
        return self.st["rod_t"] >= 5.0

    def _rod_angle(self):
        from main import actuator
        c, i = self.dev("task", "actuator", "姿勢モーター")
        if i is None:
            return None
        return actuator.output_angle(c, i, self.world)

    def check_console(self):
        return self._seated("console")

    def check_ring(self):
        c = self.con("drone")
        return c is not None and float(np.linalg.norm(c.center_of_mass() - self.points["ring"])) < sites.RING_R * 0.95

    def check_gate(self):
        c, i = self.dev("gate", "actuator")
        if i is None:
            return True                     # 遮断機を壊した（それでも通れる）
        a = float(c.devs[i]["state"].get("angle", 0.0)) % 360.0
        return 50.0 < a < 130.0 or 230.0 < a < 310.0

    def check_ride(self):
        return self._seated("bike")

    def check_reach(self):
        return self._near(self.where("ship"), 14.0)

    def _ship_progress(self):
        c = self.con("ship")
        if c is None:
            return 0, 0, 0
        holes = sum(tuple(h) in c.index for h in sites.SHIP_HOLES)
        thr = 0
        for t in sites.SHIP_THRUSTERS:
            i = c.index.get(tuple(t))
            if i is not None and c.devs[i] and c.devs[i]["def"] == "thruster":
                thr += 1
        tanks = [d for d in c.devs if d and d["def"] == "tank"]
        full = sum(d["state"].get("contents", {}).get("fuel", 0) >= 790 for d in tanks)
        return holes, thr, (full, len(tanks))

    def check_hull(self):
        return self._ship_progress()[0] >= len(sites.SHIP_HOLES)

    def check_thrusters(self):
        return self._ship_progress()[1] >= len(sites.SHIP_THRUSTERS)

    def check_fuel(self):
        f = self._ship_progress()[2]
        return bool(f) and f[1] > 0 and f[0] >= f[1]

    def check_seat(self):
        return self._seated("ship")

    def check_launch(self):
        c, i = self.dev("ship", "computer")
        return i is not None and bool(c.devs[i]["state"].get("running")) and c.devs[i]["state"].get("script") == "moon.py"

    def check_land(self):
        c = self.con("ship")
        if c is None:
            return False
        pad = self.points["pad"]
        u = self.points["pad_up"]
        rel = c.center_of_mass() - pad
        h = float(rel @ u)
        side = float(np.linalg.norm(rel - u * h))
        return h < 2.0 and side < sites.PAD_HALF + 1.5 and float(np.linalg.norm(c.velocity())) < 0.4

    def check_mio(self):
        return bool(self.st["met"])

    def progress(self, gid):
        """目標の進み具合の文（数があるもの）。"""
        if gid == "hull":
            return f"{self._ship_progress()[0]}/{len(sites.SHIP_HOLES)}"
        if gid == "thrusters":
            return f"{self._ship_progress()[1]}/{len(sites.SHIP_THRUSTERS)}"
        if gid == "fuel":
            f = self._ship_progress()[2]
            return f"{f[0]}/{f[1]}" if f else ""
        if gid == "engine":
            return f"{self._engines_lit()}/2"
        if gid == "dig":
            return f"{min(self.st['dig'], 3)}/3"
        if gid == "rod":
            return f"{min(self.st['rod_t'], 5.0):.0f}/5 秒"
        return ""

    # ------------------------------------------------------------------
    # 進み方
    # ------------------------------------------------------------------
    def say(self, who, text, gap=0.4):
        st = self.st
        t = self.world.time
        last = st["log"][-1] if st["log"] else None
        due = t if last is None else max(t, last[3] + _say_time(last[2]) + gap)
        st["n"] += 1
        st["log"].append([st["n"], who, text, due])
        del st["log"][:-40]

    def _enter(self, ch, intro=True):
        st = self.st
        st["ch"] = ch
        st["goals"] = {}
        st["ch_t"] = self.world.time
        st["hinted"] = []
        st["rod_t"] = 0.0
        if ch >= len(script.CHAPTERS):
            st["done"] = True
            return
        c = script.CHAPTERS[ch]
        self.say(script.SYS, f"―― {c['title']} ――")
        if intro:
            for who, text in c["intro"]:
                self.say(who, text)

    def _finish_chapter(self):
        st = self.st
        c = script.CHAPTERS[st["ch"]]
        for who, text in c["outro"]:
            self.say(who, text)
        if c["id"] == "repair":
            self._give_program()
        if c["id"] == "meet":
            st["done"] = True
        self._enter(st["ch"] + 1)

    def _give_program(self):
        c, i = self.dev("ship", "computer")
        if i is not None:
            c.devs[i]["state"]["script"] = "moon.py"
            c.devs[i]["state"]["blocks"] = "moon.blocks.json"
            c.devs[i]["state"].pop("error", None)

    def before_physics(self, world, dt):
        if not self.active or not self.st.get("built"):
            return set()
        self._watch_look()
        self._watch_rod(dt)
        self._t += dt
        if self._t < CHECK_EVERY:
            return set()
        self._t = 0.0
        st = self.st
        if st["done"] or st["ch"] >= len(script.CHAPTERS):
            return set()
        c = script.CHAPTERS[st["ch"]]
        for gid, text, _ in c["goals"]:
            if st["goals"].get(gid):
                continue
            try:
                ok = getattr(self, "check_" + gid)()
            except Exception:  # noqa: BLE001
                L.exception("目標 %s の判定に失敗", gid)
                ok = False
            if ok:
                st["goals"][gid] = True
                world.message(f"◎ 達成：{text.split('（')[0]}")
        if all(st["goals"].get(g) for g, _, _ in c["goals"]):
            self._finish_chapter()
            return set()
        # ヒント
        for gid, line in (c.get("hints") or {}).items():
            if not st["goals"].get(gid) and gid not in st.get("hinted", []) and world.time - st["ch_t"] > HINT_AFTER:
                st.setdefault("hinted", []).append(gid)
                self.say(*line)
        # 月へ向かう間のセリフ
        if c["id"] == "launch":
            self._flight_events(c)
        return set()

    def _flight_events(self, c):
        ship = self.con("ship")
        if ship is None:
            return
        p = ship.center_of_mass()
        st = self.st
        dt_ = float(np.linalg.norm(p - self.terra.center)) - self.terra.radius
        dl = float(np.linalg.norm(p - self.luna.center))
        dp = float(np.linalg.norm(p - self.points["pad"]))
        conds = {"lift": dt_ > 25 and self.check_launch(), "space": dt_ > 400,
                 "half": dl < 2300, "near": dp < 350}
        for k, (who, text) in c.get("events", {}).items():
            if k not in st["events"] and conds.get(k):
                st["events"].append(k)
                self.say(who, text)

    def _watch_look(self):
        st = self.st
        if st["ch"] != 0 or st["goals"].get("look"):
            return
        for p in self._players():
            f = np.asarray(p.look_fwd, float)
            f = f - p.up * float(f @ p.up)
            n = float(np.linalg.norm(f))
            if n < 1e-6:
                continue
            f /= n
            prev = self._look_prev.get(id(p))
            if prev is not None:
                st["look"] += float(np.degrees(np.arccos(np.clip(f @ prev, -1, 1))))
            self._look_prev[id(p)] = f

    def _watch_rod(self, dt):
        st = self.st
        if script.CHAPTERS[min(st["ch"], len(script.CHAPTERS) - 1)]["id"] != "control" or st["goals"].get("rod"):
            return
        a = self._rod_angle()
        c, i = self.dev("task", "computer", "課題")
        running = i is not None and c.devs[i]["state"].get("running")
        if a is not None and running and abs(a) < 5.0:
            st["rod_t"] += dt
        else:
            st["rod_t"] = 0.0

    # ------------------------------------------------------------------
    # 操作（画面から addon_call で呼ぶ）
    # ------------------------------------------------------------------
    def talk(self, tag):
        """景色の触れる物（ミオの受付）を E。"""
        if not self.active:
            return None
        if tag == "mio":
            st = self.st
            if script.CHAPTERS[min(st["ch"], len(script.CHAPTERS) - 1)]["id"] == "meet" and not st["met"]:
                st["met"] = True
                return "ミオ「来てくれたんだね！」"
            if st["done"]:
                self.say(script.MIO, "またいつでも来てね。次はどこへ行くの？")
                return "ミオと話しました"
            self.say(script.MIO, "ここは管制塔の受付。ルミナス号で来るのを待ってるね！")
            return "ミオと話しました"
        return None

    def jump(self, ch):
        """章の始めへ（やり直し・比べるとき用）。それより前の章で済ませることは済ませた状態にする。"""
        if not self.active:
            return
        ch = max(0, min(int(ch), len(script.CHAPTERS) - 1))
        ids = [c["id"] for c in script.CHAPTERS]
        w = self.world
        client = w.net is not None and getattr(w.net, "role", "") == "client"
        if client:                         # 参加者の PC では自分の体を動かすだけ（世界はホストが変える）
            self._teleport(ids[ch])
            return
        if ch > ids.index("power"):
            for d in self.con("power").devs if self.con("power") else []:
                if d and d["def"] == "valve":
                    d["state"]["open"] = float(d["state"].get("set", 0.15) or 0.15)
                if d and d["def"] == "thruster":
                    d["state"]["ignite"] = True
            self.st["lights"] = True
        if ch >= ids.index("launch"):
            self._rebuild_ship()
        if ch >= ids.index("meet"):
            self.st["met"] = False
        self.st["done"] = False
        self.st["events"] = []
        self.st["log"] = []
        self._teleport(ids[ch])
        self._enter(ch)
        if ids[ch] == "meet":
            self.say(script.MIO, "管制塔の下の受付まで来てね。")
        w.message(f"「{script.CHAPTERS[ch]['title']}」から始めます")

    def _teleport(self, cid):
        spots = {"wake": (self.map, -1.5, -5.0), "valley": (self.map, 0.0, 14.0), "power": (self.map, -2.0, 46.0),
                 "control": (self.map, -12.5, 59.0), "drone": (self.map, 6.0, 63.0), "bike": (self.map, -9.0, 99.0),
                 "repair": (self.map, 0.0, 141.0), "launch": (self.map, -2.0, 144.0),
                 "meet": (self.lmap, 0.0, -PADM)}
        m, x, z = spots[cid]
        p, up = m.ground(x, z, 1.2)
        pl = self.world.player
        pl.seated = None
        pl.pos, pl.vel, pl.up = p, np.zeros(3), up

    def _rebuild_ship(self):
        """ルミナス号を、直して燃料を入れた状態に作り直す（同じ ID・今の場所の地面に立てて）。"""
        w = self.world
        old = self.con("ship")
        cid = self.st["ids"].get("ship")
        if old is not None:
            for pl in self._players():
                if pl.seated and pl.seated[0] == old.id:
                    pl.seated = None
            del w.constructs[old.id]
        c = self._spawn(sites.ship_voxels(broken=False), sites.PLACES["ship"], self.map, "ルミナス号", id_=cid)
        self.st["ids"]["ship"] = c.id

    # ------------------------------------------------------------------
    # コアの口
    # ------------------------------------------------------------------
    def on_dig(self, world, hit, resource):
        if self.active and script.CHAPTERS[min(self.st["ch"], len(script.CHAPTERS) - 1)]["id"] == "valley":
            self.st["dig"] += 1

    def on_device_placed(self, world, c, i):
        """ルミナス号の下の四隅に推進機を置いたら、向きを上向きにそろえる。"""
        if not self.active or c.id != self.st["ids"].get("ship"):
            return
        d = c.devs[i]
        if d and d["def"] == "thruster" and tuple(c.cells[i]) in sites.SHIP_THRUSTERS:
            if d["dir"] != 2:
                d["dir"] = 2
                c.rebuild()
                world.message("ミオ「推進機の向きを合わせておいたよ」")

    def colliders(self, world, lo, hi, margin):
        out = []
        for s in self.scenes:
            ctr = (lo + hi) / 2
            if np.linalg.norm(ctr - s.center) > s.radius + np.linalg.norm(hi - lo) / 2 + margin:
                continue
            rb = np.linalg.norm(s.E, axis=1)
            m = s.col & np.all(s.C + rb[:, None] + margin > lo, axis=1) & np.all(s.C - rb[:, None] - margin < hi, axis=1)
            if m.any():
                idx = np.nonzero(m)[0]
                out.append((s, s.C[m], s.A[m], s.E[m], idx))
        return out

    def raycast(self, world, o, dvec, max_t):
        best = None
        for s in self.scenes:
            if np.linalg.norm(o - s.center) > s.radius + max_t:
                continue
            r = ray_boxes(o, dvec, s.C, s.A, s.E, max_t)
            if r and (best is None or r[0] < best["t"]):
                k = int(r[1])
                best = {"kind": "story", "t": r[0], "scene": s, "part": k, "point": o + dvec * r[0], "normal": r[2],
                        "placeable": True, "tag": s.parts[k]["tag"]}
        return best

    def push_sphere(self, world, center, r):
        out = []
        for s, C, A, E, idx in self.colliders(world, center - r, center + r, 1.0):
            d, n, k = box_sdf(center[None], C, A, E)
            if d[0] < r:
                out.append((n[0] * (r - d[0]), n[0], s))
        return out

    # ------------------------------------------------------------------
    # マルチプレイ
    # ------------------------------------------------------------------
    def net_state(self, world):
        if not self.active:
            return None
        return json.loads(json.dumps(self.st))

    def net_apply(self, world, state):
        if not self.active or not isinstance(state, dict):
            return
        self.st.clear()
        self.st.update(state)


PADM = sites.PAD_HALF + 4.0
