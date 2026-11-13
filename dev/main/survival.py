"""サバイバル（本体の遊び方の1つ）：ブロックを置くのに資源を払い、掘って・精錬して資源を集める（Rust：game/survival.rs）。

world.json の "mode" が "survival" のワールドで、ワールドの拡張の先頭に入る（MOD ではない）。
"mode" が無い・"creative" ならクリエイティブ（資源なしで何でも置ける・タンクは満タン）。
費用は各定義の "cost"、資源は sauce/base/resources、精錬機・加工機のレシピは機器の params.recipes。
"""
from . import native

MODES = ("survival", "creative")
DIG_AMOUNT = 10.0        # 1回掘ると手に入る資源
WATER_AMOUNT = 20.0      # 1回くむと手に入る水
STARTER_KIT = {"iron": 250, "silica": 40, "parts": 12, "fuel": 60}

mode_of = native.require().survival_mode_of            # ワールドの遊び方："survival" か "creative"
SurvivalRules = native.require().SurvivalRules
GOALS = [(g, t) for g, t, _ in SurvivalRules().goals_state(type("W", (), {"settings": {}})())]
