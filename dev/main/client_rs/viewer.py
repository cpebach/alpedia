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
        from . import ursina_shim
        ursina_shim.install()            # 今までの画面の部品（ursina）を、この画面で動かす
        self.view = gfx.VIEW if gfx.active() else gfx.start(win.get("width", 1280), win.get("height", 720),
                                                             "Alpedia（Rust の画面）", win.get("vsync", True))
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
        # 画面の上：左上の状態・左下の知らせ・照準
        gfx.font(g.get("font"))
        self.hud = gfx.RsText(None, overlay=True, bg=(0.02, 0.03, 0.05, 0.55), bin=10)
        self.msg = gfx.RsText(None, overlay=True, bg=(0.02, 0.03, 0.05, 0.55), bin=10)
        self.cross = gfx.RsDynamicMesh(gfx.RsNode(None, "cross"), "cross")
        gfx.VIEW.mesh_overlay(self.cross.mid, True)
        gfx.quad(self.cross, -0.003, 0.003, 0.003, -0.003, (1, 1, 1, 0.9))
        # 手の操作：狙っている物の枠・置く場所の枠・説明・アイテムバー
        self.highlight = gfx.wire_box(self.root, (1.0, 0.92, 0.2, 1.0), 0.025)
        self.ghost = gfx.wire_box(self.root, (0.4, 1.0, 0.4, 1.0), 0.02)
        self.aim_text = gfx.RsText(None, overlay=True, bg=(0.02, 0.03, 0.05, 0.55), bin=10)
        self.item_text = gfx.RsText(None, overlay=True, bg=(0.02, 0.03, 0.05, 0.55), bin=10)
        self.hit = None
        self.sel = 0
        self.dir_index = 0
        self.bodies = {}
        from .ui import UI
        self.ui = UI(self.view)
        from . import ursina_shim
        self.shim = ursina_shim
        self.editor = None             # 今までの画面の部品（ursina のまね）で開いている画面（プログラムなど）
        self.game = GameAdapter(self)
        self.hotbar = None
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
            if self.editor is not None and self.shim.event(self.view, kind, a, b, n):
                continue
            if self.ui.event(kind, a, b, n):
                continue
            if kind == "close":
                self.quit = True
                self.close_app = True
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
            elif kind == "button" and a:
                self._click(n)
            elif kind == "wheel" and a:
                self.sel = (self.sel + (-1 if a > 0 else 1)) % 9
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

    KEYMAP = {"control": "left control", "shift": "left shift", "alt": "left alt"}

    def _click(self, button):
        """左：壊す・掘る、右：置く。"""
        w = self.w
        if w.player.seated or self.hit is None:
            if button == 1 and not w.player.seated:
                self._place(None)
            return
        if button == 0:
            if self.hit["kind"] == "voxel":
                w.remove(self.hit)
            elif self.hit["kind"] in ("terrain", "water"):
                w.dig(self.hit)
        elif button == 1:
            self._place(self.hit)

    def _place(self, hit):
        w = self.w
        item = w.player.hotbar[self.sel]
        if item is None:
            w.message("アイテムバーのこの枠は空です")
            return
        _, _, _, look = self.look_vectors()
        w.place(hit, item, self.dir_index, look)

    def open_screen(self, name):
        """一時停止メニュー・インベントリを開く（マウスを出して、歩くのを止める）。"""
        from . import screens
        self.held.clear()
        self.grabbed = False
        self.view.grab(False)
        self.ui.open({"pause": screens.Pause, "inventory": screens.Inventory}[name], self)

    def open_editor(self):
        """P：見ている機器の設定（アクチュエーター・ほかの機器）。コンピューターのプログラムの画面は次の段で。"""
        from .. import actuator as act
        from . import screens_dev
        w = self.w
        h = self.hit
        if not (h and h["kind"] == "voxel" and h["construct"].devs[h["index"]]):
            w.message("機器を見ながら P を押すと設定を開けます")
            return
        c, i = h["construct"], h["index"]
        dd = c.reg.device(c.devs[i]["def"]) or {}
        self.held.clear()
        if dd.get("type") == "computer":
            self.open_program(c, i)
            return
        self.grabbed = False
        self.view.grab(False)
        if act.is_actuator(c, i):
            self.ui.open(screens_dev.ActuatorPanel, self, c, i)
        else:
            self.ui.open(screens_dev.DevicePanel, self, c, i)

    def open_program(self, c, i):
        """コンピューターのプログラムの画面（今までの画面の ScriptPanel・BlockEditor を ursina のまねで動かす）。"""
        self.grabbed = False
        self.view.grab(False)
        self.game.ui_root.enabled = False
        if c.devs[i]["state"].get("blocks"):
            from ..client.block_editor import BlockEditor
            self.editor = BlockEditor(self.game, c, i)
        else:
            from ..client.script_panel import ScriptPanel
            self.editor = ScriptPanel(self.game, c, i)

    def screen_closed(self):
        if not self.ui.stack:
            self.grabbed = True
            self.view.grab(True)

    def _key(self, k, c):
        w = self.w
        from ..client.game import SEAT_KEYS
        if w.player.seated and k != "escape":
            kk = self.KEYMAP.get(k, k)
            if kk in SEAT_KEYS or kk == "x":
                w.key_pressed(kk)
            if k == c["interact"]:
                w.interact(None)
            return
        if k in "123456789" and len(k) == 1:
            self.sel = int(k) - 1
        elif k == c["rotate"]:
            self.dir_index = (self.dir_index + 1) % 6
        elif k == c["interact"]:
            if self.hit is not None:
                w.interact(self.hit)
        elif k == c["new_construct"]:
            self._place(None)
        if k == "escape":
            self.open_screen("pause")
        elif k == c.get("inventory", "i"):
            self.open_screen("inventory")
        elif k == c.get("edit_script", "p"):
            self.open_editor()
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
        if not w.player.seated and not self.ui.stack and self.editor is None:
            move += fwd * (H(c["forward"]) - H(c["back"])) + right * (H(c["right"]) - H(c["left"]))
            nm = np.linalg.norm(move)
            if nm > 1:
                move /= nm
        jump = bool(H(c["jump"])) and not w.player.seated
        thrust = move * 0.6 + up * (H(c["jump"]) - H("control"))
        from ..client.game import SEAT_KEYS
        w.input_keys = {self.KEYMAP.get(k, k) for k in self.held} & set(SEAT_KEYS) if w.player.seated else set()
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
        self._bodies(origin)
        g = self.cfg["graphics"]
        ww, hh = self.view.size()
        hfov = np.radians(float(g.get("fov", 80)))           # Panda3D と同じく横の視野角
        vfov = float(np.degrees(2 * np.arctan(np.tan(hfov / 2) * hh / max(ww, 1))))
        self.view.camera([0.0, 0.0, 0.0], [float(x) for x in look], [float(x) for x in up], vfov, 0.05)
        self._aim(origin, look)
        self._overlay(ww, hh)
        self.shim.frame(self.view, dt)                # 今までの画面の部品（プログラムの画面など）
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
            self.fps_text = txt.split("  ", 1)[1]
            L.info("重さ: %s（構築物 %d）", txt, len(w.constructs))
            self.perf = {"frames": 0, "t": t2, "phys": 0.0, "rend": 0.0, "ticks": 0}

    def _aim(self, origin, look):
        """狙っている物：枠と説明（今までの画面と同じ play.aim）。"""
        from ..client import play
        w = self.w
        p = w.player
        self.hit = None if p.seated else w.raycast(p.eye(), look, float(p.defn.get("reach", 8.0)))
        a = play.aim(w, self.hit, self.look_vectors())
        for node, box in ((self.highlight, a["highlight"]), (self.ghost, a["ghost"])):
            if box is None:
                node.hide()
            else:
                gfx.set_box(node, box[0] - origin, box[1], box[2])
                node.show()
        self._aim_txt = a["text"]

    def _bodies(self, origin):
        """ほかの人（マルチプレイ）の体：定義の色の箱。"""
        w = self.w
        seen = set()
        for key, p in list(getattr(w, "remote", {}).items()):
            seen.add(key)
            b = self.bodies.get(key)
            if b is None:
                node = gfx.RsNode(self.root, "body")
                m = gfx.RsDynamicMesh(node, "body")
                col = p.defn.get("color", [1, 1, 1])
                v, t = gfx.box_vertices((col[0], col[1], col[2], 1.0))
                m.set(v, t)
                b = self.bodies[key] = node
            up = p.up
            f = np.asarray(p.look_fwd, float)
            f = f - up * (f @ up)
            f = f / np.linalg.norm(f) if np.linalg.norm(f) > 1e-6 else np.cross(up, [1.0, 0, 0])
            r = np.cross(up, f)
            gfx.set_box(b, p.pos + up * (p.height / 2 - p.radius) - origin, np.stack([r, up, f]),
                        np.array([p.radius * 2, p.height, p.radius * 1.4]))
        for key in [k for k in self.bodies if k not in seen]:
            self.bodies.pop(key).removeNode()

    def _hotbar(self):
        """画面下のアイテムバー（アイコン・選んでいる枠は黄色。今までの画面と同じ並べ方）。"""
        from ..client.inventory import SLOTS
        from . import ui as U
        w = self.w
        hb = w.player.hotbar
        state = (tuple(hb), self.sel, tuple(bool(it) and w.item_allowed(it) for it in hb))
        if self.hotbar is not None and self.hotbar[0] == state:
            return
        if self.hotbar is not None:
            self.hotbar[1].removeNode()
        root = gfx.RsNode(None, "hotbar")
        sw, gap, y = 0.07, 0.008, -0.448
        for k in range(SLOTS):
            x = (k - (SLOTS - 1) / 2) * (sw + gap)
            s = sw * (1.12 if k == self.sel else 1.0)
            U.rect_mesh(root, x - s / 2, y - s / 2, x + s / 2, y + s / 2,
                        (1.0, 0.82, 0.25, 0.9) if k == self.sel else (0, 0, 0, 0.5))
            if hb[k]:
                m = U.rect_mesh(root, x - sw * 0.44, y - sw * 0.44, x + sw * 0.44, y + sw * 0.44,
                                (1, 1, 1, 1.0 if state[2][k] else 0.18), U.item_tex(w.registry, hb[k]))
                gfx.VIEW.mesh_props(m.mid, True, False, True, 0, 11)
            t = gfx.RsText(root, overlay=True, bin=11)
            t.set(str(k + 1), 0.0138, (1, 1, 1, 0.8))
            t.place(x - sw / 2 + 0.005, y + sw / 2 - 0.003)
        if not getattr(self, "_hud_shown", True):
            root.hide()
        self.hotbar = (state, root)

    def _overlay(self, ww, hh):
        """画面の上の文字（左上：場所・高度・速度・重さ、左下：知らせ）。プログラムなどの画面を開いている間は隠す。"""
        w = self.w
        shown = self.editor is None
        if shown != getattr(self, "_hud_shown", True):
            self._hud_shown = shown
            for t in (self.hud, self.msg, self.aim_text, self.item_text):
                t.show(shown)
            (self.cross.np.show if shown else self.cross.np.hide)()
            if self.hotbar is not None:
                (self.hotbar[1].show if shown else self.hotbar[1].hide)()
        if not shown:
            return
        p = w.player
        pl = w.nearest_planet(p.pos)
        alt = float(pl.sdf(p.pos[None])[0]) if pl else 0.0
        lines = [f"{pl.name if pl else '宇宙'}  高度 {alt:.1f} m  速度 {float(np.linalg.norm(p.vel)):.1f} m/s"]
        if p.jetpack:
            lines.append("ジェットパック ON")
        if p.seated and p.seated[0] in w.constructs:
            c = w.constructs[p.seated[0]]
            lines.append(f"操縦中: {c.name}  速度 {float(np.linalg.norm(c.velocity())):.1f} m/s")
        if getattr(self, "fps_text", ""):
            lines.append(self.fps_text)
        if not self.grabbed and not self.ui.stack:
            lines.append("クリックで操作（マウスで見回す）・Esc でメニュー・I でインベントリ")
        aspect = ww / max(hh, 1)
        self.hud.set("\n".join(lines), 0.026)
        self.hud.place(-aspect / 2 + 0.015, 0.5 - 0.015)
        now = time.time()
        msgs = [t for ts, t in list(w.msgs)[-5:] if now - ts < 8.0]
        _, h = self.msg.set("\n".join(msgs), 0.024, max_width=aspect * 0.6)
        self.msg.show(bool(msgs))
        self.msg.place(-aspect / 2 + 0.015, -0.5 + 0.03 + h)
        tw, th = self.aim_text.set(getattr(self, "_aim_txt", ""), 0.022, max_width=aspect * 0.5)
        self.aim_text.show(bool(getattr(self, "_aim_txt", "")))
        self.aim_text.place(-tw / 2, -0.03)
        from ..client import play
        it = play.item_text(w, p.hotbar[self.sel], self.sel, self.dir_index)
        tw, th = self.item_text.set(it, 0.022)
        self.item_text.place(-tw / 2, -0.5 + 0.13 + th)
        self._hotbar()
        for s_ in self.ui.stack:
            s_.update(0.0)

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
        self.frame_no = getattr(self, "frame_no", 0) + 1
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
            if key is None or (self.frame_no + hash(cid)) % 6 == 0:     # 看板（止まっている物は6フレームに1回）
                v.update_displays(origin, far)
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

    def destroy(self):
        """画面から消す（メニューに戻るとき）。"""
        for v in self.views.values():
            v.remove()
        self.views = {}
        for n in [self.root, self.hud.node, self.msg.node, self.aim_text.node, self.item_text.node, self.cross.np]:
            n.removeNode()
        if self.hotbar is not None:
            self.hotbar[1].removeNode()
        for s_ in list(self.ui.stack):
            s_.close()
        if self.editor is not None and hasattr(self.editor, "close"):
            try:
                self.editor.close()
            except Exception:  # noqa: BLE001
                pass
        self.shim.destroy(self.game.ui_root)
        self.view.grab(False)

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


class GameAdapter:
    """今までの画面の部品（ScriptPanel・BlockEditor・DevicePanel など）が使う game のまね。"""

    def __init__(self, viewer):
        from . import ursina_shim as U
        self.v = viewer
        self.world = viewer.w
        self.cfg = viewer.cfg
        self.U = dict(Entity=U.Entity, Text=U.Text, Button=U.Button, camera=U.camera, window=U.window, color=U.color,
                      held_keys=U.held_keys, mouse=U.mouse, utime=U.time, application=U.application)
        self.ui_root = U.Entity(parent=U.camera.ui)
        self.editor = None

    @property
    def sel(self):
        return self.v.sel

    @sel.setter
    def sel(self, k):
        self.v.sel = k

    @property
    def editor(self):
        return self.v.editor

    @editor.setter
    def editor(self, e):
        self.v.editor = e

    def editor_closed(self):
        self.v.editor = None
        self.ui_root.enabled = True
        self.v.screen_closed()

    def set_mouse_capture(self, on):
        self.v.grabbed = bool(on)
        self.v.view.grab(bool(on))

    def sfx(self, *a, **k):
        pass
