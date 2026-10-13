"""logs/ にエラーや読み込み可否を書き出すロガー。"""
import logging
import time

from . import paths

_configured = False


def setup(level: str = "INFO", keep: int = 10) -> logging.Logger:
    global _configured
    logger = logging.getLogger("alpedia")
    if _configured:
        return logger
    paths.LOGS.mkdir(parents=True, exist_ok=True)
    # 古いログを整理
    old = sorted(paths.LOGS.glob("alpedia-*.log"))
    for f in old[:-max(keep - 1, 0)] if len(old) >= keep else []:
        try:
            f.unlink()
        except OSError:
            pass
    stamp = time.strftime("%Y%m%d-%H%M%S")
    fmt = logging.Formatter("%(asctime)s [%(levelname)s] %(name)s: %(message)s", "%H:%M:%S")
    logger.setLevel(getattr(logging, level.upper(), logging.INFO))
    for path in (paths.LOGS / "latest.log", paths.LOGS / f"alpedia-{stamp}.log"):
        h = logging.FileHandler(path, mode="w", encoding="utf-8")
        h.setFormatter(fmt)
        logger.addHandler(h)
    sh = logging.StreamHandler()
    sh.setFormatter(fmt)
    logger.addHandler(sh)
    _configured = True
    return logger


def get(name: str = "") -> logging.Logger:
    return logging.getLogger("alpedia" + ("." + name if name else ""))
