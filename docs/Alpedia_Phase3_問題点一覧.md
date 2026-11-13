# Alpedia フェーズ3 問題点一覧

- 作成日：2026-11-14（[Alpedia_Phase3_Pre_Report.md](Alpedia_Phase3_Pre_Report.md) の「4. 現在の問題点」と同じ番号）
- 最終更新：2026-11-14

## この表の使い方

- 直したら、その行の **状態** を「済」にして、**直した日** と **直した内容**（コミットがあればその番号）を書く。行は消さない。
- 一覧に無かった物を直したときは、下の「一覧に無かった修正」に行を足す（番号は `X-01` から）。
- 新しく見つけた問題は、表の種類ごとの最後に行を足す（番号は続きから）。
- 状態：**未**＝手つかず／**作業中**／**済**＝直して確かめた／**見送り**＝直さないと決めた（理由を書く）
- 重要度：**高**＝配布・開発の継続に差し支える／**中**＝早めに直したい／**低**＝気づいたら直す／**—**＝仕様上の制限（直すかどうかから決める）

## 問題点

| 番号 | 種類 | 重要度 | 問題 | 根拠・場所 | 状態 | 直した日 | 直した内容 |
|---|---|---|---|---|---|---|---|
| G-01 | 管理 | 高 | Rust への書き直しを含む 69 コミットが GitHub に上がっていない | `git status`：`main...origin/main [ahead 69]` | 済 | 2026-11-14 | `origin/main` へ push（この表と報告書のコミットまで） |
| G-02 | 管理 | 高 | ビルドに要る修正（`ModLoadError` を abi3 で作れる形に）が未コミット。`alpedia_view/Cargo.lock` も | `dev/native/alpedia_core/src/game/modcode.rs`・`mod.rs` | 済 | 2026-11-14 | コミット `d0de1a5`（3 ファイル）。コミット済みのソースからの作り直しは未確認 |
| B-01 | 配布 | 高 | パッケージ版が書き直し前の中身のまま（`main`・`mods` は 11-11 の Python 版、`lib` だけ新しい）。同梱の Python に使わないライブラリが残る | `packages/windows/Alpedia/game/`・`runtime/Lib/site-packages`（panda3d・ursina・numba・scipy・skimage） | 済 | 2026-11-14 | 作り直した（1.1 GB → 792 MB。ライブラリは numpy・pillow・av だけ、`main`・`mods` は開発版と同じ）。描画なしの起動テストと、同梱の Python 3.12 でのシナリオ・MOD・街のテスト 34 件が通った。画面を出しての起動・前からあるセーブ（`world1`）の読み込みは未確認。原因は X-01・X-02 |
| M-01 | ムービー | 高 | ムービーを作る道具が、消した ursina・panda3d と Python 版の描画に頼っていて動かない見込み（未実行） | `movies/cinema/stage.py`・`actor.py`・`post.py` | 未 | | |
| D-01 | 文書 | 中 | 「Rust が無くても numba で動きます」が誤り。`alpedia_view` の説明が無い | `docs/開発の進め方.md` 43 行目ほか | 済 | 2026-11-14 | numba の記述を消し、Rust が本体で無いと起動しないこと・`alpedia_view`・`tools`・`movies` を足した |
| D-02 | 文書 | 中 | 「コード構成」が `main/*.py` を本体として説明（今は Rust の入口だけ）。「決まりごとは Python のまま」も古い | `dev/README.md`「コード構成」「高速化」 | 済 | 2026-11-14 | 「コード構成」を Rust のファイルと Python の入口の対応表に書き直し、「高速化」「画面」「フォルダ構成」も今の作りに |
| D-03 | 文書 | 中 | 「Rust 版と Python 版を比べるテスト」の説明が古い。`src/game/` の説明が無い | `dev/native/README.md` | 済 | 2026-11-14 | 2つのモジュールの説明と「ゲーム本体（`src/game/`）」の節を足し、テストの説明・ビルドでできる物を今の形に |
| D-04 | 文書 | 低 | 必要なものに ursina・scipy と書いてある | `movies/README.md` 24 行目 | 済 | 2026-11-14 | 今の `dev/` とは合わないこと・要るライブラリ・描き直すための道を冒頭に書いた（道具そのものは M-01 のまま） |
| D-05 | 文書 | 低 | Panda3D の wheel を使う予備の手順が残る | `packages/windows/README.md` 43 行目 | 済 | 2026-11-14 | 予備の手順は今もスクリプトにあり正しかった。Panda3D の wheel は部品を取り出すだけで同梱しないことと、MOD の入れ替え方を書き足した |
| D-06 | 文書 | 低 | ルートの README に Phase2 最終報告書が無い。`ストーリー改訂案.md` が「提案・未実装」のまま（今のシナリオと別物） | `README.md`・`docs/ストーリー改訂案.md` | 済 | 2026-11-14 | ルートの README に Phase2 最終・Phase3 の文書と `movies/` を足し、改訂案の冒頭に「前の案」の断り書きを入れた |
| T-01 | テスト | 中 | Rust の単体テストが 0 件。Python 版の本体を消したので、比べる相手はテストの中に numpy で書いた式だけ | `dev/native/`（`#[test]` なし）・`dev/tests/test_native.py` | 未 | | |
| T-02 | テスト | 中 | `dev/tools/*.py`（設計図・効果音・台本）を書き直し後に動かして確かめていない | `dev/tools/` | 未 | | |
| G-03 | 管理 | 中 | 登場人物の VRM 13 体とエンドロールのロゴが git に無い。BGM の曲を配る物に入れるか・権利の確認も未決 | `dev/mods/skinmodel(dummy)/*.vrm`・`dev/mods/story/credits/`・`dev/mods/bgm/` | 未 | | |
| G-05 | 管理 | 中 | リポジトリが OneDrive の中（`.git` 357 MB・`target` 93 MB も同期される） | フォルダの場所・`dev/native/alpedia_core/target` | 未 | | |
| B-02 | 配布 | 中 | Windows 版しかない（Linux は予定地だけ。画面のビルドの手順も無い） | `packages/linux/README.md`・`dev/native/README.md` | 未 | | |
| B-03 | 配布 | 中 | 自動のビルド・テスト（CI）が無い | `.github/` なし | 未 | | |
| C-01 | コード | 中 | Rust の中身が Python の値を触る形（属性の読み書き・メソッド呼び出しが約 5,600 か所）。型の検査・速さの得が限られる | `dev/native/alpedia_core/src/game/` | 未 | | |
| C-02 | コード | 中 | ファイルが大きい（`construct.rs` 3,531 行、`ursina.rs` 2,665 行、`world.rs` 2,499 行 など 6 つが 2,000 行以上） | `dev/native/alpedia_core/src/game/` | 未 | | |
| P-01 | 速さ | 中 | シナリオで 20〜52 fps（描画 13〜27 ms／フレーム。物理は 4〜7 ms／回） | `dev/logs/latest.log`（構築物 133） | 未 | | |
| S-01 | 安全 | 中 | ゲーム内スクリプトのサンドボックスが簡易。読み込んだ構築物のプログラムが動く | `dev/README.md`「現時点の制限」・`game/scripting.rs` | 未 | | |
| S-02 | 安全 | 中 | マルチプレイに認証・暗号が無い。届いた MOD のプログラムをそのまま動かす（LAN の知り合い前提） | `dev/README.md`「マルチプレイ」・`src/net.rs` | 未 | | |
| G-04 | 管理 | 低 | 個人の設定（プレイヤー名・音量など）が git に入っていて毎回差分になる | `dev/config/settings.json` | 未 | | |
| G-06 | 管理 | 低 | ムービー・VRM を git に直接入れている（追跡ファイル 計 122 MB） | `dev/mods/story/movies/`・`movies/output/` | 未 | | |
| G-07 | 管理 | 低 | `Claude outputs/`（149 MB）に済んだ作業のバンドル・パッチが残る | `Claude outputs/` | 未 | | |
| C-03 | コード | 低 | MOD の画面が ursina・panda3d の「まね」に頼る（MOD は今も `from ursina import …`） | `game/ursina.rs`・`game/panda.rs`・`dev/mods/story/code/`・`dev/mods/city/code/` | 未 | | |
| C-04 | コード | 低 | `unwrap()`・`expect()` が 124 か所 | `dev/native/alpedia_core/src/`・`alpedia_view/src/` | 未 | | |
| C-05 | コード | 低 | `world.json` の `blocks` が保存形式の表に無く、読み込むたびに警告 | `game/savefmt.rs`・`game/buildkit.rs` 968 行目 | 未 | | |
| C-06 | コード | 低 | エンドロールに、もう使っていない Panda3D・Ursina が残る（残すかは判断しだい） | `dev/mods/story/code/script.py` 457〜458 行目 | 未 | | |
| P-02 | 速さ | 低 | ワールドに入った直後の1フレームが約 1.8 秒 | `dev/logs/latest.log` | 未 | | |
| P-03 | 速さ | 低 | VRM の骨の変形を CPU でしている（1 体 2〜4 万頂点） | `dev/native/README.md`「画面」 | 未 | | |
| L-01 | 制限 | — | 地形は崩れない。水は流れない | `dev/README.md`「現時点の制限」 | 未 | | |
| L-02 | 制限 | — | 惑星は公転しない。大気の抵抗が無い | 同上 | 未 | | |
| L-03 | 制限 | — | やわらかいブロックのモードで、大きな構築物ほどやわらかくなる | 同上 | 未 | | |
| L-04 | 制限 | — | MOD をプレイ中に読み直せない。使っている MOD を外すとワールドを始められない | `dev/mods/README.md` | 未 | | |
| L-05 | 制限 | — | Python → ブロックへの変換が無い。Python の編集は外部エディタ | `dev/README.md` | 未 | | |
| L-06 | 制限 | — | HP・敵とのやりとりが無い。サバイバルの目標は月に降りる所まで | `dev/README.md` | 未 | | |
| L-07 | 制限 | — | シナリオは vol.1 の1本だけ | `dev/mods/story` | 未 | | |

