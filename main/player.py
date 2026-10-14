"""プレイヤー（描画とは独立した物理・所持品）。"""
import numpy as np


class Player:
    def __init__(self, pdef, pos):
        self.defn = pdef
        self.pos = np.asarray(pos, dtype=np.float64)   # 足元の球の中心
        self.vel = np.zeros(3)
        self.up = np.array([0.0, 1.0, 0.0])
        self.on_ground = False
        self.ground_vel = np.zeros(3)
        self.seated = None          # (construct_id, cell)
        self.inventory = {}
        self.hotbar = None          # アイテムバー（9枠）: [["material"|"device", id] or None]
        self.jetpack = False
        self.mass = 80.0
        self.in_water = 0.0
        self.starter_given = False   # サバイバルの最初の資材を受け取ったか
        self.credits_given = False   # 最初のクレジットを受け取ったか
        self.out_of_fuel = False

    @property
    def radius(self):
        return float(self.defn["radius"])

    @property
    def height(self):
        return float(self.defn["height"])

    def eye(self):
        return self.pos + self.up * (self.height - self.radius - 0.15)

    def to_dict(self):
        return {"position": self.pos.tolist(), "velocity": self.vel.tolist(), "inventory": self.inventory,
                "seated": [self.seated[0], list(self.seated[1])] if self.seated else None,
                "jetpack": self.jetpack, "def": self.defn["id"], "starter_given": self.starter_given, "credits_given": self.credits_given,
                "hotbar": [list(x) if x else None for x in self.hotbar] if self.hotbar else None}

    def load_dict(self, d):
        self.pos = np.asarray(d.get("position", self.pos), dtype=np.float64)
        self.vel = np.asarray(d.get("velocity", [0, 0, 0]), dtype=np.float64)
        self.inventory = dict(d.get("inventory", {}))
        s = d.get("seated")
        self.seated = (s[0], tuple(s[1])) if s else None
        self.jetpack = bool(d.get("jetpack", False))
        self.starter_given = bool(d.get("starter_given", False))
        self.credits_given = bool(d.get("credits_given", False))
        hb = d.get("hotbar")
        self.hotbar = [tuple(x) if x else None for x in hb] if hb else None

    def update(self, world, dt, move, jump, thrust_dir):
        """move: 地表に沿った希望速度（ワールド座標）、thrust_dir: ジェットパック方向。"""
        pl = world.nearest_planet(self.pos)
        if pl is not None:
            target_up = pl.up(self.pos)
            self.up = self.up + (target_up - self.up) * min(1.0, dt * 5)
            self.up /= np.linalg.norm(self.up)
        if self.seated:
            c = world.constructs.get(self.seated[0])
            i = c.index.get(self.seated[1]) if c else None
            if i is None:
                self.seated = None
            else:
                ctr = c.pos[8 * i:8 * i + 8].mean(0)
                fr = c.voxel_frames()[i]
                self.pos = ctr + fr[1] * (c.size * 0.5 + self.radius)
                self.vel = c.vel[8 * i:8 * i + 8].mean(0)
                self.up = fr[1]
                return
        # ジェットパック（燃料などの決まりは MOD が決める。サバイバルでは手持ちの燃料が要る）
        flying, thrust_dir = world.jetpack(self, dt, thrust_dir)
        g = world.gravity(self.pos[None])[0]
        self.vel += g * dt
        # 水中：浮力と抵抗、泳ぐ（Space で上、Ctrl で下、WASD で前後左右）
        self.in_water = 0.0
        if pl is not None and pl.has_water:
            wd = float(pl.water_depth(self.pos[None])[0])
            frac = float(np.clip((wd + self.radius) / (self.height * 0.8), 0.0, 1.0))
            self.in_water = frac
            if frac > 0:
                self.vel -= g * dt * frac * 0.97
                self.vel *= max(0.0, 1.0 - dt * 2.5 * frac)
                if not flying:
                    vert = float(np.dot(thrust_dir, self.up))
                    self.vel += self.up * vert * 9.0 * dt * min(1.0, frac * 1.5)
                    if not self.on_ground:
                        self.vel += move * 6.0 * dt * frac
        if flying and np.linalg.norm(thrust_dir) > 0:
            self.vel += thrust_dir * float(self.defn["jetpack_accel"]) * dt
        if self.on_ground:
            rel = self.vel - self.ground_vel
            rel_t = rel - self.up * np.dot(rel, self.up)
            want = move * float(self.defn["walk_speed"])
            change = want - rel_t
            k = min(1.0, dt * 12)
            self.vel += change * k
            if jump:
                self.vel += self.up * float(self.defn["jump_speed"])
        elif not flying and np.linalg.norm(move) > 0:
            self.vel += move * 2.0 * dt  # 空中での僅かな制御
        self.pos = self.pos + self.vel * dt
        self.on_ground = False
        self.ground_vel = np.zeros(3)
        r = self.radius
        # 惑星
        for p in world.planets:
            d, n = p.sdf_grad(self.pos[None])
            d, n = float(d[0]), n[0]
            if d < r:
                self.pos += n * (r - d)
                vn = np.dot(self.vel, n)
                if vn < 0:
                    self.vel -= n * vn
                if np.dot(n, self.up) > 0.5:
                    self.on_ground = True
                    self.vel *= max(0.0, 1 - dt * 2)
        # 拡張の物体（街の建物など。動いているステーションの上では、足元の建物と一緒に動く）
        if getattr(world, "addons", None):
            was_ground = getattr(self, "_ext_ground", False)
            blocked = False
            self._ext_ground = False
            for off in (0.0, self.height - 2 * r):
                center = self.pos + self.up * off
                for push, n, body in world.push_sphere(center, r):
                    self.pos += push
                    vb = body.point_velocity(self.pos) if body.is_moving() else np.zeros(3)
                    vn = np.dot(self.vel - vb, n)
                    if vn < 0:
                        self.vel -= n * vn
                    # 段の角に乗ったとき（法線が斜め）も足場とみなす（角の上で止まって動けなくならないように）
                    if off == 0.0 and np.dot(n, self.up) > 0.3:
                        self.on_ground = True
                        self._ext_ground = True
                        self.ground_vel = vb
                    if off == 0.0 and abs(np.dot(n, self.up)) < 0.6 and np.dot(n, move) < -0.3:
                        blocked = True
            # 低い段差（STEP_UP まで）は歩くだけで上れる（縁石・舗装の段差・階段を1段ずつ上らなくてよい）
            if blocked and (was_ground or self.on_ground or self.in_water > 0) and np.linalg.norm(move) > 0:
                mv = move / np.linalg.norm(move)
                for h in (0.15, 0.3, 0.45, 0.6, 0.72):
                    probe = self.pos + self.up * h + mv * 0.12
                    clear = True
                    for off in (0.0, self.height - 2 * r):
                        for push, n, body in world.push_sphere(probe + self.up * off, r * 0.98):
                            if np.dot(n, self.up) < 0.5 or off > 0:
                                clear = False
                                break
                        if not clear:
                            break
                    if clear:
                        # 少し前にも出して、段の角に乗ったまま止まらないようにする
                        self.pos = self.pos + self.up * h + mv * 0.08
                        self.on_ground = True
                        vu = np.dot(self.vel, self.up)
                        if vu < 0:
                            self.vel -= self.up * vu
                        break
        # ボクセル（足元と頭の2球）
        for off in (0.0, self.height - 2 * r):
            center = self.pos + self.up * off
            for c, i, push, n in world.voxel_contacts(center, r):
                self.pos += push
                vv = c.vel[8 * i:8 * i + 8].mean(0)
                rel = self.vel - vv
                vn = np.dot(rel, n)
                if vn < 0:
                    self.vel -= n * vn
                if off == 0.0 and np.dot(n, self.up) > 0.5:
                    self.on_ground = True
                    self.ground_vel = vv
                    # 体重を構築物に伝える
                    c.force[8 * i:8 * i + 8] += -n * self.mass * np.linalg.norm(world.gravity(center[None])[0]) / 8
