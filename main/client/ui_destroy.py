"""画面部品（ursina の Entity）を中身ごと消す。

ursina の destroy() は渡した部品だけを消し、子の部品（ボタン・文字など）は消さない。
消されなかった子は画面からは見えなくなるが ursina の部品一覧やマウスの当たり判定の一覧に残り続け、
毎フレームの処理が少しずつ重くなっていく（画面を開いて閉じるたびに数十個ずつ増える）。
そこで子から順にまとめて消す。ursina 本体には手を入れていない。
"""


def destroy(entity, *args, **kwargs):
    from ursina import destroy as _destroy
    if entity is None:
        return
    for ch in list(getattr(entity, "children", None) or []):
        destroy(ch)
    _destroy(entity, *args, **kwargs)
