"""サバイバルの決まり（ワールド側）。"""
import numpy as np

from main.hooks import Addon

DIG_AMOUNT = 20.0        # 1回掘ると手に入る資源
WATER_AMOUNT = 20.0      # 1回くむと手に入る水（海や池の水は減らない）
# 最初の所持品（詰まないように：精錬機・手回し発電機・太陽光パネル・タンクが作れて、少し飛べる）
STARTER_KIT = {"iron": 250, "silica": 40, "parts": 12, "fuel": 60}


class SurvivalRules(Addon):
    name = "survival"
    resource_rules = True

    # --- 読み込み -----------------------------------------------------------
    def on_load(self, world):
        p = world.player
        if getattr(p, "starter_given", False):
            return
        for k, v in STARTER_KIT.items():
            p.inventory[k] = max(p.inventory.get(k, 0.0), float(v))
        p.starter_given = True
        world.message("サバイバル開始: 最初の資材を受け取りました（I でインベントリ）")

    def on_player_join(self, world, player):
        """マルチプレイ：参加した人にも最初の資材を（world.player はその人）。"""
        self.on_load(world)

    # --- 置く・外す ---------------------------------------------------------
    def pay(self, world, cost):
        inv = world.player.inventory
        short = {k: v - inv.get(k, 0.0) for k, v in cost.items() if inv.get(k, 0.0) < v}
        if short:
            world.message("資源が足りません（不足: " + world.registry.res_text(short, ", ") + "）")
            return False
        for k, v in cost.items():
            inv[k] -= v
        return True

    def refund(self, world, cost):
        inv = world.player.inventory
        for k, v in cost.items():
            inv[k] = inv.get(k, 0.0) + v

    def on_device_placed(self, world, c, i):
        """置いたばかりのバッテリーは空（置き直しで電気が増えないように）。"""
        dd = world.registry.device(c.devs[i]["def"])
        if dd and dd["type"] == "battery":
            c.devs[i]["state"]["charge"] = 0.0

    # --- 集める ---------------------------------------------------------------
    def _gain(self, world, res, amount):
        inv = world.player.inventory
        inv[res] = inv.get(res, 0.0) + amount
        world.message(f"{world.registry.res_name(res)} +{amount:.0f}（所持 {inv[res]:.0f}）")

    def on_dig(self, world, hit, resource):
        self._gain(world, resource, DIG_AMOUNT)

    def on_water(self, world, hit):
        self._gain(world, "water", WATER_AMOUNT)
        return True

    # --- 機器 -----------------------------------------------------------------
    def interact_device(self, world, c, i, d, dd):
        if not dd:
            return None
        if dd["type"] == "tank":
            return self._fill_tank(world, d, dd)
        if dd["type"] == "processor":
            return self._exchange(world, c, d, dd)
        return None

    def _fill_tank(self, world, d, dd):
        """手持ちの燃料をタンクに入れる（入りきらない分は手元に残る）。"""
        p = world.player
        cap = float(dd["params"].get("capacity", 0))
        cont = d["state"].setdefault("contents", {})
        have = p.inventory.get("fuel", 0.0)
        room = max(0.0, cap - sum(cont.values()))
        if have <= 0:
            now = ", ".join(f"{world.registry.res_name(k)} {v:.0f}" for k, v in cont.items() if v) or "空"
            return f"{d['name']}: {now} / {cap:.0f}（燃料を持っていません。硫黄鉱石や水を精錬機で燃料に）"
        if room <= 0:
            return f"{d['name']}: 満タンです"
        mv = min(have, room)
        cont["fuel"] = cont.get("fuel", 0.0) + mv
        p.inventory["fuel"] = have - mv
        return f"{d['name']}: 燃料 {mv:.0f} を入れました（タンク {sum(cont.values()):.0f}/{cap:.0f}、手持ち {have - mv:.0f}）"

    def _exchange(self, world, c, d, dd):
        """精錬機・加工機：所持品の原料を入れ、できた物を受け取る（何も動かなければ None：ふつうの操作へ）。"""
        p = world.player
        moved = []
        for r in dd["params"].get("recipes", []):
            for k in r.get("in", {}):
                amt = p.inventory.pop(k, 0.0)
                if amt:
                    c.storage[k] = c.storage.get(k, 0.0) + amt
                    moved.append(f"{world.registry.res_name(k)} {amt:.0f} 投入")
            for k in r.get("out", {}):
                amt = c.storage.pop(k, 0.0)
                if amt:
                    p.inventory[k] = p.inventory.get(k, 0.0) + amt
                    moved.append(f"{world.registry.res_name(k)} {amt:.0f} 回収")
        if not moved:
            return None
        st = c.status or {}
        hint = "" if (st.get("stored", 0) > 1 or st.get("production", 0) > 0) else \
            "（電気がありません：手回し発電機・太陽光パネル・発電機をつなげて）"
        return f"{d['name']}: " + ", ".join(moved) + hint

    # --- ジェットパック ---------------------------------------------------------
    def jetpack(self, world, p, dt, thrust_dir):
        """手持ちの燃料が要る（噴かしている間だけ減る）。"""
        use = float(p.defn.get("jetpack_fuel_rate", 1.0)) * dt
        if p.on_ground and float(np.dot(thrust_dir, p.up)) <= 0:
            return p.inventory.get("fuel", 0.0) > 0, np.zeros(3)   # 地上を歩いているだけなら減らない
        if np.linalg.norm(thrust_dir) > 0:
            have = p.inventory.get("fuel", 0.0)
            if have <= 0:
                if not p.out_of_fuel:
                    world.message("燃料がないのでジェットパックが使えません（硫黄鉱石や水を精錬して燃料に）")
                p.out_of_fuel = True
                return False, thrust_dir
            p.inventory["fuel"] = max(0.0, have - use)
            p.out_of_fuel = False
            return True, thrust_dir
        return p.inventory.get("fuel", 0.0) > 0, thrust_dir
