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
        self.labels = {}
        self.third_person = False
        self._was_seated = False
        from .ui import UI
        self.ui = UI(self.view)
        from . import ursina_shim
        self.shim = ursina_shim
        self.editor = None             # 今までの画面の部品（ursina のまね）で開いている画面（プログラムなど）
        self.game = GameAdapter(self)
        from ..client import sound
        self.snd = sound.get(cfg)
        self._snd_t, self._snd_lit, self._snd_loops = 0.0, set(), {}
        from ..client import inventory
        inventory.ensure_hotbar(world.player, world.registry)
        self.addons = self._load_addons()
        # 画面側の MOD が描かない拡張の景色の箱（画面側の無い MOD）はここで描く
        own = {id(getattr(a, "pack", None)) for a in self.addons}
        self.scenes = []
        for a in world.addons:
            if id(a) in own:
                continue
            for s in getattr(a, "scenes", None) or []:
                try:
                    self.scenes.append((a, SceneView(self.root, s)))
                except Exception:  # noqa: BLE001
                    L.exception("景色を描けません: %s", getattr(a, "name", a))
        self._addon_err = set()
        self.hotbar = None
        self.held = set()
        self.yaw_fwd = None
        self.pitch = 0.0
        self.grabbed = False
        self.acc = 0.0
        self.quit = False
        self.perf = {"frames": 0, "t": time.perf_counter(), "phys": 0.0, "rend": 0.0, "ticks": 0}

    # ---- MOD の画面側 ----
    def _load_addons(self):
        """MOD の画面側（ClientAddon）を作る（今までの画面と同じ modcode.client_addons）。"""
        from ..modcode import client_addons, load_client
        errors = []
        load_client(errors)
        out = client_addons(self.game, errors)
        w = self.w
        if w.addon("survival") is not None and getattr(w.addon("survival"), "mod_id", None) is None:
            try:
                from ..client.survival_ui import SurvivalClient
                out.insert(0, SurvivalClient(self.game))
            except Exception:  # noqa: BLE001
                L.exception("サバイバルの目標を出せません")
        for a in out:
            try:
                self.game.emotes.update(a.emotes() or {})
            except Exception as e:  # noqa: BLE001
                errors.append(f"MOD '{getattr(a, 'name', '?')}' のモーションを読めません: {e}")
        for e in errors:
            w.message("⚠ " + e.splitlines()[0])
        self.load_errors = errors
        return out

    def _each_addon(self, fn, first=False):
        """MOD の口を順に呼ぶ（エラーは1回だけ記録して続ける）。first：最初に真を返した物の値。"""
        for a in self.addons:
            try:
                r = fn(a)
            except Exception:  # noqa: BLE001
                key = (getattr(a, "name", "?"), getattr(fn, "__name__", ""))
                if key not in self._addon_err:
                    self._addon_err.add(key)
                    L.exception("MOD '%s' の画面側でエラー", key[0])
                    self.w.message(f"⚠ MOD '{key[0]}' の画面でエラー（logs/latest.log）")
                r = None
            if first and r:
                return r
        return None

    def _usable(self, hit):
        if not hit:
            return False
        if hit["kind"] == "voxel":
            return bool(hit["construct"].devs[hit["index"]]) or bool(self._each_addon(lambda a: a.aim(hit), True))
        return bool(self._each_addon(lambda a: a.aim(hit), True))

    def nearby_targets(self, radius=2.6):
        """近くの MOD の触れる物（お店・NPC・受付など）：近い順（MOD の nearby_priority で並べ替え）。"""
        p = self.w.player
        out = []
        for a in self.addons:
            try:
                out += a.nearby(p.pos, radius) or []
            except Exception:  # noqa: BLE001
                pass

        def prio(t):
            s = 0.0
            for a in self.addons:
                fn = getattr(a, "nearby_priority", None)
                if fn is not None:
                    try:
                        s += float(fn(t) or 0.0)
                    except Exception:  # noqa: BLE001
                        pass
            return (s, t.get("dist", 0.0))
        out.sort(key=prio)
        return out

    def do_interact(self, hit):
        """E：MOD が使わなければワールドの interact（機器・席）。"""
        if not self._each_addon(lambda a: a.interact(hit), True):
            kind = None
            if hit and hit.get("kind") == "voxel" and hit["construct"].devs[hit["index"]]:
                kind = (self.w.registry.device(hit["construct"].devs[hit["index"]]["def"]) or {}).get("type")
            self.w.interact(hit)
            self.sfx("seat" if kind == "seat" else "click")
        else:
            self.sfx("click")

    # ---- 音 ----
    def sfx(self, name, volume=1.0):
        if self.snd is not None:
            self.snd.sfx(name, volume)

    def _update_sound(self, dt):
        """BGM（MOD が決める）と、近くの推進機・アクチュエーター・ジェットパックの音（今までの画面と同じ）。"""
        snd = self.snd
        if snd is None:
            return
        snd.set_bgm(self._each_addon(lambda a: a.bgm(), True))
        if self.ui.stack and getattr(self.w, "net", None) is None:
            snd.stop_loops()
        else:
            self._snd_t -= dt
            if self._snd_t <= 0:                 # 機器の様子を数えるのは 0.1 秒ごと
                from ..client import sound
                self._snd_t = 0.1
                self._snd_loops, self._snd_lit = sound.sources(self.w, self._snd_lit, self.sfx)
            for name in ("thruster_loop", "motor_loop", "jetpack_loop"):
                snd.loop(name, self._snd_loops.get(name, 0.0))
        snd.update(dt)

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
            cm = self.game.cam_mode
            if cm is not None and self.editor is None and not self.ui.stack:
                # カメラモード：パネルのボタン・ドラッグは ursina のまねへ、キーとホイールはカメラモードへ
                self.shim.event(self.view, kind, a, b, n)
                if kind.startswith("key:"):
                    k = kind[4:]
                    if a:
                        self.held.add(k)
                        if not n:
                            cm.on_key(self.KEYMAP.get(k, k))
                    else:
                        self.held.discard(k)
                elif kind == "wheel" and a:
                    cm.on_key("scroll up" if a > 0 else "scroll down")
                elif kind == "close":
                    self.quit = True
                    self.close_app = True
                continue
            if self.editor is not None and self.shim.event(self.view, kind, a, b, n):
                continue
            if self.editor is None and not self.grabbed and not self.ui.stack:     # MOD の画面の上のボタン（目標の欄など）
                if kind == "cursor":
                    self.shim.event(self.view, kind, a, b, n)
                elif kind == "button" and a and n == 0 and self.shim._hit(self.shim.mouse.x, self.shim.mouse.y):
                    self.shim.event(self.view, kind, a, b, n)
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
                if w.remove(self.hit) is not False:
                    self.sfx("break")
            elif self.hit["kind"] in ("terrain", "water"):
                if w.dig(self.hit) is not False:
                    self.sfx("dig")
        elif button == 1:
            self._place(self.hit)

    def _place(self, hit):
        w = self.w
        item = w.player.hotbar[self.sel]
        if item is None:
            w.message("アイテムバーのこの枠は空です")
            return
        _, _, _, look = self.look_vectors()
        ok = w.place(hit, item, self.dir_index, look)
        self.sfx(("device" if item[0] == "device" else "place") if ok is not False else "error")

    def open_screen(self, name):
        """一時停止メニュー・インベントリを開く（マウスを出して、歩くのを止める）。"""
        from . import screens
        self.held.clear()
        self.grabbed = False
        self.view.grab(False)
        self.game.ui_root.enabled = False          # MOD の画面の部品（目標の欄など）は隠す
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
        self.game.ui_root.enabled = False
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

    def start_hosting(self):
        """LAN に公開する（一時停止メニューから）。"""
        from ..netserver import Host
        try:
            Host(self.w, self.cfg)
        except OSError as e:
            L.exception("LAN に公開できません")
            self.w.message(f"LAN に公開できません（ポート {self.cfg.get('multiplayer', {}).get('port')} が使えない？）: {e}")
            return
        self.w.message("LAN に公開しました: " + "  ".join(self.w.net.addresses()[:2])
                       + "（参加する人はメニューの「マルチプレイ」から）")

    def screen_closed(self):
        if not self.ui.stack and self.editor is None and self.game.cam_mode is None:
            self.game.ui_root.enabled = True
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
                self.sfx("seat")
            elif k == c.get("third_person", "v"):
                self.third_person = not self.third_person
            elif k == c.get("save", "f5"):
                w.save()
            elif k == c.get("export", "f9"):
                self.export_obj()
            elif kk not in SEAT_KEYS:
                self._each_addon(lambda a: a.on_key(kk), True)        # MOD のキー（座っている間も使えるもの）
            return
        if k in "123456789" and len(k) == 1:
            self.sel = int(k) - 1
        elif k == c["rotate"]:
            self.dir_index = (self.dir_index + 1) % 6
        elif k == c["interact"]:
            hit = self.hit
            if not self._usable(hit):          # 見ている物が使えなければ、近くの MOD の物（受付・お店など）
                near = next((t for t in self.nearby_targets() if t.get("kind") == "e"), None)
                if near is not None:
                    hit = near["hit"]
            if hit is not None:
                self.do_interact(hit)
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
        elif k == c.get("third_person", "v"):
            self.third_person = not self.third_person
        elif k == c.get("camera_mode", "c"):
            self.game.enter_camera()
        elif k == c.get("export", "f9"):
            self.export_obj()
        elif k == c.get("import", "f8"):
            self.game.open_import()
        elif k not in (c["forward"], c["back"], c["left"], c["right"], c["jump"]):
            self._each_addon(lambda a: a.on_key(k), True)

    # ---- 1フレーム ----
    def frame(self, dt):
        w = self.w
        c = self.cfg["controls"]
        self._input()
        fwd, right, up, look = self.look_vectors()
        w.local_player.look_fwd, w.local_player.pitch = fwd, self.pitch
        H = lambda k: 1 if k in self.held else 0  # noqa: E731
        move = np.zeros(3)
        cam_mode = self.game.cam_mode
        if not w.player.seated and not self.ui.stack and self.editor is None and not self.game.hold and cam_mode is None:
            move += fwd * (H(c["forward"]) - H(c["back"])) + right * (H(c["right"]) - H(c["left"]))
            nm = np.linalg.norm(move)
            if nm > 1:
                move /= nm
        jump = bool(H(c["jump"])) and not w.player.seated and cam_mode is None
        thrust = (move * 0.6 + up * (H(c["jump"]) - H("control"))) if cam_mode is None else np.zeros(3)
        from ..client.game import SEAT_KEYS
        w.input_keys = {self.KEYMAP.get(k, k) for k in self.held} & set(SEAT_KEYS) if w.player.seated else set()
        tick = 1.0 / float(self.cfg["physics"].get("tick_rate", 60))
        self.acc += dt
        n = 0
        t0 = time.perf_counter()
        net = getattr(w, "net", None)
        client = net is not None and net.role == "client"
        if self.game.hold and net is None:        # MOD が世界の時間を止めている（ムービーなど）
            self.acc = 0.0
        if client:                                 # マルチプレイに参加中：ホストから届いた世界を当てる
            import types
            net.look = look
            top = self.ui.top
            net.frame(types.SimpleNamespace(editor=self.editor or (top if hasattr(top, "c") else None)), dt)
            if net.closed:
                self.game._leave("menu", status=f"マルチプレイから抜けました：{net.closed}")
                return
        while self.acc >= tick and n < 3:
            if client:
                net.step(tick, move, jump and n == 0, thrust)      # 自分の体だけ動かす
            else:
                w.step(tick, move, jump and n == 0, thrust)
            self.acc -= tick
            n += 1
        if n == 3:
            self.acc = 0.0
        if net is not None and not client:
            net.tick(dt)                           # 参加者へ送る・参加者の操作を行う
        t1 = time.perf_counter()
        origin, cam_look, cam_up = self._camera(dt, look, up)
        self._render(origin)
        self._bodies(origin)
        g = self.cfg["graphics"]
        ww, hh = self.view.size()
        fov = float(self.shim.camera.fov) if cam_mode is not None else float(g.get("fov", 80))
        hfov = np.radians(fov)                                # Panda3D と同じく横の視野角
        vfov = float(np.degrees(2 * np.arctan(np.tan(hfov / 2) * hh / max(ww, 1))))
        self.view.camera([0.0, 0.0, 0.0], [float(x) for x in cam_look], [float(x) for x in cam_up], vfov, 0.05)
        self.shim.camera.fov_pair = (float(np.degrees(hfov)), vfov)
        far = float(g.get("view_distance", 3000))
        self._each_addon(lambda a: a.update(origin, far))
        if cam_mode is not None and not cam_mode.frame_on:
            self.hit = None
            self.highlight.hide()
            self.ghost.hide()
            self._aim_txt = ""
        else:
            self._aim(origin, look)
        self._overlay(ww, hh)
        self.shim.frame(self.view, dt)                # 今までの画面の部品（プログラムの画面など）
        self._update_sound(dt)
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

    def _camera(self, dt, look, up):
        """カメラの位置・視線・上（今までの画面と同じ：カメラモード・遠隔操作・三人称・一人称）。"""
        from ..client import play
        w = self.w
        origin = w.player.eye()
        seated = w.player.seated is not None             # 座ったら三人称（降りたら元に）。V でいつでも切り替え
        if seated and not self._was_seated:
            self._third_before_seat = self.third_person
            self.third_person = True
        elif self._was_seated and not seated:
            self.third_person = getattr(self, "_third_before_seat", False)
        self._was_seated = seated
        cm = self.game.cam_mode
        if cm is not None:
            cm.update(dt, _Held(self), {}, np.zeros(2))
            pos, cl, cu, _ = cm.camera()
            return pos, cl, cu
        follow = play.seat_follow(w, self.addons) if self.third_person else None
        if follow is not None:                           # 遠隔操作：操っている物（ドローンなど）の後ろから
            com, dist = follow
            return com - look * dist + up * dist * 0.25, look, up
        if self.third_person:
            dist = 5.0
            if seated:                                   # 乗り物の大きさに合わせて離れる
                c = w.constructs.get(w.player.seated[0])
                if c is not None and c.n_vox:
                    ext = float(np.linalg.norm(c.pos.max(0) - c.pos.min(0)))
                    dist = float(np.clip(ext * 1.3, 5.0, 40.0))
            origin = play.camera_clip(w, origin, -look * dist + up * (0.8 + dist * 0.1))
        return origin, look, up

    def export_obj(self):
        """F9：見ている（座っている）構築物を OBJ で出力。"""
        w = self.w
        target = None
        if w.player.seated:
            target = w.constructs.get(w.player.seated[0])
        elif self.hit and self.hit["kind"] == "voxel":
            target = self.hit["construct"]
        if target is None:
            w.message("出力する構築物を見てください")
            return
        try:
            w.export_obj(target)
        except Exception as e:  # noqa: BLE001
            L.exception("OBJ 出力失敗")
            w.message(f"OBJ 出力失敗: {e}")

    def _aim(self, origin, look):
        """狙っている物：枠と説明（今までの画面と同じ play.aim）。"""
        from ..client import play
        w = self.w
        p = w.player
        self.hit = None if p.seated else w.raycast(p.eye(), look, float(p.defn.get("reach", 8.0)))
        h = self.hit
        if h and h["kind"] not in ("voxel", "terrain", "water"):
            r = self._each_addon(lambda a_: a_.aim(h), True)       # MOD の物（街の建物・NPC など）
            if r:
                txt, placeable = r
                self.highlight.hide()
                if placeable:
                    size = float(w.settings.get("voxel_size", 0.5))
                    fwd, right, up, _ = self.look_vectors()
                    gfx.set_box(self.ghost, h["point"] + h["normal"] * size * 0.55 - origin,
                                np.stack([right, h["normal"], fwd]), np.full(3, size))
                    self.ghost.show()
                else:
                    self.ghost.hide()
                self._aim_txt = txt
                if placeable:
                    h["placeable"] = True
                return
        a = play.aim(w, self.hit, self.look_vectors())
        for node, box in ((self.highlight, a["highlight"]), (self.ghost, a["ghost"])):
            if box is None:
                node.hide()
            else:
                gfx.set_box(node, box[0] - origin, box[1], box[2])
                node.show()
        self._aim_txt = a["text"]

    def _bodies(self, origin):
        """ほかの人（マルチプレイ）の体：MOD（VRM の見た目など）か、定義の色の箱。"""
        w = self.w
        seen = set()
        net = getattr(w, "net", None)
        others = []
        if net is not None:
            others = net.others() if callable(getattr(net, "others", None)) else list(net.others.values())
        for key, p in [("me", w.local_player)] + [(f"p{p.pid}", p) for p in others]:
            seen.add(key)
            b = self.bodies.get(key)
            if b is not None and (b._defid if hasattr(b, "_defid") else None) not in (None, p.defn.get("id")):
                b.destroy()                       # 見た目を変えた：作り直す
                b = self.bodies[key] = None
            if b is None:
                mb = self._each_addon(lambda a: a.player_body(p), True)      # MOD が体の見た目を引き受ける
                if mb is not None:
                    self.bodies[key] = mb
                    b = mb
            if b is None:
                from ..client.game import BoxBody
                b = self.bodies[key] = BoxBody(self.game, p)            # MOD が引き受けなければ定義の色の箱
            if not hasattr(b, "_defid"):
                try:
                    b._defid = p.defn.get("id")
                except AttributeError:
                    pass
            if key == "me":                       # 自分：三人称・カメラモードのときだけ
                shown = self.third_person or self.game.cam_mode is not None
            else:
                q = p.pos - origin
                shown = float(q @ q) < 500.0 ** 2
                self._label(key, p, origin, shown)
            try:
                b.update(origin, shown, 1 / 60)
            except Exception:  # noqa: BLE001
                if ("body", type(b).__name__) not in self._addon_err:
                    self._addon_err.add(("body", type(b).__name__))
                    L.exception("体の見た目でエラー")
        for key in [k for k in self.bodies if k not in seen]:
            self.bodies.pop(key).destroy()
        for key in [k for k in self.labels if k not in seen or k == "me"]:
            self.labels.pop(key).remove()

    def _label(self, key, p, origin, shown):
        """ほかの人の頭の上の名前（いつもカメラの方を向く）。"""
        t = self.labels.get(key)
        if t is None:
            t = self.labels[key] = gfx.RsText(self.root, overlay=False, bg=(0.02, 0.03, 0.05, 0.55))
        w_, h_ = t.set(p.name or "?", 0.22)
        t.show(shown)
        if not shown:
            return
        _, right, up, look = self.look_vectors()
        cu = np.cross(look, right)                       # カメラの上
        c = p.pos + p.up * (p.height - p.radius + 0.35) - origin
        o = c - right * (w_ / 2) + cu * (h_ / 2)
        t.node.setMat([*right, 0, *cu, 0, *(-look), 0, *o, 1])

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
        cm = self.game.cam_mode
        shown = self.editor is None and (cm is None or cm.ui_on)
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
        net = getattr(w, "net", None)
        if net is not None and net.role == "client":
            lines.append(f"マルチプレイ参加中（ホスト {net.host_name}）  {1 + len(net.others)} 人")
        elif net is not None:
            lines.append(f"LAN に公開中 {'  '.join(net.addresses()[:1])}  {1 + len(w.remote)} 人")
        if getattr(self, "fps_text", ""):
            lines.append(self.fps_text)
        if not self.grabbed and not self.ui.stack:
            lines.append("クリックで操作（マウスで見回す）・Esc でメニュー・I でインベントリ")
        if self.frame_no % 10 == 0:
            near = None
            if not p.seated and not self._usable(self.hit):
                near = next((t for t in self.nearby_targets() if t.get("kind") == "e"), None)
            self._near_txt = f"［E］{near['label']}" if near is not None else ""
        aspect = ww / max(hh, 1)
        self.hud.set("\n".join(lines), 0.026)
        self.hud.place(-aspect / 2 + 0.015, 0.5 - 0.015)
        now = time.time()
        msgs = [t for ts, t in list(w.msgs)[-5:] if now - ts < 8.0]
        _, h = self.msg.set("\n".join(msgs), 0.024, max_width=aspect * 0.6)
        self.msg.show(bool(msgs))
        self.msg.place(-aspect / 2 + 0.015, -0.5 + 0.115 + h)          # アイテムバーより上
        at = getattr(self, "_aim_txt", "") or getattr(self, "_near_txt", "")     # 何も狙っていなければ近くの MOD の物
        tw, th = self.aim_text.set(at, 0.022, max_width=aspect * 0.5)
        self.aim_text.show(bool(at))
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
        if self.snd is not None:
            self.snd.set_bgm(None)
            self.snd.set_override(None)
            self.snd.stop_loops()
            self.snd.update(10.0)               # 曲を止める
        self._each_addon(lambda a: a.destroy())
        self.addons = []
        for b in self.bodies.values():
            try:
                b.destroy()
            except Exception:  # noqa: BLE001
                pass
        self.bodies = {}
        for t in self.labels.values():
            t.remove()
        self.labels = {}
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


