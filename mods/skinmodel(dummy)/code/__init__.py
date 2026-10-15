"""VRM の見た目 MOD：VRM のモデルをプレイヤーの見た目にする（三人称で表示）。

使い方は、この MOD のフォルダの README.md を参照。
    vrm.py     VRM（glTF バイナリ）を読む（numpy だけ）
    motion.py  手で書いた簡単なモーション（numpy だけ）
    view.py    Panda3D で描く（骨での変形はシェーダー）
    client.py  画面側の拡張（プレイヤーの体を引き受ける）
"""


def world_addon():
    from main.hooks import Addon

    class SkinModel(Addon):
        name = "skinmodel"
    return SkinModel()


def client_addon(game):
    from .client import SkinClient
    return SkinClient(game)
