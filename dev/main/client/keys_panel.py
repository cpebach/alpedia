"""操作の説明とキー設定の画面（一時停止メニュー・F1・メニューのゲーム設定から開く）。

キーのボタンを押してから、割り当てたいキーを押すと変わる（Esc でやめる）。ほかの操作と同じキーなら入れ替える。
設定は config/settings.json の controls に保存する。
"""
from .. import log, native
from .touch_ui import key_name  # noqa: F401

L = log.get("keys")

ACTIONS = [
    ("forward", "前へ"), ("back", "後ろへ"), ("left", "左へ"), ("right", "右へ"),
    ("jump", "ジャンプ・上昇"), ("down", "下降"), ("jetpack", "ジェットパック"),
    ("interact", "使う・座る／降りる"), ("edit_script", "機器の設定・プログラム"),
    ("rotate", "置く向きを変える"), ("new_construct", "空中に置く"), ("inventory", "持ち物"),
    ("third_person", "視点（一人称／三人称）"), ("camera_mode", "カメラモード"), ("save", "保存"), ("export", "OBJ 出力"), ("import", "読み込み"),
    ("city_marker", "目印（街）"), ("city_map", "マップ（街）"), ("story_menu", "ストーリーの章"),
    ("toggle_help", "この画面"), ("debug_hud", "開発用の表示"),
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


KeysPanel = native.require().KeysPanel       # 画面（本体は Rust）：KeysPanel(U, cfg, parent, on_close, extra_help=(), on_change=None)
