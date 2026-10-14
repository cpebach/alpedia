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
