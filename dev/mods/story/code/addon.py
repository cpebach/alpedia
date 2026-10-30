"""シナリオモード（ワールド側）：舞台を最初に作る、章と目標の進み方、無線のセリフ、人物の立ち位置、ムービーの順番。

状態は world.settings["story"] に入れてセーブする：
    v      状態の版（2：「それでも」。前の版「墜落した技術者」のセーブは読み込みのときに読み替える）
    built  構築物・地面の跡・スクリプトを作ったか
    ch     いまの章（script.CHAPTERS の番号）　goals  済んだ目標 {ID: True}
    ids    構築物の ID {役: ID}　log  セリフ [[番号, 話す人, 文, 出す時刻]]　n  セリフの通し番号
    movies まだ流していないムービーの名前のリスト（画面側が流して movie_done を呼ぶ）
    seen   流したムービー　events  一度だけのセリフ（済んだもの）　cnt  数える物 {名前: 数}
    flags  章の中の出来事 {名前: 値}　lights  基地の明かり　done  クリア
舞台：テラの基地（sites.py）・月のルナ・ポート（場所の MOD「lunaport」）・軌道都市アマツ（場所の MOD「amatsu」）と
そのまわりの艦隊（space.py）。人物は登場人物の MOD（「cast」）、ミサイル・砲塔はシナリオ再現ブロックの MOD（「weapons」）。
マルチプレイでは、ホストが進み方を決めて参加者へ送る（net_state）。
"""
import json

import numpy as np

from main import log
from main.construct import Construct
from main.geom import box_sdf
from main.hooks import Addon
from main.scenery import SceneryHost

from . import script, sites, space

L = log.get("story")

CHECK_EVERY = 0.2          # 目標を調べる間隔（秒・ワールド時刻）
HINT_AFTER = 75.0          # 目標が済まないとき、この秒数でヒントのセリフ
VERSION = 2
ANCHORED = ("interceptor", "krobot", "unit", "kdrone")    # シナリオが位置を決める（物理で動かさない）構築物

IDX = {cid: k for k, cid in enumerate(script.CHAPTER_IDS)}


def _say_time(text):
    return 1.6 + len(text) * 0.075


