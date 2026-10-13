"""機器の設定メニュー（P キー）：名前の変更など。

同じ名前の機器はコンピューターからまとめて動くので、名前を分けると個別に制御できる。
日本語は直接入力できないため、よく使う語（左・右・前・後…）や番号をボタンで付け足せる。
"""
from .. import log

L = log.get("device_ui")

TYPE_NAMES = {
    "thruster": "推進機", "tank": "タンク", "pipe": "配管", "valve": "バルブ", "battery": "バッテリー",
    "generator": "発電機", "crank": "手回し発電機", "computer": "コンピューター", "actuator": "アクチュエーター",
    "processor": "精錬機・加工機", "seat": "操縦席", "air_vent": "通気口", "antenna": "アンテナ",
}
NAME_LIMIT = 24


def same_named(c, name):
    return [k for k, d in enumerate(c.devs) if d and d["name"] == name]


def number_same_named(c, name):
    """同じ名前の機器に 1, 2, 3… と番号を付ける（位置の順）。付けた数を返す。"""
    idx = same_named(c, name)
    if len(idx) < 2:
        return 0
    idx.sort(key=lambda k: (c.cells[k][1], c.cells[k][2], c.cells[k][0]))
    used = {d["name"] for d in c.devs if d}
    n = 0
    k = 1
    for i in idx:
        while f"{name}{k}" in used:
            k += 1
        c.devs[i]["name"] = f"{name}{k}"
        used.add(f"{name}{k}")
        k += 1
        n += 1
    return n


def update_programs(world, c, old, new):
    """この構築物のコンピューターのブロックプログラムに出てくる機器名を置き換えて保存し直す。"""
    from .. import blocks as B
    count = 0
    for d in c.devs:
        if not d or not d["state"].get("blocks"):
            continue
        path = world.dir / "scripts" / d["state"]["blocks"]
        try:
            prog = B.load(path)
        except Exception:  # noqa: BLE001
            continue
        hit = []

        def walk(x):
            if isinstance(x, dict):
                if x.get("dev") == old:
                    x["dev"] = new
                    hit.append(1)
                for v in x.values():
                    walk(v)
            elif isinstance(x, list):
                for v in x:
                    walk(v)
        walk(prog)
        if hit:
            B.save(path, prog)
            base = d["state"]["blocks"][:-len(".blocks.json")]
            py = world.dir / "scripts" / f"{base}.py"
            py.write_text(B.to_python(prog, d["state"]["blocks"]), encoding="utf-8")
            world.runner.invalidate(py.name)
            d["state"].pop("_env", None)
            count += 1
    return count


CODE_FONT_CANDIDATES = [
    "C:/Windows/Fonts/BIZ-UDGothicR.ttc", "C:/Windows/Fonts/msgothic.ttc",
    "/System/Library/Fonts/Osaka.ttf",
]


def find_code_font(cfg):
    """日本語が表示できる等幅フォント（入力欄用）。"""
    from pathlib import Path
    custom = cfg.get("graphics", {}).get("code_font")
    for f in ([custom] if custom else []) + CODE_FONT_CANDIDATES:
        if f and Path(f).exists():
            return Path(f)
    return None