class _Held:
    """カメラモードに渡す「押しているキー」（ursina の held_keys の形）。"""

    def __init__(self, v):
        self.keys = {v.KEYMAP.get(k, k) for k in v.held}

    def __getitem__(self, k):
        return 1 if k in self.keys else 0


class _Pointers:
    """カメラモードのドラッグ：マウスの左ボタンを押している間、指1本として位置を返す。"""

    def current(self):
        from . import ursina_shim as U
        return {0: (U.mouse.x, U.mouse.y)} if U.mouse.left else {}


class _TouchStub:
    def __init__(self):
        self.ptr = _Pointers()

    def _release_all(self):
        pass


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
        self.scene_root = viewer.root          # MOD の 3D の物（DynamicMesh・SceneView）を付ける所
        self.hold = False                      # MOD が世界の時間を止める（ムービーなど）
        self.cam_mode = None
        self.emotes = {}
        self.leaving = False
        self.load_errors = []
        self.touch_hud = _TouchStub()

    @property
    def addons(self):
        return getattr(self.v, "addons", [])

    @property
    def third_person(self):
        return self.v.third_person

    @third_person.setter
    def third_person(self, on):
        self.v.third_person = bool(on)

    def _camera_hook(self, on):
        self.v._each_addon(lambda a: a.on_camera(on))

    def enter_camera(self):
        """C：カメラモード（今までの画面の CameraMode を ursina のまねで）。"""
        if self.cam_mode is not None or self.editor is not None or self.v.ui.stack or self.world.player.seated:
            return
        from ..client.camera_mode import CameraMode
        self.v.held.clear()
        self.cam_mode = CameraMode(self)
        self._camera_hook(True)

    def open_import(self):
        """F8：読み込み（設計図・OBJ）。"""
        from ..client.import_ui import ImportPanel
        self._open_panel(lambda: ImportPanel(self))

    def open_keys(self):
        """操作とキー設定。"""
        from ..client.keys_panel import KeysPanel
        extra = [a.help_text for a in self.addons if a.help_text]
        self._open_panel(lambda: KeysPanel(self.U, self.cfg, self.ui_root.parent, self.editor_closed, extra))

    @property
    def paused(self):
        return bool(self.v.ui.stack)

    def look_vectors(self):
        return self.v.look_vectors()

    def exit_camera(self):
        if self.cam_mode is None:
            return
        self.cam_mode.destroy()
        self.cam_mode = None
        self._camera_hook(False)
        self.ui_root.enabled = True
        self.set_mouse_capture(True)

    def _open_panel(self, make):
        """MOD の画面（今までの ursina の部品）を開く。"""
        self.set_mouse_capture(False)
        self.v.held.clear()
        self.ui_root.enabled = False
        try:
            self.editor = make()
        except Exception as e:  # noqa: BLE001
            L.exception("画面を開けませんでした")
            root = getattr(getattr(self, "editor", None), "root", None)
            self.editor = None
            if root is not None:
                from . import ursina_shim
                ursina_shim.destroy(root)
            self.editor_closed()
            self.world.message(f"⚠ 画面を開けませんでした: {e}")

    def _leave(self, action, save=True, status=""):
        """ワールドを出る（action："menu"・"play:名前"）。"""
        if self.leaving:
            return
        self.leaving = True
        self.v.leave_action = (action, save, status)
        self.v.quit = True

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

    def sfx(self, name, volume=1.0):
        self.v.sfx(name, volume)
