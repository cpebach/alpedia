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
```

ワールドで使うには `saves/<ワールド>/mods.json` の `"enabled"` に MOD 名を追加します。
読み込み結果やエラーは `logs/latest.log` に出ます。

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
`world.json` の `"player"` に ID を書くと使われます。
