"""街の画面：取引（商人）・部屋の売買と賃貸（不動産屋）・収納箱。閉じるボタン（または Esc）で閉じる。"""
import numpy as np

from ... import log
from . import citylife

L = log.get("city_ui")


class _Panel:
    TITLE = ""

    def __init__(self, game, city):
        from ursina import Entity, Text, Button, camera, color, mouse
        self.game = game
        self.world = game.world
        self.reg = game.world.registry
        self.pack = game.world.addon("city")
        self.eco = self.pack.economy
        self.city = city
        self.U = dict(Entity=Entity, Text=Text, Button=Button, color=color)
        mouse.locked = False
        self.status = ""
        self.root = Entity(parent=camera.ui, z=-0.3)
        bg = Button(parent=self.root, model="quad", scale=(3, 2), color=color.rgba(0, 0, 0, 0.55),
                    highlight_color=color.rgba(0, 0, 0, 0.55), pressed_color=color.rgba(0, 0, 0, 0.55), z=0.1)
        bg.on_click = lambda: None
        Entity(parent=self.root, model="quad", scale=(1.5, 0.9), position=(0, 0, 0.05), color=color.rgb(0.07, 0.08, 0.11))
        self.title = Text(parent=self.root, text=self.TITLE, position=(-0.72, 0.42), origin=(-0.5, 0.5), scale=1.25)
        self.sub = Text(parent=self.root, text="", position=(-0.72, 0.36), origin=(-0.5, 0.5), scale=0.72,
                        color=color.light_gray)
        self.body = Entity(parent=self.root)
        self.status_text = Text(parent=self.root, text="", position=(-0.72, -0.37), origin=(-0.5, 0.5), scale=0.75,
                                color=color.yellow)
        self._btn("閉じる", (0.62, -0.39), self.close, w=0.16, col=color.rgb(0.3, 0.3, 0.33))
        panel = self

        class Keys(Entity):
            def input(self_, key):
                if key == "escape":
                    panel.close()
        self.keys = Keys(parent=self.root)

    def _btn(self, text, pos, fn, w=0.1, h=0.04, col=None, parent=None, ts=0.65):
        B, color = self.U["Button"], self.U["color"]
        b = B(parent=parent or self.root, text=text, position=pos, scale=(w, h),
              color=col or color.rgb(0.22, 0.32, 0.45), text_size=ts)
        b.on_click = fn
        return b

    def _text(self, text, pos, scale=0.7, col=None, origin=(-0.5, 0)):
        T, color = self.U["Text"], self.U["color"]
        return T(parent=self.body, text=text, position=pos, origin=origin, scale=scale, color=col or color.white)

    def _clear(self):
        from ...client.ui_destroy import destroy
        for ch in list(self.body.children):
            destroy(ch)

    def act(self, fn):
        def run():
            msg = fn()
            if msg:
                self.status = msg
                self.world.message(msg)
            self.refresh()
        return run

    def refresh(self):
        pass

    def close(self):
        from ...client.ui_destroy import destroy
        destroy(self.root)
        self.game.editor_closed()


