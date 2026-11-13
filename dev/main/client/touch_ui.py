"""画面のボタン（タッチ・マウスで押せる操作）と、仮想ジョイスティック。

- 右上：メニュー・持ち物・マップなどの小さなボタン（いつも出ている）
- 右下：ジャンプ・掘る・置くなどの操作ボタン（操縦席では「降りる」と、機器に割り当てたキー）
- 右の中ほど：近くにある、触れるもの（操縦席・コンピューター・お店・NPC など）のボタン（近づくと出る）
- 左下：仮想ジョイスティック（ゲーム設定・一時停止メニューで ON にしたとき）。画面のほかの所をなぞると視点が回る

指（タッチ）は画面（Rust）が届ける指の出来事を、指が無ければマウスの左ボタン（1本の指）を使う。
キーボード・マウスで遊ぶときは、Alt を押している間カーソルが出るので、ボタンをクリックできる。

中身は Rust（native/alpedia_core/src/game/touch_ui.rs）。ここは名前と値を残すだけ。
"""
from .. import native

_c = native.require()

SMALL = 0.034           # 右上のボタンの半径（画面の高さ = 1）
BIG = 0.058
MID = 0.046
STICK_R = 0.11          # ジョイスティックの半径
KEY_NAMES = {"space": "Space", "left control": "Ctrl", "left shift": "Shift", "escape": "Esc", "left alt": "Alt",
             "up arrow": "↑", "down arrow": "↓", "left arrow": "←", "right arrow": "→", "tab": "Tab",
             "enter": "Enter", "backspace": "BS"}
# 操縦席のよくあるキーの意味（機器が割り当てられていないとき）
SEAT_LABELS = {"space": "上昇", "left control": "下降", "left shift": "下降"}
HOLD_TIME = 0.45        # この秒数より長く、指を動かさずに触っていると長押し
TAP_TIME = 0.3          # これより短く離せばタップ
MOVE_TOL = 0.012        # これより動いたら、なぞる（視点）
REPEAT = 0.22           # 長押しのくり返しの間隔

key_name = _c.key_name      # キーの表示名（space → Space、e → E など）
Pointers = _c.Pointers      # いま画面に触れている指（またはマウスの左ボタン）
Btn = _c.Btn                # 丸いボタン
Pill = _c.Pill              # 近くのもの（お店・操縦席など）のボタン
TouchHUD = _c.TouchHUD      # 画面のボタン・仮想ジョイスティック：TouchHUD(game)
