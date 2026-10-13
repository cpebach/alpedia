"""都市の思惑：訪れた都市は、取引のされ方で「緊張」がたまり、ほかの都市を征服したり攻撃して滅ぼしたりする。

- 野心（取引のしすぎ）：短い時間に大量の取引をされた都市は強気になり、ほかの都市を「征服」して自分と同じ取引の傾向（地域）にする。
- 不満（放っておかれる）：訪れてから長い間取引のない都市は不満をためて、ほかの都市を「攻撃」して滅ぼそうとする。
- 緊張が PLAN_AT を超えると計画を立て（目標・実行予定時刻）、その時刻にまだ緊張が高ければ実行する。
- プレイヤーはマップ（K キー）から、訪れた都市に通信で助言できる（通信費がかかり、同じ都市へは少し間をあける）：
  説得する（緊張を下げる）／防御を促す（攻められても持ちこたえやすくなる）／攻撃の目標を変える。
- 滅ぼされた都市は建物の多くが壊れた廃墟になり、NPC はいなくなる。部屋の収納箱の中身は不動産屋が預かる（どの街の不動産屋でも受け取れる）。
- サバイバルで、街があるときだけ動く。保存は world.json の "politics"。
"""
import numpy as np

from . import log

L = log.get("politics")

MIN = 60.0
NEGLECT_AFTER = 25 * MIN        # 最後の取引（または初めて訪れて）からこれだけ取引がないと、不満がたまり始める
NEGLECT_RATE = 2.5 / MIN        # 不満のたまり方（緊張／秒）
HEAT_TAU = 10 * MIN             # 取引の勢い（クレジット）が減っていく速さ
HEAT_LIMIT = 400.0              # 取引の勢いがこれを超えると野心がたまる
HEAT_RATE = 1.0 / 100 / MIN     # （勢い − 上限）あたりの野心のたまり方
MAX_RATE = 3.0 / MIN            # 野心のたまり方の上限（どれだけ取引しても、計画まで 20 分以上かかる）
CALM_RATE = 1.0 / MIN           # どちらでもないときの緊張の下がり方
CALM_AFTER_TRADE = 3.0 / MIN    # 取引のあった都市の不満の下がり方
PLAN_AT = 70.0                  # 緊張がこれを超えると計画を立てる
PLAN_ETA = 12 * MIN             # 計画から実行までの時間
COOLDOWN = 20 * MIN             # 行動したあと次の計画までの時間
ADVICE_FEE = 20.0               # 助言の通信費（クレジット）
ADVICE_WAIT = 5 * MIN           # 同じ都市への助言の間隔
PERSUADE = 35.0                 # 説得で下がる緊張
DEFENSE_TIME = 30 * MIN         # 防御の効き目が続く時間
VISIT_MARGIN = 30.0             # 街の外周からこの距離に入ると「訪れた」

MOTIVE = {"ambition": "野心（取引が多すぎて強気）", "resentment": "不満（取引がなく放っておかれている）"}
ACTION = {"conquer": "征服", "attack": "攻撃"}


