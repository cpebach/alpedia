# MOD の作り方

`mods/<MOD名>/` にフォルダを作り、次の構成でファイルを置きます。

```
mods/<MOD名>/
  mod.json            {"id", "name", "version", "description"}
  materials/*.json    素材
  devices/*.json      機器
  planets/*.json      惑星
  players/*.json      プレイヤーの見た目・性能
  resources/*.json    資源
  textures/*.png      テクスチャ（定義から相対パスで参照）
  models/*.obj        プレイヤーモデルなど
  code/__init__.py    プログラム（任意。下の「プログラムを持つ MOD」）
```

ワールドで使うには、メニューの新規作成／ワールド設定の MOD 選択で選びます（`saves/<ワールド>/mods.json` の `"enabled"`）。
本体に入っている MOD：`survival`（サバイバル＝資源のルール。入れないとクリエイティブ）、`city`（サイバーパンクの街）。
読み込み結果やエラーは `logs/latest.log` に出ます。

## 読み込みのタイミングとエラー（大事）

- MOD は **ワールドを始めるときに一括で** 読み込みます。定義（JSON）・テクスチャ・プログラム（`code/` の中の .py は使っていないものも全部）をこのとき読みます。
  画面用のプログラム（ursina などを使うもの）は、画面を作るとき（同じくプレイが始まる前）に読みます。
- エラーがあれば、このときにまとめて一覧を出し、**ワールドは始めません**（セーブは変わりません）。
  エラーにはファイル名と行が付きます。全文は `logs/latest.log` にあります。
- **プレイ中に MOD のファイルを読みに行くことはありません。** プレイ中に MOD を書き換えても反映されません。
  ワールドを抜けて、もう一度始めると読み直します。
- ワールドが使っている MOD を外す（または素材・機器の ID を変える）と、そのワールドは始められなくなります
  （どの構築物のどの素材・機器が足りないかを一覧にします）。
- MOD はこの自由度と引き換えのリスクがあるものです。**新しい MOD を作るとき・ワールドに MOD を入れる／外すときは、
  先にワールドのフォルダ（`saves/<ワールド名>/`）をコピーしてバックアップを取ってください。**
- ゲームの物理の中心は Rust ですが、MOD のプログラムとブロックプログラムは今までどおり Python で書けます。

## 書き方

各 JSON は定義1つ（オブジェクト）か、定義の配列です。`"extends"` に既存の ID を書くと、
その定義を引き継いで書いた項目だけ上書きした「派生」になります。

```json
{"id": "titanium", "extends": "steel", "name": "チタン", "density": 4500, "break_distance": 0.2}
```

既存 ID を丸ごと置き換えたい場合は `"override": true` を付けます。

## 素材 (materials)

| 項目 | 意味 |
|---|---|
| density | 密度 kg/m³ |
| stiffness | 0〜1。ボクセルが形を保とうとする強さ（硬度） |
| joint_stiffness | 0〜1。隣のボクセルとの頂点結合の強さ（動きにくさ） |
| break_distance | 結合した頂点がこの距離（ボクセルサイズ比）以上離れるとちぎれる |
| yield_strain / plasticity | 変形がこれを超えると塑性変形（曲がったまま戻らない）。plasticity は 0〜1 |
| damping | 速度減衰 |
| friction | 地面との摩擦 0〜1 |
| rebond / rebond_distance | 離れた頂点が近づくと再結合する（液体・粘土向け） |
| color / texture | 見た目 |
| cost | サバイバル時の設置コスト {資源ID: 量} |

液体は「stiffness と joint_stiffness が低い・break_distance が大きい・rebond が true」、
硬くて脆い素材は「stiffness が高い・break_distance が小さい」で表現します。

## 機器 (devices)

`type` は次のいずれか。`params` は type ごとに異なります（`sauce/base/devices/base.json` 参照）。

thruster（推進機）, tank, pipe, valve, battery, generator, crank（手回し発電機）, computer,
actuator, processor（精錬機・加工機。recipes で入出力を定義）, seat, air_vent

共通: `material`（外装素材）, `power`（none / manual / electric）, `power_use`（W）,
`pipe_port`（配管に繋がるか）, `directional`（向きを持つか）, `cost`

