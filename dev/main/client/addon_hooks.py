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

    def nearby(self, pos, radius):
        """プレイヤーの近く（pos から radius m 以内）にある、この拡張の触れるもの（お店・NPC など）。
        画面右にボタンが出て、押す（または E）と interact(hit) が呼ばれる。
        [{"label": 表示, "hit": interact に渡すもの, "kind": "e", "dist": 距離, "icon": "interact"}]"""
        return []

    def player_body(self, player):
        """プレイヤー（自分・マルチプレイのほかの人）の体の見た目を引き受けるなら、そのオブジェクトを返す。
        オブジェクトは update(origin, shown, dt) を毎フレーム受け取り（shown: 見せるか）、destroy() で消える。
        player の pos・up・vel・look_fwd（見ている向き）・pitch・seated などを読んで動かす。
        見た目を変えたら作り直す。プレイ中にファイルを読まないように、モデルは拡張を作るときに読んでおく。"""
        return None

    def update(self, origin, far):
        """毎フレームの描画。origin はカメラの位置（浮動原点）。"""

    def destroy(self):
        """ゲーム画面を閉じる。"""
