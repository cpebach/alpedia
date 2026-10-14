"""サバイバル（資源）MOD：ブロックを置くのに資源を払い、掘って・精錬して資源を集める。

この MOD が入っていないワールドはクリエイティブ（資源なしで何でも置ける・タンクは E で満タン）。
費用は各定義の "cost"、資源は sauce/base/resources、精錬機・加工機のレシピは機器の params.recipes。
"""


def world_addon():
    from .rules import SurvivalRules
    return SurvivalRules()