class TradePanel(_Panel):
    """商人との取引。買値＝店から買う値段、売値＝店に売る値段（1単位あたり）。"""
    TITLE = "取引"

    def __init__(self, game, city, npc):
        super().__init__(game, city)
        self.npc = npc
        info = citylife.region_info(city)
        self.title.text = f"{npc['name']}（商人）  {city.name}"
        self.sub.text = f"{info['name']}：{info['note']}"
        self.refresh()

    def refresh(self):
        self._clear()
        color = self.U["color"]
        P = citylife.prices(self.city)
        inv = self.world.player.inventory
        y = 0.28
        self._text("品物", (-0.7, y), 0.7, color.light_gray)
        self._text("手持ち", (-0.42, y), 0.7, color.light_gray)
        self._text("買値", (-0.27, y), 0.7, color.light_gray)
        self._text("売値", (-0.15, y), 0.7, color.light_gray)
        for res, (buy, sell) in P.items():
            y -= 0.062
            self._text(self.reg.res_name(res), (-0.7, y))
            self._text(f"{inv.get(res, 0.0):.0f}", (-0.42, y))
            self._text(f"{buy:.2f}", (-0.27, y), col=color.rgb(1, 0.75, 0.6))
            self._text(f"{sell:.2f}", (-0.15, y), col=color.rgb(0.6, 1, 0.7))
            x = 0.0
            for lab, fn, col in ((f"買う 1", lambda r=res: self.eco.buy(self.city, r, 1), None),
                                 (f"買う 10", lambda r=res: self.eco.buy(self.city, r, 10), None),
                                 (f"売る 1", lambda r=res: self.eco.sell(self.city, r, 1), color.rgb(0.2, 0.45, 0.3)),
                                 (f"売る 10", lambda r=res: self.eco.sell(self.city, r, 10), color.rgb(0.2, 0.45, 0.3)),
                                 (f"全部売る", lambda r=res: self.eco.sell(self.city, r, inv.get(r, 0.0)),
                                  color.rgb(0.25, 0.5, 0.35))):
                self._btn(lab, (x + 0.05, y), self.act(fn), w=0.1 if lab != "全部売る" else 0.12, parent=self.body)
                x += 0.115
        self._text(f"所持金  {self.eco.credits():.1f} クレジット", (-0.7, y - 0.065), 0.85, color.rgb(1.0, 0.85, 0.3))
        self._text("値段は1単位あたり。街の地域ごとに違います（軌道都市では燃料がとても高い など）。",
                   (-0.7, y - 0.105), 0.6, color.light_gray)
        self.status_text.text = self.status


class EstatePanel(_Panel):
    TITLE = "部屋（不動産屋）"

    def __init__(self, game, city, npc):
        super().__init__(game, city)
        self.npc = npc
        self.title.text = f"{npc['name']}（不動産屋）  {city.name}"
        self.sub.text = ("部屋には収納箱とベッドがあります。ベッドで休むとそこが自宅に（M キーで目印）。"
                         f"借りると {citylife.RENT_SECONDS / 60:.0f} 分（ゲーム内の時間）使えます。")
        self.refresh()

    def refresh(self):
        self._clear()
        color = self.U["color"]
        y = 0.27
        if not self.city.homes:
            self._text("この街には部屋がありません", (-0.7, y))
        for h in self.city.homes:
            usable = self.city.state[h["bld"]] < 2
            st = self.eco.home_status(h["id"])
            if not usable:
                txt = "（建物が壊れていて使えません）"
            elif st == "own":
                txt = "あなたの部屋"
            elif st == "rent":
                txt = f"借りている（残り {self.eco.rent_left(h['id']) / 60:.0f} 分）"
            elif st == "expired":
                txt = "借りていた（期限切れ・荷物は預かり中）"
            else:
                txt = "空き"
            price = citylife.home_price(self.city, h)
            rent = citylife.rent_price(self.city, h)
            self._text(h["name"], (-0.7, y))
            self._text(txt, (-0.38, y), 0.65, color.light_gray)
            if usable and st != "own":
                self._btn(f"買う {price:.0f}", (0.2, y), self.act(lambda h=h: self.eco.buy_home(self.city, h)),
                          w=0.16, parent=self.body, col=color.rgb(0.25, 0.5, 0.35))
                self._btn(("延長 " if st == "rent" else "借りる ") + f"{rent:.0f}", (0.38, y),
                          self.act(lambda h=h: self.eco.rent_home(self.city, h)), w=0.16, parent=self.body)
            y -= 0.07
        self._text(f"所持金  {self.eco.credits():.1f} クレジット", (-0.7, y - 0.04), 0.85, color.rgb(1.0, 0.85, 0.3))
        esc = self.eco.state.get("escrow") or {}
        if esc:
            self._text("預かり品（滅びた街の部屋の収納箱から）：" + self.reg.res_text(esc), (-0.7, y - 0.1), 0.62,
                       color.light_gray)
            self._btn("預かり品を受け取る", (0.35, y - 0.1), self.act(self.eco.claim_escrow), w=0.24, parent=self.body,
                      col=color.rgb(0.25, 0.5, 0.35))
        self.status_text.text = self.status


