"""操作の説明とキー設定の画面（一時停止メニュー・F1・メニューのゲーム設定から開く）。

キーのボタンを押してから、割り当てたいキーを押すと変わる（Esc でやめる）。ほかの操作と同じキーなら入れ替える。
設定は config/settings.json の controls に保存する。
"""
from .. import log
from .touch_ui import key_name

L = log.get("keys")

ACTIONS = [
    ("forward", "前へ"), ("back", "後ろへ"), ("left", "左へ"), ("right", "右へ"),
    ("jump", "ジャンプ・上昇"), ("down", "下降"), ("jetpack", "ジェットパック"),
    ("interact", "使う・座る／降りる"), ("edit_script", "機器の設定・プログラム"),
    ("rotate", "置く向きを変える"), ("new_construct", "空中に置く"), ("inventory", "持ち物"),
    ("third_person", "視点（一人称／三人称）"), ("camera_mode", "カメラモード"), ("save", "保存"), ("export", "OBJ 出力"), ("import", "読み込み"),
    ("city_marker", "目印（街）"), ("city_map", "マップ（街）"), ("toggle_help", "この画面"),
]

OTHER = """・マウス：視点
・左クリック：壊す・掘る
・右クリック：置く
・ホイール・1〜9：アイテムバーの選択
・Esc：一時停止メニュー
・Alt を押している間：
　カーソルが出て、画面のボタンを押せる
・操縦席：機器に割り当てたキー
　（推進機は Space、アクチュエーターは X など）
・使える物（操縦席・機器・お店・NPC）に
　近づくと、画面右にボタンが出る
　（E でいちばん上を使う）
・仮想ジョイスティック ON：
　左下のスティックで移動、
　ほかの所をなぞって視点"""


class KeysPanel:
    def __init__(self, U, cfg, parent, on_close, extra_help=(), on_change=None):
        from ursina import Entity, Text, Button, color
        self.U, self.cfg, self.on_close, self.on_change = U, cfg, on_close, on_change
        self.color = color
        self.Button, self.Text = Button, Text
        self.root = Entity(parent=parent)
        Button(parent=self.root, model="quad", scale=(3, 2), color=color.rgba(0, 0, 0, 0.6), z=0.2,
               highlight_color=color.rgba(0, 0, 0, 0.6), pressed_color=color.rgba(0, 0, 0, 0.6))
        Entity(parent=self.root, model="quad", scale=(1.5, 0.92), color=color.rgb(0.07, 0.08, 0.11), z=0.1)
        Text(parent=self.root, text="操作とキー設定", position=(0, 0.42), origin=(0, 0), scale=1.4)
        Text(parent=self.root, text="キーのボタンを押してから、割り当てたいキーを押します（Esc でやめる）",
             position=(0, 0.37), origin=(0, 0), scale=0.65, color=color.light_gray)
        self.waiting = None
        self.status = Text(parent=self.root, text="", position=(0, -0.335), origin=(0, 0), scale=0.7,
                           color=color.orange)
        self.btns = {}
        c = cfg["controls"]
        for k, (aid, label) in enumerate(ACTIONS):
            col, row = k // 10, k % 10
            x = -0.68 + col * 0.5
            y = 0.31 - row * 0.052
            Text(parent=self.root, text=label, position=(x, y), origin=(-0.5, 0), scale=0.7)
            b = Button(parent=self.root, text=key_name(c.get(aid, "")), position=(x + 0.36, y), scale=(0.13, 0.042),
                       color=color.rgb(0.22, 0.32, 0.45), text_size=0.7)
            b.on_click = (lambda a=aid: self._listen(a))
            self.btns[aid] = b
        lines = OTHER + ("\n" + "\n".join("・" + h for h in extra_help) if extra_help else "")
        Text(parent=self.root, text="[そのほかの操作]\n" + lines, position=(0.3, 0.33), origin=(-0.5, 0.5), scale=0.62,
             color=color.rgb(0.85, 0.88, 0.92), line_height=1.1)
        b = Button(parent=self.root, text="初期値に戻す", position=(-0.15, -0.4), scale=(0.22, 0.05),
                   color=color.rgb(0.3, 0.3, 0.33), text_size=0.75)
        b.on_click = self._reset
        b = Button(parent=self.root, text="閉じる", position=(0.15, -0.4), scale=(0.22, 0.05),
                   color=color.rgb(0.2, 0.5, 0.3), text_size=0.75)
        b.on_click = self.close
        panel = self

        class Keys(Entity):
            def input(self_, key):
                panel._key(key)
        self.keys = Keys(parent=self.root)

    def _listen(self, aid):
        self.waiting = aid
        for a, b in self.btns.items():
            b.color = self.color.rgb(0.75, 0.5, 0.15) if a == aid else self.color.rgb(0.22, 0.32, 0.45)
        self.status.text = f"「{dict(ACTIONS)[aid]}」に割り当てるキーを押してください"

    def _key(self, key):
        if self.waiting is None:
            if key == "escape":
                self.close()
            return
        if key.endswith(" up") or "mouse" in key or key.startswith("scroll") or key.endswith(" hold"):
            return
        aid = self.waiting
        self.waiting = None
        for b in self.btns.values():
            b.color = self.color.rgb(0.22, 0.32, 0.45)
        if key == "escape":
            self.status.text = "やめました"
            return
        c = self.cfg["controls"]
        old = c.get(aid)
        other = next((a for a, _ in ACTIONS if a != aid and c.get(a) == key), None)
        c[aid] = key
        msg = f"「{dict(ACTIONS)[aid]}」を {key_name(key)} にしました"
        if other is not None:
            c[other] = old
            msg += f"（「{dict(ACTIONS)[other]}」と入れ替え：{key_name(old)}）"
        self._save(msg)

    def _reset(self):
        from .. import config as config_mod
        for aid, _ in ACTIONS:
            self.cfg["controls"][aid] = config_mod.DEFAULTS["controls"].get(aid, "")
        self.waiting = None
        self._save("初期値に戻しました")

    def _save(self, msg):
        for aid, b in self.btns.items():
            b.text = key_name(self.cfg["controls"].get(aid, ""))
        try:
            from .menu import save_config
            save_config(self.cfg)
            self.status.text = msg
        except Exception as e:  # noqa: BLE001
            L.exception("設定を保存できません")
            self.status.text = f"保存できません: {e}"
        if self.on_change:
            self.on_change()

    def close(self):
        from .ui_destroy import destroy
        destroy(self.root)
        if self.on_close:
            self.on_close()