## 一覧に無かった修正

上の表に無い物を直したら、ここに足す。

| 番号 | 直した日 | 種類 | 何が問題だったか | 直した内容 | 場所・コミット |
|---|---|---|---|---|---|
| X-01 | 2026-11-14 | 配布 | パッケージを作るとき、ダウンロードの置き場に残っている wheel を全部同梱していた（requirements から外した Panda3D・ursina・numba・scipy なども入り続ける） | 毎回、今の requirements に要る物だけを空のフォルダに集め直す。消せずに残った空のフォルダももう一度消す | `packages/windows/build/build_package.py`（`download_wheels`・`_clear`） |
| X-02 | 2026-11-14 | 配布 | 本体の印（`builtin`）が無い同梱 MOD（VRM の見た目・モーション・BGM・例）は足りないファイルを足すだけで、変わったプログラムが入れ替わらなかった。前の版のサバイバルの MOD も残り続けていた | 開発版のほうが新しいファイルは入れ替える。今は無い本体の MOD は消す | `packages/windows/build/build_package.py`（`copy_game`） |

## 更新の記録

| 日付 | 内容 |
|---|---|
| 2026-11-14 | 作成（37 件） |
| 2026-11-14 | G-01・G-02 を済に |
| 2026-11-14 | B-01 を済に。X-01・X-02 を追加 |
| 2026-11-14 | D-01〜D-06 を済に（文書を今の作りに合わせた）。T-01 の書き方を正確に |
