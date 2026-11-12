"""画面側：選んだプレイヤーの定義の "model" が .vrm なら、その体をこの MOD が描く。"""
from main import log
from main.client import render
from main.client.addon_hooks import ClientAddon

from .view import VRMBody, VRMModel
from .vrm import VRM

L = log.get("skinmodel")


class SkinClient(ClientAddon):
    """VRM のプレイヤーの見た目を描く。ワールドを始めるときに、定義にある VRM を全部読み込んでおく
    （ゲーム中に見た目を変えたり、ほかの参加者が別の見た目でも、プレイ中にファイルを読みに行かない）。"""
    name = "skinmodel"

    def __init__(self, game):
        super().__init__(game)
        self.models = {}            # モデルのパス → VRMModel
        for d in game.world.registry.defs["players"].values():
            model = d.get("model")
            if not model or not str(model).lower().endswith(".vrm") or model in self.models:
                continue
            v = VRM(model)          # 読めなければここでエラー（ワールドは始まらない）
            m = v.meta
            L.info("VRM を読み込み: %s（%s、作者 %s、骨 %d、頂点 %d）", d["id"], m.get("title") or m.get("name") or "?",
                   m.get("author") or ", ".join(m.get("authors", []) or []) or "?",
                   len(v.joint_node), sum(len(x.pos) for x in v.meshes))
            self.models[model] = VRMModel(d, v, render.SUN)

    def player_body(self, player):
        """player の見た目が VRM なら、その体を作る（違えば None：コアが描く）。"""
        mm = self.models.get(player.defn.get("model"))
        return VRMBody(self.game, player, mm) if mm is not None else None
