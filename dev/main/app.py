"""起動処理。"""
import sys
import time

import numpy as np

from . import paths, log, config


def create_empty_world(name):
    from .tutorial import create_world
    return create_world(name, [])


def prepare_world(name, new=False):
    wdir = paths.world_dir(name)
    if not wdir.exists():
        from .tutorial import SCENARIO_WORLD, create_tutorial
        if name in ("tutorial", SCENARIO_WORLD) and not new:
            create_tutorial(name)
        else:
            create_empty_world(name)
    from .world import World
    return World


def _check_core():
    """物理コア（lib/alpedia_core）があるか。無ければ作り方を出して終わる。"""
    from . import native
    if native.load() is None:
        raise SystemExit(native.reason)


def _rust_view_ok(L):
    """画面（lib/alpedia_view）が使えるか。"""
    from . import native
    if str(native.LIB) not in sys.path:
        sys.path.insert(0, str(native.LIB))
    try:
        import alpedia_view  # noqa: F401
        return True
    except ImportError as e:
        L.error("画面（lib/alpedia_view）がありません（%s）。Rust を入れて native\\build_windows.bat で作ってください", e)
        return False


def main(args):
    paths.ensure_dirs()
    cfg = config.load()
    L = log.setup(cfg.get("log_level", "INFO"))
    from . import winperf
    winperf.boost(cfg)
    _check_core()
    if not args.headless:
        if not _rust_view_ok(L):
            raise SystemExit("画面（lib/alpedia_view）がありません。native\\build_windows.bat で作ってください")
        L.info("Alpedia 起動")
        from .client_rs.client import run_client
        if args.world:
            prepare_world(args.world, getattr(args, "new", False))
        elif not any((d / "world.json").exists() for d in paths.SAVES.iterdir() if d.is_dir()):
            from .tutorial import SCENARIO_WORLD, create_tutorial  # 初回はシナリオのワールドを用意
            create_tutorial(SCENARIO_WORLD)
        run_client(cfg, args.world)
        return
    name = args.world or cfg["game"].get("default_world", "scenario")
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
