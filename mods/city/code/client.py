"""街パック（画面側）：街と NPC の描画、目印、マップ（K）、NPC・家具の操作、視線の先の説明。"""
import time as _time

import numpy as np

from main.client.addon_hooks import ClientAddon
from . import citylife


class CityClient(ClientAddon):
    name = "city"
    help_text = "M 目印（街・自宅） / K マップ"

    def __init__(self, game):
        super().__init__(game)
        self.pack = game.world.addon("city")
        U = game.U
        Entity, Text, color = U["Entity"], U["Text"], U["color"]
        self.city_views = {}
        self.npc_views = {}
        # 最寄りの街の目印（画面の外なら端に矢印）
        self.marker_on = True
        self.marker_mode = "city"
        self.marker = Entity(parent=game.ui_root, enabled=False)
        Entity(parent=self.marker, model="quad", scale=0.022, rotation_z=45, color=color.rgb(1.0, 0.25, 0.85))
        Entity(parent=self.marker, model="quad", scale=0.012, rotation_z=45, color=color.rgb(0.1, 0.95, 1.0), z=-0.01)
        self.marker_text = Text(parent=self.marker, position=(0, -0.02), origin=(0, 0.5), scale=0.7, text="",
                                color=color.rgb(0.6, 1.0, 1.0))
        self._marker_t = 0.0
        self._marker_city = None

    def destroy(self):
        for v in self.city_views.values():
            v.remove()
        self.city_views = {}
        for v in self.npc_views.values():
            v.remove()
        self.npc_views = {}

    # ------------------------------------------------------------------
    # キー
    # ------------------------------------------------------------------
    def on_key(self, key):
        c = self.game.cfg["controls"]
        w = self.world
        if key == c.get("city_map", "k"):
            if not self.pack.enabled:
                w.message("このワールドには街がありません")
            else:
                from . import ui
                self.game._open_panel(lambda: ui.MapPanel(self.game))
            return True
        if key == c.get("city_marker", "m"):
            # 目印：最寄りの街 → 自宅（あれば）→ なし
            modes = ["city"] + (["home"] if self.pack.home_id() else []) + ["off"]
            cur = self.marker_mode if self.marker_mode in modes else "off"
            self.marker_mode = modes[(modes.index(cur) + 1) % len(modes)]
            self.marker_on = self.marker_mode != "off"
            self._marker_city = None
            w.message("目印: " + {"city": "最寄りの街", "home": "自宅", "off": "なし"}[self.marker_mode])
            return True
        return False

    # ------------------------------------------------------------------
    # E：NPC・家具（カウンター・ベッド・収納箱）
    # ------------------------------------------------------------------
    def interact(self, h):
        w = self.world
        if not h or h["kind"] not in ("npc", "city"):
            return False
        from . import ui
        game = self.game
        city = h["city"]
        survival = not w.creative
        npc = h.get("npc")
        if h["kind"] == "city":
            tag = city.parts[h["part"]].get("tag")
            if not tag:
                return False
            if tag[0] == "counter":
                npc = next((n for n in city.npcs if n["id"] == tag[1]), None)
                if npc is None or not citylife.npc_alive(city, npc):
                    return True
            else:
                home = next((x for x in city.homes if x["id"] == tag[1]), None)
                if home is None:
                    return True
                if city.state[home["bld"]] >= 2:
                    w.message("建物が壊れていて使えません")
                    return True
                if not self.pack.can_use_home(home["id"]):
                    w.message(f"{home['name']} は使えません（不動産屋で買うか借りると使えます）")
                    return True
                if tag[0] == "bed":
                    self.pack.set_home(home["id"])
                    w.save()
                    w.message(f"{home['name']} で休みました（保存しました）。ここが自宅です（M で自宅の目印）")
                    return True
                game._open_panel(lambda: ui.StoragePanel(game, city, home))
                return True
        if npc is None:
            return False
        if getattr(city, "fallen", False):
            w.message(f"{city.name} は滅びてしまいました")
            return True
        if npc["role"] == "trader" and survival:
            game._open_panel(lambda: ui.TradePanel(game, city, npc))
        elif npc["role"] == "estate" and survival:
            game._open_panel(lambda: ui.EstatePanel(game, city, npc))
        else:
            w.message(f"{npc['name']}「{citylife.talk_line(city, npc)}」")
        return True

    # ------------------------------------------------------------------
    # 視線の先の説明
    # ------------------------------------------------------------------
    def aim(self, h):
        w = self.world
        if not h or h["kind"] not in ("npc", "city"):
            return None
        if h["kind"] == "npc":
            n = h["npc"]
            act = {"trader": "取引する", "estate": "部屋を見る", "citizen": "話す"}[n["role"]]
            if w.creative and n["role"] != "citizen":
                act = "話す（取引はサバイバルだけ）"
            return f"{n['name']}（{citylife.ROLE_NAMES[n['role']]}）［E {act}］", False
        city = h["city"]
        tag = city.parts[h["part"]].get("tag")
        if tag:
            if tag[0] == "counter":
                return "カウンター［E 取引する］", False
            home = next((x for x in city.homes if x["id"] == tag[1]), None)
            name = home["name"] if home else "部屋"
            if not self.pack.can_use_home(tag[1]):
                return f"{name} の{'ベッド' if tag[0] == 'bed' else '収納箱'}（不動産屋で買うか借りると使えます）", False
            if tag[0] == "bed":
                return f"{name} のベッド［E 休む：保存して、ここを自宅に］", False
            return f"{name} の収納箱［E 開く］", False
        st = int(city.state[city.bld[h["part"]]]) if city.bld[h["part"]] >= 0 else 0
        kind_ = city.parts[h["part"]]["kind"]
        what = {"ground": "地面", "stair": "階段"}.get(kind_, "建物")
        txt = f"{city.name} の{what}" + ("（壊れている）" if st == 1 else "")
        if city.kind == "orbit":
            txt += "\n右クリックで置いたブロックは街に固定（推進機で街ごと動かせる）"
        return txt, True

    # ------------------------------------------------------------------
    # 描画
    # ------------------------------------------------------------------
    def update(self, origin, far):
        if not self.pack.enabled:
            self.marker.enabled = False
            return
        from . import views
        g = self.game.cfg["graphics"]
        detail = float(g.get("city_detail_distance", 100.0))
        far_c = min(far, float(g.get("city_view_distance", 2500.0)))
        root = self.game.scene_root
        for c in self.pack.city_list():
            v = self.city_views.get(c.id)
            if v is None:
                v = self.city_views[c.id] = views.CityView(root, c)
            v.update(origin, origin, far_c, detail)
            if c.npcs:
                nv = self.npc_views.get(c.id)
                if nv is None:
                    nv = self.npc_views[c.id] = views.NpcView(root, c)
                nv.update(origin, self.world.time)
        self._update_marker(origin)

    def _update_marker(self, origin):
        """最寄りの街の方向に目印。画面の外にあるときは画面の端に寄せ、矢印で向きを示す。"""
        if not self.marker_on or not self.pack.city_list():
            self.marker.enabled = False
            return
        now = _time.time()
        if now - self._marker_t > 0.5 or self._marker_city is None:
            self._marker_t = now
            self._marker_city, _, self._marker_surface = self.pack.marker_target(origin)
        c = self._marker_city
        home = None
        if self.marker_mode == "home":
            hc, hh = self.pack.home()
            if hc is not None:
                c, home = hc, hh
        if c is None:
            self.marker.enabled = False
            return
        U = self.game.U
        # 街の中心より少し上（地表の街ならビルの高さの辺り）を指す
        up = c.R[:, 1] if c.kind != "surface" else (c.x - c.host.center) / np.linalg.norm(c.x - c.host.center)
        target = c.x + up * 15.0
        if home is not None:
            target = c.x + c.R @ home["door"] + up * 1.5
        rel = target - origin
        dist = float(np.linalg.norm(rel))
        # カメラの向き（cam.lookAt(look, up) と同じ）で、画面のどこに映るかを計算する
        # ursina は左手系（y-up-left）なので、画面の右は up × look（look_vectors の right と同じ向き）
        _, _, cup, look = self.game.look_vectors()
        right = np.cross(cup, look)
        right /= max(np.linalg.norm(right), 1e-9)
        upo = np.cross(look, right)
        xc, yc, zc = float(rel @ right), float(rel @ upo), float(rel @ look)
        fov = U["application"].base.cam.node().getLens().getFov()
        tx, ty = np.tan(np.radians(fov[0]) / 2), np.tan(np.radians(fov[1]) / 2)
        aspect = U["window"].aspect_ratio
        inside = False
        if zc > 0.1:
            fx, fy = xc / zc / tx, yc / zc / ty
            inside = abs(fx) <= 0.95 and abs(fy) <= 0.9
        dtxt = f"{dist / 1000:.1f} km" if dist >= 1000 else f"{dist:.0f} m"
        if inside:
            x, y = fx * aspect / 2, fy * 0.5
            arrow = ""
        else:
            dx, dy = xc, yc
            if abs(dx) < 1e-6 and abs(dy) < 1e-6:
                dx = 1.0
            L_ = max((dx * dx + dy * dy) ** 0.5, 1e-9)
            dx, dy = dx / L_, dy / L_
            hx, hy = aspect / 2 - 0.12, 0.5 - 0.1
            k = min(hx / max(abs(dx), 1e-9), hy / max(abs(dy), 1e-9))
            x, y = dx * k, max(dy * k, -0.33)     # 下はアイテムバーに重ならないように
            arrow = ("◀ " if dx < 0 else "▶ ") if abs(dx) >= abs(dy) else ("▲ " if dy > 0 else "▼ ")
        self.marker.position = (x, y, -0.05)
        self.marker_text.text = f"{arrow}{home['name'] + '（自宅）' if home else c.name}  {dtxt}"
        self.marker.enabled = True
