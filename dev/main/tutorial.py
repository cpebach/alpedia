"""新しいワールドを作る（空のワールド・MOD が形を決めるワールド）。

シナリオ（物語のチュートリアル）は「シナリオモード」の MOD（mods/story）として遊ぶ。
新しいワールドを作るときにその MOD を選ぶと、MOD の mod.json の "world" の形（惑星・ワールドの設定）で作り、
最初に読み込んだときに MOD が舞台（構築物・スクリプト）を作る。始め方はこの1つだけ
（前の版の「チュートリアル」ボタン・テンプレートはなくなった。前の版のチュートリアルのワールドはそのまま続きから遊べる）。
"""
from . import log, native

_c = native.require()

L = log.get("newworld")

STORY_MOD = "story"
SCENARIO_WORLD = "scenario"          # 初めて起動したときに作るシナリオのワールドの名前


create_world = _c.tut_create_world          # ワールドを作る（選んだ MOD の "world" の形があればその形で）
create_tutorial = _c.tut_create_tutorial    # シナリオモードのワールドを作る（テスト・初回起動用）
is_story_world = _c.tut_is_story_world      # シナリオ（物語）のワールドか
