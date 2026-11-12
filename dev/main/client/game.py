"""遊ぶ画面で共通に使う物：キーの一覧・フォントの候補・ほかの人の体（色の箱）・機器の状態の言葉。

画面そのものは Rust の画面（main/client_rs/viewer.py）。今までの Panda3D・ursina の画面は無くなった。
"""
from pathlib import Path

import numpy as np

FONT_CANDIDATES = [
    "C:/Windows/Fonts/meiryo.ttc", "C:/Windows/Fonts/YuGothM.ttc", "C:/Windows/Fonts/BIZ-UDGothicR.ttc",
    "C:/Windows/Fonts/msgothic.ttc",
    "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc", "/System/Library/Fonts/Hiragino Sans GB.ttc",
    "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc", "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
]

HELP = """[操作]  （Esc →「操作とキー設定」・F1 でいつでも見られます）
WASD 移動 / Space ジャンプ・上昇 / Ctrl 下降 / F ジェットパック
マウス 視点 / 左クリック 削除・掘る / 右クリック 設置
I インベントリ / ホイール・数字キー アイテムバーの選択 / R 向き変更 / N 空中に新しい構築物
E 機器の操作・操縦席に座る/降りる・街に固定・NPC と話す/取引 / V 三人称
P 機器の設定（名前）／プログラム編集／アクチュエーターの動作設定
F5 保存 / F9 見ている構築物を OBJ 出力 / F8 読み込みメニュー
Esc 一時停止メニュー（保存・メニューへ戻る・終了）
[操縦席] Space などの割り当てキーで推進機、X でアクチュエータ
"""

SEAT_KEYS = ["w", "a", "s", "d", "q", "e", "space", "left control", "left shift", "x", "z", "c",
             "up arrow", "down arrow", "left arrow", "right arrow", "1", "2", "3", "4"]


def find_font(cfg):
    """日本語の出るフォント（設定の graphics.font、無ければいつもの候補から）。"""
    custom = cfg.get("graphics", {}).get("font")
    for f in ([custom] if custom else []) + FONT_CANDIDATES:
        if f and Path(f).exists():
            return Path(f)
    return None


class BoxBody:
    """体の見た目（MOD が引き受けないとき）：定義の色の箱。update(origin, shown, dt)・destroy()。"""

    def __init__(self, game, player, model=None):
        from . import gfx
        col = player.defn.get("color", [1, 1, 1])
        self.game, self.player = game, player
        c = [x / 255.0 for x in col] if max(col) > 1 else list(col)
        self.e = gfx.RsNode(game.scene_root, "body")
        v, t = gfx.box_vertices((c[0], c[1], c[2], 1.0))
        gfx.RsDynamicMesh(self.e, "body").set(v, t)
        self.e.hide()

    def update(self, origin, shown, dt):
        from . import gfx
        if not shown:
            self.e.hide()
            return
        p = self.player
        up = p.up / np.linalg.norm(p.up)
        f = np.asarray(p.look_fwd, float)
        f = f - up * (f @ up)
        f = f / np.linalg.norm(f) if np.linalg.norm(f) > 1e-6 else np.cross(up, [1.0, 0, 0])
        right = np.cross(up, f)
        center = p.pos + up * (p.height / 2 - p.radius) - origin
        gfx.set_box(self.e, center, np.stack([right, up, f]), np.array([p.radius * 2, p.height, p.radius * 1.4]))
        self.e.show()

    def destroy(self):
        self.e.removeNode()


def device_status_text(c, i, d, dd):
    """機器の今の状態を、遊ぶ人が読める言葉で（内部の値の名前は出さない）。"""
    st = d.get("state", {})
    t_ = dd.get("type")
    reg = c.reg
    out = ""
    if t_ == "thruster":
        if st.get("lit"):
            out = f"燃えている　出力 {float(st.get('throttle', st.get('firing', 0.0)) or 0.0) * 100:.0f}%"
        else:
            out = "止まっている"
    elif t_ == "valve":
        out = f"開き具合 {float(st.get('open', 0.0)) * 100:.0f}%" + ("（閉）" if float(st.get("open", 0.0)) <= 0.001 else "")
    elif t_ == "tank":
        cap = float((dd.get("params") or {}).get("capacity", 0.0))
        cont = {k: v for k, v in (st.get("contents") or {}).items() if v >= 0.05}
        if not cont:
            out = "空" + (f"（{cap:.0f} まで入る）" if cap else "")
        else:
            out = "中身: " + reg.res_text(cont, ", ") + (f" / {cap:.0f}" if cap else "")
    elif t_ == "battery":
        cap = float((dd.get("params") or {}).get("capacity", 0.0))
        ch = float(st.get("charge", 0.0))
        out = f"電気 {ch / 1e6:.2f}" + (f" / {cap / 1e6:.1f} MJ（{ch / cap * 100:.0f}%）" if cap else " MJ")
    elif t_ == "computer":
        out = "動いている" if st.get("running") else "止まっている"
        if st.get("blocks") or st.get("script"):
            out += f"　プログラム「{st.get('blocks') or st.get('script')}」".replace(".blocks.json", "").replace(".py", "")
    elif t_ == "actuator":
        from .. import actuator as act
        m = act.motion(d, reg)
        val = (f"伸び {st.get('ext', 0):.2f}" if m == "piston" else f"角度 {st.get('angle', 0):.0f}°" if m != "both"
               else f"角度 {st.get('angle', 0):.0f}°  伸び {st.get('ext', 0):.2f}")
        out = (f"{act.mode_name(st, d) if m == 'hinge' else act.MOTION_NAMES[m]}  {val}  "
               f"結合 {'あり' if act.is_attached(c, i) else 'なし'}\n設定: {act.preset_text(st, act.is_hinge_dev(d, reg))}")
    elif t_ == "processor":
        store = {k: v for k, v in (st.get("store") or {}).items() if v >= 0.5}
        out = ("動いている" if st.get("working") else ("止めている" if not st.get("enabled", True) else "待機中"))
        if store:
            out += "　中: " + reg.res_text(store, ", ")
    elif t_ in ("generator", "antenna", "air_vent", "custom"):
        out = "ON" if st.get("enabled", True) else "OFF"
        if t_ == "air_vent":
            out += "　" + {"fill": "与圧", "drain": "減圧"}.get(st.get("mode"), str(st.get("mode", "")))
    if st.get("error"):
        out += ("\n" if out else "") + "⚠ " + str(st["error"])
    return out
