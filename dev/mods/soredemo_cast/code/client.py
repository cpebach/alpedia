"""登場人物（画面側）：人物を VRM の体（skinmodel の VRMBody）で描く。近づくと「話す」ボタン。"""
import numpy as np

from main.client.addon_hooks import ClientAddon

from .poses import NAMES, POSES

SHOW_DIST = 400.0


class CastClient(ClientAddon):
    name = "cast"

    def __init__(self, game):
        super().__init__(game)
        self.pack = game.world.addon("cast")
        self.bodies = {}                 # key → (定義の ID, 体)
        self.skin = None

    def emotes(self):
        # 人物のポーズは "（人物）…" の名前で game.emotes に入る（カメラモードでも選べる）
        return {f"（人物）{NAMES[k]}": fn for k, fn in POSES.items()}

    def _skin(self):
        if self.skin is None:
            self.skin = next((a for a in self.game.addons if getattr(a, "name", "") == "skinmodel"), None)
        return self.skin

    def _body(self, n):
        cur = self.bodies.get(n.key)
        if cur is not None and cur[0] == n.defn["id"]:
            return cur[1]
        if cur is not None:
            cur[1].destroy()
        skin = self._skin()
        mm = skin.models.get(n.defn.get("model")) if skin is not None else None
        if mm is None:                          # VRM が無い・読めない：箱の体で出す（話しかけられるように）
            from main.client.game import BoxBody
            b = BoxBody(self.game, n)
            self.bodies[n.key] = (n.defn["id"], b)
            return b
        from importlib import import_module
        view = import_module(type(skin).__module__.rsplit(".", 1)[0] + ".view")
        b = view.VRMBody(self.game, n, mm)
        self.bodies[n.key] = (n.defn["id"], b)
        return b

    def update(self, origin, far):
        if self.pack is None:
            return
        import time as _t
        dt = min(float(self.game.U["utime"].dt), 0.1)
        alive = set()
        for key, n in self.pack.npcs.items():
            alive.add(key)
            b = self._body(n)
            if b is None:
                continue
            n.emote = n.emote if n.emote in POSES else None
            if n.emote is not None:
                n.emote_t0 = getattr(n, "emote_t0", 0.0) or _t.monotonic()
                self.game.emotes.setdefault(f"__npc_{n.emote}", POSES[n.emote])
            shown = float(np.linalg.norm(n.pos - origin)) < SHOW_DIST
            real = n.emote
            n.emote = f"__npc_{real}" if real else None
            try:
                b.update(origin, shown, dt)
            finally:
                n.emote = real
        for key in [k for k in self.bodies if k not in alive]:
            self.bodies.pop(key)[1].destroy()

    def aim(self, hit):
        if not hit or hit.get("kind") != "npc" or self.pack is None:
            return None
        n = self.pack.npcs.get(hit.get("key"))
        return (f"{n.label}（E で話す）" if n is not None else "", False)

    def interact(self, hit):
        if not hit or hit.get("kind") != "npc":
            return False
        msg = self.world.addon_call("cast", "talk", hit.get("key"))
        if msg:
            self.world.message(msg)
        return True

    def nearby(self, pos, radius):
        if self.pack is None:
            return []
        out = []
        for d, n in self.pack.near(pos, max(radius, 3.0)):
            if n.talkable:
                out.append({"label": f"{n.label}と話す", "hit": {"kind": "npc", "key": n.key}, "kind": "e",
                            "dist": d, "icon": "interact"})
        return out[:2]

    def destroy(self):
        for _, b in self.bodies.values():
            b.destroy()
        self.bodies = {}
