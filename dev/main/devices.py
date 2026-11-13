"""機器の動作：電力（配線不要・同一構築物で共有）、配管（連続したボクセルが必要）とバルブ、
アクチュエーターも配管と同じく燃料・物資を通す（結合している出力面の先へも。分離すると通らない）、
推進機（点火・燃焼・発電）・アクチュエータ・精錬/加工機（物資の受け渡し）・通気口・気密。
中身は Rust（game/devices.rs と、1ティックの計算は devices.rs の DevPlan）。

推進機：点火のときだけ電気を使い（ignite_energy）、燃えている間は燃料を使って推力と電気（power_output）を出す。
  タンクと直接（配管で）つながっていると、点火したら燃料が尽きるまで全開で燃える（固体ロケットのように止められない）。
  あいだにバルブを入れると、バルブの開き具合で燃料の量（＝推力）を変えられ、閉じると消える。
精錬機・加工機：原料は配管でつながった所（タンク・ほかの機械の作った物）から受け取り、作った物は自分の中にためる。
  タンクは、つながった機械が作った燃料などの流れる物を受け取る。バルブを閉じると、その先へは流れない。"""
from . import native

_C = native.require()
Networks = _C.Networks                     # 構築物ごとのキャッシュ：配管のつながり（区画とバルブ）・空洞
rebuild_networks = _C.dev_rebuild_networks
is_fluid = _C.dev_is_fluid
valve_opening = _C.dev_valve_opening
proc_outputs = _C.dev_proc_outputs         # 精錬機・加工機が出す物（作る物のうち、自分の原料ではないもの）
proc_inputs = _C.dev_proc_inputs
tanks_in = _C.dev_tanks_in
fluid_amount = _C.dev_fluid_amount
storage_total = _C.dev_storage_total       # 構築物の精錬機・加工機に入っている物の合計
world_axis = _C.dev_world_axis
cavity_report = _C.dev_cavity_report
interact = _C.dev_interact                 # E キーで機器を操作。メッセージを返す


def dev_def(c, i):
    d = c.devs[i]
    return c.reg.device(d["def"]) if d else None


def update(c, dt, input_keys=frozenset(), seated=False, ambient_kpa=0.0):
    """機器の1ティック。"""
    _C.dev_update(c, float(dt), input_keys, bool(seated), float(ambient_kpa))
