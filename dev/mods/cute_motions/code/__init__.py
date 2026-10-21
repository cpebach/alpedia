"""かわいいモーション MOD：カメラモード（C・カメラのボタン）の「モーション」で選べる動きを足す。

動きは motions.py に関数で書く（秒 t → 骨の向き）。書き方は mods/README.md の「モーション」。
体を動かすのは見た目の MOD（VRM の見た目）なので、一緒に入れて使う（宇宙飛行士の箱の体は動かない）。
"""


def world_addon():
    from main.hooks import Addon

    class CuteMotions(Addon):
        name = "cute_motions"
    return CuteMotions()


def client_addon(game):
    from main.client.addon_hooks import ClientAddon
    from .motions import MOTIONS

    class CuteMotionsClient(ClientAddon):
        name = "cute_motions"

        def emotes(self):
            return dict(MOTIONS)
    return CuteMotionsClient(game)