class StoragePanel(_Panel):
    TITLE = "収納箱"

    def __init__(self, game, city, home):
        super().__init__(game, city)
        self.home = home
        self.title.text = f"収納箱  {home['name']}"
        self.sub.text = "手持ちの資源を預けたり、引き出したりできます（クレジットも預けられます）。"
        self.refresh()

    def refresh(self):
        self._clear()
        color = self.U["color"]
        inv = self.world.player.inventory
        box = self.eco.storage.get(self.home["id"], {})
        keys = sorted(set(k for k, v in inv.items() if v > 0) | set(k for k, v in box.items() if v > 0))
        y = 0.28
        self._text("資源", (-0.7, y), 0.7, color.light_gray)
        self._text("手持ち", (-0.45, y), 0.7, color.light_gray)
        self._text("箱の中", (-0.3, y), 0.7, color.light_gray)
        if not keys:
            self._text("手持ちも箱の中も空です", (-0.7, y - 0.07))
        for k in keys[:9]:
            y -= 0.065
            self._text(self.reg.res_name(k), (-0.7, y))
            self._text(f"{inv.get(k, 0.0):.0f}", (-0.45, y))
            self._text(f"{box.get(k, 0.0):.0f}", (-0.3, y))
            x = -0.12
            for lab, amt in (("預ける 10", 10), ("全部預ける", 1e12), ("出す 10", -10), ("全部出す", -1e12)):
                self._btn(lab, (x + 0.06, y), self.act(lambda k=k, a=amt: (self.eco.store(self.home["id"], k, a), None)[1]),
                          w=0.13, parent=self.body, col=color.rgb(0.25, 0.4, 0.3) if amt > 0 else None)
                x += 0.145
        self.status_text.text = self.status


# ---------------------------------------------------------------------------
# マップ（訪れた都市）と都市への助言
# ---------------------------------------------------------------------------
REGION_COL = {"plain": (0.75, 0.75, 0.8), "coast": (0.3, 0.7, 1.0), "volcano": (1.0, 0.45, 0.2),
              "orbit": (0.85, 0.45, 1.0), "frontier": (0.6, 1.0, 0.6)}


