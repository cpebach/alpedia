"""街の暮らし：内装のある建物（店・不動産屋・住宅）、NPC、通貨（クレジット）と取引、家（購入・賃貸・収納箱・ベッド）。

- 建物・NPC は街の生成（cities.py）から呼ばれ、街のローカル座標で作る（街の seed から毎回同じ）。
- 取引はサバイバルだけ。値段は街の地域（平地・海辺・火山・軌道・辺境）で変わる。
  どこでも「鉱石を買って精錬して売る」と少しもうかる甘めの設定。地域ごとの得意・不得意で、街めぐりに価値が出る。
- 保存：world.json の "economy"（家の持ち主・賃貸の期限・収納箱の中身・自宅）。所持金はプレイヤーの所持品 "credits"。
"""
import numpy as np

from . import log

L = log.get("citylife")

# ---------------------------------------------------------------------------
# 値段
# ---------------------------------------------------------------------------
CURRENCY = "credits"
# 基準の値段（1単位あたりのクレジット）。精錬：鉱石10→鉄4+ケイ砂3、硫黄10→燃料20、星鉱石10→燃料5+鉄8+ケイ砂6+部品1
BASE = {"ore": 1.0, "sulfur": 1.2, "rich_ore": 3.0, "water": 0.2, "fuel": 0.8,
        "iron": 2.4, "silica": 1.5, "parts": 16.0}
BUY_SPREAD = 1.10      # 店から買う値段 = 基準 × これ
SELL_SPREAD = 0.90     # 店に売る値段 = 基準 × これ
REGIONS = {
    "plain":    {"name": "平地の街", "mult": {}, "home": 400,
                 "note": "どれもふつうの値段"},
    "coast":    {"name": "海辺の街", "mult": {"fuel": 0.5, "water": 0.3, "sulfur": 1.3}, "home": 450,
                 "note": "燃料と水が安い（海の水から作れるので）。硫黄は高め"},
    "volcano":  {"name": "火山の街", "mult": {"sulfur": 0.4, "fuel": 0.85, "iron": 1.15, "parts": 1.1}, "home": 300,
                 "note": "硫黄がとても安い。鉄と部品は高め"},
    "orbit":    {"name": "軌道都市", "mult": {"ore": 0.6, "sulfur": 0.6, "rich_ore": 0.6, "fuel": 3.0, "water": 3.0,
                                         "parts": 1.3, "iron": 0.7, "silica": 0.7}, "home": 900,
                 "note": "鉱石は安いが、燃料と水がとても高い"},
    "frontier": {"name": "辺境の基地", "mult": {"rich_ore": 0.6, "fuel": 1.8, "water": 2.0, "parts": 1.2, "ore": 0.9,
                                           "iron": 0.9},
                 "home": 350, "note": "星鉱石が安い。燃料と水が高い"},
}
ROLE_NAMES = {"trader": "商人", "estate": "不動産屋", "citizen": "住民"}
FIRST = ["カイ", "ユナ", "レン", "ミオ", "ジン", "サラ", "トウマ", "リコ", "ノア", "エマ", "ケン", "アヤ", "ルカ", "ハル", "ニナ", "ゼン"]
LAST = ["-07", "・K", "・ゼロ", "・ネオ", "-X", "・ブルー", "・V", "-22", "・シン", ""]
STARTER_CREDITS = 200.0


def _rng(*keys):
    from .cities import _rng as r
    return r(*keys)


def region_info(city):
    return REGIONS.get(getattr(city, "region", "plain"), REGIONS["plain"])


def prices(city):
    """{資源: (買値, 売値)}（1単位あたり）。街ごとに少しだけばらつく（毎回同じ）。"""
    cache = getattr(city, "_prices", None)
    if cache is not None:
        return cache
    rng = _rng("prices", city.id)
    mult = region_info(city)["mult"]
    out = {}
    for res, v in BASE.items():
        base = v * mult.get(res, 1.0) * float(rng.uniform(0.93, 1.07))
        out[res] = (round(base * BUY_SPREAD, 2), round(base * SELL_SPREAD, 2))
    city._prices = out
    return out


def home_price(city, home):
    rng = _rng("home", home["id"])
    return float(round(region_info(city)["home"] * rng.uniform(0.8, 1.2) / 10) * 10)


