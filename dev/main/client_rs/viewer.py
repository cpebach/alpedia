"""Rust の画面（native/alpedia_view）で遊ぶ（第4段の1：試験版）。

  python run.py --view rust --world 名前

今の版で描く物：構築物（ブロック・遠景の箱・ライト・機器の記号）、地表と掘った跡、惑星、景色の箱（シナリオ・場所の MOD）、空の色。
操作：WASD で歩く・マウスで見回す（クリックでマウスを閉じ込める、Esc で外す。もう一度 Esc で終わる）・Space でジャンプ・
F でジェットパック・Ctrl で下・F5 で保存。文字（HUD・看板）・メニュー・道具・MOD の画面側はまだ（窓のタイトルに fps を出す）。
"""
import time

import numpy as np

from .. import log
from ..client import gfx, render
from ..client.scenery_view import SceneView

L = log.get("client_rs")

SKY_DAY = (0.45, 0.65, 0.9)
SKY_SPACE = (0.012, 0.016, 0.04)


class Viewer:
    def __init__(self, world, cfg):
        self.w = world
        self.cfg = cfg
        g = cfg["graphics"]
        win = cfg.get("window", {})
        self.view = gfx.start(win.get("width", 1280), win.get("height", 720), "Alpedia（Rust の画面）", win.get("vsync", True))
        from .. import paths as _paths
        render.STYLE = {"edges": bool(g.get("block_edges", True)), "ao": bool(g.get("block_ao", True))}
        tex_dir = _paths.ROOT / "sauce" / "textures"
        render.EDGE_TEXTURE = str(tex_dir / "px_edge.png")
        render.DEVICE_TEXTURE = str(tex_dir / "px_device.png")
        self.root = gfx.RsNode(None, "scene")
        self.views = {}
        self.planet_views = [render.PlanetView(self.root, None, p) for p in world.planets]
        self.patch = render.TerrainPatch(self.root, None, int(g.get("terrain_patch_cells", 48)),
                                         float(g.get("terrain_patch_step", 1.0)))
        self.carve = render.CarveView(self.root, None)
        self.lod = render.LodLayer(self.root)
        self.lights = render.LightLayer(self.root)
        self.static_root = gfx.RsNode(self.root, "static")
        self.anchor, self.anchor_ver = None, 0
        self.loaded = set()
        self.scenes = []
        for a in world.addons:
            for s in getattr(a, "scenes", None) or []:
                try:
                    self.scenes.append((a, SceneView(self.root, s)))
                except Exception:  # noqa: BLE001
                    L.exception("景色を描けません: %s", getattr(a, "name", a))
        self.held = set()
        self.yaw_fwd = None
        self.pitch = 0.0
        self.grabbed = False
        self.acc = 0.0
        self.quit = False
        self.perf = {"frames": 0, "t": time.perf_counter(), "phys": 0.0, "rend": 0.0, "ticks": 0}

    # ---- 視点 ----
    def look_vectors(self):
        up = self.w.player.up
        if self.yaw_fwd is None:
            ref = np.array([0, 0, 1.0]) if abs(up[2]) < 0.9 else np.array([1.0, 0, 0])
            self.yaw_fwd = ref - up * np.dot(ref, up)
        f = self.yaw_fwd - up * np.dot(self.yaw_fwd, up)
        if np.linalg.norm(f) < 1e-3:
            ref = np.array([0, 0, 1.0]) if abs(up[2]) < 0.9 else np.array([1.0, 0, 0])
            f = ref - up * np.dot(ref, up)
        f /= max(np.linalg.norm(f), 1e-9)
        self.yaw_fwd = f
        look = f * np.cos(self.pitch) + up * np.sin(self.pitch)
        return f, np.cross(up, f), up, look

    def _input(self):
        c = self.cfg["controls"]
        dx = dy = 0.0
        for kind, a, b, n in self.view.pump():
            if kind == "close":
                self.quit = True
            elif kind.startswith("key:"):
                k = kind[4:]
                if a:
                    self.held.add(k)
                    if not n:
                        self._key(k, c)
                else:
                    self.held.discard(k)
            elif kind == "mouse":
                dx += a
                dy += b
            elif kind == "button" and a and not self.grabbed:
                self.grabbed = True
                self.view.grab(True)
            elif kind == "focus" and not a:
                self.held.clear()
        if dx or dy:
            sens = float(c.get("mouse_sensitivity", 60.0)) / 60.0 * 0.0025
            self.look_vectors()
            up = self.w.player.up
            f = self.yaw_fwd
            ang = dx * sens
            self.yaw_fwd = f * np.cos(ang) + np.cross(up, f) * np.sin(ang)
            self.pitch = float(np.clip(self.pitch - dy * sens, -1.55, 1.55))

    def _key(self, k, c):
        w = self.w
        if k == "escape":
            if self.grabbed:
                self.grabbed = False
                self.view.grab(False)
            else:
                self.quit = True
        elif k == c.get("jetpack", "f"):
            w.player.jetpack = not w.player.jetpack
        elif k == c.get("save", "f5"):
            w.save()

    # ---- 1フレーム ----
    def frame(self, dt):
        w = self.w
        c = self.cfg["controls"]
        self._input()
        fwd, right, up, look = self.look_vectors()
        w.local_player.look_fwd, w.local_player.pitch = fwd, self.pitch
        H = lambda k: 1 if k in self.held else 0  # noqa: E731
        move = np.zeros(3)
        if not w.player.seated:
            move += fwd * (H(c["forward"]) - H(c["back"])) + right * (H(c["right"]) - H(c["left"]))
            nm = np.linalg.norm(move)
            if nm > 1:
                move /= nm
        jump = bool(H(c["jump"])) and not w.player.seated
        thrust = move * 0.6 + up * (H(c["jump"]) - H("control"))
        w.input_keys = set()
        tick = 1.0 / float(self.cfg["physics"].get("tick_rate", 60))
        self.acc += dt
        n = 0
        t0 = time.perf_counter()
        while self.acc >= tick and n < 3:
            w.step(tick, move, jump and n == 0, thrust)
            self.acc -= tick
            n += 1
        if n == 3:
            self.acc = 0.0
        t1 = time.perf_counter()
        origin = w.player.eye()
        self._render(origin)
        g = self.cfg["graphics"]
        ww, hh = self.view.size()
        hfov = np.radians(float(g.get("fov", 80)))           # Panda3D と同じく横の視野角
        vfov = float(np.degrees(2 * np.arctan(np.tan(hfov / 2) * hh / max(ww, 1))))
        self.view.camera([0.0, 0.0, 0.0], [float(x) for x in look], [float(x) for x in up], vfov, 0.05)
        self.view.render()
        t2 = time.perf_counter()
        pf = self.perf
        pf["frames"] += 1
        pf["phys"] += t1 - t0
        pf["rend"] += t2 - t1
        pf["ticks"] += n
        el = t2 - pf["t"]
        if el >= 1.0:
            f = pf["frames"]
            txt = (f"Alpedia（Rust の画面）  fps {f / el:.0f}  物理 {pf['phys'] / max(pf['ticks'], 1) * 1000:.1f} ms/回"
                   f"  描画 {pf['rend'] / f * 1000:.1f} ms")
            self.view.set_title(txt)
            L.info("重さ: %s（構築物 %d）", txt, len(w.constructs))
            self.perf = {"frames": 0, "t": t2, "phys": 0.0, "rend": 0.0, "ticks": 0}

    def _render(self, origin):
        w = self.w
        for cid in list(self.views):
            if cid not in w.constructs:
                self.views.pop(cid).remove()
        self.loaded &= set(w.constructs)
        g = self.cfg["graphics"]
        far = float(g.get("view_distance", 3000))
        decal_dist = float(g.get("decal_distance", 30.0))
        load = float(g.get("load_distance", 0) or 0)
        if self.anchor is None or float(np.linalg.norm(origin - self.anchor)) > 256.0:
            self.anchor = np.array(origin, float)
            self.anchor_ver += 1
        self.static_root.setPos(*[float(x) for x in (self.anchor - origin)])
        cs, C, _, R = w._bounds_arrays()
        D = np.sqrt(((C - origin) ** 2).sum(1)) - R if len(cs) else np.zeros(0)
        seat = w.player.seated[0] if w.player.seated else None
        lod_list = []
        budget = time.perf_counter() + 0.008
        from .. import shape
        for c, dist in zip(cs, D.tolist()):
            cid = c.id
            if w.constructs.get(cid) is not c:
                continue
            v = self.views.get(cid)
            if dist > far:
                if v is not None:
                    v.set_visible(False)
                continue
            seated = seat == cid
            near = load <= 0 or seated or dist < load or (cid in self.loaded and dist < load * 1.1)
            if not near:
                if v is not None:
                    self.views.pop(cid).remove()
                    c._mesh_cache = None
                self.loaded.discard(cid)
                lod_list.append(c)
                continue
            if v is None:
                if not seated and c.n_vox > 150 and not shape.request_mesh(c):
                    lod_list.append(c)
                    continue
                if time.perf_counter() > budget and cid not in self.loaded and not seated:
                    lod_list.append(c)
                    continue
                v = self.views[cid] = render.ConstructView(self.root, None, c)
            self.loaded.add(cid)
            v.set_visible(True)
            tf = c.render_transform()
            static = tf is not None and not seated and (c.is_static or c.asleep) and not c.dev_ids("thruster")
            key = (c.version, tf[2], self.anchor_ver) if static else None
            if key is None or v.__dict__.get("_static_key") != key:
                v._static_key = key
                if static:
                    v.update(origin, static_root=self.static_root, anchor=self.anchor)
                else:
                    v.update(origin)
                v.update_decals(origin, origin, decal_dist)
        self.lod.update(lod_list, origin)
        self.lights.update([w.constructs[k] for k in self.loaded if k in w.constructs], origin, float(w.time))
        npl = w.nearest_planet(w.player.pos)
        self.carve.update(npl, w.player.pos, origin)
        self.patch.update(npl, w.player.pos, origin, self.carve)
        for pv in self.planet_views:
            pv.update(origin, w.player.pos, self.patch.frame_for(pv.p))
        for a, sv in self.scenes:
            sv.update(origin, bool(getattr(a, "alert", False) or getattr(a, "lights", False)), time.time())
        # 空の色
        air = 0.0
        if npl is not None:
            p = min(max(float(npl.defn.get("atmosphere_pressure", 101.3)) / 101.3, 0.0), 1.0)
            alt = max(float(np.linalg.norm(origin - npl.center)) - npl.radius, 0.0)
            air = p * float(np.exp(-alt / max(npl.radius * 0.25, 30.0)))
        self.view.set_clear(*[s_ + (d - s_) * air for s_, d in zip(SKY_SPACE, SKY_DAY)])

    def run(self):
        last = time.perf_counter()
        while not self.quit:
            now = time.perf_counter()
            dt = min(now - last, 0.1)
            last = now
            self.frame(dt)
        self.w.save()


def run(cfg, world_name, frames=None, shot=None):
    """Rust の画面でワールドを開く。frames・shot：試験用（その数だけ描いて、最後の画面を保存して終わる）。"""
    from ..app import prepare_world
    from .. import paths
    name = world_name or cfg["game"].get("default_world", "scenario")
    World = prepare_world(name)
    if not (paths.world_dir(name) / "world.json").exists():
        raise SystemExit(f"ワールド {name} がありません")
    w = World(name, cfg).load()
    v = Viewer(w, cfg)
    if frames is None:
        v.run()
        return v
    last = time.perf_counter()
    for k in range(int(frames)):
        now = time.perf_counter()
        dt = min(now - last, 0.1)
        last = now
        if shot and k == int(frames) - 1:
            v.view.screenshot(str(shot))
        v.frame(dt)
    return v