class MapPanel(_Panel):
    """訪れた都市の地図。天体ごとに経度・緯度で並べる（軌道都市は今いる真下の位置）。
    都市を選ぶと、ようす（緊張・計画・防御）と、助言のボタンが出る。"""
    TITLE = "マップ（訪れた都市）"
    MX0, MX1, MY0, MY1 = -0.74, 0.02, -0.14, 0.26

    def __init__(self, game):
        super().__init__(game, None)
        cm = self.pack.cities
        self.pol = self.pack.politics
        self.cm = cm
        pl = self.world.nearest_planet(self.world.player.pos)
        self.map_root = self.U["Entity"](parent=self.root)
        self.side_root = self.U["Entity"](parent=self.root)
        self.planet = pl
        self.sel = None
        self.target_mode = False
        from . import politics as P
        self.sub.text = (f"都市を選ぶと、通信で助言できます（通信費 {P.ADVICE_FEE:.0f} クレジット・"
                         f"同じ都市へは {P.ADVICE_WAIT / 60:.0f} 分あける）。閉じる：閉じるボタン / Esc")
        self._t = 0.0
        panel = self

        class Tick(self.U["Entity"]):
            def update(self_):
                import time as _t
                if _t.time() - panel._t > 1.0:
                    panel._t = _t.time()
                    panel.refresh_side()
        self.tick = Tick(parent=self.root)
        self.refresh()

    # --- 中身 ---------------------------------------------------------------
    def visited_cities(self):
        return [c for c in self.cm.cities if self.pol.visited(c)]

    def bodies(self):
        out = []
        for c in self.visited_cities():
            if c.host not in out:
                out.append(c.host)
        if self.planet is not None and self.planet not in out:
            out.insert(0, self.planet)
        return out

    def _xy(self, pl, p):
        d = np.asarray(p, float) - pl.center
        d /= max(np.linalg.norm(d), 1e-9)
        lon = np.arctan2(d[2], d[0])
        lat = np.arcsin(np.clip(d[1], -1, 1))
        x = self.MX0 + (lon + np.pi) / (2 * np.pi) * (self.MX1 - self.MX0)
        y = (self.MY0 + self.MY1) / 2 + lat / np.pi * (self.MY1 - self.MY0)
        return x, y

    def refresh(self):
        from ...client.ui_destroy import destroy
        for ch in list(self.map_root.children):
            destroy(ch)
        U, color = self.U, self.U["color"]
        E, T = U["Entity"], U["Text"]
        # 天体のタブ
        for k, pl in enumerate(self.bodies()[:6]):
            on = pl is self.planet
            b = self._btn(pl.name, (-0.66 + k * 0.15, 0.3), (lambda pl=pl: self._set_planet(pl)), w=0.14,
                          parent=self.map_root, col=color.rgb(0.3, 0.45, 0.65) if on else color.rgb(0.18, 0.2, 0.26))
        pl = self.planet
        w_, h_ = self.MX1 - self.MX0, self.MY1 - self.MY0
        cx, cy = (self.MX0 + self.MX1) / 2, (self.MY0 + self.MY1) / 2
        base = np.asarray(pl.defn.get("color", [0.3, 0.4, 0.3])) if pl is not None else np.array([0.2, 0.2, 0.25])
        E(parent=self.map_root, model="quad", scale=(w_, h_), position=(cx, cy, 0.02),
          color=color.rgb(*(base * 0.45 + 0.04)))
        for gx in range(1, 4):   # 経線
            E(parent=self.map_root, model="quad", scale=(0.002, h_), position=(self.MX0 + w_ * gx / 4, cy, 0.01),
              color=color.rgba(1, 1, 1, 0.12))
        E(parent=self.map_root, model="quad", scale=(w_, 0.002), position=(cx, cy, 0.01), color=color.rgba(1, 1, 1, 0.12))
        if pl is None:
            return
        from . import politics as P
        for c in self.visited_cities():
            if c.host is not pl:
                continue
            x, y = self._xy(pl, c.x)
            s = self.pol.st(c)
            fallen = getattr(c, "fallen", False)
            col = (0.4, 0.4, 0.42) if fallen else REGION_COL.get(c.region, (1, 1, 1))
            size = 0.026 if c is self.sel else 0.018
            if s.get("plan") and not fallen:
                E(parent=self.map_root, model="quad", scale=size * 1.9, position=(x, y, 0.005),
                  color=color.rgb(1.0, 0.15, 0.15), rotation_z=45 if c.kind == "orbit" else 0)
            b = U["Button"](parent=self.map_root, model="quad", scale=size, position=(x, y, 0),
                            color=color.rgb(*col), rotation_z=45 if c.kind == "orbit" else 0)
            b.on_click = (lambda c=c: self._select(c))
            label = c.name + ("（廃墟）" if fallen else "") + ("  ⚠" if s.get("plan") and not fallen else "")
            T(parent=self.map_root, text=label.replace("⚠", "!"), position=(x + 0.016, y), origin=(-0.5, 0), scale=0.5,
              color=color.rgb(1, 0.5, 0.5) if s.get("plan") else color.light_gray)
        # プレイヤー
        p = self.world.player.pos
        if self.world.nearest_planet(p) is pl:
            x, y = self._xy(pl, p)
            E(parent=self.map_root, model="quad", scale=0.012, position=(x, y, -0.01), color=color.white, rotation_z=45)
            T(parent=self.map_root, text="現在地", position=(x, y - 0.018), origin=(0, 0.5), scale=0.45)
        # 凡例
        y0 = self.MY0 - 0.035
        for k, (rg, nm) in enumerate((("plain", "平地"), ("coast", "海辺"), ("volcano", "火山"), ("orbit", "軌道"),
                                      ("frontier", "辺境"))):
            E(parent=self.map_root, model="quad", scale=0.014, position=(self.MX0 + 0.01 + k * 0.12, y0),
              color=color.rgb(*REGION_COL[rg]), rotation_z=45 if rg == "orbit" else 0)
            T(parent=self.map_root, text=nm, position=(self.MX0 + 0.025 + k * 0.12, y0), origin=(-0.5, 0), scale=0.5,
              color=color.light_gray)
        T(parent=self.map_root, text="赤い枠＝征服・攻撃を計画中", position=(self.MX0 + 0.62, y0),
          origin=(-0.5, 0), scale=0.45, color=color.rgb(1, 0.5, 0.5))
        self.refresh_side()

    def _set_planet(self, pl):
        self.planet = pl
        self.refresh()

    def _select(self, c):
        self.sel = c
        self.target_mode = False
        self.refresh()

    def refresh_side(self):
        from ...client.ui_destroy import destroy
        from . import politics as P
        for ch in list(self.side_root.children):
            destroy(ch)
        U, color = self.U, self.U["color"]
        T = U["Text"]
        x0 = 0.08
        # ニュース
        lines = [f"{(self.world.time - t) / 60:4.0f} 分前  {txt}" for t, txt in self.pol.news[-5:]][::-1]
        T(parent=self.side_root, text="ニュース\n" + ("\n".join(lines) if lines else "（まだありません）"),
          position=(-0.72, -0.2), origin=(-0.5, 0.5), scale=0.55, color=color.light_gray)
        c = self.sel
        if c is None:
            T(parent=self.side_root, text="地図の都市を選んでください。\n\n都市は、短い間に取引しすぎると野心を、\n"
              "訪れてから長く取引しないと不満をためて、\nほかの都市を征服・攻撃しようとします。",
              position=(x0, 0.3), origin=(-0.5, 0.5), scale=0.62, color=color.light_gray)
            self.status_text.text = self.status
            return
        s = self.pol.st(c)
        info = citylife.region_info(c)
        fallen = getattr(c, "fallen", False)
        t = self.world.time
        bar_n = int(round(s["tension"] / 10))
        lines = [c.name + ("（廃墟）" if fallen else ""),
                 f"{info['name']}：{info['note']}"]
        if s.get("owner"):
            lines.append(f"支配：{self.pol._name(s['owner'])}")
        if not fallen:
            lines.append(f"緊張 {'■' * bar_n}{'□' * (10 - bar_n)} {s['tension']:.0f}")
            if s.get("motive") and s["tension"] > 5:
                lines.append(f"理由：{P.MOTIVE[s['motive']]}")
            plan = s.get("plan")
            if plan:
                lines.append(f"計画：{self.pol._name(plan['target'])} への{P.ACTION[plan['type']]}"
                             f"（あと {max(0, plan['eta'] - t) / 60:.0f} 分）")
            if s["defense"] and s["defense_until"] > t:
                lines.append(f"防御 {s['defense']}（あと {(s['defense_until'] - t) / 60:.0f} 分）")
            lt = s.get("last_trade")
            lines.append(f"最後の取引：{'なし' if lt is None else f'{(t - lt) / 60:.0f} 分前'}　取引の勢い {s['heat']:.0f}")
            lines.append(f"力（建物）{c.alive()}")
        T(parent=self.side_root, text="\n".join(lines), position=(x0, 0.3), origin=(-0.5, 0.5), scale=0.62)
        if not fallen:
            y = -0.02
            self._btn(f"説得する（緊張を下げる）", (x0 + 0.15, y), self.act(lambda: self.pol.persuade(c)), w=0.3,
                      parent=self.side_root, col=color.rgb(0.25, 0.45, 0.35))
            self._btn(f"防御を促す", (x0 + 0.47, y), self.act(lambda: self.pol.urge_defense(c)), w=0.24,
                      parent=self.side_root)
            if s.get("plan"):
                y -= 0.055
                T(parent=self.side_root, text="攻撃の目標を変える：", position=(x0, y), origin=(-0.5, 0), scale=0.6)
                for k, tc in enumerate([x for x in self.pol.candidates(c, 7) if x.id != s["plan"]["target"]][:6]):
                    yy = y - 0.048 * (1 + k // 2)
                    xx = x0 + 0.15 + (k % 2) * 0.32
                    self._btn(f"→ {tc.name}", (xx, yy), self.act(lambda tc=tc: self.pol.retarget(c, tc)), w=0.3,
                              parent=self.side_root, col=color.rgb(0.5, 0.25, 0.25))
        self.status_text.text = self.status

    def act(self, fn):
        def run():
            msg = fn()
            if msg:
                self.status = msg
                self.world.message(msg)
            self.refresh()
        return run