RENT_SECONDS = 3600.0      # 1回の賃貸の期間（ゲーム内の時間）
RENT_RATE = 0.1            # 賃料 = 家の値段 × これ（1回分）


def rent_price(city, home):
    return float(round(home_price(city, home) * RENT_RATE))


# ---------------------------------------------------------------------------
# 内装のある建物
# ---------------------------------------------------------------------------
WALL = np.array([0.3, 0.31, 0.37])      # 内装のある建物の壁（窓の模様なし）
FURN = np.array([0.42, 0.4, 0.45])
WOOD = np.array([0.45, 0.32, 0.22])
SIGN = {"trader": np.array([1.0, 0.7, 0.15]), "estate": np.array([0.3, 1.0, 0.5]), "home": np.array([0.1, 0.95, 1.0])}
H_IN = 3.4                 # 内側の天井の高さ
T = 0.25                   # 壁の厚さ
DOOR_W, DOOR_H = 1.6, 2.6


def _walls(city, base, fr, w, d, b, doors, partitions=()):
    """中が空いた箱（壁・天井）。doors：前（+z）の壁の扉の位置（x 座標のリスト）。partitions：仕切り壁の x 座標。"""
    X, U, Z = fr
    ht = H_IN + 0.3
    hw, hd = w / 2, d / 2
    # 後ろ・左右
    city.add(base + U * (ht / 2) - Z * (hd - T / 2), fr, (hw, ht / 2, T / 2), WALL, None, True, b, "wall")
    for s in (-1, 1):
        city.add(base + U * (ht / 2) + X * s * (hw - T / 2), fr, (T / 2, ht / 2, hd - T), WALL, None, True, b, "wall")
    for px in partitions:
        city.add(base + U * (ht / 2) + X * px, fr, (T / 2, ht / 2, hd - T), WALL, None, True, b, "wall")
    # 前の壁（扉の所は空ける）
    xs = sorted(doors)
    edges = [-hw]
    for dx in xs:
        edges += [dx - DOOR_W / 2, dx + DOOR_W / 2]
    edges.append(hw)
    for k in range(0, len(edges), 2):
        a, c = edges[k], edges[k + 1]
        if c - a > 0.05:
            city.add(base + U * (ht / 2) + X * ((a + c) / 2) + Z * (hd - T / 2), fr, ((c - a) / 2, ht / 2, T / 2),
                     WALL, None, True, b, "wall")
    for dx in xs:   # 扉の上
        city.add(base + U * (DOOR_H + (ht - DOOR_H) / 2) + X * dx + Z * (hd - T / 2), fr,
                 (DOOR_W / 2, (ht - DOOR_H) / 2, T / 2), WALL, None, True, b, "wall")
    # 天井（屋根）
    city.add(base + U * (ht + 0.15), fr, (hw, 0.15, hd), WALL, None, True, b, "wall")
    return base + U * (ht + 0.3)            # 屋根の上（上にビルを重ねるとき用）


def _light(city, base, fr, x, z, b, col=(0.9, 0.95, 1.0)):
    X, U, Z = fr
    city.add(base + U * (H_IN - 0.05) + X * x + Z * z, fr, (0.6, 0.03, 0.3), col, col, False, b, "neon")


def build_shop(city, rng, base, fr, w, d, b, role):
    """店（商人）・不動産屋：中にカウンターと NPC。前の真ん中に扉。"""
    X, U, Z = fr
    top = _walls(city, base, fr, w, d, b, [0.0])
    hd = d / 2
    sc = SIGN[role]
    # 扉の上の看板と、入口の光
    city.add(base + U * (DOOR_H + 0.55) + Z * (hd + 0.08), fr, (min(w / 2 - 0.3, 2.2), 0.3, 0.06), sc, sc, False, b, "sign")
    city.add(base + U * 0.02 + Z * (hd + 0.6), fr, (DOOR_W / 2, 0.02, 0.5), sc * 0.6, sc * 0.6, False, b, "neon")
    # カウンター
    cz = -hd + 1.7
    cnt = city.add(base + U * 0.55 + Z * cz, fr, (w / 2 - T - 0.6, 0.55, 0.3), WOOD if role == "estate" else FURN,
                   None, True, b, "furn")
    # 棚（両側の壁）と天井の明かり
    for s in (-1, 1):
        city.add(base + U * 1.3 + X * s * (w / 2 - T - 0.25), fr, (0.22, 1.0, hd - 1.2), FURN * 0.8, None, True, b, "furn")
        for yy in (0.8, 1.5, 2.2):
            city.add(base + U * yy + X * s * (w / 2 - T - 0.48), fr, (0.03, 0.02, hd - 1.3), sc * 0.8, sc * 0.8,
                     False, b, "neon")
    _light(city, base, fr, 0.0, 0.0, b)
    npc = add_npc(city, rng, role, base + Z * (cz - 0.9), Z, U, b)
    city.parts[cnt]["tag"] = ("counter", npc["id"])
    return top


