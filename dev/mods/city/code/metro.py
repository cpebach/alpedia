"""A-10：大都市の暮らし。百貨店と値下げ・高い部屋（駅近・高層階）・収益物件・工場の仕事。

大都市圏 MOD（megalopolis）が、場所ごとに街（City）を作るときに、ここの建物を使う。
- 大きな街（stype "metro"）：大都市圏の建物の中に、百貨店・不動産屋・商人・駅近とふつうの部屋が点在する。
  いちばん高いビルの上の方の階が高層階の部屋（1 階のロビーのエレベーターで上がる）。店の前に住民。
- 工業地帯（stype "works"）：工場の建物の中に仕事場（班長・置き場・装置・作業員）、倉庫の中に商人。

百貨店（店員 role "dept"）：ふだんは商人より高い（DEPT_MARKUP 倍）。時間帯（SALE_SLOT 秒）ごとに、ときどき
  1〜2 品を数分だけ大きく値下げする（街と時刻で決まる）。大きな値下げ（BIG_SALE 以上）はどこにも出さず、
  町の情報通（NPC の "gossip"）に話しかけると、いつ・どの品か教えてくれる。
収益物件：大都市の不動産屋で、工業地帯の工場・集落の鉱山・農場を買える（とても高い）。PROP_PERIOD 秒ごとに
  資源やクレジットが手持ちに届く（遊んでいない間は進まない。たまる分は MAX_BACKLOG 回まで）。
工場の仕事（班長 role "foreman"）：話しかけると仕事が始まり、渡された物を決められた置き場に置くか、決められた装置に入れる
  （E）とお金がもらえる。早いとおまけ。もう一度班長に話すと終わる。仕事・売買はサバイバルだけ。
保存：world.json の "economy"（"props"：持っている物件、"job"：仕事中の状態）。
"""
import numpy as np

from . import citylife
from .citylife import FURN, WOOD, H_IN, T, add_npc, build_housing, build_shop, _light, _walls

# ---------------------------------------------------------------------------
# 百貨店
# ---------------------------------------------------------------------------
DEPT_ITEMS = ["fuel", "water", "iron", "silica", "parts", "electronics", "rich_ore"]
DEPT_MARKUP = 1.35          # ふだんの値段 = 商人の買値 × これ
SALE_SLOT = 900.0           # 値下げを決める時間帯の長さ（秒）
SALE_CHANCE = 0.55          # 時間帯に値下げがある割合
SALE_OFF = [(0.2, 4.0), (0.3, 4.0), (0.5, 3.0), (0.6, 2.0), (0.7, 1.5), (0.8, 1.0)]   # (割引, 重み)
SALE_DUR = (180.0, 420.0)   # 値下げの長さ（秒）
BIG_SALE = 0.5              # これ以上の割引は「大きな値下げ」（うわさでしかわからない）


def _rng(*keys):
    from .cities import _rng as r
    return r(*keys)


def sales(city, slot):
    """時間帯 slot の値下げ：[(資源, 割引, 始まりの時刻, 終わりの時刻)]（街と時間帯で毎回同じ）"""
    r = _rng("sale", city.id, int(slot))
    if r.random() > SALE_CHANCE:
        return []
    n = 1 if r.random() < 0.7 else 2
    items = [DEPT_ITEMS[int(i)] for i in r.choice(len(DEPT_ITEMS), n, replace=False)]
    w = np.array([x[1] for x in SALE_OFF])
    w /= w.sum()
    out = []
    for res in items:
        off = SALE_OFF[int(r.choice(len(SALE_OFF), p=w))][0]
        dur = float(r.uniform(*SALE_DUR))
        t0 = slot * SALE_SLOT + float(r.uniform(0, SALE_SLOT - dur))
        out.append((res, off, t0, t0 + dur))
    return out


