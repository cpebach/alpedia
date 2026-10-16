"""起動処理。"""
import json
import time

import numpy as np

from . import paths, log, config, content


def create_empty_world(name):
    wdir = paths.world_dir(name)
    for sub in ("scripts", "exports", "imports"):
        (wdir / sub).mkdir(parents=True, exist_ok=True)
    from .world import DEFAULT_SETTINGS, default_bodies
    from .savefmt import FORMAT
    s = dict(DEFAULT_SETTINGS)
    s["name"] = name
    s["format"] = FORMAT
    s["planets"] = default_bodies()
    (wdir / "world.json").write_text(json.dumps(s, ensure_ascii=False, indent=1), encoding="utf-8")
    (wdir / "constructs.json").write_text("[]", encoding="utf-8")
    (wdir / "spawn.json").write_text("{}", encoding="utf-8")
    (wdir / "mods.json").write_text(json.dumps({"enabled": []}), encoding="utf-8")
    (wdir / "scripts" / "README.txt").write_text(content.SCRIPT_README, encoding="utf-8")
    return wdir


def prepare_world(name, new=False):
    wdir = paths.world_dir(name)
    if not wdir.exists():
        if name == "tutorial" and not new:
            from .tutorial import create_tutorial
            create_tutorial(name)
        else:
            create_empty_world(name)
    from .world import World
    return World


def _start_warmup(L):
    """物理演算の高速化（numba）のコンパイルを裏で始めておく（初回だけ数十秒かかる。2回目からは保存済みを使う）。"""
    import threading
    from . import fastphys

    def run():
        t = time.time()
        try:
            fastphys.warmup()
            L.info("物理演算の高速化を準備しました（%.1f 秒）", time.time() - t)
        except Exception:  # noqa: BLE001
            L.exception("物理演算の高速化の準備に失敗しました")
    threading.Thread(target=run, daemon=True, name="numba-warmup").start()


def main(args):
    paths.ensure_dirs()
    cfg = config.load()
    L = log.setup(cfg.get("log_level", "INFO"))
    _start_warmup(L)
    if not args.headless:
        L.info("Alpedia 起動")
        from .client.menu import run_client
        if args.world:
            prepare_world(args.world, getattr(args, "new", False))
        elif not any((d / "world.json").exists() for d in paths.SAVES.iterdir() if d.is_dir()):
            from .tutorial import create_tutorial  # 初回はチュートリアルを用意
            create_tutorial("tutorial")
        run_client(cfg, args.world)
        return
    name = args.world or cfg["game"].get("default_world", "tutorial")
    L.info("Alpedia 起動: ワールド '%s'（ヘッドレス）", name)
    try:
        World = prepare_world(name, getattr(args, "new", False))
        world = World(name, cfg).load()
    except Exception:
        L.exception("ワールドの読み込みに失敗しました")
        raise
    run_headless(world, cfg, args.steps)


def run_headless(world, cfg, steps):
    L = log.get("headless")
    dt = 1.0 / float(cfg["physics"].get("tick_rate", 60))
    t0 = time.time()
    for k in range(steps):
        world.step(dt)
        if k % 120 == 0:
            L.info("t=%.1fs 構築物 %d 頂点 %d", world.time, len(world.constructs),
                   sum(c.n_vert for c in world.constructs.values()))
    el = time.time() - t0
    L.info("%d ステップ %.2f 秒（1ステップ %.1f ms）", steps, el, el / max(steps, 1) * 1000)
    for c in world.constructs.values():
        L.info("  %s: ボクセル %d, 速度 %.2f m/s, 位置 %s", c.name, c.n_vox,
               float(np.linalg.norm(c.velocity())), np.round(c.center_of_mass(), 1).tolist())