def build_housing(city, rng, base, fr, w, d, b):
    """住宅（何部屋か横に並ぶ）。部屋ごとに扉・ベッド・収納箱・明かり。"""
    X, U, Z = fr
    n = 3 if w >= 8.5 else 2
    rw = w / n
    xs = [-w / 2 + rw * (k + 0.5) for k in range(n)]
    parts = [-w / 2 + rw * k for k in range(1, n)]
    top = _walls(city, base, fr, w, d, b, xs, parts)
    hd = d / 2
    for k, cx in enumerate(xs):
        hid = f"{city.id}-h{len(city.homes)}"
        bed = city.add(base + U * 0.3 + X * (cx - rw / 2 + T + 0.55) + Z * (-hd + T + 1.05), fr, (0.5, 0.3, 1.0),
                       np.array([0.3, 0.35, 0.7]), None, True, b, "furn")
        chest = city.add(base + U * 0.4 + X * (cx + rw / 2 - T - 0.45) + Z * (-hd + T + 0.35), fr, (0.4, 0.4, 0.3),
                         WOOD, None, True, b, "furn")
        city.parts[bed]["tag"] = ("bed", hid)
        city.parts[chest]["tag"] = ("chest", hid)
        _light(city, base, fr, cx, 0.0, b, (1.0, 0.85, 0.6))
        sc = SIGN["home"]
        city.add(base + U * (DOOR_H + 0.25) + X * cx + Z * (hd + 0.07), fr, (0.35, 0.12, 0.05), sc, sc, False, b, "sign")
        city.homes.append({"id": hid, "name": f"{city.name} {len(city.homes) + 1}号室", "bld": b,
                           "door": base + X * cx + Z * (hd + 1.2)})
    return top


# ---------------------------------------------------------------------------
# NPC
# ---------------------------------------------------------------------------
TALK = {
    "trader": ["いらっしゃい。{note}。", "ここは{region}だからね。{note}。", "鉱石を精錬して持ってくれば、高く買い取るよ。"],
    "estate": ["{city}の部屋、空いてるよ。買ってもいいし、1時間だけ借りてもいい。", "部屋には収納箱とベッドが付いてる。"],
    "citizen": ["軌道都市じゃ燃料がとても高く売れるらしいよ。", "火山の街は硫黄が安いんだって。",
                "海辺の街なら燃料が安く買えるよ。", "月や小惑星の基地は星鉱石が安いらしい。",
                "このあたりは{region}。{note}。", "部屋を持ってると、荷物を預けておけるよ。",
                "ベッドで休むと、そこが自宅になる。M キーで自宅の目印が出せるよ。", "ネオンの線は段差の目印だよ。気をつけて。"],
}


def add_npc(city, rng, role, pos, face, up, b, wander=0.0):
    nid = f"{city.id}-n{len(city.npcs)}"
    name = FIRST[int(rng.integers(len(FIRST)))] + LAST[int(rng.integers(len(LAST)))]
    col = np.array([0.2, 0.22, 0.28]) + rng.uniform(0, 0.25, 3)
    visor = np.array([[1.0, 0.2, 0.75], [0.1, 0.95, 1.0], [1.0, 0.8, 0.15], [0.3, 1.0, 0.5]])[int(rng.integers(4))]
    npc = {"id": nid, "name": name, "role": role, "pos": np.asarray(pos, float), "face": np.asarray(face, float),
           "up": np.asarray(up, float), "bld": b, "color": col, "visor": visor, "wander": float(wander),
           "phase": float(rng.uniform(0, 2 * np.pi)), "speed": float(rng.uniform(0.25, 0.45)),
           "talk": int(rng.integers(100))}
    city.npcs.append(npc)
    return npc