## 惑星 (planets)

radius, surface_gravity, noise_amp, noise_freq, seed, color, texture, dig_resource, atmosphere_pressure

ワールドへの配置は `saves/<ワールド>/world.json` の `"planets"` に
`{"def": "<惑星ID>", "position": [x, y, z]}` を追加します。

## プレイヤー (players)

color, model（.obj）, height, radius, walk_speed, jump_speed, jetpack_accel, reach。
ゲーム中の Esc（一時停止）の「見た目 ◀ ▶」で、ワールドに入っている MOD の見た目から選べます（プレイヤーごと。`player.json` の `"def"`）。
VRM のモデル（.vrm）を使うときは `skinmodel(dummy)` MOD（VRM の見た目）を参照してください。

## プログラムを持つ MOD

`code/__init__.py` に次の関数を書くと、ワールドを読み込むときにプログラムが動きます（例は `mods/survival`, `mods/city`）。

```python
def world_addon():          # ワールド側（必須）
    from .rules import MyRules
    return MyRules()        # main.hooks.Addon を継承したもの

def client_addon(game):     # 画面側（任意）：main.client.addon_hooks.ClientAddon を継承したもの
    from .client import MyClient
    return MyClient(game)
```

- `code/` は `alpedia_mods.<MOD名>` というパッケージとして読み込まれます。中では `from . import xxx`、本体は `from main.hooks import Addon` のように import します。
- `Addon` の口（必要なものだけ上書き）：
  - 読み込み・保存：`on_load` / `on_save`（`settings_keys` に world.json に書く項目名）
  - 物理：`before_physics`（物理で動かさない構築物の ID を返す）/ `colliders`（当たり判定の箱）/ `after_physics`
  - 視線・プレイヤー・操作：`raycast` / `push_sphere` / `interact`（機器以外）/ `interact_device`（機器。コアより先）/ `on_placed`
  - 資源のルール：`resource_rules = True` にするとクリエイティブでなくなる。`pay` / `refund` / `on_device_placed` / `on_dig` / `on_water` / `jetpack`
- 画面側 `ClientAddon` の口：`on_key` / `interact` / `aim`（視線の先の説明）/ `update`（毎フレームの描画）/ `destroy` と `help_text`、
  `player_body(player)`（プレイヤーの体の見た目を引き受けるとき。自分にもマルチプレイのほかの人にも呼ばれる。例は `skinmodel(dummy)`）。

## マルチプレイ（LAN）で動くようにする

ホストの PC が世界を動かし、参加者の PC は結果を受け取って描きます。MOD（プログラムも）は参加する人に自動で届きます。

- 参加者の PC では、ティックごとの口（`before_physics` / `after_physics` など）は呼ばれません。`on_load` は呼ばれます
  （ホストから受け取ったセーブで、同じ物ができるように作ってください）。
- `world.player` は「いま操作している人」です。参加者の操作をホストで行う間はその人になるので、
  支払い（`pay`）・持ち物・メッセージはそのまま参加者ごとになります。
- `Addon` の口：
  - `on_player_join(world, player)`：参加者が入った（最初の資材を渡すなど。サバイバル・街が使っています）
  - `net_state(world)` → `net_apply(world, state)`：ホストの状態を参加者へ（変わったときだけ、1秒に10回まで）。動く物・壊れ具合・経済など
  - `net_client_tick(world, dt)`：参加者の PC で毎フレーム。次の状態が届くまで動く物を進めておく
  - `net_ref_of(obj)` / `net_ref(world, kind, id)`：下の `addon_call` の引数（街・部屋など）を ID でやり取りする
- 画面から MOD の状態を変える操作（売り買いなど）は `world.addon_call("MOD の名前", "economy.buy", 引数…)` で呼びます。
  参加者の PC でもすぐに行い（画面にすぐ出すため）、ホストでも行います（ホストの結果が正しく、`net_state` で揃います）。
  例は `mods/city/code/ui.py`。
- `mod.json` の `"builtin": true` は本体に入っている MOD の印です（パッケージを作り直すと上書きされます）。

