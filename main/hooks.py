"""コアに機能を足す口（MOD のプログラム）。

コア（ワールド・構築物・物理）は、街・経済・資源のルールなどの具体的な中身を知らない。
MOD のプログラム（mods/<MOD名>/code/、modcode.py を参照）は Addon を継承し、必要な口だけを上書きする。
ワールドを読み込むと、そのワールドで選ばれた MOD の Addon が作られる（テストでは World(name, cfg, addons=[...])）。

当たり判定に使う「動くかもしれない物体」（owner）は、次の2つを持つ:
    owner.is_moving() -> bool
    owner.point_velocity(P) -> (N,3)  その点での速度
"""


class Addon:
    """拡張の口。どれも何もしない既定の実装を持つので、必要なものだけ上書きする。"""

    name = "addon"
    settings_keys = ()          # world.json に書く自分の項目の名前（保存形式の検査で使う）

    # --- 読み込み・保存 ---------------------------------------------------
    def on_load(self, world):
        """ワールドを読み込んだ直後（構築物・プレイヤーは読み込み済み）。"""

    def on_save(self, world):
        """保存する直前。world.settings に自分の状態を書く。"""

    # --- 1ティックごと ----------------------------------------------------
    def before_physics(self, world, dt):
        """物理計算の前。物理で動かさない（この拡張が位置を決める）構築物の ID の集合を返す。"""
        return set()

    def colliders(self, world, lo, hi, margin):
        """範囲 (lo, hi)±margin にある、当たり判定用の箱：[(owner, C, A, E, 部品番号)]"""
        return []

    def after_physics(self, world, cons):
        """物理計算の後。cons の各構築物の c.ext_events（この拡張の owner に当たった記録）を処理する。
        記録は (owner, {部品番号: 当たった速さ} または None, 押した力積 J (N,3) または None, 位置 (N,3))"""

    # --- 問い合わせ・操作 -------------------------------------------------
    def raycast(self, world, o, dvec, max_t):
        """視線などの交差：{"kind", "t", "point", "normal", ...}（ブロックを置ける面なら "placeable": True）か None"""
        return None

    def push_sphere(self, world, center, r):
        """球（プレイヤー）を押し出す：[(押し出し, 法線, owner)]"""
        return []

    def interact(self, world, c, i):
        """E で構築物のブロック（機器以外）を触った。メッセージを返したら、そこで処理済み。"""
        return None

    def on_placed(self, world, hit, c):
        """hit（この拡張の raycast の結果）の上に新しい構築物 c を置いた。"""

    def interact_device(self, world, c, i, d, dd):
        """E で機器を触った（コアの操作より先に呼ばれる）。メッセージを返したら、そこで処理済み。"""
        return None

    # --- 資源のルール（サバイバルなど） ------------------------------------
    resource_rules = False      # True の MOD が入っていると「クリエイティブ」ではなくなる（world.creative が False）

    def pay(self, world, cost):
        """ブロックを置く・読み込む費用 {資源: 量}。払えなければ False（置かない）。"""
        return True

    def refund(self, world, cost):
        """ブロックを外したときの返却。"""

    def on_device_placed(self, world, c, i):
        """プレイヤーが機器 i を置いた。"""

    def on_dig(self, world, hit, resource):
        """地面を掘った（resource はその場所の資源）。"""

    def on_water(self, world, hit):
        """水面をクリックした。使ったら True。"""
        return False

    def jetpack(self, world, player, dt, thrust_dir):
        """ジェットパックを使えるか：(使えるか, 噴射の向き) を返す。"""
        return player.jetpack, thrust_dir

    # --- マルチプレイ（LAN） -------------------------------------------------
    # ホストの PC が世界を動かし、参加者の PC は結果を受け取って描く。参加者の PC では
    # before_physics / after_physics などのティックごとの口は呼ばれない（on_load は呼ばれる）。
    # world.player は「いま操作している人」。参加者の操作をホストで行う間はその人になる（支払い・メッセージの宛先）。
    def on_player_join(self, world, player):
        """ホスト：参加者が初めて（または戻って）入った。最初の資材を渡すなど（world.player はその人）。"""

    def net_state(self, world):
        """ホスト：参加者へ送るこの拡張の状態（JSON で書けるもの。変わったときだけ送られる）。None なら送らない。"""
        return None

    def net_apply(self, world, state):
        """参加者：net_state で送られた状態を受け取った。"""

    def net_client_tick(self, world, dt):
        """参加者：毎フレーム。次の状態が届くまで、動いている物を進めておく（動きをなめらかにする）。"""

    def net_ref_of(self, obj):
        """world.addon_call の引数を相手に送れる形にする（この拡張の物なら (種類, ID)、そうでなければ None）。"""
        return None

    def net_ref(self, world, kind, id_):
        """net_ref_of の逆（ホストで、種類と ID から物を探す）。"""
        return None