def sale_now(city, t):
    """今の値下げ {資源: 割引}"""
    return {res: off for res, off, a, b in sales(city, int(t // SALE_SLOT)) if a <= t < b}


def upcoming(city, t, ahead=2):
    """今から ahead 時間帯先までの、まだ終わっていない大きな値下げ（始まる順）"""
    s0 = int(t // SALE_SLOT)
    out = [x for s in range(s0, s0 + ahead + 1) for x in sales(city, s) if x[1] >= BIG_SALE and x[3] > t]
    return sorted(out, key=lambda x: x[2])


def dept_prices(city, t):
    """{資源: (今の値段, 割引, ふだんの値段)}（1単位あたり）"""
    base = citylife.prices(city)
    on = sale_now(city, t)
    out = {}
    for res in DEPT_ITEMS:
        if res not in base:
            continue
        p = base[res][0] * DEPT_MARKUP
        off = on.get(res, 0.0)
        out[res] = (round(p * (1.0 - off), 2), off, round(p, 2))
    return out


def dept_buy(eco, city, res, amount):
    if eco.creative:
        return "クリエイティブでは取引はありません"
    pr = dept_prices(city, eco.world.time).get(res)
    if pr is None:
        return "その品物は扱っていません"
    price = pr[0]
    amount = float(np.floor(min(float(amount), eco.credits() / price if price > 0 else 0)))
    if amount <= 0:
        return "クレジットが足りません"
    cost = round(price * amount, 2)
    eco._add_credits(-cost)
    eco._politics_trade(city, cost)
    inv = eco.world.player.inventory
    inv[res] = inv.get(res, 0.0) + amount
    sale = f"（{int(round(pr[1] * 10))}割引）" if pr[1] > 0 else ""
    return f"{eco.world.registry.res_name(res)} {amount:.0f} を {cost:.1f} クレジットで買いました{sale}"


def _metro_cities(pack):
    return [c for c in pack.cities.cities if getattr(c, "stype", "") == "metro" and not getattr(c, "fallen", False)]


def tip(pack, city, t):
    """情報通のうわさ：百貨店の大きな値下げ（この街の百貨店を先に）。無ければ None"""
    stores = sorted(_metro_cities(pack), key=lambda c: (c is not city, float(np.linalg.norm(c.x - city.x))))
    for c in stores:
        up = upcoming(c, t)
        if not up:
            continue
        res, off, a, b = up[0]
        name = pack.world.registry.res_name(res)
        where = "ここの百貨店" if c is city else f"{c.name}の百貨店"
        pct = int(round(off * 10))
        if a <= t:
            return f"ねえ、{where}で今、{name}が{pct}割引だって！あと {max(1.0, (b - t) / 60):.0f} 分くらいで終わるよ"
        return f"ここだけの話、{where}で {max(1.0, (a - t) / 60):.0f} 分後から{name}が{pct}割引になるらしいよ（{(b - a) / 60:.0f} 分くらいだけ）"
    return None


# ---------------------------------------------------------------------------
# 収益物件
# ---------------------------------------------------------------------------
PROPS = {
    "factory": {"name": "工場", "price": 30000, "out": {"credits": 120.0, "electronics": 3.0, "parts": 2.0}},
    "mine": {"name": "鉱山の採掘権", "price": 15000, "out": {"ore": 60.0}},
    "farm": {"name": "農場・牧場", "price": 12000, "out": {"credits": 150.0, "water": 40.0}},
}
PROP_KIND = {"works": "factory", "mine": "mine", "farm": "farm"}     # 街の stype → 物件の種類
PROP_PERIOD = 600.0         # 収入が届く間隔（ゲーム内の秒）
MAX_BACKLOG = 6             # 一度に届く回数の上限


def properties(pack):
    """買える収益物件：[{id, kind, city, name, price}]"""
    out = []
    for c in pack.cities.cities:
        kind = PROP_KIND.get(getattr(c, "stype", None))
        if kind is None:
            continue
        r = _rng("prop", c.id)
        price = float(round(PROPS[kind]["price"] * r.uniform(0.85, 1.15) / 100) * 100)
        out.append({"id": f"prop-{c.id}", "kind": kind, "city": c, "name": f"{c.name}の{PROPS[kind]['name']}", "price": price})
    return out


def prop_output(p):
    """1 回分の収入 {資源: 量}。鉱山は場所で中身が変わる"""
    out = dict(PROPS[p["kind"]]["out"])
    if p["kind"] == "mine":
        reg = getattr(p["city"], "region", "plain")
        if reg == "volcano":
            out["sulfur"] = 30.0
        elif reg == "frontier":
            out["rich_ore"] = 15.0
    return out


def owned(eco, pid):
    return pid in (eco.state.get("props") or {})


def buy_property(eco, pid):
    if eco.creative:
        return "クリエイティブでは物件は買えません"
    p = next((x for x in properties(eco.pack) if x["id"] == pid), None)
    if p is None:
        return "その物件はありません"
    props = eco.state.setdefault("props", {})
    if pid in props:
        return "もうあなたの物件です"
    if getattr(p["city"], "fallen", False):
        return "滅びた場所の物件は買えません"
    if eco.credits() < p["price"]:
        return f"クレジットが足りません（{p['price']:.0f} 必要）"
    eco._add_credits(-p["price"])
    props[pid] = {"paid": eco.world.time}
    txt = eco.world.registry.res_text(prop_output(p))
    return f"{p['name']} を {p['price']:.0f} クレジットで買いました（{PROP_PERIOD / 60:.0f} 分ごとに {txt} が届きます）"


def pay_properties(eco):
    props = eco.state.get("props") or {}
    if not props or eco.creative:
        return
    t = eco.world.time
    by = {p["id"]: p for p in properties(eco.pack)}
    got = {}
    for pid, s in props.items():
        p = by.get(pid)
        if float(s.get("paid", t)) > t:
            s["paid"] = t
        if p is None or getattr(p["city"], "fallen", False):
            s["paid"] = t
            continue
        n = int((t - float(s["paid"])) // PROP_PERIOD)
        if n <= 0:
            continue
        s["paid"] = float(s["paid"]) + n * PROP_PERIOD
        for k, v in prop_output(p).items():
            got[k] = got.get(k, 0.0) + v * min(n, MAX_BACKLOG)
    if got:
        inv = eco.world.player.inventory
        for k, v in got.items():
            inv[k] = round(float(inv.get(k, 0.0)) + v, 2)
        eco.world.message("収益物件から届きました：" + eco.world.registry.res_text(got))


# ---------------------------------------------------------------------------
# 工場の仕事
# ---------------------------------------------------------------------------
JOB_PAY = 14.0              # 1 件のお金
JOB_FAST = 35.0             # これより早く終えるとおまけ（秒）
JOB_BONUS = 8.0
N_PADS = 4
MACHINES = [("プレス機（赤）", (1.0, 0.25, 0.2)), ("溶接機（青）", (0.2, 0.5, 1.0)), ("検査装置（黄）", (1.0, 0.85, 0.15))]
CARRY_ITEMS = ["部品の箱", "鉄板の束", "配線のロール", "電子部品のトレー", "ネジの袋"]
FEED_ITEMS = [["鉄板", "アルミの板"], ["フレーム", "パイプ"], ["基板", "組み立てた部品"]]


def _new_task(eco, job):
    r = np.random.default_rng(int(eco.world.time * 1000) % (2 ** 31) + 7 * job["done"])
    if r.random() < 0.5:
        k = int(r.integers(N_PADS))
        item = CARRY_ITEMS[int(r.integers(len(CARRY_ITEMS)))]
        job.update(type="pad", target=k, item=item, t0=eco.world.time)
        return f"「{item}」を受け取りました → 置き場 {k + 1} に置いてください（E）"
    k = int(r.integers(len(MACHINES)))
    item = FEED_ITEMS[k][int(r.integers(len(FEED_ITEMS[k])))]
    job.update(type="machine", target=k, item=item, t0=eco.world.time)
    return f"「{item}」を受け取りました → {MACHINES[k][0]} に入れてください（E）"


def job_talk(eco, city, npc):
    """班長に話す：仕事を始める／終える"""
    if eco.creative:
        return f"{npc['name']}「仕事はサバイバルでな」"
    job = eco.state.get("job")
    if job and job.get("city") == city.id:
        eco.state["job"] = None
        return f"{npc['name']}「お疲れさん。{job['done']} 件で {job['earned']:.0f} クレジットだ。またな」"
    job = {"city": city.id, "done": 0, "earned": 0.0}
    eco.state["job"] = job
    return f"{npc['name']}「よし、仕事だ。」" + _new_task(eco, job) + "（やめるときはもう一度話しかける）"


def job_act(eco, city, kind, k):
    """置き場（"pad"）・装置（"machine"）で E"""
    job = eco.state.get("job")
    what = f"置き場 {k + 1}" if kind == "pad" else MACHINES[k][0]
    if eco.creative or not job or job.get("city") != city.id:
        return f"{what}（班長に話しかけると仕事がもらえます）"
    if job.get("type") != kind or job.get("target") != k:
        want = f"置き場 {job['target'] + 1}" if job["type"] == "pad" else MACHINES[job["target"]][0]
        return f"ここは{what}です。「{job['item']}」は {want} へ"
    fast = eco.world.time - float(job.get("t0", eco.world.time)) <= JOB_FAST
    pay = JOB_PAY + (JOB_BONUS if fast else 0.0)
    eco._add_credits(pay)
    job["done"] += 1
    job["earned"] = float(job["earned"]) + pay
    done = "置きました" if kind == "pad" else "入れました"
    return f"「{job['item']}」を{done}。+{pay:.0f} クレジット{'（早い！）' if fast else ''}　次：" + _new_task(eco, job)


def job_target(pack):
    """仕事中なら (街, 街ローカルの点, 名前)。目印に使う"""
    eco = pack.economy
    job = (eco.state.get("job") if eco else None) or {}
    city = pack.cities.by_id.get(job.get("city"))
    if city is None or "type" not in job:
        return None
    for k, p in enumerate(city.parts):
        tag = p.get("tag")
        if tag and tag[0] == job["type"] and tag[1] == job["target"]:
            name = f"置き場 {tag[1] + 1}" if tag[0] == "pad" else MACHINES[tag[1]][0]
            return city, city.C0[k] if hasattr(city, "C0") else p["c"], name
    return None


# ---------------------------------------------------------------------------
# 建物
# ---------------------------------------------------------------------------
def build_dept(city, rng, base, fr, w, d, b, walls=True):
    """百貨店：店員のカウンターと、売り場の台・棚・明かり"""
    X, U, Z = fr
    top = build_shop(city, rng, base, fr, w, d, b, "dept", walls)
    sc = citylife.SIGN["dept"]
    for x in (-w / 4, w / 4):
        for z in (-0.6, 1.6):
            city.add(base + U * 0.45 + X * x + Z * z, fr, (1.2, 0.45, 0.6), FURN, None, True, b, "furn")
            city.add(base + U * 0.93 + X * x + Z * z, fr, (1.1, 0.03, 0.5), sc * 0.7, sc * 0.7, False, b, "neon")
    for x in (-w / 3, 0.0, w / 3):
        _light(city, base, fr, x, 1.0, b, (1.0, 0.92, 0.95))
    return top


def lift(city, base, fr, x, z, b, hid, way):
    """エレベーター（E で上の部屋へ／下へ）"""
    X, U, Z = fr
    k = city.add(base + U * 1.25 + X * x + Z * z, fr, (0.6, 1.25, 0.45), (0.18, 0.2, 0.26), None, True, b, "furn")
    city.parts[k]["tag"] = ("lift", hid, way)
    c = np.array([0.4, 1.0, 1.0])
    city.add(base + U * 2.55 + X * x + Z * z, fr, (0.65, 0.05, 0.5), c, c, False, b, "neon")
    city.add(base + U * 1.25 + X * x + Z * (z + 0.47), fr, (0.03, 1.1, 0.02), c, c, False, b, "neon")
    return k


def build_view_floor(city, rng, base, fr, w, d, b, lobby, lobby_x, name, inside=None):
    """高層階の部屋：既にある高いビルの中の上の階に、床と家具（ビルの窓から街が見える）。
    base：床の上面の真ん中（街ローカル）、w×d：床の広さ。lobby：1 階のロビーの床の点（街ローカル）、lobby_x：ロビーのエレベーターの横の位置。
    1 階のエレベーターと、この階のエレベーターでつなぐ。部屋（city.homes の 1 つ）を返す"""
    X, U, Z = fr
    city.add(base - U * 0.15, fr, (w / 2, 0.15, d / 2), (0.32, 0.3, 0.3), None, True, b, "found")      # 床
    hid = f"{city.id}-h{len(city.homes)}"
    rw, rd = min(w, 18.0), min(d, 14.0)                                   # 住む所（床の真ん中あたり）
    bed = city.add(base + U * 0.3 + X * (-rw / 2 + 0.8) + Z * (-rd / 2 + 1.2), fr, (0.65, 0.3, 1.05),
                   np.array([0.78, 0.78, 0.84]), None, True, b, "furn")
    chest = city.add(base + U * 0.4 + X * (rw / 2 - 0.6) + Z * (-rd / 2 + 0.5), fr, (0.4, 0.4, 0.3), WOOD, None, True, b, "furn")
    city.parts[bed]["tag"] = ("bed", hid)
    city.parts[chest]["tag"] = ("chest", hid)
    city.add(base + U * 0.3 + X * (rw / 4) + Z * (rd / 2 - 1.5), fr, (1.6, 0.3, 0.5), np.array([0.5, 0.2, 0.25]), None, True, b,
             "furn")                                                       # ソファ（窓の方）
    city.add(base + U * 0.25 + X * (rw / 4) + Z * (rd / 2 - 3.0), fr, (0.8, 0.25, 0.5), WOOD, None, True, b, "furn")   # テーブル
    city.add(base + U * 0.01 + X * (rw / 4) + Z * (rd / 2 - 2.6), fr, (2.2, 0.01, 1.6), np.array([0.55, 0.45, 0.35]), None, False,
             b, "decor")                                                   # 敷物
    for x in (-rw / 4, rw / 4):
        _light(city, base, fr, x, 0.0, b, (1.0, 0.9, 0.75))
    lift(city, base, fr, -rw / 4, rd / 2 - 0.8, b, hid, "down")
    lift(city, lobby, fr, lobby_x, 0.0, b, hid, "up")
    stand = base + X * (-rw / 4) + Z * (rd / 2 - 2.2)
    home = {"id": hid, "name": name, "bld": b, "grade": "view", "door": stand, "lift_up": stand,
            "lift_down": lobby + X * lobby_x - Z * 1.6, "inside": inside}
    city.homes.append(home)
    return home


def housing(city, rng, base, fr, w, d, b, grade="normal", label=""):
    """部屋の並ぶ建物（citylife.build_housing）。格と名前をつける"""
    n0 = len(city.homes)
    top = build_housing(city, rng, base, fr, w, d, b)
    for k, h in enumerate(city.homes[n0:]):
        h["grade"] = grade
        if label:
            h["name"] = f"{city.name} {label} {n0 + k + 1}号室"
    return top


def build_factory_hall(city, rng, base, fr, w, d, b, walls=True):
    """工場（仕事場）：班長と机・置き場（床の光る四角と番号）・装置 3 つ・ベルトコンベア・作業員。
    walls=False：既にある工場の建物の中に置く"""
    X, U, Z = fr
    top = _walls(city, base, fr, w, d, b, [0.0]) if walls else None
    hd, hw = d / 2, w / 2
    sc = citylife.SIGN["factory"]
    if walls:
        city.add(base + U * (citylife.DOOR_H + 0.55) + Z * (hd + 0.08), fr, (2.4, 0.3, 0.06), sc, sc, False, b, "sign")
    for x in (-w / 3, 0.0, w / 3):
        for z in (-d / 4, d / 4):
            _light(city, base, fr, x, z, b)
    # 班長と机（入口の左）
    city.add(base + U * 0.5 + X * (-hw + 3.0) + Z * (hd - 3.0), fr, (1.0, 0.5, 0.5), WOOD, None, True, b, "furn")
    add_npc(city, rng, "foreman", base + X * (-hw + 3.0) + Z * (hd - 4.2), Z, U, b)
    # 置き場（奥の壁ぞい）
    for k in range(N_PADS):
        x = -hw + 3.0 + k * (w - 6.0) / max(N_PADS - 1, 1)
        z = -hd + T + 1.5
        p = city.add(base + U * 0.04 + X * x + Z * z, fr, (0.9, 0.04, 0.9), sc * 0.6, sc * 0.6, False, b, "neon")
        city.parts[p]["tag"] = ("pad", k)
        # 番号（壁の上の札：点の数）
        for j in range(k + 1):
            city.add(base + U * 2.4 + X * (x - 0.3 * k / 2 + 0.3 * j) + Z * (-hd + T + 0.06), fr, (0.1, 0.1, 0.03),
                     sc, sc, False, b, "sign")
    # 装置（真ん中に並ぶ）
    for k, (_, col) in enumerate(MACHINES):
        x = (k - 1) * w / 4
        c = np.array(col)
        m = city.add(base + U * 0.85 + X * x + Z * 0.5, fr, (0.9, 0.85, 0.9), c * 0.55, None, True, b, "furn")
        city.parts[m]["tag"] = ("machine", k)
        city.add(base + U * 1.75 + X * x + Z * 0.5, fr, (0.95, 0.05, 0.95), c, c, False, b, "neon")
        city.add(base + U * 1.0 + X * x + Z * 1.42, fr, (0.5, 0.25, 0.03), c, c, False, b, "neon")     # 投入口
    # ベルトコンベア（右の壁ぞい）と荷物
    city.add(base + U * 0.45 + X * (hw - 1.5), fr, (0.6, 0.45, hd - 2.0), (0.2, 0.2, 0.22), None, True, b, "furn")
    for z in np.linspace(-hd + 3, hd - 3, 5):
        city.add(base + U * 1.15 + X * (hw - 1.5) + Z * z, fr, (0.35, 0.25, 0.35), (0.55, 0.42, 0.25), None, False, b,
                 "decor")
    # 作業員
    for k in range(3):
        p = base + X * rng.uniform(-hw + 4, hw - 4) + Z * rng.uniform(-hd + 3.5, hd - 4)
        add_npc(city, rng, "worker", p, Z, U, b, wander=float(rng.uniform(1.0, 2.0)))
    return top
