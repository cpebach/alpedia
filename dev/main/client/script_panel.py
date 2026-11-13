"""コンピューター（Python スクリプト）の画面：ファイルの選択・実行・停止・エラーと出力の表示。

スクリプトの編集はゲームの外のエディタ（メモ帳・VS Code など）で行う。
「エディタで開く」でファイルを開き、保存するとゲームが自動で読み込み直す（実行中でも次の tick から新しいコード）。
"""
from .. import log, native

L = log.get("script_panel")

NEW_TEMPLATE = '''"""新しいスクリプト

tick(api) が一定間隔で呼ばれます。使える機能は scripts/README.txt を参照。
保存するとゲームが自動で読み込み直します（実行中でも）。
"""


def tick(api):
    # 例: 操縦席で W を押している間だけ、バルブを開けて推進機を燃やす
    # （推進機とタンクのあいだにバルブが必要。バルブを閉じると火が消え、開けたら点火し直す）
    on = "w" in api.input
    for v in api.devices("valve"):
        v.open = 1.0 if on else 0.0
    if on:
        for t in api.devices("thruster"):
            t.ignite()
'''


_c = native.require()
open_external = _c.sp_open_external        # OS の既定のアプリでファイル・フォルダを開く
_win_editors = _c.sp_win_editors           # Windows で使えるエディタ（見つかった順）
open_in_editor = _c.sp_open_in_editor      # テキストエディタでファイルを開く（command：設定 game.editor）
_wrap = _c.sp_wrap
ScriptPanel = _c.ScriptPanel               # コンピューターの画面：ScriptPanel(game, construct, index)
