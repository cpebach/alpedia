"""画面側の拡張の口。ワールド側の拡張（hooks.Addon）と組で使う。

ゲーム画面（game.Game）は拡張の中身を知らず、次の口だけを呼ぶ。
拡張はワールドの中身を直接触らず、自分のワールド側（world.addon(名前)）の問い合わせ関数を使う。
"""


class ClientAddon:
    name = "addon"
    help_text = ""                      # 操作説明に足す行

    def __init__(self, game):
        self.game = game
        self.world = game.world

    def on_key(self, key):
        """キーが押された。使ったら True。"""
        return False

    def interact(self, hit):
        """E で hit（視線の先）を使う。使ったら True（コアの操作はしない）。"""
        return False

    def aim(self, hit):
        """視線の先の説明。(文, ブロックを置ける面なら True) か None（この拡張のものでない）。"""
        return None

    def update(self, origin, far):
        """毎フレームの描画。origin はカメラの位置（浮動原点）。"""

    def destroy(self):
        """ゲーム画面を閉じる。"""
