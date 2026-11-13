# Alpedia

物理演算つきのボクセルサンドボックスゲーム（Minecraft × KSP）。このフォルダは次の3つに分かれています。

| フォルダ | 中身 | 使うとき |
|---|---|---|
| `docs/` | 方針・報告書・開発の進め方 | 何を作っているか・どう作るかを見るとき |
| `dev/` | 開発版のゲーム一式（Rust の本体・Python の入口・MOD・セーブ・テスト） | ゲームを直す・試すとき（`dev/start.bat` で起動） |
| `packages/` | 配る用のパッケージ版（OS ごと） | Python なしで遊ぶ・ほかの人に渡すとき |
| `movies/` | シナリオのムービーを描く道具 | ムービーを作り直すとき（`movies/README.md`） |

- 遊び方・操作・仕組みの説明：`dev/README.md`（パッケージ版にも同じものが入ります）
- MOD の作り方：`dev/mods/README.md`
- 開発の流れ（起動・テスト・Rust・パッケージ作り）：`docs/開発の進め方.md`
- `Claude outputs/` は Claude が作った画像などの置き場です（ゲームには使いません）。

## docs/

| ファイル | 中身 |
|---|---|
| `方針.txt` | 最初に決めたゲームの方針 |
| `開発の進め方.md` | フォルダの役割、開発版の起動・テスト、Rust の物理コアとパッケージの作り直し方 |
| `ストーリー台本.md` | シナリオモード「それでも」：章・目標と判定・セリフ・ムービー・舞台（移植・更新のときの見比べ用） |
| `キャラクター・勢力設定 ブラッシュアップ版.pdf` 等 | 登場人物・勢力の設定 |
| `Alpedia_Phase1_Report.pdf` | フェーズ1 報告書 |
| `Alpedia_Phase2_Interim_Report.pdf` | フェーズ2 中間報告書 |
| `Alpedia_Phase2_Final_Report.pdf` | フェーズ2 最終報告書 |
| `Alpedia_Phase3_Pre_Report.md` | フェーズ3 事前報告書（できること・問題点のまとめ。2026-11-14 時点） |
| `Alpedia_Phase3_問題点一覧.md` | 問題点の一覧表（直したら・見つけたらここに追記） |
| `ストーリー改訂案.md` | 前の案「黒鉄の艦隊」（今のシナリオの元になった提案。今の内容は `ストーリー台本.md`） |

## packages/

| フォルダ | 中身 |
|---|---|
| `windows/` | Windows 版。`windows/Alpedia/Alpedia.exe` で起動。作り直しは `windows/パッケージを作る.bat`（`windows/README.md`） |
| `linux/` | 予定地（まだありません） |
