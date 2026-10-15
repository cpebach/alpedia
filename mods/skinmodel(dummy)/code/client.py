"""画面側：選んだプレイヤーの定義の "model" が .vrm なら、その体をこの MOD が描く。"""
from main import log
from main.client import render
from main.client.addon_hooks import ClientAddon

from .view import VRMBody
from .vrm import VRM

L = log.get("skinmodel")


class SkinClient(ClientAddon):
    name = "skinmodel"

    def __init__(self, game):
        super().__init__(game)
        self.body = None
        d = game.world.player.defn
        model = d.get("model")
        if not model or not str(model).lower().endswith(".vrm"):
            return                      # VRM でないプレイヤー（宇宙飛行士など）はコアが描く
        v = VRM(model)                  # 読めなければここでエラー（ワールドは始まらない）
        m = v.meta
        L.info("VRM を読み込み: %s（作者 %s、%s、骨 %d、頂点 %d）", m.get("title") or m.get("name") or "?",
               m.get("author") or ", ".join(m.get("authors", []) or []) or "?",
               m.get("licenseName") or m.get("licenseUrl") or "利用条件の記載なし",
               len(v.joint_node), sum(len(x.pos) for x in v.meshes))
        self.body = VRMBody(game, d, v, render.SUN)

    def player_body(self):
        return self.body

    def destroy(self):
        if self.body is not None:
            self.body.destroy()
            self.body = None