class StoryAddon(SceneryHost, Addon):
    name = "story"
    settings_keys = ("story",)
    scene_kind = "story"

    def __init__(self):
        self.world = None
        self.active = False
        self.scenes = []
        self.points = {}
        self._t = 0.0
        self._look_prev = {}
        self._door = None
        self._crane = None
        self.slow_ticks = 0          # スローモーションになったティックの数（テスト用）

    # ------------------------------------------------------------------
    # 読み込み・保存
    # ------------------------------------------------------------------
    def on_load(self, world):
        self.world = world
        st = world.settings.get("story")
        if not isinstance(st, dict):
            return                       # シナリオのワールドではない（MOD だけ入れた）
        self.terra = next((p for p in world.planets if p.defn["id"] == "terra"), None)
        self.luna = next((p for p in world.planets if p.defn["id"] == "luna"), None)
        self.port = world.addon("lunaport")
        self.amatsu = world.addon("amatsu")
        self.cast = world.addon("cast")
        self.weapons = world.addon("weapons")
        if self.terra is None or self.luna is None or self.port is None or not self.port.active:
            L.warning("シナリオにはテラと月ルナ（とルナ・ポートの MOD）が必要です")
            return
        self.active = True
        self.map = sites.terra_map(self.terra, sites.terra_craters(sites.terra_map(self.terra)))
        self.lmap = self.port.map
        self.scenes = [sites.build_terra(self.terra)]
        self.deck = self.amatsu.deck if self.amatsu is not None and self.amatsu.active else None
        if self.deck is not None:
            self.scenes.append(space.build_fleet(self.deck))
        self.points = {}
        for s in self.scenes:
            self.points.update(s.points)
        self.points.update({k: v for k, v in self.port.points.items()})
        for k, v in {"built": False, "ch": 0, "goals": {}, "ids": {}, "log": [], "n": 0, "dig": 0, "look": 0.0,
                     "rod_t": 0.0, "events": [], "lights": False, "done": False, "ch_t": 0.0, "movies": [],
                     "seen": [], "cnt": {}, "flags": {}}.items():
            st.setdefault(k, v)
        self.st = st
        host = world.net is None or getattr(world.net, "role", "") != "client"
        if host and st.get("v") != VERSION:
            self._upgrade(st)
        if not st["built"] and host:
            self.build_world(world)
        elif host:
            self.ensure_constructs()
        if self.weapons is not None:
            self.weapons.listeners.append(self._on_weapon)
            for role in ("interceptor", "robot", "unit"):
                c = self.con(role)
                if c is not None:
                    self.weapons.protected.add(c.id)
        self._stage()

    def on_save(self, world):
        pass                             # world.settings["story"] をそのまま使っている

    def _upgrade(self, st):
        """前の版（「墜落した技術者」）のセーブ：章の番号を読み替える。足りない構築物は ensure_constructs で足す。"""
        if st.get("built") and "v" not in st:
            old = script.OLD_IDS[min(int(st.get("ch", 0)), len(script.OLD_IDS) - 1)]
            new = script.OLD_TO_NEW.get(old, "wake")
            if st.get("done"):
                new = "port"
                st["done"] = False
            st["ch"] = IDX[new]
            st["goals"] = {}
            st["ch_t"] = self.world.time
            st["events"] = []
            st["log"] = []
            if new == "port":
                st["flags"]["landed"] = True
            L.info("前の版のシナリオのセーブを読み替えました（%s → %s）", old, new)
            self.world.message("シナリオが新しくなりました（「それでも」）。続きから遊べます（J で章の一覧）")
        st["v"] = VERSION

    # ------------------------------------------------------------------
    # 最初に1回：地面の跡・構築物・スクリプト・プレイヤーの位置
    # ------------------------------------------------------------------
    def build_world(self, world):
        st = self.st
        self.terra.craters += sites.terra_craters(sites.terra_map(self.terra))
        self.terra.version += 1
        self.ensure_constructs()
        self._write_scripts()
        # プレイヤー：ポッドの手前、奥（+Z）を向いて
        p, up = self.map.ground(-1.5, -5.0, 1.2)
        pl = world.local_player
        pl.pos, pl.vel, pl.up = p, np.zeros(3), up
        pl.hotbar = [("material", "steel"), ("material", "hull_white"), ("material", "rock"), ("material", "glass"),
                     ("device", "thruster"), ("device", "tank"), ("device", "pipe"), ("device", "computer"),
                     ("device", "seat")]
        st["built"] = True
        st["ch_t"] = world.time
        self._enter(0, intro=True)
        L.info("シナリオのワールドを作りました（構築物 %d）", len(st["ids"]))

    def ensure_constructs(self):
        """役の構築物が無ければ作る（前の版のセーブ・壊した物の作り直し）。"""
        from main import tutorial_drone as drone
        from main import tutorial_rod as rod
        ids = self.st["ids"]
        P = sites.PLACES
        made = []

        def put(role, name, vox, place, m=None):
            if ids.get(role) in self.world.constructs:
                return
            c = self._spawn(vox, place, m or self.map, name)
            ids[role] = c.id
            made.append(role)
            return c

        put("pod", "脱出ポッド", sites.pod_voxels(), P["pod"])
        put("beacon", "救難ビーコン", sites.beacon_voxels(), P["beacon"])
        if "rocks" not in ids:
            c = put("rocks", "落石", sites.rock_voxels(5, 13, 6, 3), (-3.25, 27.0, 0.0, 0.05))
            self.st["rocks0"] = len(c.cells)
        put("power", "観測基地の電源", sites.power_voxels(), P["power"])
        put("model", "お手本の装置", rod.apparatus("お手本", "otehon.blocks.json", "otehon.py"), P["model"])
        put("task", "課題の装置", rod.apparatus("課題", "kadai.blocks.json", "kadai.py"), P["task"])
        put("console", "ドローンの操縦台", drone.remote_voxels(), P["console"])
        put("drone", "ドローン", drone.drone_voxels(), P["drone"])
        put("gate", "格納庫の遮断機", sites.gate_voxels(), P["gate"])
        put("bike", "ホバーバイク", sites.bike_voxels(), P["bike"])
        if "ship" not in ids:
            put("ship", "ルミナス号", sites.ship_voxels(broken=True), P["ship"])
        put("sign_base", "看板", sites.sign_voxels("観測基地 ALP-7\n（無人・電源停止中）", width=2.4), P["sign_base"])
        if "sign_luna" not in ids:
            put("sign_luna", "看板", sites.sign_voxels("ようこそ\n自由港ルナ・ポートへ", width=2.6, color=(0.6, 1.0, 1.0)),
                (-4.0, -14.0, 180.0, 0.05), self.lmap)
        put("crane", "搬入クレーン", sites.crane_voxels(), P["crane"])
        put("factory", "工場", sites.factory_voxels(), P["factory"])
        if self.deck is not None:
            d = self.deck
            self._put_space("interceptor", "連盟の迎撃艦", space.interceptor_voxels(),
                            space.fleet_origin(d), d.frame(0.0), (6.0, 0.0, 12.0))
            rx, rz = space.ROBOT_START
            self._put_space("robot", "トワのロボット", space.robot_voxels(), d.point(rx, rz, 0.3), d.frame(0.0),
                            (1.5, 0.0, 2.5))
            kx, kz = space.KROBOT_START
            self._put_space("krobot", "キンギのロボット", space.krobot_voxels(), d.point(kx, kz, 0.6), d.frame(180.0),
                            (1.5, 0.0, 2.5))
            ux, uz = space.UNIT_AT
            self._put_space("unit", "再加速の作業台", space.unit_voxels(), d.point(ux, uz, 0.05), d.frame(0.0),
                            (4.0, 0.0, 4.0))
        if made:
            self._write_scripts(only_missing=True)
            L.info("構築物を作りました: %s", made)

    def _put_space(self, role, name, vox, pos, R, pivot):
        """宇宙の構築物：ボクセルの (pivot) が pos に来るように置く。"""
        ids = self.st["ids"]
        if ids.get(role) in self.world.constructs:
            return None
        size = float(self.world.settings.get("voxel_size", 0.5))
        origin = np.asarray(pos, float) - R @ (np.asarray(pivot, float) * size)
        c = Construct.from_voxels(self.world.registry, vox, origin, R, size=size, name=name)
        self.world.add_construct(c)
        ids[role] = c.id
        return c

    def _write_scripts(self, only_missing=False):
        from main import blocks as B
        from main import tutorial_rod as rod
        from main.content import SCRIPT_README
        from . import programs
        sd = self.world.dir / "scripts"
        sd.mkdir(parents=True, exist_ok=True)
        files = {"beacon.py": sites.BEACON_SCRIPT, "power.py": sites.POWER_SCRIPT, "sway.py": rod.SWAY_SCRIPT,
                 "factory.py": sites.FACTORY_SCRIPT, "README.txt": SCRIPT_README}
        for fname, text in files.items():
            if not (only_missing and (sd / fname).exists()):
                (sd / fname).write_text(text, encoding="utf-8")
        progs = [("kadai", rod.starter_program()), ("otehon", rod.solution_program())]
        progs += [(name, make()) for name, make in programs.PROGRAMS.items()]
        pad, up = self.points.get("pad"), self.points.get("pad_up")
        if pad is not None:
            progs.append(("moon", programs.moon_program(pad, up)))
        for role, base in (("bike", "bike"), ("drone", "drone"), ("console", "drone_remote")):
            c, i = self.dev(role, "computer")
            if i is not None:
                c.devs[i]["state"].update(script=f"{base}.py", blocks=f"{base}.blocks.json")
        for base, prog in progs:
            if only_missing and (sd / f"{base}.blocks.json").exists():
                continue
            B.save(sd / f"{base}.blocks.json", prog)
            (sd / f"{base}.py").write_text(B.to_python(prog, f"{base}.blocks.json"), encoding="utf-8")
        self._sign_target()

    def _spawn(self, vox, place, m, name, id_=None):
        """地図の場所に構築物を置く。地面（くぼ地の丸みなど）にめり込まないように、角がすべて地面より上になるまで持ち上げる。"""
        w = self.world
        x, z, yaw, lift = place
        p, _ = m.ground(x, z, lift)
        R = m.frame(x, z, yaw)
        c = Construct.from_voxels(w.registry, vox, p, R, size=float(w.settings.get("voxel_size", 0.5)), name=name,
                                  id_=id_)
        scenes = self.scenes + (self.port.scenes if m is self.lmap else [])
        for _ in range(8):                 # 地面と景色の箱（床・台）の上に出るまで持ち上げる
            dmin = float(m.pl.sdf(c.pos).min())
            for sc in scenes:
                if sc.map is not None and np.linalg.norm(c.pos[0] - sc.center) < sc.radius + 10:
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

    def _devpos(self, role, def_id=None, name=None):
        c, i = self.dev(role, def_id, name)
        return c.pos[8 * i:8 * i + 8].mean(0) if c is not None and i is not None else None

    def where(self, key):
        """目印の場所（構築物なら今の位置）。"""
        if key is None:
            return None
        special = {"navi": ("ship", "computer", None), "valve": ("power", "valve", None),
                   "engine": ("power", "thruster", None), "gate": ("gate", "actuator", None),
                   "refiner": ("factory", "refiner", None), "fabricator": ("factory", "fabricator", None),
                   "fuel_valve": ("factory", "valve", None), "turret_pc": ("interceptor", "computer", None),
                   "unit_pc": ("unit", "computer", "再加速の航法")}
        if key in special:
            return self._devpos(*special[key])
        if key == "pipe":
            c = self.con("factory")
            if c is not None:
                return self._cell_world(c, sites.FACTORY_CUT)
            return None
        if key in ("sakura", "mikaga", "kensu") and self.cast is not None and key in self.cast.npcs:
            return self.cast.npcs[key].pos + self.cast.npcs[key].up * 1.0
        if key in self.points:
            return self.points[key]
        c = self.con(key)
        return c.center_of_mass() if c is not None else None

    def _cell_world(self, c, cell):
        R, t0 = c.rigid_fit()
        return R @ ((np.asarray(cell, float) + 0.5) * c.size) + t0

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

    def _chapter(self):
        st = self.st
        if st["done"] or st["ch"] >= len(script.CHAPTERS):
            return None
        return script.CHAPTERS[st["ch"]]

    def _cid(self):
        c = self._chapter()
        return c["id"] if c else "end"

    def _flag(self, k, default=None):
        return self.st["flags"].get(k, default)

    def _count(self, k):
        return int(self.st["cnt"].get(k, 0))

    def _inc(self, k, n=1):
        self.st["cnt"][k] = self._count(k) + n

    def _program(self, fname):
        """scripts/ のブロックプログラム（無ければ None）。"""
        from main import blocks as B
        p = self.world.dir / "scripts" / fname
        try:
            return B.load(p)
        except Exception:  # noqa: BLE001
            return None

    def _running(self, role, name=None):
        c, i = self.dev(role, "computer", name)
        return i is not None and bool(c.devs[i]["state"].get("running"))

    # ------------------------------------------------------------------
    # 目標の判定（目標の ID → 済んだら True）
    # ------------------------------------------------------------------
    # 序章
    def check_look(self):
        return self.st["look"] >= 150.0

    def check_walk(self):
        return self._near(self.where("beacon"), 3.6)

    def check_beacon(self):
        c, i = self.dev("beacon", "computer")
        return i is not None and bool(c.devs[i]["state"].get("running"))

    # 第1章
    def check_sakura(self):
        return bool(self._flag("sakura_named"))

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

    # 第2章
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
        self._event("lit")
        return True

    def check_terminal(self):
        return bool(self._flag("terminal"))

    # 第3章
    def _act_angle(self, role, name):
        c, i = self.dev(role, "actuator", name)
        return None if i is None else float(c.devs[i]["state"].get("angle", 0.0))

    def _crane_program_ok(self):
        """搬入の制御が、待つブロックを使ったプログラムで動いているか。"""
        from main import blocks as B
        if not self._running("crane"):
            return False
        prog = self._program("crane.blocks.json")
        return prog is not None and any(B.has_wait(s.get("body")) for s in prog.get("scripts", []))

    def _watch_crane(self):
        """扉の開け閉め・クレーンの往復を数える（プログラムで動かしている間だけ）。"""
        if self._cid() != "openloop":
            return
        ok = self._crane_program_ok()
        a = self._act_angle("crane", "扉のモーター")
        if a is not None:
            state = "open" if abs(a) > 60 else ("closed" if abs(a) < 15 else None)
            if state and state != self._door:
                if ok and self._door == "open" and state == "closed":
                    self._inc("door")
                self._door = state
        b = self._act_angle("crane", "クレーンの旋回")
        if b is not None:
            state = "far" if abs(b) > 165 else ("home" if abs(b) < 15 else None)
            if state and state != self._crane:
                if ok and self._crane == "far" and state == "home":
                    self._inc("trips")
                self._crane = state

    def check_door(self):
        return self._count("door") >= 2

    def check_trips(self):
        return self._count("trips") >= 3

    # 第4章
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

    # 第5章
    def check_console(self):
        return self._seated("console")

    def check_ring(self):
        c = self.con("drone")
        return c is not None and float(np.linalg.norm(c.center_of_mass() - self.points["ring"])) < sites.RING_R * 0.95

    # 第6章
    def check_ore(self):
        return self._count("dig_ore") >= 3

    def check_sulfur(self):
        return self._count("dig_sulfur") >= 3

    def check_load(self):
        return bool(self._flag("loaded"))

    def check_pipe(self):
        c = self.con("factory")
        if c is None or not self._flag("pipe_cut"):
            return False
        i = c.index.get(tuple(sites.FACTORY_CUT))
        return i is not None and bool(c.devs[i]) and c.devs[i]["def"] == "pipe"

    def _factory_store(self, def_id):
        c, i = self.dev("factory", def_id)
        return c.devs[i]["state"].get("store", {}) if i is not None else {}

    def check_fuel(self):
        c, i = self.dev("factory", "tank")
        return i is not None and float(c.devs[i]["state"].get("contents", {}).get("fuel", 0.0)) >= 80.0

    def check_parts(self):
        return float(self._factory_store("fabricator").get("parts", 0.0)) >= 4.0 or bool(self._flag("parts_taken"))

    # 第7章
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

    # 第8章
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

    def check_fuel_ship(self):
        f = self._ship_progress()[2]
        return bool(f) and f[1] > 0 and f[0] >= f[1]

    # 第9章
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
        ok = h < 2.0 and side < sites.PAD_HALF + 1.5 and float(np.linalg.norm(c.velocity())) < 0.4
        if ok:
            self.st["flags"]["landed"] = True
        return ok

    # 第10章
    def check_mikaga(self):
        return bool(self._flag("met"))

    # 第11章
    def check_kensu(self):
        return bool(self._flag("kensu"))

    def check_intercept_prog(self):
        prog = self._program("intercept.blocks.json")
        if prog is None or not self._running("interceptor"):
            return False
        txt = json.dumps(prog, ensure_ascii=False)
        return "nearest_missile" in txt and "turret_fire" in txt and "turret_aim" in txt

    def check_intercept(self):
        return self._count("intercepted") >= 12

    # 第12章
    def check_board(self):
        return self._seated("robot")

    def check_robot_walk(self):
        return self._robot_moved() >= 25.0

    def _robot_moved(self):
        c = self.con("robot")
        start = self._flag("robot_start")
        if c is None or start is None:
            return 0.0
        return float(np.linalg.norm(c.center_of_mass() - np.asarray(start, float)))

    def check_dodge(self):
        return self._count("dodged") >= 8

    def check_clash(self):
        return float(self.st["flags"].get("clash_t", 0.0)) >= 2.0

    # 終章
    def _unit_modules(self):
        c = self.con("unit")
        if c is None:
            return 0
        n = 0
        for cell in space.UNIT_SLOTS:
            i = c.index.get(tuple(cell))
            if i is not None and c.devs[i] and c.devs[i]["def"] == "rocket_module":
                n += 1
        return n

    def check_modules(self):
        return self._unit_modules() >= 4

    def _unit_balance(self):
        """(点火している数, バルブの開き具合のリスト)"""
        c = self.con("unit")
        if c is None:
            return 0, []
        lit = sum(1 for d in c.devs if d and d["def"] == "rocket_module" and d["state"].get("lit"))
        opens = [float(d["state"].get("open", 0.0)) for d in c.devs if d and d["def"] == "valve"]
        return lit, opens

    def check_thrust(self):
        lit, opens = self._unit_balance()
        ok = lit >= 4 and len(opens) == 4 and min(opens) >= 0.6 and max(opens) - min(opens) <= 0.05
        if lit >= 1:
            self._event("lit")
        return ok

    def target_point(self):
        """終章：アマツを戻す「目標の地点」（甲板の上・前の遠く。整数に丸める）。"""
        if self.deck is None:
            return None
        return np.round(self.deck.point(0.0, 400.0, 300.0))

    def check_navi(self):
        if not self._running("unit", "再加速の航法"):
            return False
        prog = self._program("reaccel.blocks.json")
        tgt = self.target_point()
        if prog is None or tgt is None:
            return False
        found = []

        def walk(x):
            if isinstance(x, dict):
                if x.get("type") == "vto":
                    try:
                        found.append([float(x[k]["v"]) for k in ("x", "y", "z")])
                    except (KeyError, TypeError, ValueError):
                        pass
                for v in x.values():
                    walk(v)
            elif isinstance(x, list):
                for v in x:
                    walk(v)
        walk(prog)
        return any(float(np.linalg.norm(np.asarray(f) - tgt)) <= 10.0 for f in found)

    def check_rise(self):
        return float(self.st["flags"].get("rise_t", 0.0)) >= 12.0

    # ------------------------------------------------------------------
    def progress(self, gid):
        """目標の進み具合の文（数があるもの）。"""
        st = self.st
        if gid == "hull":
            return f"{self._ship_progress()[0]}/{len(sites.SHIP_HOLES)}"
        if gid == "thrusters":
            return f"{self._ship_progress()[1]}/{len(sites.SHIP_THRUSTERS)}"
        if gid == "fuel_ship":
            f = self._ship_progress()[2]
            return f"{f[0]}/{f[1]}" if f else ""
        if gid == "engine":
            return f"{self._engines_lit()}/2"
        if gid == "dig":
            return f"{min(st['dig'], 3)}/3"
        if gid == "rod":
            return f"{min(st['rod_t'], 5.0):.0f}/5 秒"
        if gid == "door":
            return f"{min(self._count('door'), 2)}/2"
        if gid == "trips":
            return f"{min(self._count('trips'), 3)}/3"
        if gid == "ore":
            return f"{min(self._count('dig_ore'), 3)}/3"
        if gid == "sulfur":
            return f"{min(self._count('dig_sulfur'), 3)}/3"
        if gid == "fuel":
            c, i = self.dev("factory", "tank")
            return f"{c.devs[i]['state'].get('contents', {}).get('fuel', 0.0):.0f}/80" if i is not None else ""
        if gid == "parts":
            return f"{min(float(self._factory_store('fabricator').get('parts', 0.0)), 4):.0f}/4"
        if gid == "intercept":
            return f"{min(self._count('intercepted'), 12)}/12"
        if gid == "robot_walk":
            return f"{min(self._robot_moved(), 25):.0f}/25 m"
        if gid == "dodge":
            return f"{min(self._count('dodged'), 8)}/8"
        if gid == "clash":
            return f"{min(float(st['flags'].get('clash_t', 0.0)), 2.0):.1f}/2 秒"
        if gid == "modules":
            return f"{self._unit_modules()}/4"
        if gid == "thrust":
            lit, opens = self._unit_balance()
            if opens:
                return f"点火 {lit}/4・開き " + " ".join(f"{o:.2f}" for o in opens)
            return ""
        if gid == "navi":
            t = self.target_point()
            return "" if t is None else f"目標 ({t[0]:.0f}, {t[1]:.0f}, {t[2]:.0f})"
        if gid == "rise":
            return f"{min(float(st['flags'].get('rise_t', 0.0)), 12.0):.0f}/12 秒"
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
        del st["log"][:-60]

    def _queue(self, names):
        for n in names or []:
            if n in script.MOVIES:
                self.st["movies"].append(n)

    def movie_done(self, name):
        """画面側がムービーを流し終えた（飛ばした）。"""
        if not self.active:
            return
        st = self.st
        if name in st["movies"]:
            st["movies"].remove(name)
        if name not in st["seen"]:
            st["seen"].append(name)

    def _event(self, key):
        st = self.st
        c = self._chapter()
        if c is None or key in st["events"]:
            return False
        st["events"].append(key)
        line = (c.get("events") or {}).get(key)
        if line:
            self.say(*line)
        self._queue((c.get("event_movies") or {}).get(key))
        return True

    KEEP_FLAGS = ("sakura_named", "landed")

    def _enter(self, ch, intro=True):
        st = self.st
        st["ch"] = ch
        st["goals"] = {}
        st["ch_t"] = self.world.time
        st["hinted"] = []
        st["rod_t"] = 0.0
        st["cnt"] = {}
        st["events"] = []
        st["flags"] = {k: v for k, v in st["flags"].items() if k in self.KEEP_FLAGS}
        if ch >= len(script.CHAPTERS):
            st["done"] = True
            self._stage()
            return
        c = script.CHAPTERS[ch]
        if self.weapons is not None:          # 前の章のミサイルは消す
            self.weapons.missiles = [m for m in self.weapons.missiles if m.get("tag") not in ("fleet", "duel")]
        self._queue(c.get("movie_in"))
        self.say(script.SYS, f"―― {c['title']} ――")
        if intro:
            for who, text in c["intro"]:
                self.say(who, text)
        self._setup_chapter(c["id"])
        self._stage()

    def _finish_chapter(self):
        st = self.st
        c = script.CHAPTERS[st["ch"]]
        for who, text in c["outro"]:
            self.say(who, text)
        self._queue(c.get("movie_out"))
        cid = c["id"]
        if cid == "openloop":
            self._drop_crate()
        if cid == "repair":
            self._give_program()
        if cid in ("bike", "factory"):
            self._remove_role("kdrone")
        if cid == "reaccel":
            if self.amatsu is not None:
                self.amatsu.alert = False
            st["done"] = True
            st["ch"] = len(script.CHAPTERS)
            self._teleport("end")
            for who, text in script.ENDING:
                self.say(who, text)
            self._stage()
            return
        self._enter(st["ch"] + 1)

    def _setup_chapter(self, cid):
        """章の始めの準備（ホストだけ）：場所の移動、構築物の状態。"""
        w = self.world
        if w.net is not None and getattr(w.net, "role", "") == "client":
            return
        if self.amatsu is not None:
            self.amatsu.alert = cid in ("fleet", "robot", "reaccel")
        if cid in ("fleet", "robot", "reaccel") and self.deck is not None:
            self._teleport(cid)
        if IDX.get(cid, 99) > IDX["fleet"]:              # 艦隊戦は終わった：迎撃艦の砲塔は止める
            c, i = self.dev("interceptor", "computer")
            if i is not None:
                c.devs[i]["state"]["running"] = False
        if cid == "robot":
            c = self.con("robot")
            if c is not None:
                self.st["flags"]["robot_start"] = c.center_of_mass().tolist()
        if cid == "reaccel":
            self._sign_target()

    def _give_program(self):
        c, i = self.dev("ship", "computer")
        if i is not None:
            c.devs[i]["state"]["script"] = "moon.py"
            c.devs[i]["state"]["blocks"] = "moon.blocks.json"
            c.devs[i]["state"].pop("error", None)

    def _sign_target(self):
        """終章の看板に「目標の地点」を出す。"""
        c, i = self.dev("unit", "computer", "看板")
        t = self.target_point()
        if i is None or t is None:
            return
        c.devs[i]["state"]["display"] = {
            "text": f"再加速の目標の地点\n({t[0]:.0f}, {t[1]:.0f}, {t[2]:.0f})", "face": 5, "width": 2.4,
            "color": [0.6, 1.0, 0.8], "background": [0.02, 0.06, 0.05, 0.92]}

    def _remove_role(self, role):
        c = self.con(role)
        if c is not None:
            for pl in self._players():
                if pl.seated and pl.seated[0] == c.id:
                    pl.seated = None
            del self.world.constructs[c.id]
        self.st["ids"].pop(role, None)

    # ------------------------------------------------------------------
    # 毎ティック
    # ------------------------------------------------------------------
    def _anchored(self):
        out = set()
        for role in ANCHORED:
            cid = self.st["ids"].get(role)
            if cid in self.world.constructs:
                out.add(cid)
        return out

    def before_physics(self, world, dt):
        if not self.active or not self.st.get("built"):
            return set()
        anchored = self._anchored()
        self._watch_look()
        self._watch_rod(dt)
        self._watch_crane()
        world.time_scale = 1.0
        try:
            self._chapter_tick(world, dt)
        except Exception:  # noqa: BLE001
            L.exception("章の動きでエラー")
        self._t += dt
        if self._t < CHECK_EVERY:
            return anchored
        self._t = 0.0
        st = self.st
        c = self._chapter()
        if c is None:
            return anchored
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
                self._goal_done(c["id"], gid)
        if all(st["goals"].get(g) for g, _, _ in c["goals"]):
            self._finish_chapter()
            return self._anchored()
        for gid, line in (c.get("hints") or {}).items():
            if not st["goals"].get(gid) and gid not in st.get("hinted", []) and world.time - st["ch_t"] > HINT_AFTER:
                st.setdefault("hinted", []).append(gid)
                self.say(*line)
        if c["id"] == "launch":
            self._flight_events()
        return anchored

    def _goal_done(self, cid, gid):
        """目標を済ませたときの出来事。"""
        st = self.st
        if gid == "load":
            st["flags"]["drone_t0"] = self.world.time          # キンギのドローンが来る
            self._ensure_kdrone(self.map.ground(-31.0, 140.0, 40.0)[0])
        if gid == "sakura":
            self._stage()
        if gid == "ride":
            st["flags"]["chase_t0"] = self.world.time
            self._ensure_kdrone(self.map.ground(-13.0, 70.0, 20.0)[0])
        if gid == "kensu":
            st["flags"]["wave_t"] = self.world.time + 3.0
        if gid == "board":
            c = self.con("robot")
            if c is not None:
                st["flags"]["robot_start"] = c.center_of_mass().tolist()
        if gid == "robot_walk":
            st["flags"]["volley_t"] = self.world.time + 1.0
            self._event("volley")

    def _chapter_tick(self, world, dt):
        cid = self._cid()
        st = self.st
        f = st["flags"]
        t = world.time
        if cid == "factory" and "drone_t0" in f:
            self._drone_cut(t - f["drone_t0"], dt)
        elif cid == "bike" and "chase_t0" in f:
            self._drone_chase(t - f["chase_t0"], dt)
        elif cid == "fleet":
            self._fleet_waves(t)
        elif cid == "robot":
            self._duel(t, dt)
        elif cid == "reaccel":
            self._rise(dt)

    # --- 第6章・第7章：キンギのドローン ------------------------------------
    def _ensure_kdrone(self, pos):
        if self.con("kdrone") is not None:
            return self.con("kdrone")
        size = float(self.world.settings.get("voxel_size", 0.5))
        up = self.terra.up(np.asarray(pos, float))
        R = self.map.frame(*self.map_xz(pos), 0.0)
        R = np.stack([R[:, 0], up, np.cross(R[:, 0], up)], axis=1)
        c = Construct.from_voxels(self.world.registry, sites.kdrone_voxels(), np.asarray(pos, float), R, size=size,
                                  name="黒いドローン")
        self.world.add_construct(c)
        self.st["ids"]["kdrone"] = c.id
        return c

    def _move_to(self, c, target, max_speed, dt):
        cur = c.center_of_mass()
        d = np.asarray(target, float) - cur
        L_ = float(np.linalg.norm(d))
        step = min(L_, max_speed * dt)
        if L_ > 1e-6:
            c.pos += d / L_ * step
            if getattr(c, "_prev", None) is not None:
                c._prev = c.pos.copy()
        c.vel[:] = 0.0
        return L_

    def _drone_cut(self, age, dt):
        """第6章：工場の上に飛んできて、配管を切って去る。"""
        c = self.con("kdrone")
        fac = self.con("factory")
        if c is None or fac is None:
            return
        cut = self._cell_world(fac, sites.FACTORY_CUT)
        up = self.terra.up(cut)
        if age < 9.0:
            self._move_to(c, cut + up * 5.0, 14.0, dt)
            if age > 6.0 and not self._flag("pipe_cut"):
                self._event("cut")
                if self.weapons is not None:
                    self.weapons.effects.append(("beam", c.center_of_mass(), cut, self.world.time))
                i = fac.index.get(tuple(sites.FACTORY_CUT))
                if i is not None:
                    fac.remove_voxels([i])
                    for part in fac.split():
                        self.world.add_construct(part)
                self.st["flags"]["pipe_cut"] = True
                self._event("cut2")
        elif age < 18.0:
            self._move_to(c, cut + up * 60.0 + self.map.right0 * 120.0, 18.0, dt)

    def _drone_chase(self, age, dt):
        """第7章：バイクを後ろから追う。速く走ると振り切れる。"""
        c = self.con("kdrone")
        if c is None:
            return
        f = self.st["flags"]
        if age > 1.0:
            self._event("chase")
        if age > 4.0:
            self._event("chase2")
        if f.get("lost"):
            p = c.center_of_mass()
            self._move_to(c, p + self.terra.up(p) * 30.0, 12.0, dt)
            return
        pl = self.world.local_player
        target = pl.pos + pl.up * 6.0 - np.asarray(pl.look_fwd, float) * 6.0
        dist = self._move_to(c, target, 11.0, dt)
        if age > 6.0 and dist > 35.0:
            f["lost"] = True
            self._event("lost")

    # --- 第11章：ミサイルの雨 -----------------------------------------------
    def _fleet_waves(self, t):
        f = self.st["flags"]
        ship = self.con("interceptor")
        if ship is None or self.weapons is None or "wave_t" not in f or self.deck is None:
            return
        if t < f["wave_t"]:
            return
        self._event("wave")
        live = sum(1 for m in self.weapons.missiles if m.get("tag") == "fleet")
        if live >= 8:
            f["wave_t"] = t + 0.5
            return
        rng = np.random.default_rng(int(t * 1000) % 100000)
        R = self.deck.R
        com = ship.center_of_mass()
        src = com - R[:, 2] * 420.0 + R[:, 0] * rng.uniform(-160, 160) + R[:, 1] * rng.uniform(-40, 120)
        v = (com - src) / np.linalg.norm(com - src) * 30.0 + R[:, 0] * rng.uniform(-12, 12) + R[:, 1] * 10.0
        self.weapons.spawn_missile(src, v, target=ship.id, speed=42.0, turn=0.7, life=30.0, blast=0.0, tag="fleet")
        f["wave_t"] = t + 1.6

    def _on_weapon(self, kind, info):
        cid = self._cid()
        m = info.get("missile") or {}
        if m.get("tag") != {"fleet": "fleet", "robot": "duel"}.get(cid):
            return
        if cid == "fleet":
            if kind == "intercept":
                self._inc("intercepted")
                if self._count("intercepted") >= 6:
                    self._event("half")
            elif kind == "hit":
                self._inc("hits")
                self._event("hit")
        elif cid == "robot":
            robot = self.con("robot")
            if kind == "hit" and robot is not None and info.get("construct") == robot.id:
                self._inc("robot_hits")
            else:
                self._inc("dodged")

    # --- 第12章：キンギのロボット --------------------------------------------
    def _duel(self, t, dt):
        f = self.st["flags"]
        kr, rb = self.con("krobot"), self.con("robot")
        if kr is None or rb is None or self.deck is None:
            return
        d = self.deck
        clash = float(f.get("clash_t", 0.0))
        rp = rb.center_of_mass() - d.top
        rx_, rz_ = float(rp @ d.R[:, 0]), float(rp @ d.R[:, 2])
        kx, kz = space.KROBOT_START
        if clash <= 0:
            if self.st["goals"].get("dodge"):            # ミサイルを撃ち尽くした：組み合いに来る
                x, z = float(np.clip(rx_, -28.0, 28.0)), float(np.clip(rz_ + 4.0, -90.0, 90.0))
            elif self.st["goals"].get("robot_walk"):     # 立ちはだかる：トワのロボットの前に回りこむ
                x = float(np.clip(rx_, -24.0, 24.0)) + 6.0 * np.sin(t * 0.8)
                z = float(np.clip(rz_ + 30.0, kz - 10.0, kz + 35.0))
            else:
                x, z = 14.0 * np.sin(t * 0.35), kz
            self._move_to(kr, d.point(x, z, 3.6), 7.0, dt)
        dist = float(np.linalg.norm(rb.center_of_mass() - kr.center_of_mass()))
        if self.st["goals"].get("robot_walk"):
            if dist < 9.0:
                f["clash_t"] = clash + dt
                self._event("near")
            elif dist < 20.0:
                self._event("near")
            if self.weapons is not None and not self.st["goals"].get("dodge") and t >= f.get("volley_t", 1e18):
                for k in range(2):
                    p = kr.center_of_mass() + d.R[:, 1] * 3.0 + d.R[:, 0] * (k * 2 - 1) * 1.5
                    v = d.R[:, 1] * 16.0 + d.R[:, 0] * (k * 2 - 1) * 6.0 - d.R[:, 2] * 6.0
                    self.weapons.spawn_missile(p, v, target=rb.id, owner=kr.id, speed=26.0, turn=1.3, life=9.0,
                                               blast=0.0, tag="duel")
                f["volley_t"] = t + 3.2
            # 近くのミサイルでスローモーション
            if self.weapons is not None and self._seated("robot"):
                rp = rb.center_of_mass()
                for m in self.weapons.missiles:
                    if m.get("tag") == "duel" and float(np.linalg.norm(m["p"] - rp)) < 20.0:
                        self.world.time_scale = 0.4
                        self.slow_ticks += 1
                        self._event("slow")
                        break

    # --- 終章：上昇 ---------------------------------------------------------
    def _rise(self, dt):
        g = self.st["goals"]
        f = self.st["flags"]
        if not (g.get("modules") and g.get("thrust") and g.get("navi")):
            return
        f["rise_t"] = float(f.get("rise_t", 0.0)) + dt
        r = f["rise_t"]
        if r > 3.0 and self._event("collapse"):
            if self.cast is not None:
                self.cast.set_pose("kingi", "slump")
        if r > 5.0 and self._event("donachi"):
            if self.cast is not None and "kingi" in self.cast.npcs:
                k = self.cast.npcs["kingi"]
                self.cast.walk_to("donachi", k.feet() + self.deck.R[:, 0] * 1.2, speed=1.6)
        if r > 8.0 and self._event("rising"):
            if self.amatsu is not None:
                self.amatsu.alert = False

    # --- 月へ（第9章） ------------------------------------------------------
    def _flight_events(self):
        ship = self.con("ship")
        if ship is None:
            return
        p = ship.center_of_mass()
        dt_ = float(np.linalg.norm(p - self.terra.center)) - self.terra.radius
        dl = float(np.linalg.norm(p - self.luna.center))
        dp = float(np.linalg.norm(p - self.points["pad"]))
        conds = {"lift": dt_ > 25 and self.check_launch(), "space": dt_ > 400, "rutero": dt_ > 700,
                 "half": dl < 2300, "near": dp < 350}
        for k in ("lift", "space", "rutero", "half", "near"):
            if conds[k]:
                self._event(k)

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
        if self._cid() != "control" or st["goals"].get("rod"):
            return
        a = self._rod_angle()
        c, i = self.dev("task", "computer", "課題")
        running = i is not None and c.devs[i]["state"].get("running")
        if a is not None and running and abs(a) < 5.0:
            st["rod_t"] += dt
        else:
            st["rod_t"] = 0.0

    def _drop_crate(self):
        """第3章の終わり：風で荷物がずれて落ちる（クレーンから外して、横へ飛ばす）。"""
        c = self.con("crane")
        if c is None:
            return
        idx = [c.index[tuple(x)] for x in sites.CRANE_CRATE if tuple(x) in c.index and c.mats[c.index[tuple(x)]] == "wood"]
        if not idx:
            return
        vox = [((0, 0, k), "wood", None) for k in range(len(idx))]
        P = c.voxel_centers()[idx].mean(0)
        c.remove_voxels(idx)
        for part in c.split():
            self.world.add_construct(part)
        R, _ = c.rigid_fit()
        size = c.size
        box = Construct.from_voxels(self.world.registry, vox, P, R, velocity=R[:, 0] * 2.5 + R[:, 1] * 0.5,
                                    size=size, name="落ちた荷物")
        self.world.add_construct(box)

    # ------------------------------------------------------------------
    # 人物の立ち位置
    # ------------------------------------------------------------------
    def _stage(self):
        cast = self.cast
        if cast is None:
            return
        cast.clear(owner="story")
        cid = self._cid()
        k = IDX.get(cid, len(script.CHAPTERS))
        st = self.st
        m = self.map

        def on_terra(key, x, z, face_xz, pose=None):
            p, up = m.ground(x, z)
            tgt, _ = m.ground(*face_xz)
            cast.place(key, p, up, tgt - p, pose=pose, owner="story")

        if cid == "valley" and not st["flags"].get("sakura_named"):
            on_terra("sakura", -0.5, 6.5, (-1.5, -2.0))
        elif IDX["valley"] <= k < IDX["bike"]:
            on_terra("sakura", 6.5, 46.0, (0.0, 44.0), pose="wave" if cid == "power" else None)
        elif IDX["bike"] <= k <= IDX["launch"] and cid != "bike" or (cid == "bike" and st["goals"].get("reach")):
            on_terra("sakura", 8.0, 142.0, (2.0, 150.0), pose="wave")
        if cid == "port":
            p = self.points.get("desk")
            if p is not None:
                up = self.luna.up(p)
                cast.place("mikaga", p, up, self.points["pad"] - p, owner="story")
        if cid == "fleet":
            ship = self.con("interceptor")
            if ship is not None:
                p = self._cell_world(ship, (5, 1, 15)) - ship.voxel_frames()[0][1] * ship.size * 0.5
                R, _ = ship.rigid_fit()
                cast.place("kensu", p, R[:, 1], -R[:, 2], pose="crossed", owner="story")
        if cid == "reaccel" and self.deck is not None:
            d = self.deck
            ux, uz = space.UNIT_AT
            cast.place("kingi", d.point(ux - 4.0, uz + 6.0), d.R[:, 1], -d.R[:, 0],
                       pose="slump" if "collapse" in st["events"] else "smoke", owner="story")
            cast.place("donachi", d.point(ux - 9.0, uz + 9.0), d.R[:, 1], d.R[:, 0], pose="crossed", owner="story")
        if st["done"]:
            self._stage_ending()

    def _stage_ending(self):
        cast = self.cast
        p0 = self.points.get("plaza")
        if p0 is None:
            return
        lm = self.lmap
        x0, z0 = 6.0, -2.0
        crew = [("mikaga", None), ("sakura", "wave"), ("kingi", "smoke"), ("kensu", None), ("sirotsu", "crossed"),
                ("nekosa", None), ("dabai", None), ("rutero", "wave"), ("siyama", "think"), ("donachi", "crossed")]
        for k, (key, pose) in enumerate(crew):
            a = np.radians(-70 + 140 * k / (len(crew) - 1))
            x, z = x0 + np.sin(a) * 6.0, z0 + 4.0 + np.cos(a) * 6.0
            p, up = lm.ground(x, z)
            tgt, _ = lm.ground(x0, z0)
            cast.place(key, p + up * 0.02, up, tgt - p, pose=pose, owner="story")

    # ------------------------------------------------------------------
    # 場所の移動（章の一覧・宇宙の章へ）
    # ------------------------------------------------------------------
    def _teleport(self, cid):
        pl = self.world.player
        pl.seated = None
        if cid in ("fleet", "robot", "reaccel") and self.deck is not None:
            d = self.deck
            if cid == "fleet":
                ship = self.con("interceptor")
                R, _ = ship.rigid_fit()
                p = self._cell_world(ship, (6, 1, 8)) + R[:, 1] * 0.6
                up = R[:, 1]
            elif cid == "robot":
                rx, rz = space.ROBOT_START
                p, up = d.point(rx + 4.0, rz - 3.0, 1.2), d.R[:, 1]
            else:
                ux, uz = space.UNIT_AT
                p, up = d.point(ux - 2.0, uz - 4.0, 1.2), d.R[:, 1]
            pl.pos, pl.vel, pl.up = p, np.zeros(3), up
            return
        spots = {"wake": (self.map, -1.5, -5.0), "valley": (self.map, -1.0, 2.0), "power": (self.map, -2.0, 46.0),
                 "openloop": (self.map, 16.0, 50.0), "control": (self.map, -12.5, 59.0),
                 "drone": (self.map, 6.0, 63.0), "factory": (self.map, -24.0, 69.0), "bike": (self.map, -9.0, 99.0),
                 "repair": (self.map, 0.0, 141.0), "launch": (self.map, -2.0, 144.0),
                 "port": (self.lmap, 0.0, -PADM), "end": (self.lmap, 6.0, -6.0)}
        m, x, z = spots[cid]
        p, up = m.ground(x, z, 1.2)
        pl.pos, pl.vel, pl.up = p, np.zeros(3), up

    def jump(self, ch):
        """章の始めへ（やり直し・比べるとき用）。それより前の章で済ませることは済ませた状態にする。"""
        if not self.active:
            return
        ch = max(0, min(int(ch), len(script.CHAPTERS) - 1))
        ids = script.CHAPTER_IDS
        w = self.world
        client = w.net is not None and getattr(w.net, "role", "") == "client"
        if client:                         # 参加者の PC では自分の体を動かすだけ（世界はホストが変える）
            self._teleport(ids[ch])
            return
        st = self.st
        if ch > IDX["power"]:
            for d in self.con("power").devs if self.con("power") else []:
                if d and d["def"] == "valve":
                    d["state"]["open"] = float(d["state"].get("set", 0.15) or 0.15)
                if d and d["def"] == "thruster":
                    d["state"]["ignite"] = True
            st["lights"] = True
        if ch > IDX["valley"]:
            st["flags"]["sakura_named"] = True
        if ch > IDX["factory"]:
            self._repair_pipe()
            self._remove_role("kdrone")
        if ch >= IDX["launch"]:
            self._rebuild_ship(on_pad=ch >= IDX["port"])
        st["done"] = False
        st["events"] = []
        st["log"] = []
        st["movies"] = []
        self._teleport(ids[ch])
        self._enter(ch)
        w.message(f"「{script.CHAPTERS[ch]['title']}」から始めます")

    def replay(self, name):
        """章の一覧から、ムービーをもう一度見る。"""
        if self.active and name in script.MOVIES:
            self.st["movies"].append(name)

    def _repair_pipe(self):
        fac = self.con("factory")
        if fac is None:
            return
        cell = tuple(sites.FACTORY_CUT)
        if cell not in fac.index:
            try:
                fac.add_voxel(cell, "steel", {"def": "pipe", "dir": 2, "name": "配管"})
            except Exception:  # noqa: BLE001
                L.exception("配管を直せません")

    def _rebuild_ship(self, on_pad=False):
        """ルミナス号を、直して燃料を入れた状態に作り直す（同じ ID。墜落地点か、月の発着場に）。"""
        w = self.world
        old = self.con("ship")
        cid = self.st["ids"].get("ship")
        if old is not None:
            for pl in self._players():
                if pl.seated and pl.seated[0] == old.id:
                    pl.seated = None
            del w.constructs[old.id]
        if on_pad:
            c = self._spawn(sites.ship_voxels(broken=False), (0.0, 0.0, 0.0, 0.1), self.lmap, "ルミナス号", id_=cid)
        else:
            c = self._spawn(sites.ship_voxels(broken=False), sites.PLACES["ship"], self.map, "ルミナス号", id_=cid)
        self.st["ids"]["ship"] = c.id
        self._give_program()

    # ------------------------------------------------------------------
    # 話す・触れる（画面から addon_call で呼ぶ）
    # ------------------------------------------------------------------
    def talk(self, tag):
        """シナリオの景色の触れる物（端末・写真）を E。"""
        if not self.active:
            return None
        cid = self._cid()
        if tag == "terminal":
            if IDX.get(cid, 99) < IDX["power"] or (cid == "power" and not self.st["lights"]):
                return "古い端末：電気が来ていない。画面は真っ暗だ"
            self.st["flags"]["terminal"] = True
            if cid == "power":
                self.say(script.MIKAGA, "……聞こえる？ ふふ、端末からも話せるの。この基地の機械、少し知ってるから。")
            else:
                self.say(script.MIKAGA, "その端末、まだ動くんだね。……懐かしいな。")
            return "古い端末が光った"
        if tag == "photo":
            self.st["events"].append("photo")
            line = (script.CHAPTERS[IDX["factory"]].get("events") or {}).get("photo")
            if line:
                self.say(*line)
            return "古い記録写真を見た"
        return None

    def on_scenery_talk(self, world, place, tag):
        if not self.active or place != "lunaport" or tag != "desk":
            return None
        if self._cid() == "port" and not self._flag("met"):
            return "受付「管制官のミカガなら、そこにいますよ」"
        return None

    def on_npc_talk(self, world, key):
        if not self.active:
            return None
        cid = self._cid()
        st = self.st
        f = st["flags"]
        S = script
        if key == "sakura":
            if cid == "valley" and not f.get("sakura_named"):
                f["sakura_named"] = True
                self.say(S.SAKURA, "生きてる！？ ……ねえ、あなた名前は？")
                self.say(S.TOWA, "……三号。")
                self.say(S.SAKURA, "サンゴウ？ それ、番号じゃん。……じゃあ、トワ！ 今日からトワね！")
                self.say(S.TOWA, "……トワ。")
                self.say(S.SAKURA, "谷の向こうに古い基地があるの。落石でふさがってるけど……がんばって！")
                return "サクラ「よろしくね、トワ！」"
            if cid in ("repair", "launch"):
                return "サクラ「ルミナス号、かっこいいじゃん！ 特ダネ写真、撮っとくね！」"
            if st["done"]:
                return "サクラ「トワ！ 次の特ダネ、待ってるからね！」"
            return "サクラ「特ダネ、まだかなあ……。あ、トワ！ がんばってね！」"
        if key == "mikaga":
            if cid == "port" and not f.get("met"):
                f["met"] = True
                self.say(S.MIKAGA, "やっと会えたね。……ようこそ、ルナ・ポートへ。")
                return "ミカガ「……あなたの目、わたしと同じ色だね」"
            if st["done"]:
                return "ミカガ「行っておいで。……今度は、見届けるから」"
            return None
        if key == "kensu" and cid == "fleet":
            if not f.get("kensu"):
                f["kensu"] = True
                self.say(S.KENSU, "迎撃の制御は艦橋の前のコンピューターだ。P で開いて、砲塔の向きを決めてくれ。")
                self.say(S.KENSU, "撃ち方は任せる。……昔、似たような戦場を、似たような止め方で止めたことがあってね。")
                return "ケンス「頼りにしてるよ、技術者さん」"
            return "ケンス「矢印を見ろ。ミサイルの方を向けば、当たる」"
        if key == "kingi" and cid == "reaccel":
            if "collapse" in st["events"]:
                return "キンギ「……悪ぃ。少し、休む……」"
            return "キンギ「解体はおれの仕事だ。……壊すのにも、救うのにも、使えるらしい」"
        if key == "donachi" and cid == "reaccel":
            return "ドナチ「……無謀だね。……だから、面白い」"
        if st["done"]:
            done_lines = {"kingi": "キンギ「……火、つけらんねえんだよな。医者がうるさくてよ」",
                          "kensu": "ケンス「艦王さまの新しい伝説ができちまったな。……主役は君だけど」",
                          "sirotsu": "シロツ「あの時と同じだった。……攻撃が止まった瞬間、全部わかったよ」",
                          "nekosa": "先生「あの計算、合ってて本当によかった……！ ……もう一回確かめてもいいですか？」",
                          "dabai": "ダバイ「あのポッド……設計の癖に見覚えがあるんです。……いえ、なんでもないです」",
                          "rutero": "ルテロ「あたしの都市、取り戻してくれてありがと。……次は、もっとすごいのを作るわよ！」",
                          "siyama": "ヤマシナ「家と向き合うよ。……逃げるのは、もうやめた」",
                          "donachi": "ドナチ「……守るために飛ぶのも、悪くない」"}
            return done_lines.get(key)
        return None

    # ------------------------------------------------------------------
    # コアの口
    # ------------------------------------------------------------------
    def on_dig(self, world, hit, resource):
        if not self.active:
            return
        cid = self._cid()
        if cid == "valley":
            self.st["dig"] += 1
        if cid == "factory" or IDX.get(cid, 0) > IDX["factory"]:
            p = np.asarray(hit.get("point", world.player.pos), float)
            for key, res, cnt in (("quarry_ore", "ore", "dig_ore"), ("quarry_sulfur", "sulfur", "dig_sulfur")):
                q = self.points.get(key)
                if q is not None and float(np.linalg.norm(p - q)) < sites.QUARRY_R + 2.0:
                    inv = world.player.inventory
                    inv[res] = inv.get(res, 0.0) + 20.0
                    self._inc(cnt)
                    world.message(f"{world.registry.res_name(res)} +20（所持 {inv[res]:.0f}）")

    def interact_device(self, world, c, i, d, dd):
        if not self.active or c.id != self.st["ids"].get("factory"):
            return None
        p = world.player
        if d["def"] == "refiner":
            store = d["state"].setdefault("store", {})
            moved = []
            for k in ("ore", "sulfur"):
                amt = p.inventory.pop(k, 0.0)
                if amt:
                    store[k] = store.get(k, 0.0) + amt
                    moved.append(f"{world.registry.res_name(k)} {amt:.0f}")
            if moved:
                self.st["flags"]["loaded"] = True
                return "精錬機に入れました：" + "、".join(moved)
            now = "、".join(f"{world.registry.res_name(k)} {v:.0f}" for k, v in store.items() if v >= 0.5) or "空"
            return f"精錬機：{now}（採掘場で掘った物を持って E で入れます）"
        if d["def"] == "fabricator":
            store = d["state"].setdefault("store", {})
            n = store.get("parts", 0.0)
            if n >= 1:
                p.inventory["parts"] = p.inventory.get("parts", 0.0) + n
                store["parts"] = 0.0
                if n >= 4:
                    self.st["flags"]["parts_taken"] = True
                return f"機械部品 {n:.0f} を受け取りました"
            return "加工機：鉄が届くと機械部品を作ります（精錬機の鉄は配管で流れてきます）"
        if dd and dd["type"] == "tank":
            fuel = d["state"].get("contents", {}).get("fuel", 0.0)
            return f"燃料タンク：燃料 {fuel:.0f}（燃料のバルブを開けると、精錬機からたまります）"
        return None

    def on_device_placed(self, world, c, i):
        """ルミナス号の下の四隅の推進機・作業台の推進モジュールは、向きを上向きにそろえる。"""
        if not self.active:
            return
        d = c.devs[i]
        if not d:
            return
        fix = (c.id == self.st["ids"].get("ship") and d["def"] == "thruster"
               and tuple(c.cells[i]) in sites.SHIP_THRUSTERS) or \
              (c.id == self.st["ids"].get("unit") and d["def"] == "rocket_module")
        if fix and d["dir"] != 2:
            d["dir"] = 2
            c.rebuild()
            world.message("ミカガ「向きを合わせておいたよ」")

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