def npc_pose(city, npc, t):
    """NPC の世界座標の足元・前向き・上向き（住民はゆっくり円を描いて歩く）。"""
    p = npc["pos"]
    f = npc["face"]
    up = npc["up"]
    if npc["wander"] > 0:
        side = np.cross(up, f)
        a = npc["phase"] + t * npc["speed"] / npc["wander"]
        p = p + (f * np.cos(a) + side * np.sin(a)) * npc["wander"]
        f = -f * np.sin(a) + side * np.cos(a)
    R = city.R
    return city.x + R @ p, R @ f, R @ up


def npc_alive(city, npc):
    if getattr(city, "fallen", False):
        return False                     # 滅びた都市には誰もいない
    b = npc["bld"]
    return b < 0 or city.state[b] < 2


NPC_HALF = np.array([0.3, 0.9, 0.22])


def npc_box(city, npc, t):
    """当たり判定・視線用の箱（中心・軸・半分の大きさ）。"""
    p, f, up = npc_pose(city, npc, t)
    side = np.cross(up, f)
    return p + up * NPC_HALF[1], np.stack([side, up, f]), NPC_HALF


def talk_line(city, npc):
    info = region_info(city)
    lines = TALK[npc["role"]]
    k = npc["talk"] % len(lines)
    npc["talk"] += 1
    return lines[k].format(note=info["note"], region=info["name"], city=city.name)