class DevicePanel:
    """機器の名前などを設定するメニュー。owner がいれば、その上に重ねて開く（エディタなどから）。"""

    def __init__(self, game, construct, index, owner=None):
        from ursina import Entity, Text, Button, InputField, camera, color, mouse
        self.game = game
        self.world = game.world
        self.c = construct
        self.cell = construct.cells[index]
        self.owner = owner
        self.U = dict(Entity=Entity, Text=Text, Button=Button, color=color)
        mouse.locked = False
        if owner is not None:
            owner.modal = self
            for f in ("field", "name_field"):
                if hasattr(owner, f):
                    getattr(owner, f).active = False
        d = construct.devs[index]
        dd = construct.reg.device(d["def"]) or {}
        self.default_name = dd.get("name", d["def"])
        self.type_name = TYPE_NAMES.get(dd.get("type"), dd.get("type", "機器"))
        self.root = Entity(parent=camera.ui, z=-0.5)
        blocker = Button(parent=self.root, model="quad", scale=(3, 2), color=color.rgba(0, 0, 0, 0.55),
                         highlight_color=color.rgba(0, 0, 0, 0.55), pressed_color=color.rgba(0, 0, 0, 0.55), z=0.05)
        blocker.on_click = lambda: None
        Entity(parent=self.root, model="quad", scale=(1.1, 0.8), color=color.rgb(0.07, 0.08, 0.11), z=0.02)
        Text(parent=self.root, text=f"{self.type_name} の設定", position=(0, 0.35), origin=(0, 0), scale=1.4)
        Text(parent=self.root, text="名前", position=(-0.5, 0.25), origin=(-0.5, 0), scale=0.9)
        # 日本語が表示できるフォントで入力欄を作る（等幅の日本語フォントがあればそれを使う）
        from ursina import application
        old_mono, old_folder = Text.default_monospace_font, application.fonts_folder
        code_font = find_code_font(game.cfg)
        if code_font:
            application.fonts_folder = code_font.parent
            Text.default_monospace_font = code_font.name
        else:
            Text.default_monospace_font = Text.default_font
        try:
            self.field = InputField(parent=self.root, default_value=d["name"], position=(0.05, 0.25),
                                    scale=(0.62, 0.05), character_limit=NAME_LIMIT, active=True)
        finally:
            Text.default_monospace_font, application.fonts_folder = old_mono, old_folder
        self.field.submit_on = ["enter"]
        self.field.on_submit = self.ok

        def btn(text, pos, fn, w=0.08, col=None, ts=0.7):
            b = Button(parent=self.root, text=text, position=pos, scale=(w, 0.042),
                       color=col or color.rgb(0.22, 0.32, 0.45), text_size=ts)
            b.on_click = fn
            return b
        btn("既定の名前", (-0.27, 0.07), lambda: self.set_text(self.default_name), w=0.17)
        btn("1文字消す", (-0.09, 0.07), self.backspace, w=0.17)
        btn("全部消す", (0.09, 0.07), lambda: self.set_text(""), w=0.17)
        btn("同名に番号を振る", (0.29, 0.07), self.number_all, w=0.2, col=color.rgb(0.6, 0.45, 0.15))
        self.info = Text(parent=self.root, text="", position=(-0.5, 0.0), origin=(-0.5, 0.5), scale=0.72,
                         color=color.light_gray)
        Text(parent=self.root, position=(-0.5, -0.17), origin=(-0.5, 0.5), scale=0.62, color=color.light_gray,
             text="同じ名前の機器は、コンピューターからまとめて動きます。\n"
                  "名前を分けると、ブロックの機器選択や api.device(\"名前\") で個別に制御できます。\n"
                  "（名前はキーボードで入力します）")
        btn("OK", (0.2, -0.33), self.ok, w=0.16, col=color.rgb(0.2, 0.5, 0.3), ts=0.8)
        btn("キャンセル", (0.38, -0.33), self.close, w=0.16, col=color.rgb(0.3, 0.3, 0.33), ts=0.8)

        class Keys(Entity):
            def input(self_, key):
                if key == "escape":
                    self.close()

            def update(self_):
                self.refresh()
        self.keys = Keys(parent=self.root)
        self.refresh()

    @property
    def index(self):
        return self.c.index.get(self.cell)

    def text(self):
        return self.field.text_field.text.strip()

    def set_text(self, s):
        self.field.text_field.text = s[:NAME_LIMIT]
        self.field.text_field.cursor.x = len(self.field.text_field.text)
        self.field.text_field.cursor.y = 0
        self.field.text_field.render()
        self.field.active = True

    def backspace(self):
        self.set_text(self.field.text_field.text[:-1])

    def number_all(self):
        i = self.index
        if i is None:
            return
        name = self.text() or self.c.devs[i]["name"]
        self.c.devs[i]["name"] = name
        n = number_same_named(self.c, name)
        self.set_text(self.c.devs[i]["name"])
        self.world.message(f"「{name}」{n} 個に番号を付けました" if n else f"「{name}」という名前の機器は1つだけです")

    def refresh(self):
        i = self.index
        if i is None:
            self.info.text = "機器が見つかりません"
            return
        name = self.text()
        dup = [k for k in same_named(self.c, name) if k != i]
        lines = [f"構築物: {self.c.name}　種類: {self.type_name}（{self.c.devs[i]['def']}）"]
        if not name:
            lines.append("名前が空です")
        elif dup:
            lines.append(f"⚠ 同じ名前の機器がほかに {len(dup)} 個あります（まとめて動きます）")
        else:
            lines.append("この構築物の中で一つだけの名前です（個別に制御できます）")
        lines.append(f"コンピューターからは  api.device(\"{name}\")")
        self.info.text = "\n".join(lines).replace("<", "＜").replace(">", "＞")

    def ok(self):
        i = self.index
        name = self.text()
        if i is not None and name:
            old = self.c.devs[i]["name"]
            self.c.devs[i]["name"] = name
            if old != name:
                n = 0
                if not same_named(self.c, old):
                    n = update_programs(self.world, self.c, old, name)
                self.world.message(f"名前を変更: {old} → {name}" + (f"（ブロックプログラム {n} 個も更新）" if n else ""))
                L.info("機器の名前を変更: %s → %s", old, name)
        self.close()

    def close(self):
        from .ui_destroy import destroy
        destroy(self.root)
        if self.owner is not None:
            self.owner.modal = None
            if hasattr(self.owner, "after_modal"):
                self.owner.after_modal()
        else:
            self.game.editor_closed()