class Politics:
    def __init__(self, world):
        self.world = world
        self.cm = world.cities
        st = world.settings.setdefault("politics", {})
        self.cities = st.setdefault("cities", {})     # {都市ID: 状態}
        self.news = st.setdefault("news", [])         # [(時刻, 文)]
        self._acc = 0.0
        for c in self.cm.cities:
            s = self.cities.get(c.id)
            if s:
                if s.get("region"):
                    c.region = s["region"]
                    c._prices = None
                if s.get("fallen"):
                    c.fallen = True

    # --- 基本 ---------------------------------------------------------------
    @property
    def active(self):
        return self.cm.enabled and not self.world.settings.get("creative", True)

    def st(self, city):
        s = self.cities.get(city.id)
        if s is None:
            s = self.cities[city.id] = {"visited": False, "tension": 0.0, "heat": 0.0, "last_trade": None,
                                        "plan": None, "defense": 0, "defense_until": 0.0, "cooldown": 0.0,
                                        "advice_at": -1e9, "owner": None, "region": None, "fallen": False}
        return s

    def visited(self, city):
        return bool((self.cities.get(city.id) or {}).get("visited"))

    def city(self, cid):
        return self.cm.by_id.get(cid)

    def say(self, text):
        self.news.append([round(self.world.time, 1), text])
        del self.news[:-30]
        self.world.message(text)
        L.info(text)

    def strength(self, city):
        """都市の力（残っている建物の数）×防御。"""
        s = self.st(city)
        base = float(city.alive())
        if s["defense"] > 0 and s["defense_until"] > self.world.time:
            base *= 1.0 + 0.6 * s["defense"]
        return base

    # --- 記録 ---------------------------------------------------------------
    def record_trade(self, city, value):
        if not self.active:
            return
        s = self.st(city)
        s["heat"] = float(s["heat"]) + abs(float(value))
        s["last_trade"] = self.world.time
        s["visited"] = True

    def mark_visit(self, pos):
        for c in self.cm.cities:
            if getattr(c, "fallen", False):
                continue
            if np.linalg.norm(pos - c.x) < c.radius + VISIT_MARGIN:
                s = self.st(c)
                if not s["visited"]:
                    s["visited"] = True
                    s["last_trade"] = self.world.time
                    self.world.message(f"{c.name} を訪れました（K でマップに記録）")

    # --- 時間の経過 ---------------------------------------------------------
    def update(self, dt):
        if not self.active:
            return
        self._acc += dt
        if self._acc < 1.0:
            return
        step, self._acc = self._acc, 0.0
        t = self.world.time
        self.mark_visit(self.world.player.pos)
        for cid, s in list(self.cities.items()):
            c = self.city(cid)
            if c is None or s.get("fallen") or not s.get("visited"):
                continue
            s["heat"] = float(s["heat"]) * np.exp(-step / HEAT_TAU)
            since = t - float(s["last_trade"] if s["last_trade"] is not None else t)
            d = 0.0
            motive = None
            if s["heat"] > HEAT_LIMIT:
                d += min((s["heat"] - HEAT_LIMIT) * HEAT_RATE, MAX_RATE) * step
                motive = "ambition"
            if since > NEGLECT_AFTER:
                d += NEGLECT_RATE * step
                motive = motive or "resentment"
            if d == 0.0:
                d = -(CALM_AFTER_TRADE if since < 5 * MIN else CALM_RATE) * step
            s["tension"] = float(np.clip(s["tension"] + d, 0.0, 100.0))
            if motive:
                s["motive"] = motive
            if s["defense"] and s["defense_until"] <= t:
                s["defense"] = 0
            plan = s.get("plan")
            if plan is None and s["tension"] >= PLAN_AT and t >= s["cooldown"]:
                self._make_plan(c, s)
            elif plan is not None:
                if s["tension"] < PLAN_AT - 15:
                    s["plan"] = None
                    self.say(f"{c.name} は {self._name(plan['target'])} への{ACTION[plan['type']]}をやめました")
                elif t >= plan["eta"]:
                    self._execute(c, s)

    def _name(self, cid):
        c = self.city(cid)
        return c.name if c else "？"

    def candidates(self, city, k=6):
        out = []
        for c in self.cm.cities:
            if c is city or getattr(c, "fallen", False) or not c.alive():
                continue
            if (self.cities.get(c.id) or {}).get("owner") == city.id:
                continue                         # もう自分の支配下
            out.append((float(np.linalg.norm(c.x - city.x)), c))
        out.sort(key=lambda x: x[0])
        return [c for _, c in out[:k]]

    def _make_plan(self, c, s):
        cands = self.candidates(c)
        if not cands:
            return
        typ = "conquer" if s.get("motive") == "ambition" else "attack"
        target = cands[0]
        s["plan"] = {"type": typ, "target": target.id, "eta": self.world.time + PLAN_ETA}
        self.say(f"⚠ {c.name} が {target.name} への{ACTION[typ]}を計画しています（約 {PLAN_ETA / MIN:.0f} 分後）")

    def _execute(self, c, s):
        plan = s["plan"]
        s["plan"] = None
        s["cooldown"] = self.world.time + COOLDOWN
        tgt = self.city(plan["target"])
        if tgt is None or getattr(tgt, "fallen", False):
            return
        ts = self.st(tgt)
        # 攻める側は緊張の高さのぶん勢いがある。守る側はプレイヤーが促した防御で持ちこたえやすくなる
        atk = float(c.alive()) * (1.0 + s["tension"] / 100.0)
        if atk <= self.strength(tgt):
            s["tension"] = max(0.0, s["tension"] - 50)
            self.say(f"{tgt.name} が {c.name} の{ACTION[plan['type']]}を退けました")
            self._damage(tgt, 0.15)
            return
        s["tension"] = 20.0
        if plan["type"] == "conquer":
            ts["owner"] = c.id
            ts["region"] = c.region
            ts["tension"] = 0.0
            ts["plan"] = None
            tgt.region = c.region
            tgt._prices = None
            self._damage(tgt, 0.1)
            self.say(f"{c.name} が {tgt.name} を征服しました（取引の傾向が {c.name} と同じになりました）")
        else:
            self._fall(tgt)
            self.say(f"{c.name} の攻撃で {tgt.name} が滅びました")

    def _damage(self, city, frac):
        rng = np.random.default_rng(int(self.world.time * 1000) % (2 ** 31))
        for b in range(len(city.state)):
            if b == getattr(city, "core", None):
                continue
            if city.state[b] == 0 and rng.random() < frac:
                city.state[b] = 1
        city.version += 1

    def _fall(self, city):
        rng = np.random.default_rng(abs(hash(city.id)) % (2 ** 31))
        for b in range(len(city.state)):
            if b == getattr(city, "core", None):
                city.state[b] = 1
                continue
            city.state[b] = 2 if rng.random() < 0.75 else 1
        city.version += 1
        city.fallen = True
        s = self.st(city)
        s.update(fallen=True, plan=None, tension=0.0)
        for o in self.cities.values():
            if o.get("plan") and o["plan"]["target"] == city.id:
                o["plan"] = None
        self.world.economy.city_fallen(city)

    # --- プレイヤーの助言 -----------------------------------------------------
    def _advice_check(self, city):
        if not self.active:
            return "助言はサバイバルだけです"
        s = self.st(city)
        if not s["visited"]:
            return "訪れたことのない都市には助言できません"
        if getattr(city, "fallen", False):
            return "この都市は滅びています"
        wait = s["advice_at"] + ADVICE_WAIT - self.world.time
        if wait > 0:
            return f"{city.name} への助言は {wait / MIN:.0f} 分後にできます"
        if self.world.economy.credits() < ADVICE_FEE:
            return f"通信費 {ADVICE_FEE:.0f} クレジットが足りません"
        return None

    def _pay(self, city):
        self.world.economy._add_credits(-ADVICE_FEE)
        self.st(city)["advice_at"] = self.world.time

    def persuade(self, city):
        err = self._advice_check(city)
        if err:
            return err
        self._pay(city)
        s = self.st(city)
        s["tension"] = max(0.0, s["tension"] - PERSUADE)
        s["heat"] = float(s["heat"]) * 0.5
        msg = f"{city.name} を説得しました（緊張 {s['tension']:.0f}）"
        if s.get("plan") and s["tension"] < PLAN_AT - 15:
            plan = s["plan"]
            s["plan"] = None
            msg += f"。{self._name(plan['target'])} への{ACTION[plan['type']]}をやめました"
        return msg

    def urge_defense(self, city):
        err = self._advice_check(city)
        if err:
            return err
        self._pay(city)
        s = self.st(city)
        s["defense"] = min(3, int(s["defense"] if s["defense_until"] > self.world.time else 0) + 1)
        s["defense_until"] = self.world.time + DEFENSE_TIME
        return f"{city.name} に防御を促しました（防御 {s['defense']}・{DEFENSE_TIME / MIN:.0f} 分）"

    def retarget(self, city, target):
        err = self._advice_check(city)
        if err:
            return err
        s = self.st(city)
        if not s.get("plan"):
            return f"{city.name} は今、何も計画していません"
        if target is city or getattr(target, "fallen", False):
            return "その都市は目標にできません"
        self._pay(city)
        s["plan"]["target"] = target.id
        return f"{city.name} の{ACTION[s['plan']['type']]}の目標を {target.name} に変えさせました"