# ---------------------------------------------------------------------------
# 経済（取引・家）
# ---------------------------------------------------------------------------
class Economy:
    def __init__(self, world):
        self.world = world
        st = world.settings.setdefault("economy", {})
        self.homes = st.setdefault("homes", {})        # {家ID: {"mode": "own"/"rent", "until": 時刻}}
        self.storage = st.setdefault("storage", {})    # {家ID: {資源: 量}}
        self.state = st

    @property
    def creative(self):
        return self.world.settings.get("creative", True)

    def credits(self):
        return float(self.world.player.inventory.get(CURRENCY, 0.0))

    def _add_credits(self, v):
        inv = self.world.player.inventory
        inv[CURRENCY] = round(float(inv.get(CURRENCY, 0.0)) + v, 2)

    # --- 取引 ---------------------------------------------------------------
    def buy(self, city, res, amount):
        """店から買う。戻り値：メッセージ"""
        if self.creative:
            return "クリエイティブでは取引はありません"
        buy, _ = prices(city)[res]
        amount = min(float(amount), self.credits() / buy if buy > 0 else 0)
        amount = float(np.floor(amount))
        if amount <= 0:
            return "クレジットが足りません"
        cost = round(buy * amount, 2)
        self._add_credits(-cost)
        self._politics_trade(city, cost)
        inv = self.world.player.inventory
        inv[res] = inv.get(res, 0.0) + amount
        return f"{self.world.registry.res_name(res)} {amount:.0f} を {cost:.1f} クレジットで買いました"

    def sell(self, city, res, amount):
        if self.creative:
            return "クリエイティブでは取引はありません"
        _, sell = prices(city)[res]
        inv = self.world.player.inventory
        amount = float(np.floor(min(float(amount), inv.get(res, 0.0))))
        if amount <= 0:
            return f"{self.world.registry.res_name(res)} を持っていません"
        inv[res] = inv.get(res, 0.0) - amount
        gain = round(sell * amount, 2)
        self._add_credits(gain)
        self._politics_trade(city, gain)
        return f"{self.world.registry.res_name(res)} {amount:.0f} を {gain:.1f} クレジットで売りました"

    def _politics_trade(self, city, value):
        pol = getattr(self.world, "politics", None)
        if pol is not None:
            pol.record_trade(city, value)

    # --- 家 -----------------------------------------------------------------
    def city_fallen(self, city):
        """都市が滅びた：その都市の部屋は持ち主なしに。収納箱の中身は不動産屋が預かる（どこの不動産屋でも受け取れる）。"""
        esc = self.state.setdefault("escrow", {})
        lost = []
        for h in city.homes:
            box = self.storage.pop(h["id"], None)
            if box:
                for k, v in box.items():
                    esc[k] = esc.get(k, 0.0) + v
            if self.homes.pop(h["id"], None):
                lost.append(h["name"])
            if self.state.get("home") == h["id"]:
                self.state["home"] = None
        if lost:
            self.world.message("部屋を失いました：" + "、".join(lost) + "（収納箱の中身は不動産屋が預かっています）")

    def claim_escrow(self):
        esc = self.state.get("escrow") or {}
        if not esc:
            return "預かり品はありません"
        inv = self.world.player.inventory
        for k, v in esc.items():
            inv[k] = inv.get(k, 0.0) + v
        txt = self.world.registry.res_text(esc)
        self.state["escrow"] = {}
        return f"預かり品を受け取りました：{txt}"

    def home_status(self, hid):
        """None（空き）/ "own" / "rent"（期限内）/ "expired"（期限切れ）"""
        h = self.homes.get(hid)
        if not h:
            return None
        if h.get("mode") == "own":
            return "own"
        return "rent" if float(h.get("until", 0)) > self.world.time else "expired"

    def can_use(self, hid):
        city = self._city_of_home(hid)
        if city is not None and getattr(city, "fallen", False):
            return False
        return self.creative or self.home_status(hid) in ("own", "rent")

    def _city_of_home(self, hid):
        for c in getattr(self.world.cities, "cities", []):
            if hid.startswith(c.id + "-h"):
                return c
        return None

    def rent_left(self, hid):
        h = self.homes.get(hid) or {}
        return max(0.0, float(h.get("until", 0)) - self.world.time)

    def buy_home(self, city, home):
        if self.creative:
            return "クリエイティブでは部屋は買わなくても使えます"
        if self.home_status(home["id"]) == "own":
            return "もうあなたの部屋です"
        price = home_price(city, home)
        if self.credits() < price:
            return f"クレジットが足りません（{price:.0f} 必要）"
        self._add_credits(-price)
        self.homes[home["id"]] = {"mode": "own"}
        self.state.setdefault("home", home["id"])
        return f"{home['name']} を {price:.0f} クレジットで買いました（ベッドで休むと自宅になります）"

    def rent_home(self, city, home):
        if self.creative:
            return "クリエイティブでは部屋は借りなくても使えます"
        st = self.home_status(home["id"])
        if st == "own":
            return "もうあなたの部屋です"
        price = rent_price(city, home)
        if self.credits() < price:
            return f"クレジットが足りません（{price:.0f} 必要）"
        self._add_credits(-price)
        start = max(self.world.time, float((self.homes.get(home["id"]) or {}).get("until", 0)))
        self.homes[home["id"]] = {"mode": "rent", "until": start + RENT_SECONDS}
        self.state.setdefault("home", home["id"])
        left = (start + RENT_SECONDS - self.world.time) / 60
        return f"{home['name']} を {price:.0f} クレジットで借りました（残り {left:.0f} 分）"

    def store(self, hid, res, amount):
        """手持ち → 収納箱（amount < 0 で 収納箱 → 手持ち）"""
        inv = self.world.player.inventory
        box = self.storage.setdefault(hid, {})
        src, dst = (inv, box) if amount > 0 else (box, inv)
        amount = abs(float(amount))
        mv = min(amount, src.get(res, 0.0))
        if mv <= 0:
            return False
        src[res] = src.get(res, 0.0) - mv
        dst[res] = dst.get(res, 0.0) + mv
        for d in (inv, box):
            if d.get(res, 0.0) <= 1e-9:
                d.pop(res, None)
        return True

    def set_home(self, hid):
        self.state["home"] = hid

    def home_id(self):
        hid = self.state.get("home")
        return hid if hid and (self.creative or self.home_status(hid) in ("own", "rent", "expired")) else None

    def give_starter(self):
        p = self.world.player
        if self.creative or getattr(p, "credits_given", False):
            return
        p.inventory[CURRENCY] = float(p.inventory.get(CURRENCY, 0.0)) + STARTER_CREDITS
        p.credits_given = True
        self.world.message(f"{STARTER_CREDITS:.0f} クレジットを受け取りました（街の商人と取引できます）")
