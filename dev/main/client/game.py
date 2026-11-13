"""遊ぶ画面で共通に使う物：キーの一覧・フォントの候補・ほかの人の体（色の箱）・機器の状態の言葉。

画面そのものは Rust の画面（main/client_rs/viewer.py）。処理は Rust（alpedia_core）。
"""
from .. import native

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


_c = native.require()
find_font = _c.game_find_font                 # 日本語の出るフォント（設定の graphics.font、無ければいつもの候補から）
BoxBody = _c.BoxBody                          # 体の見た目（MOD が引き受けないとき）：定義の色の箱
device_status_text = _c.game_device_status_text   # 機器の今の状態を、遊ぶ人が読める言葉で
