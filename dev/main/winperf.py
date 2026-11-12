"""Windows で速く動かす設定（起動時に1回）。

ノート PC（とくに Intel Core Ultra などの、速いコアと省電力のコアが混ざった CPU）では、Windows が
「省電力のために速さを抑えてよいプロセス」と判断すると、ゲームが省電力のコア・低いクロックで動き、
CPU もグラフィックもほとんど使っていないのに重い、ということが起きる。それを止める：

- 電力の調整（EcoQoS：実行速度を抑える・タイマーの精度を落とす）をこのプロセスでは切る
- 優先度を「通常以上」にする
- タイマーの精度を 1 ms にする（待ち時間が 15.6 ms 単位に丸められないように）

Windows 以外・古い Windows では何もしない（失敗しても続ける）。
"""
import sys

from . import log

L = log.get("winperf")


def boost(cfg=None):
    if sys.platform != "win32":
        return []
    if cfg is not None and not cfg.get("performance", {}).get("windows_boost", True):
        return []
    import ctypes
    from ctypes import wintypes
    done = []
    k32 = ctypes.WinDLL("kernel32", use_last_error=True)
    proc = k32.GetCurrentProcess()

    class PPTS(ctypes.Structure):           # PROCESS_POWER_THROTTLING_STATE
        _fields_ = [("Version", wintypes.ULONG), ("ControlMask", wintypes.ULONG), ("StateMask", wintypes.ULONG)]
    try:
        # ControlMask：実行速度（1）とタイマーの精度（4）を自分で決める。StateMask 0 ＝ どちらも抑えない
        st = PPTS(1, 0x1 | 0x4, 0)
        fn = k32.SetProcessInformation
        fn.argtypes = [wintypes.HANDLE, ctypes.c_int, ctypes.c_void_p, wintypes.DWORD]
        fn.restype = wintypes.BOOL
        if fn(proc, 4, ctypes.byref(st), ctypes.sizeof(st)):          # 4 = ProcessPowerThrottling
            done.append("省電力の速度制限を解除")
    except Exception:  # noqa: BLE001
        pass
    try:
        if k32.SetPriorityClass(proc, 0x8000):                        # ABOVE_NORMAL_PRIORITY_CLASS
            done.append("優先度：通常以上")
    except Exception:  # noqa: BLE001
        pass
    try:
        if ctypes.WinDLL("winmm").timeBeginPeriod(1) == 0:
            done.append("タイマー 1 ms")
    except Exception:  # noqa: BLE001
        pass
    if done:
        L.info("Windows の設定: %s", "・".join(done))
    return done
