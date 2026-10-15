"""揺れもの（髪・ベルトなど）。VRM のスプリングボーンの決まり（慣性・元の形に戻る力・重力・体との当たり）を、
骨の段ごとにまとめて numpy で計算する。ワールド座標で計算するので、走ったり向きを変えたりすると揺れる。

VRM 0.x（secondaryAnimation）と VRM 1.0（VRMC_springBone）の両方を読む。
"""
import numpy as np

from .vrm import _mat_to_quat, qmat


class Spring:
    def __init__(self, vrm):
        self.v = vrm
        joints = []         # (ノード, 尾の向き（ローカル）, 長さ, 硬さ, 抵抗, 重力の向き*強さ, 当たりの半径, 当たりの組)
        children = {}
        for i, p in enumerate(vrm.parent):
            if p >= 0:
                children.setdefault(int(p), []).append(i)
        ext = vrm.js.get("extensions", {})
        if vrm.version == 0:
            sa = vrm.ext.get("secondaryAnimation", {})
            cols = [[(cg["node"], c.get("offset", {}), float(c.get("radius", 0))) for c in cg.get("colliders", [])]
                    for cg in sa.get("colliderGroups", [])]
            for g in sa.get("boneGroups", []):
                gd = g.get("gravityDir", {"x": 0, "y": -1, "z": 0})
                grav = np.array([gd.get("x", 0), gd.get("y", -1), gd.get("z", 0)], float) * float(g.get("gravityPower", 0))
                prm = (float(g.get("stiffiness", g.get("stiffness", 1.0))), float(g.get("dragForce", 0.4)), grav,
                       float(g.get("hitRadius", 0.02)), [c for k in g.get("colliderGroups", []) if k < len(cols) for c in cols[k]])
                for root in g.get("bones", []):
                    stack = [root]
                    while stack:
                        n = stack.pop()
                        joints.append((n, prm))
                        stack += children.get(n, [])
        elif "VRMC_springBone" in ext:
            sb = ext["VRMC_springBone"]
            colliders = []
            for c in sb.get("colliders", []):
                sh = c.get("shape", {}).get("sphere")
                colliders.append((c["node"], sh.get("offset", [0, 0, 0]), float(sh.get("radius", 0))) if sh else None)
            cgs = [[colliders[i] for i in cg.get("colliders", []) if colliders[i]] for cg in sb.get("colliderGroups", [])]
            for s in sb.get("springs", []):
                cl = [c for k in s.get("colliderGroups", []) for c in cgs[k]]
                for j in s.get("joints", []):
                    grav = np.array(j.get("gravityDir", [0, -1, 0]), float) * float(j.get("gravityPower", 0))
                    joints.append((j["node"], (float(j.get("stiffness", 1.0)), float(j.get("dragForce", 0.5)), grav,
                                               float(j.get("hitRadius", 0.0)), cl)))
        # 重複を除き、親から順の段に分ける
        seen, uniq = set(), []
        for n, prm in joints:
            if n not in seen:
                seen.add(n)
                uniq.append((n, prm))
        self.n = len(uniq)
        if not self.n:
            return
        self.node = np.array([n for n, _ in uniq], np.int64)
        idx = {n: k for k, n in enumerate(self.node)}
        axis, length = np.zeros((self.n, 3)), np.zeros(self.n)
        for k, n in enumerate(self.node):
            ch = children.get(int(n), [])
            if ch:
                t = vrm.T[ch[0]] * vrm.S[ch[0]]
            else:                                   # 末端：自分の向きに 7cm 伸ばす（VRM と同じ）
                t = vrm.T[n]
                t = t / max(np.linalg.norm(t), 1e-9) * 0.07
            length[k] = np.linalg.norm(t)
            axis[k] = t / max(length[k], 1e-9)
        self.axis, self.length = axis, length
        self.stiff = np.array([p[0] for _, p in uniq])
        self.drag = np.array([p[1] for _, p in uniq])
        self.grav = np.array([p[2] for _, p in uniq])
        self.hit = np.array([p[3] for _, p in uniq])
        # 当たりの球（全部まとめる。組ごとの区別はしない）
        cs = {}
        for _, p in uniq:
            for node, off, r in p[4]:
                o = [off.get("x", 0), off.get("y", 0), off.get("z", 0)] if isinstance(off, dict) else list(off)
                cs[(node, tuple(o), r)] = 1
        self.col_node = np.array([c[0] for c in cs], np.int64)
        self.col_off = np.array([c[1] for c in cs], float).reshape(-1, 3)
        self.col_r = np.array([c[2] for c in cs], float)
        # 段（揺れる骨の親子の深さ）
        depth = np.zeros(self.n, np.int64)
        for k, n in enumerate(self.node):
            d, p = 0, vrm.parent[n]
            while p >= 0 and int(p) in idx:
                d, p = d + 1, vrm.parent[p]
            depth[k] = d
        self.levels = [np.nonzero(depth == d)[0] for d in range(int(depth.max()) + 1)]
        self.lv_nodes = [self.node[lv] for lv in self.levels]
        self.lv_par = [vrm.parent[self.node[lv]] for lv in self.levels]
        self.lv_rl = [qmat(vrm.R[self.node[lv]]) for lv in self.levels]     # 揺れる骨のローカルの初期の向き
        # 揺れる骨の下に揺れない骨が残っていれば、最後に全体の行列を計算し直す
        sset = set(int(n) for n in self.node)
        self.complete = all(all(c in sset for c in children.get(int(n), [])) for n in self.node)
        self.cur = self.prev = None

    def reset(self):
        self.cur = self.prev = None

    def step(self, G, T, R, A, t, dt):
        """G: 姿勢をつけた骨の行列（モデル座標）、T, R: ローカルの位置と回転、
        A, t: モデル → ワールドの変換（3×3 と位置）、dt: 秒。
        返り値: (G, R)  揺れを入れた骨の行列とローカルの回転（R は complete でないときだけ更新）"""
        if not self.n or dt <= 0:
            return G, R
        dt = min(dt, 1 / 20)
        Ainv = np.linalg.inv(A)
        scale = float(np.cbrt(abs(np.linalg.det(A))))
        G = G.copy()
        cc = None
        if len(self.col_r):
            cc = np.einsum("nij,nj->ni", G[self.col_node, :3, :3], self.col_off) + G[self.col_node, :3, 3]
            cc = cc @ A.T + t
            rr = self.col_r * scale
        first = self.cur is None
        if first:
            self.cur = np.zeros((self.n, 3))
            self.prev = np.zeros((self.n, 3))
        newR = {} if not self.complete else None
        for lv, nodes, par, Rl in zip(self.levels, self.lv_nodes, self.lv_par, self.lv_rl):
            Gp = G[par]
            Lp = Gp[:, :3, :3]
            head_m = (Lp @ T[nodes][:, :, None])[:, :, 0] + Gp[:, :3, 3]
            LR = Lp @ Rl
            rest_m = (LR @ self.axis[lv][:, :, None])[:, :, 0]
            head = head_m @ A.T + t
            rest_w = rest_m @ A.T
            rest_w /= _len(rest_w) + 1e-12
            ln = (self.length[lv] * scale)[:, None]
            if first:
                self.cur[lv] = head + rest_w * ln
                self.prev[lv] = self.cur[lv]
            cur, prev = self.cur[lv], self.prev[lv]
            far = _len(cur - head)[:, 0] > 4 * ln[:, 0] + 1.0       # 瞬間移動したら元の形から
            if far.any():
                cur[far] = head[far] + rest_w[far] * ln[far]
                prev[far] = cur[far]
            nxt = cur + (cur - prev) * (1 - self.drag[lv, None]) + rest_w * (self.stiff[lv, None] * dt) \
                + (self.grav[lv] @ A.T) * (dt / max(scale, 1e-9))
            d = nxt - head
            nxt = head + d * (ln / (_len(d) + 1e-12))
            if cc is not None:
                q = nxt[:, None, :] - cc[None, :, :]
                dist = np.sqrt(np.einsum("nkj,nkj->nk", q, q))
                lim = rr[None, :] + self.hit[lv, None] * scale
                hitm = dist < lim
                if hitm.any():
                    push = np.where(hitm, (lim - dist) / np.maximum(dist, 1e-9), 0.0)
                    nxt = nxt + np.einsum("nk,nkj->nj", push, q)
                    d = nxt - head
                    nxt = head + d * (ln / (_len(d) + 1e-12))
            self.prev[lv] = cur
            self.cur[lv] = nxt
            # 回転：初期の向き a → 新しい向き b（モデル座標）。ロドリゲスの式
            b = (nxt - head) @ Ainv.T
            b /= _len(b) + 1e-12
            a = rest_m / (_len(rest_m) + 1e-12)
            v = np.cross(a, b)
            c = np.einsum("ni,ni->n", a, b)
            K = np.zeros((len(nodes), 3, 3))
            K[:, 0, 1], K[:, 0, 2], K[:, 1, 2] = -v[:, 2], v[:, 1], -v[:, 0]
            K[:, 1, 0], K[:, 2, 0], K[:, 2, 1] = v[:, 2], -v[:, 1], v[:, 0]
            Q = np.eye(3) + K + (K @ K) / (1 + np.maximum(c, -0.999))[:, None, None]
            W = Q @ LR                                  # 骨の新しい向き（モデル座標、拡大縮小なし）
            G[nodes, :3, :3] = W * self.v.S[nodes][:, None, :]
            G[nodes, :3, 3] = head_m
            if newR is not None:
                Lpn = Lp / np.linalg.norm(Lp, axis=1, keepdims=True)
                newR[lv.tobytes()] = (nodes, _mat_to_quat(np.swapaxes(Lpn, 1, 2) @ W))
        if newR is not None:
            R = R.copy()
            for nodes, q in newR.values():
                R[nodes] = q
        return G, R


def _len(x):
    return np.sqrt((x * x).sum(1, keepdims=True))

