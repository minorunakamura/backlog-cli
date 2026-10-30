# Backlog CLI/TUI 初版 実装計画

## 目的

Rust製の共通libraryと、人向けTUI `bl`・非対話CLI `bl-cli`を新規開発する。Apple Silicon Macで、プロジェクト選択、課題検索・閲覧・作成・部分更新、コメント一覧・個別閲覧・追加・本文更新を安全に実行できる初版を作る。

正式入力は [基本設計](../designs/backlog-cli.md)。特に「実行ファイル」「対象API」「profileと設定」「検索・名前解決・ページ送り」「TUIの画面・操作」「課題の項目・作成・更新」「課題コメント」「本文入力とeditor」「本文変換・表示の安全性」「JSON・通信・書込み結果」「品質・配布」を要求・受入条件として参照する。本Planは、その「後続の実装計画で具体化する事項」を補う。基本設計の目的・責務・対象外を維持し、表示方針は合意済みの再計画節に従って改訂する。

## 要件・決定事項

### 現状・採用技術

- 調査時のリポジトリは基本設計書のみで、既存実装・Cargo設定・テスト・CIはない。既存利用者の移行やデータmigrationは不要。
- 単一Cargo packageに `src/lib.rs`、`src/bin/bl.rs`、`src/bin/bl-cli.rs`を置く。共通処理をTUI/CLIが直接呼び、内部RPC・常駐service・汎用API frameworkを作らない。
- toolchainはRust **1.99.1**、edition 2024、targetは `aarch64-apple-darwin`。主要依存は下表の版で固定し、`Cargo.lock`を追跡する。Cargoのtest/clippy/buildとcargo-auditの導入には `--locked` を使い、監査実行は `cargo audit --file Cargo.lock` で追跡対象のlockfileを直接指定する。

| 用途 | 版・設定 |
| --- | --- |
| Ratatui | 0.30.2、Crossterm 0.29 backendを明示 |
| Crossterm | 0.29.0 |
| Clap | 4.5.54、derive |
| Tokio | 1.53.3、runtime・signal・sync・time・processに必要なfeature |
| Reqwest | 0.12.28、default-features=false、rustls-tls |
| Serde / serde_json | 1.0.228 / 1.0.149、derive / arbitrary_precision |
| pulldown-cmark | 0.13.5 |
| 単行入力の採用候補 | tui-input 0.15.5、後述の早期実端末確認を経て確定 |

本文一時ファイルには `tempfile`を利用する。その他の補助・dev dependencyは、必要性があるものだけ追加してlockfileに固定する。EventStream用の追加依存や独自入力frameworkは不要。

一時ディレクトリで上表の同時ビルドとReqwest設定APIの利用、結合文字・ZWJ emojiのgrapheme単位削除、表示列cursor、大整数・未知fieldのJSON保持をassert確認した。これは製品実装や実IMEの検証ではない。Probeの途中ファイルは残さない。

### Issue #1 再計画：正式監査コマンドの訂正

対象は [Issue #1](https://github.com/minorunakamura/backlog-cli/issues/1) の `REPLAN_REQUIRED`。初回Plan作成後、Cargo設定・lib/2bin・テスト・README/CONTRIBUTING・CIの部分実装が未コミットで存在する。新しいImplementationセッションでは、それらを保持したまま現在のコードと差分を確認して再開する。

- **停止理由・確認済みの原因**：固定済みのcargo-audit 0.22.2で `cargo audit --locked` は `unexpected argument '--locked'` により終了コード2となり、監査されない。同版のhelpに `--locked` はなく、`--file <FILE>` は監査するCargo lockfileの指定として提供されている。
- **合意した変更と理由**：正式監査コマンドをリポジトリ直下の `cargo audit --file Cargo.lock` に訂正し、cargo-audit **0.22.2** と導入方法 `cargo install cargo-audit --version 0.22.2 --locked` を固定する。導入時の `--locked` と監査対象のlockfile指定は別の契約である。test/clippy/buildの `--locked` は維持する。製品仕様・基本設計「品質・配布」の目的は変更しない。
- **Probeの観測**：一時ディレクトリに現在の `Cargo.lock` だけをコピーし、旧コマンドの終了2と新コマンドの216依存の監査・終了0を確認した。実行前後でコピーと元lockfileのSHA-256は不変だった。ただしTokio 1.49.0のunsound警告 `RUSTSEC-2026-0386` が1件出ており、終了0を警告なし・安全性保証とは扱わない。advisory databaseの更新で将来の結果は変わる。
- **適用範囲・タイミング**：今回のPlanningで本Planを同じ正式パスに改訂保存し、既存Issue #1の受入条件・検証手順を整合させる。`CONTRIBUTING.md` の正式品質コマンドと `.github/workflows/quality.yml` の監査stepの訂正・検証は、新しいImplementationセッションでIssue #1の残作業として行う。これらの旧記述より本節と下記の正式チェックを優先する。READMEの利用説明、既存ソース・テスト・Cargo設定・lockfileをこの訂正のためだけに変更しない。
- **維持する範囲**：警告・失敗・確認不能を隠さず報告し、無条件ignoreを追加しない。新しい `--deny` 方針、依存更新、advisory databaseの固定、独自監査wrapperは今回追加しない。既存の未コミット変更をcommit / stash / 破棄しない。後続Issueの製品要件や依存関係を変更しない。
- **回帰確認**：コマンド訂正のみなので、この訂正に新規unit testやTDDは追加せず、同じcargo-audit版での実コマンドとlockfile不変確認を回帰検証にする。Implementationでリポジトリ直下から `cargo audit --version` を確認し、`shasum -a 256 Cargo.lock` → `cargo audit --file Cargo.lock` → `shasum -a 256 Cargo.lock` を順に実行する。引数エラーではなく対象lockfileの監査が実行され、ハッシュが変わらないこと、実際の終了コード・警告・失敗を記録することを確認する。監査失敗を成功扱いにせず、対応に要件・固定依存の変更が必要なら停止して再計画する。既存の正式チェックを再実行し、過去のチェック・Review・Code Review Approveを変更後の差分へ流用しない。Plan・Issue #1・ローカル手順・CIの版・導入・監査コマンドを照合し、CIのtrigger等を維持する。GitHub上での実行のためだけのpush/PRは行わない。

### Tokio RUSTSEC-2026-0386：固定依存の保守改訂

#### 目的と採用版

Tokio を **1.53.3** に更新し、追跡対象の `Cargo.lock` に対する `RUSTSEC-2026-0386` の unsound 警告を解消する。本改訂は初版 Plan の固定依存に関する決定の更新であり、製品仕様・基本設計・完全一致版で固定する方針を変更しない。

正式な Approved Plan は本ファイル `docs/plans/backlog-cli-initial.md` の改訂全文とし、別の保守 Plan は作らない。本節と採用版表は Planning の承認後に保存する。依存更新と更新後の実測結果の追記は、別の新しい Implementation セッションで行う。採用版表の改訂を依存更新や検証の完了とは扱わない。

本保守作業の要求・範囲・完了条件・動作確認は本節を用いる。本 Plan の他の製品機能・初版全体の受入をこの保守作業に取り込まず、他節にある TDD・実IME/TUI・配布物の受入を今回の完了条件に追加しない。

#### 確認済みの根拠・更新前の観測

- Planning 時点の `Cargo.toml` は Tokio `=1.49.0`、`Cargo.lock` は Tokio 1.49.0 / tokio-macros 2.6.1。既存 feature は `rt-multi-thread`, `macros`, `signal`, `sync`, `time`, `process`。
- cargo-audit **0.22.2** でリポジトリ直下の `cargo audit --file Cargo.lock` を実行し、223依存の監査で当該 unsound 警告1件、終了コード **0** を再現した。監査前後の lockfile の SHA-256 はともに `522302749ee772aee2c2405cce1d4f1631cb28c1bab6686604838ea37ef4c7d8`。これは今回の更新前の観測であり、更新後の結果ではない。
- [RustSec 公式 advisory](https://rustsec.org/advisories/RUSTSEC-2026-0386.html) と [advisory-db 原文](https://raw.githubusercontent.com/RustSec/advisory-db/main/crates/tokio/RUSTSEC-2026-0386.md) の patched 範囲は **`>=1.51.6, <1.52.0` または `>=1.53.3`**、unaffected は `<0.3.2`。1.49.0 は対象、1.53.3 は patched。1.52 系を修正済みとして扱わない。
- 原因は `AsyncFd` の safe constructor が FD の所有・有効性を保証せず I/O safety に違反すること。修正には旧 constructor の非推奨化と unsafe な `register` / `register_with_interest` の追加が含まれる。patched 判定を製品全体の安全性保証とは扱わない。
- [Tokio 1.53.3 公式 release](https://github.com/tokio-rs/tokio/releases/tag/tokio-1.53.3)、[公式 Cargo.toml](https://raw.githubusercontent.com/tokio-rs/tokio/tokio-1.53.3/tokio/Cargo.toml)、[crates.io metadata](https://crates.io/api/v1/crates/tokio/1.53.3) で公開済み・調査時点 yanked=false・MSRV Rust 1.71 を確認した。固定 toolchain は Rust 1.99.1 / edition 2024 / `aarch64-apple-darwin`、実行環境も rustc 1.99.1 / Darwin arm64。MSRV の充足だけでビルド互換性を確定しない。
- Tokio 1.53.3 の `tokio-macros` 要求は `~2.7.0`。既存の `macros` feature を維持するため、tokio-macros 2.6.1 の更新も必要となる。その他の推移依存の実際の差分は依存解決後に確認する。
- [Issue #1](https://github.com/minorunakamura/backlog-cli/issues/1) は CLOSED を確認済み。今回の作業は新規 Work Item とし、Issue #1 の再オープン・本文変更・今回作業のコメント追記はしない。

#### 改訂範囲・維持する履歴

- 上表の Tokio 採用版を 1.49.0 から **1.53.3** に改訂し、本節を追記する。`Cargo.toml` と `Cargo.lock` は Implementation で整合させる。完全一致指定・既存 feature・他の直接依存の指定は維持する。
- 直前の「Issue #1 再計画：正式監査コマンドの訂正」節、当時の216依存・終了0・当該警告1件という Probe 観測、および既存の同時ビルド等の Probe 記録は変更しない。直前の節にある「依存更新を追加しない」は当時のコマンド訂正作業の範囲として残し、今回の新規保守作業は本節に従う。
- 製品側の変更は `Cargo.toml`, `Cargo.lock`, 本ファイルに限定する。`CONTRIBUTING.md`「品質確認」と `.github/workflows/quality.yml` は既存の正式コマンドとの整合確認だけを行い、変更しない。製品ソース・テスト・fixture の仕様変更、他の直接依存更新が必要なら無断で拡大せず再計画する。
- 無関係な依存の一括更新、feature 変更、fork / patch、警告の ignore、新しい `--deny` 方針、advisory database 固定、監査 wrapper は対象外。
- toolchain、edition、MSRV 設定、cargo-audit 0.22.2 と導入方法、CI の実行方針・trigger、製品仕様・基本設計は変更しない。実サービスへの接続・書込み、実IME/TUI受入、配布・公開、確認のためだけの push / PR 作成は行わない。
- 既存の未コミット変更がある場合は保持し、commit / stash / 破棄を独断で行わない。

#### 実装方針

1. 新規 Work Item、本ファイルの本節、現在のコードと差分を確認する。RustSec の patched 判定、cargo-audit 版、固定 toolchain を再確認し、更新前の依存・監査結果を記録する。1.53.3 の patched 判定が変わっていれば、独断で別版を選ばず再計画・必要な再レビューに戻る。
2. `Cargo.toml` の Tokio を **`=1.53.3`** に変更し、feature 配列を維持する。リポジトリ直下で `cargo update -p tokio --precise 1.53.3` を実行して対象を限定する。無指定の `cargo update` や lockfile の再生成は行わない。
3. `git diff -- Cargo.toml Cargo.lock` と必要に応じて `cargo tree --locked -i <crate>` で差分を確認する。Tokio の版・checksum、tokio-macros、および新要求の解決に必要な推移依存の変更だけを採用し、変更 crate と必要性を説明する。lockfile の checksum / 依存辺を手編集しない。無関係な更新を分離できない場合は原因を調べてから進める。
4. 下記の正式チェック・更新後監査を実行する。Reqwest を使う HTTPS / timeout、`tokio::signal::ctrl_c()` による中断、`#[tokio::main]`、library の Runtime 利用を含む既存の全対象テストで回帰を確認する。根拠は `src/api.rs`, `src/bin/bl-cli.rs`, `tests/project_list.rs`, `tests/service.rs` と `CONTRIBUTING.md` の既存テスト説明。
5. **実測後にのみ**本節へ更新後の検証記録を追記する。監査の日時・cargo-audit 版・終了コード・当該 ID の有無・残る警告と失敗・監査前後の lockfile ハッシュ、および各正式チェックの結果を記録する。未実施・失敗・確認不能はそのまま区別する。過去の結果を解消済みに書き換えない。
6. 最終差分が今回の範囲内で、採用版・manifest・lockfile が整合し、既存履歴と Issue #1 が維持されていることを確認する。互換性修正のために範囲外の変更が必要なら停止して再計画する。

#### 保守作業の完了条件

- `Cargo.toml` は Tokio `=1.53.3`、`Cargo.lock` の解決版も 1.53.3 で整合し、feature と他の直接依存の完全一致指定を維持している。
- lockfile 差分は今回の依存解決に必要なものだけで、変更 crate と理由を説明できる。
- `cargo audit --file Cargo.lock` が対象 lockfile を実際に監査し、**`RUSTSEC-2026-0386` が報告されない**。終了コード0だけを解消の根拠にしない。監査前後で lockfile が不変。
- `cargo fmt --all -- --check`、`cargo test --locked --all-targets`、`cargo clippy --locked --all-targets -- -D warnings`、`cargo build --locked --release --target aarch64-apple-darwin --bins` がすべて成功する。
- 本ファイルの採用版とコードが整合し、本節に更新後の監査終了コード・残る警告や失敗・全チェックの実測結果が追記されている。過去の Probe と Issue #1 の履歴が維持されている。
- 別の警告が残れば内容を記録する。監査の非0終了・実行不能、正式チェックの失敗を成功や完了として扱わず報告し、必要な判断に戻る。無関係な警告解消を無断で同梱しない。

#### 動作確認方法

新しい製品ロジックは追加しないため、今回に新規 unit test や TDD は追加しない。更新前の当該警告の再現と更新後の非報告を同じ正式監査コマンドで比較し、既存の全対象テスト・lint・build を回帰検証とする。

リポジトリ直下で、`CONTRIBUTING.md` の環境要件（Python 3 / OpenSSL を使うローカル HTTPS harness を含む）を満たして実行する。

```sh
cargo audit --version
cargo fmt --all -- --check
cargo test --locked --all-targets
cargo clippy --locked --all-targets -- -D warnings
shasum -a 256 Cargo.lock
cargo audit --file Cargo.lock
# 直前の監査終了コードを直ちに取得・記録してから次を実行する。
shasum -a 256 Cargo.lock
cargo build --locked --release --target aarch64-apple-darwin --bins
```

各チェックの終了コードを個別に記録し、pipeline / 後続コマンドで監査の終了コードを上書きしない。cargo-audit の導入が必要なら既存の `cargo install cargo-audit --version 0.22.2 --locked` を使う。advisory database は通常どおり更新し、取得失敗を警告解消とみなさない。

GitHub Actions 上の実行は完了条件にしない。未実行ならその旨を残す。Planning 時点では更新後の依存解決・監査・ビルドは未検証であり、過去のチェック・Review の承認を更新後の差分へ流用しない。

#### Planning の受渡し

本ファイルの改訂全文を Plan Review に提出し、明示承認後に同じ正式パスへ保存する。その後 `taskflow-to-ticket` の手順で新規 Work Item の分割、最終本文・保存先の保存公開をそれぞれ承認して作成し、本ファイルの本節を参照できるようにする。Issue #1 は変更しない。新規 Work Item の作成後は停止し、Implementation は別の新しいセッションで行う。

#### Issue #36 Implementation の検証実測（2026-10-27 UTC）

- 実行環境は Darwin arm64、`rustc 1.99.1 (58cf8f9c3 2026-10-13)` / `cargo 1.99.1 (5f94df478 2026-08-27)`。RustSec 公式 advisory 原文を再取得し（終了0）、patched 範囲が `>=1.51.6, <1.52.0` または `>=1.53.3` のままであることを確認した。更新前の比較には、本節の既存観測と、今回の開始時にも一致した lockfile SHA-256 `522302749ee772aee2c2405cce1d4f1631cb28c1bab6686604838ea37ef4c7d8` を用いた。
- `Cargo.toml` の変更は Tokio `=1.49.0` → `=1.53.3` のみ。既存6 feature と他の直接依存の完全一致指定は維持した。`cargo update -p tokio --precise 1.53.3` は終了0。lockfile の変更 package は Tokio 1.49.0 → 1.53.3 と tokio-macros 2.6.1 → 2.7.2 の2件だけで、それぞれの checksum が更新された。tokio-macros は Tokio の既存 `macros` feature が要求する `~2.7.0` の解決に必要。2.7.2 の `syn = "3.0"` 要求により、その依存辺だけが `syn 2.0.119` → 既存の `syn 3.0.9` に変わり、syn 自体の追加・版更新はない。`cargo tree --locked -i tokio-macros` は終了0。更新時の「8 unchanged dependencies behind latest」は情報表示であり、無関係な更新は行っていない。
- `CONTRIBUTING.md` と `.github/workflows/quality.yml` の正式コマンド・cargo-audit 固定版・導入方法を照合し、変更しなかった。HTTPS harness の前提確認は `python3 -c 'import http.server, ssl, pty; print(ssl.OPENSSL_VERSION)'` が終了0（OpenSSL 3.6.3）、`openssl version` が終了0（CLI / Library とも OpenSSL 4.0.3）。

| 実行コマンド | 終了コード | 実測結果 |
| --- | --- | --- |
| `cargo audit --version` | 0 | 実出力 `cargo-audit-audit 0.22.2` |
| `cargo fmt --all -- --check` | 0 | 差分・警告なし |
| `cargo test --locked --all-targets` | 0 | 既存94テスト成功、失敗・無視・filterなし。HTTPS、30秒body timeout、headers前/body読取り中のCtrl+C、CLIの `#[tokio::main]` とlibraryの Runtime 利用を含む |
| `cargo clippy --locked --all-targets -- -D warnings` | 0 | 警告・失敗なし |
| `shasum -a 256 Cargo.lock`（監査前） | 0 | `b870ac297cd6fdd22f34a0dffb68052f4343620c98816665a4adaf7cd7a2b618` |
| `cargo audit --file Cargo.lock` | 0 | 2026-10-27T15:13:05Z開始、15:13:06Z終了。通常のDB取得・crates.io index更新成功、1380 advisory・223依存を監査。`RUSTSEC-2026-0386` 非報告、その他の警告・失敗もなし。監査終了コードは直後に取得 |
| `shasum -a 256 Cargo.lock`（監査後） | 0 | `b870ac297cd6fdd22f34a0dffb68052f4343620c98816665a4adaf7cd7a2b618`（監査前と一致） |
| `cargo build --locked --release --target aarch64-apple-darwin --bins` | 0 | 両binaryのrelease build成功、警告・失敗なし |

新規テスト・TDDは本保守節の方針どおり追加していない。GitHub Actionsは未実行で、確認用のpush / PR、配布・公開、実サービス接続・書込み、実IME/TUI受入は行っていない。Issue #1 および他Issueへの変更はなく、既存の未コミットPlan改訂・過去Probe・Issue #1履歴は保持した。今回の監査結果は当該時点の観測であり、将来のDB更新や製品全体の安全性を保証しない。

### CLIのflag契約

基本設計の13コマンドと短縮flagを実装する。help/versionは設定・API接続なしで利用できる。TTYでも対話せず、明示された `--body-file -`以外でstdinを読まない。globalの `--profile`・`--json`・`--no-input`はsubcommandの前後で使える。`--no-input`は動作を変えない。

| 適用先 | 入力 |
| --- | --- |
| issue-type list | project |
| issue list | project、keyword、status、assignee、milestone、limit、offset |
| issue mine | listと同じ。ただしassigneeはself固定で、担当者flagは受け付けない |
| issue create | project、title、body/body-file、issue-type、priority、assignee、start-date、due-date、milestone |
| issue update | title、body/body-file、issue-type、priority、assignee、status、resolution、start-date、due-date、milestoneと対応する解除 |
| comment list | limit、min-id、max-id |
| comment add | body/body-file、status、assignee、resolutionと担当者・完了理由の解除 |
| comment update | body/body-fileのみ、明示入力必須 |

project list / priority list / resolution list、およびviewには表にない入力flagを追加しない。update/commentのprojectは対象課題から取得し、project指定やローカルprojectによる上書きを受け付けない。

- `--issue-type-id`・`--priority-id`・`--status-id`・`--assignee-id`・`--milestone-id`・`--resolution-id`を、それぞれの名前flagの適用先に設ける。同一項目の名前flagとID flagは排他。projectは既存の `--project`でキーまたは数値IDを受け取る。
- 数値IDは正整数。resolution IDだけは0を有効値とする。名前は完全一致、候補なし・複数一致はエラー。不要な候補APIを呼ばない。単独のID入力では名前解決用候補取得を要求せず、最終的な所属・権限等はAPIの検証も受ける。
- listのstatus/assignee/milestoneは各flagを反復でき、項目内OR・項目間AND。create/updateのmilestoneも反復でき、更新では指定集合へ置換する。他の書込み項目は単一指定。同じ単一flagの重複はエラー。反復値の重複は集合として一度だけ送る。
- `--status all`は一覧の状態既定値解除。`all`と他の状態指定の混在は拒否する。`--assignee self`は選択profileの認証ユーザー。横断の `--project all`はlist/mineだけで、keywordまたはselfを必須とし、ローカル検索既定値を一切適用しない。状態・milestone・self以外の担当者入力を拒否する。
- `--clear-assignee`はcreate/update/comment add、`--clear-resolution`はupdate/comment add、`--clear-start-date`・`--clear-due-date`・`--clear-milestones`はupdateだけ。対応する値指定と排他。createのclear-assigneeは担当者既定値を適用しない指定であり、不要な更新操作を発生させない。
- 本文専用の解除flagは設けない。`--body`と`--body-file`は空文字指定も含め排他。UTF-8検証後は入力元によらず同じ値を使い、空白・改行をtrimしない。空ファイル・空stdinも空本文になる。課題更新の空本文は解除、コメント追加の空本文は本文なし、既存コメント更新の空本文は入力エラー。
- titleは空白のみ不可。日付は実在する `YYYY-MM-DD`を検証する。日付間の大小など設計にない業務制限は加えない。
- issue list/mineはlimit既定20・1〜100、offset既定0・0以上。comment listもlimit既定20・1〜100、`--min-id`・`--max-id`は正整数・境界自身を含まない。同時指定ならmin < max。CLIコメント一覧は降順固定、指定した1ページのみ返す。offsetやtotalは使わない。
- 未指定・設定由来・明示解除を区別する。更新とコメント追加の課題項目は最新値と比較し、変更項目だけ送る。全く変更のない更新、本文も実変更もないコメント追加は入力エラーとして書き込まない。既存コメントも本文が同じなら書き込まない。

### 出力・エラーと終了コード

成功JSONの形は基本設計「JSON・通信・書込み結果」をそのまま採用する。API応答を `serde_json::Value`として保持し、必要fieldを別途検証する。未知fieldと大整数を落とさず、数値を安易にf64へ変換しない。issue view/create/updateだけに検証済みorigin/keyから生成したurlを追加する。JSON取得では本文記法確認や描画のためのGETを行わない。

固定表示は、一覧ではID/キー・名前やタイトル・状態など識別と選択に必要な列、課題詳細では設計の全項目と本文、コメント詳細では本文と変更履歴を表示する。書込み成功では対象キーまたはIDと結果を表示する。空一覧と取得失敗を区別する。非TTYは装飾なし、NO_COLOR指定時は両binaryの色装飾なしとする。

JSON失敗はstderrへ改行終端の1文書、stdoutには成功文書や案内を混ぜない。構造は次のとおり。

```json
{"error":{"kind":"network","message":"安全化した説明","httpStatus":null,"apiErrors":[],"write":{"operation":"issue.create","outcome":"unknown"}}}
```

- `kind`は input / config / auth / permission / not_found / rate_limit / network / api / invalid_response / output / terminal / editor / interrupted。
- httpStatusは取得済みstatusまたはnull、apiErrorsは安全化したBacklog errors配列、取得できなければ空配列。
- `write`は書込み操作と識別できる失敗時に必須、読取りでは省略。operationは issue.create / issue.update / comment.add / comment.update。outcomeは作成系でnot_created / unknown / created、更新系でnot_updated / unknown / updated。comment.addは実際にPOSTしたか課題PATCHしたかで確度の語彙を選ぶ。
- 終了コードは0=成功/help/version、2=引数・本文・名前解決・変更なし等の入力エラー、1=設定・API・通信・表示・端末等の実行エラー、130=Ctrl+C等の処理可能な中断。書込み確度は終了コードと独立で、created/updatedでも後続処理失敗なら非0。
- Clapの引数エラーも、`--json`モードを識別できる場合は同じエラー形式で返す。help/version自体は通常のテキスト表示とする。
- secret保護はJSON値だけでなく未知field・object key・配列も対象。数値にsecretが一致する例外でも秘匿を優先し、必要ならそのscalarを伏字文字列へ置換する。keyのredactionで衝突する場合は項目を黙って失わず、安全な出力エラーにする。通常の非secretの数値・本文は保持する。raw responseや未安全化のエラーをdumpしない。

## 制約・対象外

基本設計の対象19 API・公開コマンド・キー操作・画面境界を維持する。削除・添付送信・独自alias・設定wizard/自動生成/補修・永続cache/履歴/下書き/書込み状態・競合検出/自動merge・全件自動取得・定期更新・Taskflow連携は追加しない。

保証targetはApple Silicon Macのみ。Release公開・署名・notarization・Homebrew・他OS保証は含めない。GitHub上のCI実行成功、pushやPR作成を完了条件にしない。実Backlogへの書込みは接続先と内容を確認した別の明示許可がある場合だけ行い、本Planの承認をその許可にしない。実サービスの複数project検証は現環境で不可能な制約を引き継ぎ、追加projectは要求せず自動試験で確認する。

基本設計に書かれたAPI検証結果は根拠であって、製品の検証済み記録として流用しない。SIGKILL・端末切断後の復元/一時ファイル削除、通知不能時の通知到達、editor独自backup管理、全端末での幅やIMEの完全一致は保証しない。

## 実装方針

### 1. 共通libraryと設定

config / api / service / body・safetyを機能単位のmoduleに分け、CLI/TUIが同じ検索・名前解決・変更判定を利用する。汎用trait群やplugin境界は作らない。

XDG・cwd直下settings・default profile・apiKeyEnv・editor argvの適用と検証は基本設計に従う。不在と壊れた設定を区別し、未知キー・型・不正値を拒否する。create既定値を他操作へ流用しない。起動後に設定やprofileを差し替えない。

spaceUrlはHTTPS originとして検証し、userinfo・query・fragment・root以外のpathを拒否する。rootの末尾slashは正規化する。固定API pathと検証済みID/キーをURL部品として扱い、文字列連結で任意pathやoriginへ逸脱させない。ブラウザも同じoriginと選択課題キーのHTTPS URLだけをshellなしで開く。

### 2. API通信と書込み確度

Reqwestは `redirect(Policy::none())`・`retry(reqwest::retry::never())`・`timeout(30秒)`を明示する。API keyはBacklog-API-Key headerだけへ入れる。POST/PATCHはform、配列項目はAPIの配列パラメーターで送る。

- 元JSONを保持しつつ、操作に必要なobject/field/IDと期待する成功statusを検証する。
- 更新の未指定項目はformへ含めない。日付・担当者・完了理由の解除は該当fieldの空値、milestoneの解除は空のmilestone配列パラメーターで表す。resolution ID 0と解除を区別する。
- 本文だけのコメント追加はPOST comments。課題項目の変更を伴う場合はPATCH issueへ変更項目と非空本文のcommentを一回で送り、2書込みや権限不足時のfallbackをしない。コメント本文更新はPATCH commentsだけ。
- 作成系201／更新系200を観測した直後、body読取り等の次のawaitより前に確度をcreated/updatedへ記録する。`bytes()`読取り、JSON decode、必要field検証、表示の失敗でこれを下げない。
- request送信を開始する前に「送信される可能性あり」を記録する。送信前と証明できる失敗は未実施、信頼できるAPI拒否は未実施、送信開始後に成否を確認できないtimeout・切断・不正応答・5xx等は保守的にunknownとする。redirectを追わない。
- 中断時は通信taskをabortしてjoinし、記録済み確度を読んで通知する。cancelのJoinErrorで確度を未実施へ上書きしない。通知と端末復元の一方の失敗で他方を省略しない。成功確認だけの追加GET、自動retry、永続的な結果管理は設けない。

### 3. 検索・ページと読取り

通常/横断/mineを同じ検索処理に集約する。新規検索は同一条件のcountとlistを取得し、両方の成功後だけ適用する。更新日時降順、実取得数による終端、count失敗非隠蔽は基本設計どおり。

課題一覧履歴はページ内容・選択・scrollをメモリ保持する。再訪では通信せず、Rや作成後再取得の成功時だけ先頭へ置換し履歴を破棄する。失敗では以前の表示を保つ。

コメントは独立したID境界pagingを実装する。TUIの古い側は降順とmaxId、新しい側の隣接取得は昇順とminIdを使い、画面の移動順へ整える。gは最新側、Gは昇順で最古側を直接取得し、全件走査しない。content:nullの変更履歴も移動対象とする。limitDate/assigner等の既知fieldは表示名へ変換し、未知fieldと前後値も安全表示する。

### 4. 本文変換と安全表示

編集原文・APIデータと表示用copyを分離する。Markdownはpulldown-cmark、Backlog記法は主要構造に絞った変換と安全な原文fallbackで扱う。code/escapeの保護、改行保持、HTML・未知macro等の内容保持をfixtureで確認する。画像等をfetchしない。

外部文字列は全出力経路で、解釈後の文字も含めてsecret伏字化・危険な制御文字/端末命令の無害化を適用する。改行・TAB、日本語・結合文字・emojiを必要なく壊さない。外部ANSI/OSCやHTMLを端末命令として実行しない。URLは可視textとする。

### 5. TUIの状態・通信・入力

main taskが状態と端末を単独所有する。Tokioの短いinterval（目安16ms）とCrosstermの `poll(Duration::ZERO)`、ready時だけのreadで入力を処理し、tickごとの処理数を制限する。通信は別taskから結果を返す。通信完了までUIをawaitで止めない。

結果には対象と世代を付け、検索変更・選択変更・取消・画面閉鎖後の古い結果が表示やbusyを変更しないようにする。プレビューは目安150ms間引き、選択が変わった時点で別課題の本文を表示しない。Enterだけで再取得しない。

基本設計のWorkspace/Project/Issue/Detail/Comment、左右内のfocus、キー表、100x24 / 40x12のlayout、極小時の保護を実装する。検索・R・作成後/課題更新後/コメント追加後の再取得は成功後だけ表示交換し、失敗時の状態保持と通知を実装する。課題更新で一覧を即時書換えず、Comment本文更新でも勝手に最新コメントへ移動しない。

単行入力はtui-inputの原文値・grapheme編集とvisual_cursor/visual_scrollを使い、Ratatuiの実cursorを入力位置へ置く。title/keywordとフォーム内の日付入力で同じ扱いを使う。貼付けのCRLF/CR/LF/TABは単行用spaceへ正規化し、表示変換と区別する。通常文字や編集キーをグローバル操作に横取りしない。

入力全体にsecretまたは安全変換が必要な制御文字があれば、欄全体を固定の伏字表示にしcursorもその表示内の固定位置へ置く。横scrollで見えない部分も検査し、原文を描画widgetへ流さない。編集原文は保持する。OS/IMEが受信前に描く未確定文字の秘匿は保証しない。

**入力部品の早期確定条件**：TUIの基本shellを作った段階で、本格的な画面・フォーム実装より前に、実端末で日本語IMEの未確定文字・候補のcursor追従、日本語/結合文字/ZWJ emoji、長文水平scroll・狭幅resize・focus移動・伏字時の安全表示を確認する。採用候補のビルド成功をこの確認の代わりにしない。失敗時は入力値保持や安全要件を弱めず、本格実装前に停止して部品/実現方法の再計画を行う。

### 6. editorと書込みフォーム

端末復元を単一のguard/共通処理に集約し、正常終了・処理可能な中断・失敗・panicで可能な復元を試みる。復元失敗は通知する。editor開始時はTUIのpollを止め、raw/alternate screen等を復元してからargvをshellなしで実行する。終了後は端末サイズ再取得・clear・全体再描画を直ちに行い、追加キーを要求しない。

本文用tempfileは本人だけが読み書きできるよう保護する。原文を渡し、正常終了＋読取＋UTF-8検証の全成功時だけ本文を交換する。失敗・中断で元値と他フォーム値を保持し、一時データを削除する。設定編集は既存fileだけを開き、非0終了でも保存内容を巻き戻さず、反映は再起動後とする。

課題/コメントの編集開始時は最新情報を取得する。コメント編集は認証ユーザー自身の本文だけで、本文なし履歴は閲覧専用、API権限も尊重する。

フォーム→確認→独立した明示送信の段階を分ける。確認画面だけでは書き込まず、入力差がある時だけ破棄確認する。送信中は編集・重複送信・破棄を無効にする。中断は確度を保って処理する。確認には接続先・対象・全入力/変更と解除内容を安全表示する。

### Issue #17 再計画：検索dialogの編集優先・popup・候補再利用

#### 目的・正式入力・適用範囲

[Issue #17](https://github.com/minorunakamura/backlog-cli/issues/17) の `REPLAN_REQUIRED` を受け、通常／横断検索dialogの部分実装を保持したまま、実端末で報告された操作・表示の問題を修正する。正式Planは本ファイルの改訂全文、基本設計は [docs/designs/backlog-cli.md](../designs/backlog-cli.md)「検索・名前解決・ページ送り」「TUIの画面・操作」「日本語入力と安全表示」。本節はIssue #17の改訂要求・実装方針・完了条件・検証方法を定め、検索用キー表と候補表示に関する旧記述より優先する。他Issueの機能を本Issueへ取り込まない。

#### 現在のコード・停止理由と確認済みの事実

- Planning開始時点で `CONTRIBUTING.md`, `README.md`, `docs/testing/manual-acceptance.md`, `src/tui.rs`, `src/tui/browse.rs`, `tests/tui_process.rs` が変更済み、`src/tui/search.rs`, `tests/fixtures/tui_search.json`, `tests/fixtures/tui_search_steps.json` がuntracked。Implementationはこれらを保持して、現在のコードと差分を改めて確認する。commit / stash / 破棄を独断で行わない。
- `src/tui/search.rs` の `Dialog::event` は単行入力より先にCtrl+A/K/U/Fを処理し、先頭移動・末尾削除・行削除・右移動と衝突している。`focus` は候補を毎回作り直し、描画は選択候補の行を常に先頭へ送っている。dialogは画面の四辺を1セルずつ引くだけで、ほぼ全画面になっている。
- 共有 `src/tui/input.rs` はtui-input 0.15.5の標準編集を既に利用している。検索側のdispatchを直し、入力部品や依存を置き換えない。
- `src/tui/browse.rs` はTarget別busyとgenerationの一致を結果・エラー処理より前に検査し、現世代だけを反映する。適用はcount/list両方が成功した後に条件・先頭ページ・履歴を交換する。この境界を維持する。
- 既定status解除、未確定projectでの検索防止、「自分」表示、resize時focus可視性の修正は現在のコードに存在する。Planningで既存の `search_dialog_space_deselects_repeated_default_status_without_reinserting_it` と `search_dialog_resize_reveals_milestone_and_preserves_keyword_cursor_through_tiny_size` を実行し、各1件成功を確認した。短縮名に `--exact` を付けた最初のresize試行は0件だったため、成功の証拠にはせず完全名で再実行した。他の過去チェック結果・Review承認を今回の検証として流用しない。
- 既存受入記録は過去の自動試験・製品buildの履歴であり、今回の追加要求の実装・受入成功ではない。利用者から実Backlog／実IMEの手動確認で問題が報告され、必須受入は未完了である。既存記録を消したり成功へ書き換えたりしない。

#### 合意した設計変更と理由

編集操作を優先するため、検索用Ctrlキーの有効範囲をキーワード欄の外へ限定し、検索用Ctrl+Fは廃止してEnterへ変更する。見やすさのため、中央のコンパクトなdialogと重ね表示の候補popupへ変更する。候補の再利用は同一dialog内の成功結果だけとし、永続cacheやアプリ全体cacheは追加しない。Function keyへの変更案、別の全文確認画面、PgUp/PgDn必須の操作案は採用しない。

基本設計の検索キー表・検索dialog説明・IMEの境界を、Plan明示承認後にこのPlanningで同じ方針へ整合させる。対象は検索関連箇所だけで、作成フォーム等のキー割当・製品責務・19 API・非対話CLI契約は変更しない。README／CONTRIBUTING／受入手順・テスト・fixtureの整合は新しいImplementationで行う。

#### キー操作と優先規則

通常検索dialogは開いた時にキーワードへfocusする。Tab順は **キーワード→状態→担当者→マイルストーン→キーワード**、Shift+Tabは逆順。横断検索はキーワードと担当者だけを循環し、状態・マイルストーンは利用不可とする。

| 状況 | キーと動作 |
| --- | --- |
| キーワード入力中 | 通常文字・貼付け・単行入力部品の編集キーを入力へ渡す。Ctrl+A/E/B/Fは移動、Ctrl+K/U/W/Hは部品の削除、Ctrl+Yは貼戻し。Ctrl+Uはキーワードだけを削除し、他の条件を変えない |
| キーワード欄の外 | Ctrl+K=キーワード、Ctrl+S=状態、Ctrl+A=担当者、Ctrl+G=マイルストーン。Ctrl+U=**キーワードを含む全条件クリア**。Ctrl+Fは何もしない。横断のCtrl+S/Gは何もしない |
| キーワード／候補popupが閉じている項目 | 修飾なしEnterで検索を適用。キーワードが空の通常検索も許可し、横断のkeywordまたは自分の必須条件は維持する |
| 候補項目へfocus | 候補popupを開く。popupを閉じた後はSpace／上下／j/kで再表示できる。これらの再表示操作を同時の選択変更として扱わない |
| 候補popup内 | 上下／j/kで候補を移動、Spaceで選択／解除、左右で選択行の長い名前を水平scroll。EnterまたはEscはpopupだけを閉じる。検索を同時に実行しない |
| 候補取得失敗中 | Enterはその項目の再取得だけを行い、検索しない。失敗を空の成功として扱わない |
| popupを閉じた候補項目 | 長い選択済みの値は同じ項目の一行表示を左右で水平scrollし、全文へ到達できるようにする |
| dialog全体 | 候補が閉じている時のEscは取消。Ctrl+Cによる中断130を常に最優先。適用処理中は編集・focus移動・クリア・重複適用を拒否し、Escによる適用取消は維持する |

一つのキーで二つの操作を起こさない。キーワード欄ではCtrl+S/G等を含め検索用Ctrlショートカットを実行せず、通常入力・編集キーをグローバル操作へ流さない。候補popupからTab／Shift+Tab／直接focusキーで移る時は、そのpopupを閉じて移動先だけを開く。Ctrl+Uは候補選択を全解除し、候補popupを閉じるがfocus項目は保持する。成功取得済み候補はクリアで捨てない。

適用・候補開閉・再試行のEnterは修飾なしPressだけを対象にし、Repeat／Releaseを同じ操作に使わない。ただし端末がPressとして送るキー反復を完全識別できるとは保証しない。既存の適用中ガードで重複要求を防ぐ。Alt+Enter／Ctrl+Enter等の別名操作は追加しない。40x12未満は従来どおり案内とq／Ctrl+Cによる安全な終了だけで、入力変更・候補操作・検索適用を受け付けない。

#### IME Enterの成立条件とPlanningでのProbe

Crossterm 0.29.0の [Event](https://docs.rs/crossterm/0.29.0/crossterm/event/enum.Event.html)／[KeyEvent](https://docs.rs/crossterm/0.29.0/crossterm/event/struct.KeyEvent.html) にIME composition情報はない。通常Enterと同じイベントで届くIME確定Enterをアプリだけで確実に識別することはできない。ローカルの固定依存ソースでもEventの種類とKeyEventのfieldを確認した。

Planningでproduction codeを変更せず、一時ディレクトリにraw ttyのCR/LF回数だけを数えるProbeを用意した。入力本文は表示・保存せず、ネットワーク接続なし、終了時にtermiosを復元する。先にPTYでEnter計数・入力非表示・端末復元をassert確認した。利用者は実端末で **「日本語変換の確定Enterでは0、次のEnterで1」** と回答した。これは当該時点の端末入力の観測で、製品dialogの受入や全端末の保証ではない。端末名・版・macOS・IME設定は今回の回答では未取得であり、推測して記録しない。

実装は端末が確定Enterを処理することに依存する最小方式とし、独自のIME状態推測・文字種判定・待ち時間による誤適用抑止・keyboard protocol独自実装は追加しない。製品で「IMEの変換確定だけでは検索せず、その後の通常Enterで一度だけ検索する」を実端末で必須確認する。変換候補の変更・取消・再変換、英数字入力後のEnterも確認する。誤検索した環境では受入失敗として未完了・停止し、別の操作方式等の判断へ戻る。全端末／IMEの完全一致や特定端末の製品必須化は追加しない。

#### 配置・長い値・候補scroll

- dialogの寸法は **幅=min(72,端末幅−2)、高さ=min(18,端末高−2)** とし、端末の中央へ配置する。100x24では72x18、40x12では38x10とする。
- 通常時は利用者の提示した画面案に従い、上段に「状態」と「担当者」、次段に「マイルストーン」、その下に枠付きの「キーワード」入力欄を置く。名前は **キーワード／状態／担当者／マイルストーン**。原画像のローカルパスを実装の必須参照にせず、本節の配置説明を正式な引継ぎとする。
- 狭幅では必要に応じて上段の項目を縦に並べ、focus項目・入力cursor・必要な操作案内を必ず見える範囲に収める。全項目の同時表示を40x12の必須条件にはしないが、Tab等ですべての利用可能項目へ到達できるようにする。resizeやpopup開閉で入力・選択値・キーワードcursorを失わない。
- 候補は枠付きpopupを**元のdialogに重ねる**。候補開閉によって状態・担当者・マイルストーン・キーワードの元の配置を押し下げない。popupはfocus項目の近傍に置き、全体が親dialog内に収まるよう補正する。寸法の上限は幅60列・高さ10行、親dialogの各寸法から2を引いたサイズ以下とする。40x12では候補popupは最大36x8となる。
- 候補popupは対象項目名、選択cursorとチェック状態、候補一覧、文脈に合う操作案内を表示する。40x12で背面の項目を一時的に覆ってよいが、対象項目・選択行・上下/j/k・Space・左右・閉じる操作を確認可能にする。キーワード時はTab移動・Enter検索・Esc取消、候補項目時は全条件クリアとキーワードへ戻る方法も確認できるよう、案内を領域内で短く配置する。無効なキーや入力中のq終了・Ctrl+F検索を案内しない。
- 候補は**一候補一行**。上下/j/kで選択行が今の表示範囲内にある間は縦scroll位置を保ち、上端・下端を越える時だけ最小限scrollする。常に選択行を先頭へ送らない。resizeで範囲外になった場合も選択が見える最小位置に補正する。
- 長い候補名と候補popupを閉じた項目の選択済み値は、左右キーの水平scrollで全文確認可能にする。選択済み値が複数なら全値を確認でき、概要の省略だけで値を隠さない。候補を移したらその行の水平位置は先頭へ戻し、選択印は水平scrollに埋もれさせない。キーワードの長文は既存入力部品のcursor追従水平scrollを使う。日本語・結合文字・emojiを表示列／grapheme境界で安全に扱い、文字途中で切らない。
- PgUp/PgDnを必須操作にせず、別の全文表示dialogや新しい表示モードを増やさない。選択済み値・候補名・案内を安全化してから描画し、見えない部分も含むsecret検査とキーワード全欄伏字を維持する。表示のために原文・ID・条件を変更しない。

#### 候補取得成功の再利用と寿命

- 成功候補は、project固定の同一dialog内で項目ごとに保持する。Tab周回・直接focus・popup再表示・全条件クリア後でも取得成功済みなら再利用し、追加GETをしない。**成功した空配列も取得成功**として再利用する。
- 初めて必要になった未取得項目だけを取得する。失敗は成功値として保存せず、失敗中Enterや、失敗項目へ再びfocusして開く操作で明示的に再取得する。取得中の同項目への再focusでは重複要求を発生させない。別項目へ移る／popupを閉じる時は旧要求を失効させ、後着結果を保存しない。戻った時に成功値がなければ改めて取得する。
- キーワード移動・popup閉鎖・クリア・再利用hit・取消・適用開始でも既存の候補busy／世代を正しく失効させる。古い成功・失敗は、別項目・別dialogの候補データ・エラー・新しいbusyを変更しない。新たな永続世代表や全画面共通cacheは作らない。
- dialogの取消または適用成功で候補を破棄する。適用失敗でdialogが残る場合は、そのdialog内の取得成功候補を保持してよい。再openや別projectのdialogへ持ち越さない。横断の担当者候補は既存の「自分」だけとし、通常projectの候補APIを要求しない。
- 設定由来の名前と候補IDを整合する既存処理は維持する。一意な名前だけをIDへ確定し、曖昧・不在なら元の検証エラーを維持する。既定statusをSpaceで解除した後に再挿入しない。

#### 横断検索の案内と維持する契約

横断結果の一覧に、横断検索中であることと「通常検索に戻る／作成先を決めるにはProjectへ移り、具体projectをEnterで確定」を明示する。40x12でも、Projectへ移る実際のキー（h/lまたはTab）とEnterが分かる案内を表示する。横断中の **c** は同じ手順を案内するだけで、作成フォームや書込みを始めない。**nは次ページのまま**で、作成キーへ変更しない。

具体projectのEnter後は通常検索へ戻るが、count/listの両方が成功するまで横断の結果・適用条件・履歴を保つ。失敗・取消で通常検索へ戻ったように表示しない。Project確定後のfocusは従来どおりProjectに維持する。通常検索で同一projectを再確定した時の条件・履歴保持、未確定highlightを検索対象にしない契約も維持する。

初回設定値と適用済み条件、クリア後の既定値非再挿入、編集中は検索APIを呼ばないこと、count/listの同条件・原子的交換、失敗／取消時の一覧・選択・履歴保持、横断のkeywordまたは自分の必須条件とローカル既定値非適用は変更しない。

#### 実装方針・影響範囲

1. 新しいImplementationセッションでIssue #17、改訂Plan・基本設計、未コミット／untrackedの現在の差分を確認する。過去のチェック・Review・Code Review Approveを変更後の差分へ流用しない。
2. `src/tui/search.rs` を中心に、編集優先dispatch・Enter優先・popupと水平／縦scroll・dialog寿命内の成功候補保持を最小変更で実装する。
3. `src/tui/browse.rs` の候補取得／busy失効・結果反映と横断案内、`src/tui.rs` の文脈別キー案内・必要なmodal dispatchを整合する。共有単行入力・Service/APIを再設計せず、既存の公開機能を利用する。
4. `tests/tui_process.rs`、必要な `src/tui.rs` のTestBackend試験、`tests/fixtures/tui_search.json` と `tui_search_steps.json` を新キー／表示／呼出数に整合する。README／CONTRIBUTINGの利用・再現手順と `docs/testing/manual-acceptance.md` の今後の受入手順・新しい実測記録を整合する。古い観測・hashは履歴として保持する。
5. 実装後に下記の自動・実端末・実Backlog受入を行い、未実施・失敗・環境制約を区別して記録する。後続の作成機能・依存更新・設定変更・配布公開は追加しない。

#### 完了条件

- 本節のキー優先・Tab順・Enterの役割・全条件クリアとキーワード単独削除の違いが実装され、入力中の編集キーを横取りしない。IME確定で検索しないことは製品の実端末受入でも確認済み。
- 指定寸法の中央dialog、画像由来の配置、日本語の4項目、重ね表示の候補popup、左右での全文確認、候補が表示端を越えた時だけの縦scrollを満たす。40x12でfocusと必要操作を確認でき、極小で操作が安全に拒否される。
- 成功候補と成功空配列のdialog内再利用、未取得／失敗後の再試行、閉鎖時破棄、古い応答とbusyの非破壊をAPI呼出数・表示から確認できる。
- 既存4修正と検索・履歴・失敗保持・横断制限・通常復帰が回帰せず、横断中の作成先project選択と通常復帰手順が画面から分かる。
- 関連試験、下記正式チェックと変更後release製品のローカル再現を再実行し、結果・警告・失敗・未確認事項を記録している。監査の終了0だけで全照会成功と判断しない。
- `https://lazybl.backlog.com/` に接続する変更後製品TUIで実候補・通常検索適用／クリア／結果・横断基本動作を確認し、実IME／キー衝突／表示を実端末で受け入れる。未実施・失敗ならIssue #17は未完了で、Issue #24へ先送りしない。複数project間の実サービス確認は既存環境制約により対象外とし、自動試験で確認する。

#### 動作確認方法

既存tdd Skillに従い、**実blの入力→状態／描画（TestBackend・PTY）とローカルHTTPSのHTTP送受信境界**で、失敗テスト→最小実装の縦sliceを行う。private fieldや関数形状、内部task数を固定するテストは追加しない。

- キー：Ctrl+A/E/B/F/K/U/W/H/Yの編集・貼戻し、通常文字、Tab／逆順、キーワード外の直接focusと全解除、Ctrl+F非適用、候補Enter閉鎖／失敗Enter再試行／閉鎖後Enter適用、修飾Enter・Repeat／Release非適用、適用中の編集／二重適用拒否・Ctrl+C=130と端末復元。クリア後のfocusと候補再利用、空keywordの通常検索と横断入力検証も確認する。
- 表示：100x24の72x18中央矩形、40x12の38x10、popupの親範囲内配置と背面項目位置の不変、日本語focusラベルと案内、長い日本語／結合文字／emoji／複数選択値の末尾への左右到達、候補選択印の保持。表示内j/kでは先頭行不変、下端／上端を越えた時だけ1行相当のscroll。resize／極小往復で選択・cursor・入力保持。
- 候補：同項目再focus／Tab周回／Esc再open／クリア後に追加GETゼロ、取得中の同項目重複要求なし、空成功再利用、403等の失敗→明示retry成功、閉鎖再open／別projectでは再取得。遅延成功・遅延失敗を逆転させ、再利用hit後・popup閉鎖後・別dialogの候補／エラー／busyが壊れないことを確認する。
- 検索回帰：初回既定値解除・重複status解除・曖昧／不在候補・未確定project検索禁止・project切替失敗保持・自分の表示／内部self・count/list同条件／各失敗・応答逆転・取消／再適用・ページ履歴／選択保持・横断制限／通常復帰・c案内・n次ページ。secret全欄伏字・貼付け正規化と原文保持も維持する。
- 実端末：変更後buildのhash、端末名／版・macOS・IME・利用するkeyboard設定、100x24／40x12／極小、各操作と結果を記録する。IME確定Enter非適用→次Enter適用、再変換／取消、長文水平scrollとpopup、標準編集・全条件クリア、通常検索復帰をユーザーと確認する。実IMEを自動PTYや今回のraw tty Probeで代替しない。
- 実Backlog：既存の段階的受入節のorigin確認・読取り範囲・機密を記録しない規則を維持する。障害・逆転・rate limit・複数projectはharnessで検証し、実サービスに意図的な障害や過大負荷を与えない。作成先案内の確認で書込みは不要。Plan／Issue承認を実Backlogへの書込み許可にしない。

```sh
cargo test --locked --test tui_process search_dialog_
cargo test --locked --test tui_process cross_project_dialog_
cargo test --locked --lib tui::tests::
cargo fmt --all -- --check
cargo test --locked --all-targets
cargo clippy --locked --all-targets -- -D warnings
cargo audit --file Cargo.lock
cargo build --locked --release --target aarch64-apple-darwin --bins
git diff --check
```

cargo-audit 0.22.2を維持し、lockfileを変更せず、DB／registryの取得失敗・timeout・警告も記録する。変更後release `bl` と更新済みローカルHTTPS／PTY再現fixtureでも表示・query・呼出数・GETのみ・secret非表示・端末復元を照合する。

#### 制約・対象外とPlanningの受渡し

新規依存、汎用入力framework、アプリ全体／永続cache、自動retry、設定変更、履歴／下書き永続化、IME推測、別の全文画面、他Issue機能、実Backlogへの書込み、merge／release／deployを追加しない。既存の4修正と過去の検証履歴を保持する。

改訂全文の明示Plan Review承認後、本ファイルと基本設計の合意済み検索箇所を正式保存・readbackする。taskflow-to-ticketに正式Planのパスと全文を渡し、**既存Issue #17の再利用を一つのWork Itemとする案**を別途確認する。最終本文・保存先と保存公開承認はPlan／分割承認と分ける。新規Issue・依存関係・labelやIssueの状態を独断で変更しない。正式PlanとWork Itemの保存公開後、新しいRootセッション向けの確定Issue参照による再開kickoffを提示して停止し、ここではImplementationを開始しない。


### Issue #17 再計画：Shift+H/Lによる左右領域のfocus復帰

#### 目的・正式入力・適用範囲

[Issue #17](https://github.com/minorunakamura/backlog-cli/issues/17) の追加要望に伴う `REPLAN_REQUIRED` を受け、メイン画面の左右領域をShift+L／Shift+Hで移動できるようにする。正式Planは本ファイルの改訂全文、基本設計は [docs/designs/backlog-cli.md](../designs/backlog-cli.md)「TUIの画面・操作」の「配置とフォーカス」。本節は従来の左右内部h/l・Enter/Escだけの契約を拡張し、新キー・focus記憶について優先する。直前の検索dialog再計画・実Backlog受入条件と過去の観測は維持する。

#### 確認済みの現状と停止理由

- `src/tui.rs` の公開 `Shell::key` はProject↔Issue／Detail↔Commentのh/l・Tab/Shift+Tab、IssueのEnter→Detail、右側のEsc→Issueを処理するが、Shift+H/Lは未実装。検索成功時は端末loopが `browse.focus_issue` を受けてIssueへ移す。
- dialogは通常キーより先にdispatchされ、キーワードは既存 `SingleLineInput` へ渡る。極小は40x12未満でq／Ctrl+Cだけを受け付ける。エラー通知はmodalではなく、修飾なしEnter/Esc/Spaceは通知だけを閉じ、h/l等は通知を消して実行する。
- 開始時に `CONTRIBUTING.md`, `README.md`, `docs/designs/backlog-cli.md`, 本Plan, `docs/testing/manual-acceptance.md`, `src/tui.rs`, `src/tui/browse.rs`, `tests/tui_process.rs` が変更済み、`src/tui/search.rs`, `tests/fixtures/tui_search.json`, `tests/fixtures/tui_search_steps.json` がuntracked。これらを保持する。
- `docs/testing/manual-acceptance.md`「#17 summary修正後のRoot検証・受入と追加キー要望（2026-10-29）」に変更前buildのチェック・実端末／実Backlog確認と新キー未承認による停止が記録されている。今回のPlanningでは既存 `cargo test --locked --test tui_shell` が5件成功、`git diff --check` が終了0。新キーの実装・受入は未実施で、過去の結果・Review・Code Review Approveを変更後差分へ流用しない。

#### 合意した設計変更・要件

利用者は固定移動先ではなく、左右それぞれで最後にfocusしたパネルへの復帰を選択した。方向・記憶の規則・有効範囲・既存操作との併存・通知中の動作もRootの質問への回答で確認済み。必要な最小のメモリ内focus記憶だけを追加する。

| 現在のfocus | Shift+L | Shift+H |
| --- | --- | --- |
| ProjectまたはIssue（左） | 右の最終focusへ。右未訪問時はDetail | 現在のfocusを維持 |
| DetailまたはComment（右） | 現在のfocusを維持 | 左の最終focusへ。初期値はProject |

- 起動時のfocusと左の記憶はProject、右の記憶はDetail。h/l・Tab/Shift+Tab・Enter/Esc・検索適用成功など、実際に確定したメイン画面のfocus移動を記憶へ反映する。dialog内の項目focusは対象外。取消／適用失敗は背面focusや記憶を変えず、検索成功は従来どおりIssueへ移り、左の記憶もIssueとなる。
- 例：Project→Shift+L→Detail→l→Comment→Shift+H→Project→Shift+L→Comment。Comment→EscはIssueへ戻り、次のShift+LはCommentへ戻る。Issue→Enterは記憶がCommentでもDetailへ移り、右の記憶をDetailにする。
- 記憶はShellの生存中だけ保持し、resize・project切替・通常／横断検索だけではリセットしない。実際のfocus変更だけを反映し、再起動で初期化する。設定file・永続履歴・復旧処理は追加しない。
- h/l・Tab/Shift+Tabの左右内部切替、IssueのEnter→Detail、右側のEsc→Issueと既存の読取り取消を維持する。新キーはfocusだけを変え、選択project／課題・ページ履歴・本文／scroll・適用済み検索条件を変更しない。空一覧・右側データ未取得でも移動できる。
- 通信中も新キーを使える。新キー自体ではAPI要求・再取得・読取り取消・busy／世代の失効を起こさず、取得は継続して既存の世代判定で反映する。既に開始・予約されているpreviewは継続してよいが、移動だけを原因とした追加GETは発生させない。
- 有効範囲は40x12以上の通常メイン画面。全パネル表示と単一パネル表示で同じ規則とする。入力中のH/Lは通常文字として扱い、dialog／候補popup／適用中は背面focusを変更しない。40x12未満は新キーを無視し、キーによるfocus・記憶変更をしない。q／Ctrl+C・入力／dialog優先を維持し、editor中はeditorに委ねる。将来のフォーム・editor機能を本Issueで実装しない。
- エラー通知中の有効なShift+H/Lは通知を消して移動する。同じ側へのキーでも通知は消すがfocusは維持する。入力dialogとは区別する。
- 新キーは大文字 `KeyCode::Char('H'/'L')` のPress、modifierがNONEまたはSHIFTのものを対象にする。小文字h/lを新操作へ読み替えない。Ctrl/Alt等が重なる入力、Repeat／Releaseでは新移動・記憶変更・通知消去をしない。端末がPressとして送る反復やCaps Lock由来の大文字を区別できるとは保証せず、独自keyboard protocol・キー設定は追加しない。既存キー全体のRepeat処理は変更しない。

#### 実装方針・影響範囲

1. 別の新しいImplementationでIssue #17本文・全改訂コメント、本Plan、基本設計、現在の未コミット／untracked差分と受入履歴を直接確認する。commit / stash / 破棄を独断で行わない。
2. `src/tui.rs` の既存Shellに左右の最終focusを表す最小の状態を追加し、既存キーと検索成功後のfocus設定を含む全経路で整合させる。dialog優先・極小保護・通知処理と衝突しない位置で新キーを処理し、Browseへのfallthroughによる通信・選択変更を防ぐ。汎用navigation framework・keymap・Service/APIや共有入力の再設計は不要。
3. 同ファイルの文脈別キー案内にShift+L:右／Shift+H:左と領域内切替の違いを反映する。40x12でも移動・既存Enter/Esc・安全な終了が確認できる短い案内にし、必要最小限の案内配置調整だけを許容する。メインパネル配置は再設計せず、dialog中に背面キーを有効と案内しない。
4. `tests/tui_shell.rs` の公開Shell→TestBackend、`tests/tui_process.rs` の実bl→PTY／HTTPS、必要な `src/tui.rs` の入力／通知試験を拡張する。既存 `tests/fixtures/tty_shell.py` と検索／詳細fixtureを再利用し、必要なstep・期待値だけを整合する。
5. README／CONTRIBUTINGの操作・再現手順と `docs/testing/manual-acceptance.md` の今後の受入手順・変更後実測を整合する。直前の検索dialog再計画の残作業・既存受入条件を維持し、以前の観測・hash・停止記録を残す。

#### 完了条件

- 移動表、初回値、全focus経路の記憶、既存Enter/Escとの違いを入力→focus枠で確認できる。同じ側へのキーでtoggleせず、project／検索変更・resizeでも記憶の契約を満たす。
- focus移動だけでは選択・一覧・条件・履歴・本文scroll・進行中通信を変更せず、追加GETや取消を起こさない。空状態・初期読取り中・Detail再取得中でも安全に操作できる。
- H/L文字入力、dialog／popup／適用中の背面保護、極小復帰、拒否modifier／Repeat／Release、通知消去と移動を検証済み。100x24／40x12でfocusと有効な案内を確認できる。
- 直前の検索dialog再計画の要件・正式チェック・変更後release製品の確認を維持する。新buildで実端末のShift+H/L・入力衝突と既存の実Backlog／IME受入も確認し、過去buildの確認だけで代替しない。未実施・失敗ならIssue #17は未完了。複数projectの実サービス確認の既存環境制約を維持する。
- 基本設計・Plan・Issueコメント・利用説明・テスト・受入手順が整合し、変更後の結果・失敗・未確認事項を記録する。後続Issueの機能やCommentの実データ閲覧を今回完成したとは報告しない。

#### 動作確認方法

既存 `tdd` Skillに従い、新キーの失敗テスト→最小実装の縦sliceを進める。公開 `Shell::new/key/draw` → Ratatui TestBackendのfocus枠／表示、および実bl→既存PTY／ローカルHTTPSのHTTP送受信を境界とする。private field・補助関数の形・task数を固定しない。

- 初回値と全移動元、左右内移動後の復帰、同じ側no-op、Enter／Escによる記憶更新、検索成功後の左記憶更新、project切替・通常／横断検索後の復帰を確認する。
- H/LのNONE／SHIFT Pressと、Ctrl／Alt・Repeat／Release・極小時のfocus／記憶／通知不変を確認する。入力中は文字として残り、popup／適用中から背面へ漏れない。Ctrl+C=130と端末復元を維持する。
- 取得完了後の左右往復でGET増加ゼロ、Detailの遅延再取得中の往復で取消・追加要求がなく同じ応答が反映されることを描画・HTTP記録で確認する。既存Esc取消も維持する。本文scroll・ページ選択・条件保持、通知中の移動／同じ側no-op、40x12／100x24／極小往復・NO_COLORを確認する。

```sh
cargo test --locked --test tui_shell
cargo test --locked --test tui_process
cargo test --locked --lib tui::tests::
cargo fmt --all -- --check
cargo test --locked --all-targets
cargo clippy --locked --all-targets -- -D warnings
cargo audit --file Cargo.lock
cargo build --locked --release --target aarch64-apple-darwin --bins
git diff --check
```

cargo-audit 0.22.2と既存正式コマンドを維持し、DB／registry失敗・timeout・警告も記録する。変更後releaseのローカルHTTPS／PTYで新キーと検索のquery／呼出数・GETのみ・secret非表示・端末復元を再確認する。実端末ではbuild hash・端末名／版・macOS・IME／keyboard設定、100x24／40x12／極小、各パネル復帰・Enter/Esc・H/L入力・IME確定Enterの結果を既存受入記録へ残す。`https://lazybl.backlog.com/` のoriginを確認して既存の検索受入と新focus操作を確認し、API keyや機密本文を記録しない。

#### 制約・設計更新・Planningの受渡し

本Plan改訂全文の明示承認後、同じ正式パスへ保存する。利用者確認済みの範囲として、このPlanningで基本設計「配置とフォーカス」のキー表にShift+H/Lの方向・最終focus復帰を追加し、その直後に初回値・記憶の寿命／更新・通信非取消・入力／dialog／極小時の無効化・通知時の扱いを整合する。他の設計・検索契約は変更しない。README等と製品・テスト・受入記録の更新は新しいImplementationで行う。

新規依存・設定・永続状態・汎用keymap・他Issue機能・実Backlog書込み・push／merge／release／deployは追加しない。既存未コミット／untracked・部分実装と履歴を保持する。

正式保存・readback後、`taskflow-to-ticket` に本Planの正式パスと全文を渡し、既存Issue #17一件を再利用して検索の残作業と今回のfocus操作を一つの縦sliceとして実装・検証する分割案を別途確認する。Plan・tracker設定・分割・最終本文／保存先の公開承認を流用しない。既存タイトル・本文・OPEN状態・label・Blocked by／native関係は変更せず、明示承認された改訂コメントを追記する。保存公開・readback後、確定した参照で新しいRoot向けImplementation再開kickoffを提示して停止し、Implementationは開始しない。


### Issue #18 再計画：Comment表示・TUI日時・詳細のinline差分

#### 目的・正式入力・適用範囲

[Issue #18](https://github.com/minorunakamura/backlog-cli/issues/18) の `REPLAN_REQUIRED` を受け、Commentの読みやすさを改善し、TUI全体の日時をJSTへ統一する。正式Planは本ファイルの改訂全文、基本設計は [docs/designs/backlog-cli.md](../designs/backlog-cli.md)「配置とフォーカス」「課題の項目」「課題コメント」「本文変換・表示の安全性」。今回の実装・検証には本節を適用する。旧来の種別文字ラベル必須・末尾metadata・本文保持の一般則と衝突する表示要求は、以下で明示した範囲だけ本節を優先する。初版全体・後続Issueの機能は取り込まない。

#### 確認済みの現状・停止理由・根拠

- Planning開始時、`CONTRIBUTING.md`, `README.md`, `docs/testing/manual-acceptance.md`, `src/bin/bl-cli.rs`, `src/lib.rs`, `src/service.rs`, `src/tui.rs`, `src/tui/browse.rs`, `src/tui/detail.rs`, `tests/fixtures/tty_shell.py`, `tests/fixtures/tui_detail.json`, `tests/fixtures/tui_search.json`, `tests/service.rs`, `tests/tui_process.rs` が変更済み。`src/comment.rs`, `src/tui/comment.rs`, `tests/fixtures/tui_comment.json`, `tests/fixtures/tui_comment_steps.json` がuntracked。部分実装・差分を保持し、commit / stash / 破棄を独断で行わない。
- `src/comment.rs::preview` は種別・ID・投稿者・日時を一つのheadingへ連結し、BODY／CHANGESと末尾metadataを生成する。履歴は一律に前後値を `->` で並べる。`display` はCLI固定出力の入口なので、TUI変更をCLIへ波及させない。
- `src/tui/comment.rs` はページ内選択とID境界、`src/tui/browse.rs` は20件ページと世代・busyを管理する。本文なし履歴だけなら記法確認GETをしない。`src/tui/detail.rs` はplain文書のwrap・scroll・resize補正を持つ。取得・選択の境界は維持する。
- `src/tui.rs` は100×24で右領域20行をDetail 16行／Comment 4行へ分け、描画とscrollの領域を別箇所で算出する。`tests/tui_shell.rs` の16／4期待も改訂対象。新しい固定3行と本文には外枠最低6行が必要である。
- default profileのoriginが `https://lazybl.backlog.com/` であることを確認し、既存releaseの `bl-cli --profile default --json issue comment view BLCLI-1 <id>` で読取りのみを行った。`839099888` は `content:null`, `changeLog[].field:"description"`, 旧値72 Unicode scalar→新値空文字。`839099889` は同fieldで空文字→72 scalar。createdは順に `2026-10-26T16:05:08Z`／`2026-10-26T16:05:10Z`。本文・API key・raw responseは記録せず、書込みなし。この観測はこの2件だけのもので、製品TUIの受入・任意のAPI値の完全性を保証しない。
- [コメント一覧API](https://developer.nulab.com/ja/docs/backlog/api/2/get-comment-list/) と [コメント単件API](https://developer.nulab.com/ja/docs/backlog/api/2/get-comment/) の公開資料では、10,000文字制限・超過判定／切り詰めフラグ・description旧新値の常時全文返却は確認できない。Web画像の制限をAPI契約と同一視しない。
- `docs/testing/manual-acceptance.md` のIssue #18記録には264テスト成功、fmt／Clippy／audit／release build、独立Reviewと指摘修正の履歴があるが、実端末・実Backlog受入は未完了。過去のチェック・Review・Code Review Approveを変更後差分へ流用しない。Planningではproduction codeを変更せず、`git diff --check` の終了0のみ確認した。

#### 合意した設計変更と理由

Rootの質問で利用者が以下を確認し、仕様整理の認識一致も確認した。基本設計の表示契約の改訂として扱う。

1. **日時の範囲**：CommentだけでなくTUI全体のAPI作成・更新日時をJSTへ統一する。現在はComment createdとIssue Detail created／updatedが対象。開始日・期限日等の日付専用値、本文中の日付文字列、CLI固定出力、JSONは変更しない。
2. **配置**：当初の枠上右揃えに代えて、枠内左揃えの「投稿者→日時→空行→本文」を採用する。先頭3行を固定し、n/pで日時位置を揺らさない。本文を初期表示するためComment外枠を最低6行に広げる。
3. **ラベル廃止**：COMMENT／CHANGE HISTORYと表示ID、BODY／CHANGES、末尾のID／TYPE／AUTHOR／CREATED／UPDATEDをTUIのCommentから削除する。「コメント」「変更履歴」の種別文字ラベル必須を廃止し、本文と変更項目名付き履歴の構造で区別する。
4. **詳細の差分**：`description` を「詳細」と表示し、行＋grapheme単位のinline差分を導入する。通常時は削除を赤系背景＋取り消し線、追加を緑系背景。NO_COLORの場合だけ文字記号を付ける。
5. **10,000文字超の例外**：前後の合計が10,000超のdescription文字列ペアは、画像どおり「詳細」と警告だけにする。前後本文や隠れたscroll用コピーを表示文書に入れない。TUIで受信内容を全表示する従来要件への限定例外である。警告付き全文表示案は利用者が採用しなかった。APIデータ・CLI・JSON・通常コメント・他の変更項目には広げない。
6. **設計更新の時期**：Plan全文の明示承認後、このPlanningで基本設計「配置とフォーカス」のComment最低高さ・TUI日時、「課題コメント」のラベル・縦配置・差分・超過例外、「本文変換・表示の安全性」の例外参照を整合する。他の設計方針は変更しない。README／CONTRIBUTING／今後の受入手順・製品・テストは新しいImplementationで整合し、過去の実測は保持する。新しいGLOSSARY・ADR・状態管理文書は作らない。

#### 表示契約

**日時**

- APIのoffset付き日時を固定UTC+09:00へ変換し、`yyyy/MM/dd HH:mm:ss` の19列にする。OSのtimezone・現在時刻に依存せず、小数秒は表示しない。Commentはcreatedを使い、updatedを追加しない。Issue DetailのCREATED／UPDATEDの項目自体は維持する。
- RFC3339として解釈できない値をUTC深夜等と推測して補完しない。安全な原値と変換不可表示を残し、panic・架空日時を出さない。Commentの固定行に収まらない原値は、日時行に変換不可を案内し、原値全文をscroll領域へ置く。Serviceの文字列validationやCLI／JSONを、この変更のためだけに厳格化しない。
- 実APIを模した日時fixtureはoffset付きへ整合し、date-onlyや不正値は明示的なfallback試験に残す。

**Commentの枠・固定行・本文**

- 枠はCommentというパネル名・focus印・必要な取得状態／通知を維持し、種別・IDを出さない。内部IDは取得境界・選択・将来の編集対象識別に保持する。
- 枠内先頭を投稿者1行、created日時1行、空行1行の固定領域とし、左揃えにする。投稿者は太字＋色、NO_COLORでは太字のみ。n/p/g/G・Rによる対象変更でも、同一サイズでは日時の行・開始列を固定する。本文scrollで先頭3行を動かさない。
- 長い投稿者は安全化後にterminal表示列・grapheme境界で末尾を `…` にする。省略時だけscroll領域先頭に安全な全文を折返し表示する。resizeで省略要否を再判定し、省略していないときは重複させない。名前の改行・TABは固定行向けに単行化し、全文確認側では安全な内容を保持する。
- 全パネル表示では従来の右側80／20分割を基本とし、Comment外枠を最低6行にclampして残りをDetailにする。100×24では14／6、Comment本文は最低1行。左側・Workspace・全／単一パネル境界・極小時保護は維持する。
- 固定3行を除いた本文の**同じRect**を描画・折返し行数・scroll上限・page移動に用い、resize時にclampする。
- 通常コメントは既存の記法判定・安全表示で見出しなしに表示する。同居する履歴は空行で区切り、同じ1件内で両方を表示する。履歴は変更項目名を残す。description以外は既存の既知field表示名と前後値を維持し、未知fieldも識別子と安全なJSON前後値を落とさない。API本文中の「BODY」等の文字を削除するのではない。

**詳細のinline差分・超過時**

- `changeLog[].field == "description"` かつ旧／新値が両方文字列の場合に適用する。コメントのcontentを前後値に代用しない。null・数値・配列・object等は空文字と同一視せず、「詳細」と型を保持した安全な前後値表示へfallbackする。
- 元API文字列それぞれの `chars().count()`（Unicode scalar数、改行・空白込み）の合計が **10,000超** の場合だけ、次の表示にする。ちょうど10,000は比較対象。byte／grapheme数や安全化・伏字化後の長さでは判定しない。これはTUI独自基準で、Backlog Webの算定方法・API制約とは同一だと説明しない。

  ```text
  詳細
  変更内容が10,000文字を超えるため、差分を表示できません。
  ```

  当該履歴の前後値を警告の後やscroll先へ含めない。通常時は警告色、NO_COLORでも同じ文言で判別可能にする。同居コメント本文・他の履歴・投稿者／日時・移動・Rは隠さない。自動展開・別画面・追加GETは作らない。
- 合計10,000以下では各値全文を安全化した**表示用原文**で行比較し、対応する変更行内をgrapheme単位で比較する。Markdown／Backlog記法変換後の比較にせず、descriptionだけの履歴のための記法確認GETを追加しない。
- 共通部分も省略せず、削除／追加区間を原順にinline表示する。空白・TAB・改行・末尾改行の有無を黙って捨てない。削除は可読な前景＋赤系背景＋取り消し線、追加は可読な前景＋緑系背景をRatatui styleで付け、外部ANSIを実行しない。
- NO_COLORでは色装飾をなくし、削除を `[-削除内容-]`、追加を `{+追加内容+}` にする。これらの記号は通常の色あり表示には付けない。
- 空への変更は「詳細（空へ変更）」＋削除区間、空から追加は「詳細（空から追加）」＋追加区間。同値は「詳細（変更なし）」＋安全な値、両方空文字は `（空）`。注記をAPI値へ混ぜない。
- 近似budgetは1履歴50msを目安とし、行とgrapheme比較で同じdeadlineを共有する。similarのdeadlineは近似への切替であり、厳密な時間・メモリ上限や超過検出フラグではない。細粒度比較を続けられない範囲は削除／追加の塊として全文を保持する。10,000以下で警告だけへ落とす例外は追加しない。ページ分の比較はUI task内で同期実行せず、既存取得task側で準備する。

#### 実装方針・影響範囲

1. 新しいImplementationでIssue本文と全改訂コメント、本Plan・基本設計、現在の差分・untracked・受入履歴を直接確認し、部分実装を保持して続行する。
2. `src/comment.rs` のTUI previewと `src/tui/comment.rs` に必要な最小限のstyled文書を持たせ、固定作者／日時をscroll本文から分離する。既存RatatuiのText／Line／Span・Paragraph／Wrapを再利用し、ANSI文字列・汎用文書framework・別表示モードは作らない。`comment::display` のCLI契約は維持する。
3. `src/tui.rs` の右分割・固定行・body Rect・scroll計算、`src/tui/detail.rs` の必要なstyled文書対応と日時、`src/tui/browse.rs` のpreview受渡しを整合する。shared文書の変更でDetail本文・scrollを回帰させない。page・API順序・世代管理は再設計しない。
4. 手書きの暦／Unicode差分エンジンを避け、`time = "=0.3.55"`（`default-features=false`, `parsing`／`formatting`）と `similar = "=2.7.0"`（`unicode`）を直接依存にする。time 0.3.55は既存lockfileにあり、local sourceのMSRV 1.88.0とparse／checked_to_offset／formatを確認済み。similarの [版固定API](https://docs.rs/similar/2.7.0/similar/struct.TextDiffConfig.html) に行／grapheme比較・deadlineがある。Rust 1.99.1・完全一致版・lockfile追跡を維持する。timezone DB・新規local-offset要求・macros・日時設定は不要。既存推移依存のfeatureを無断で削らない。
5. TUI向け日時変換をComment createdとDetail created／updatedから共用し、API／JSON／送信値は原値のままにする。依存解決は上記追加に必要な差分だけとし、無関係な一括update・lockfile再生成をしない。同時ビルドと解決結果はPlanningでは未検証なのでImplementationで確認する。必要な推移依存・feature以外の変更や指定版変更が必要なら再計画する。
6. `safety::text/json` で前後値全文を安全化してから分割し、Span単位だけの伏字化に置き換えない。さらにlabel・差分区間・NO_COLOR記号・投稿者省略等を組み立てた**最終テキスト投影**も検査する。secret `abcd` に対して旧 `ab`／新 `cd` を隣接表示すると、単独値が安全でもsecretを再構成し得る。該当結果は安全なplain表示へのfallback・redactionを許容し、危険なstyled片を残さない。内容・装飾より秘匿を優先し、両色モードを検証する。
7. `tests/tui_shell.rs`, `tests/tui_process.rs`, `src/tui.rs` のTestBackend試験、必要な公開preview試験、comment／関連日時fixtureを整合する。`tests/issue_comment.rs` 等でCLI／JSON不変を検証する。README／CONTRIBUTINGの利用・再現説明と `docs/testing/manual-acceptance.md` の今後の手順・変更後実測を整合し、旧観測・hash・停止理由を保持する。表示のためだけにServiceや書込みAPIを変更しない。

#### 完了条件

- TUI作成・更新日時がJST書式で、Commentはcreatedを使う。日付専用値・CLI／JSON・原データを変えず、不正日時にpanic・架空値を出さない。
- 投稿者・日時・空行の固定配置と不要表示廃止、100×24の14／6・40×12・極小保護、長い名前・resize・scroll・NO_COLORを満たす。
- descriptionの行＋grapheme差分、背景／削除取り消し線、NO_COLORだけの記号、全削除／全追加・同値・10,000／10,001境界が仕様どおり。超過時は警告のみで前後本文を隠れたscroll先にも置かず、他の内容へ例外を広げない。
- 通常本文・同居履歴・未知field・非文字列／null・空白／改行・日本語／結合文字／emojiを保持し、安全表示と組立前後のsecret検査を満たす。
- n/p/g/G、exclusive ID両方向境界・巨大ID・最古の直接昇順取得、focus・本文scroll、R成功時だけ最新へ交換、失敗・取消・古い応答で選択／本文／scroll／busyを壊さない。全件取得や表示のための追加GETを増やさない。
- 変更後の関連試験・正式チェック・release製品のローカルHTTPS／PTY・実端末／実Backlog受入を記録済み。未実施・失敗ならIssue #18は未完了。過去の264成功・Review・Code Review Approveで代替しない。
- 基本設計・Plan・Issue受入条件・利用説明・テスト・今後の受入手順が整合し、後続Issueや初版全体の完了と混同しない。

#### 動作確認方法

既存 `tdd` Skillの失敗テスト→最小実装の縦sliceを使う。公開seamは、共通libのpreview／安全表示入口（差分の意味・原値不変、実端末の見え方は捕捉しない）、公開Shellの入力→TestBackendと実bl→PTY（配置・cell style・操作・端末復元、OSの描画差は捕捉しない）、既存HTTPSのHTTP境界（GET・paging・状態保持、実API全データ保証ではない）、実CLIのstdout／JSON／終了コード（既存契約非変更）とする。private構造・task数固定試験や新harnessは不要。

- 日時：UTC／非UTC offset、JSTの日／月／年跨ぎ・閏日・小数秒、不正・date-only・変換範囲外を既知の期待値で試す。対象2件は `2026/10/27 01:05:08`／`2026/10/27 01:05:10`。日付専用値・CLI／JSON不変も確認する。
- 配置：100×24の枠座標と本文1行以上、40×12、極小往復、n/pと本文scroll中の固定行座標、長い日本語・結合文字・emoji名の省略と全文到達、resizeでの再判定・clamp。投稿者太字／色とNO_COLORの全cell fg／bg Resetを確認する。
- 差分：途中の挿入／削除／置換・複数行・繰返し・空白／TAB／改行のみ・末尾改行、日本語・結合文字・ZWJ emoji、空への変更／空から追加・同値、非文字列／null・未知field／巨大数値・本文との同居を確認する。近似時の分割形状を固定せず、10,000以下で安全な前後値を再構成できる内容保持をassertする。
- 上限：合計9,999／10,000／10,001、多byte文字・結合文字・改行、伏字化前カウント。超過値のsentinelが画面・scroll先に出ず、同居本文・他の履歴は消えない。通常コメントやCLI／JSONは10,000超でもこの例外を適用しない。
- 安全性：全文のAPI key、Span境界、旧 `ab`／新 `cd` による `abcd` 再構成、HTML／ESC／OSC／制御文字を両色モードの描画cellまで確認する。原データ・送信値は不変。
- 結合回帰：既存comment fixture／stepsで移動順・両境界・最古GETの呼出数、g/Gと本文scroll、R成功／失敗・cancel・課題切替後応答逆転を確認する。背景・取り消し線はPTYのplain screenだけで合格にせず、TestBackendのcell styleをassertする。
- 実端末：変更後release hash、端末名／版・macOS・NO_COLOR有無、100×24／40×12／極小で、固定行・JST・空行・本文・差分／警告・折返し／末尾scroll・n/p/g/G/Rをユーザー確認する。赤背景＋取り消し線／緑背景の可読性とNO_COLOR記号を目視し、未対応端末の装飾を自動PTYで保証しない。
- 実Backlog：profile origin `https://lazybl.backlog.com/` を確認し、変更後TUIでBLCLI-1の `839099888`／`839099889` の全削除／全追加・JSTを確認する。他の実コメント・本文なし履歴・前後／最新／最古／Rも従来どおり確認する。10,000超・部分差分・未知fieldはharnessで検証でき、データ準備の書込みを要求しない。APIにない前後値を現在本文から推測補完しない。
- 記録先は既存 `docs/testing/manual-acceptance.md`。Issue・build・origin・対象ID・手順・結果・未確認事項を残し、key・機密本文・raw responseを記録しない。準備／変更／復元は別途明示許可なしに行わず、実サービスに障害・過大負荷を発生させない。

```sh
cargo test --locked --test tui_shell
cargo test --locked --test tui_process
cargo test --locked --test issue_comment
cargo test --locked --lib
cargo fmt --all -- --check
cargo test --locked --all-targets
cargo clippy --locked --all-targets -- -D warnings
cargo audit --file Cargo.lock
cargo build --locked --release --target aarch64-apple-darwin --bins
git diff --check
```

cargo-audit 0.22.2と正式コマンドを維持する。新依存・必要なfeature解決後に監査し、監査前後のlockfile不変、DB／registry取得失敗・timeout・警告・終了コードを記録する。変更後releaseと更新済みHTTPS／PTYでGETのみ・追加通信なし・secret非表示・端末復元を確認する。実端末／実Backlog受入をharness成功やPlanningのAPI読取りで代替しない。

#### 制約・対象外・Planningの受渡し

新しい設定・永続状態・履歴管理・別画面・avatar／スター／メニュー等のBacklog画面全体再現・差分永続cache・外部pager・独自diff／日時エンジン・他Issueのc/e／書込みフォーム／editor・実Backlog書込み・push／merge／release／deployを追加しない。CLI／JSON契約・API取得範囲・書込み確度・既存キーは維持する。TUI超過例外をAPIデータの破棄へ転用しない。

受信した値の表示が対象であり、任意の長さでAPI旧新値が常に全文取得できることやWebとの完全一致は保証しない。既存Paragraphのu16 scroll上限（65,535 wrapped行）等の既知の限界を無制限対応済みと説明しない。通常の長文・上限境界fixtureの末尾到達を確認する。端末の文字幅・取り消し線差、近似差分の分割差は実端末受入と内容保持試験で扱う。

Plan全文をPlannotatorへ提出し、明示承認後に同じ正式パスへ保存・readback、合意した基本設計の箇所を整合する。正式Planのパスと全文を `taskflow-to-ticket` へ渡す。既存Issue #18一件を再利用して残作業と今回の改訂を実装・検証する分割案を別途確認し、最終改訂コメント本文・公開先の承認も別に得る。新規Issue・label・依存関係を増やさず、元のIssueタイトル・本文・状態・Blocked by／native関係と既存実Backlog受入コメントを維持する。正式保存・公開とreadback後、確定参照で新しいRoot向けImplementation再開kickoffを提示して停止し、Implementationは開始しない。


### Issue #18 再計画：削除差分の視認性・未設定表示・Comment枠

#### 目的・正式入力・優先関係

[Issue #18](https://github.com/minorunakamura/backlog-cli/issues/18) の追加の `REPLAN_REQUIRED` に対応する。正式Planは本ファイルの改訂全文。直前の「Issue #18 再計画：Comment表示・TUI日時・詳細のinline差分」、基本設計 [課題コメント](../designs/backlog-cli.md#課題コメント)、Issue本文・既存の実Backlog受入コメント・既存の表示改訂コメントを正式入力とする。

今回変更するのは、削除差分の配色、TUI変更履歴のnull表示、Comment枠の区切りの3点だけ。本節は既存の「非descriptionの前後値表示維持」「非文字列／nullの表示」のうち、下記で明示するnullの表示だけを置き換える。従来の赤系背景＋取り消し線、緑系背景、NO_COLOR、JST、固定行、10,000文字超の限定例外、取得・paging・安全表示・実環境受入等は維持する。過去の調査・実測は当時の履歴であり書き換えない。

#### 停止理由・確認済みの原因と根拠

- 利用者の実端末観測：削除差分の赤背景と文字のコントラストが低く読めない。報告された見え方をRootが同じ実端末で再観測したものではない。現行 `src/comment.rs::description` は削除を `Color::White` 前景＋`Color::Red` 背景＋`CROSSED_OUT` に固定する。基本色は端末paletteの影響を受けるため、現在のcell style確認だけでは可読性を保証できない。
- Rootは既存 `description_empty_equal_and_typed_fallbacks_keep_notes_outside_api_values` と `tui::tests::comment_fixed_rows_styles_and_body_scroll_share_the_viewport` を完全名で各1件実行し、各終了0を確認した。後者はWhite／Redと取り消し線を現にassertしている。これは旧表示の根拠であり、改善後の可読性の受入ではない。
- `src/comment.rs::preview` は非descriptionのnullをJSONの `to_string()` により `null` と表示する。descriptionの非文字列fallbackも同様。上記公開preview試験は `詳細: null -> ""` と `詳細: "null" -> null` を現在の期待値にしている。文字列同士のdescriptionだけはinline比較へ進む。CLIは別入口 `comment::display`／`content` であり、TUIだけの修正が可能。
- `src/tui.rs::Shell::panel` は `Comment | {comment_status()}` を無条件に生成する。`src/tui/browse.rs::comment_status` はbusyもnoticeもない場合に空白を返すため、通常時も不要な `|` が残る。状態の取得失敗ではなくtitleの組立が原因。
- 同じ `src/comment.rs` 内で、NO_COLORの削除記号生成と色あり変更改行の `↵` 可視化も背景Red／Greenを判定している。削除styleだけの変更は、削除記号・末尾改行の方向表示を壊す。既存のUnicode内容再構成試験とTestBackend cell試験もこの色に依存するため、同時に整合させる。
- 既存の `docs/testing/manual-acceptance.md`「#18 改訂・指摘修正後のRoot再検証」には281成功等と実端末／実Backlog受入未完了が記録されている。今回のPlanningでは本番APIに接続せず、製品・テスト・fixture・設定・依存を編集していない。過去の成功・Review・Code Review Approveを変更後の承認へ流用しない。

#### 合意した変更・要件

Rootの質問に対し、利用者は以下の範囲と基本設計の更新時期を確認した。「値が丸ごとnull」とobject／arrayの内部のnullの違いは具体例を示して確認済み。

1. **削除差分の視認性**：赤系背景＋取り消し線という既存要件を維持して高コントラスト化する。実装の採用配色は `Color::Rgb(255,255,255)`（#FFFFFF）前景＋`Color::Rgb(128,0,0)`（#800000）背景＋`CROSSED_OUT`。端末基本色名ではなく明示RGBを使い、追加側のBlack／Greenと通常文字は今回変更しない。Planningの小さな数値計算では、このRGBペアのsRGB相対輝度によるcontrast ratioは約10.95:1だった。数値は選定根拠にとどめ、取り消し線・実端末の色再現・実際の可読性の保証にしない。NO_COLORでは従来の色なし `[-削除-]`／`{+追加+}` を維持する。
2. **null表示の範囲**：TUI Commentの `changeLog[].originalValue`／`newValue` 自体がJSON nullなら、その表示用値を引用符なしの `未設定` にする。既知項目・description・未知fieldのすべてに適用する。API原値の型で判定し、文字列置換やJSON内部の再帰変換はしない。
   - `startDate: "2026-11-01" -> null` は `START DATE: 2026-11-01 -> 未設定`。
   - descriptionの `null -> ""` は `詳細: 未設定 -> ""`、`"null" -> null` は `詳細: "null" -> 未設定`。片方がnullならinline差分や空本文の見出しへ進めない。
   - 未知fieldも同じnull規則とし、識別子と残りの安全な前後値を保持する。
   - object／array内のnull（例 `{"x":null}`、`[1,null]`）はそのまま。文字列 `"null"` は未設定へ変換しない。空文字・数値0・falseも未設定にしない。descriptionの型を保つJSON引用と、他fieldの既存の文字列表示方法は維持し、全fieldの引用方式を一括変更しない。
   - 通常本文、content自体のnull、作者／日時、Issue Detail、API／Serviceのデータ・validation、CLI固定表示、JSON出力・送信値には適用しない。秘匿が必要な場合は既存の安全表示を優先する。
3. **Comment枠**：アプリが枠titleに生成する `|` を削除する。通常時は `Comment`（focus時は `> Comment`）。取得中は `Comment Loading... Esc: cancel`、通知時は `Comment No more comments` や `Comment Cancelled; display retained` のように必要な文字列を空白で区切る。状態／通知が空なら区切り用空白も付けない。本文や履歴の値に含まれる `|` は削除しない。focus枠・エラーpopup・失敗／取消／終端の通知内容と発生条件・消去規則を維持する。
4. **基本設計の整合**：本Plan全文の明示承認後、このPlanningで `docs/designs/backlog-cli.md`「課題コメント」の削除可読性、null表示の範囲、Comment枠の必要な状態／通知と区切りの記述だけを整合する。他の設計箇所・Domain境界は変更しない。README／CONTRIBUTING、今後の受入手順、製品・テスト・fixtureは別のImplementationで整合し、過去の観測・hash・停止記録を保持する。新しいGLOSSARY／ADRは作らない。

#### 実装方針・影響範囲

- 新しいImplementation Rootは、Issue #18本文と全改訂コメント、本Planの両Issue #18節・基本設計、現在の全差分とuntracked・受入履歴を直接確認する。既存部分実装・未コミット変更を保持し、独断でcommit／stash／破棄しない。今回の3点だけでなくIssue #18の残る必須受入までを同じWork Itemで完了させる。
- `src/comment.rs::preview` の表示用値組立で元のJSON nullを扱う最小分岐を追加する。既存 `safety::json/text` と、最終文書・NO_COLOR文書のsecret再検査を保つ。CLIの `display`／`content` へこの変換を共通化しない。descriptionの文字列同士の差分・10,000境界・取得task内準備は再設計しない。
- 同ファイルの削除styleと、NO_COLOR／変更改行の削除判定を整合する。必要なら削除背景を同ファイル内の小さな定数で共用し、変更箇所の漏れを防ぐ。色変更のために文書型・差分engine・意味判定frameworkを新設しない。内容再構成・末尾改行・TAB可視化・投影後redactionを維持する。
- `src/tui.rs::Shell::panel` のComment titleだけで、空状態を含む区切りを整える。必要なら `src/tui/browse.rs::comment_status` の空白組立も最小修正する。唯一のcallerを再確認し、busy／世代／paging／noticeの状態管理やAPI取得順を変更しない。`src/tui/comment.rs` のprojection・固定行・scrollは回帰確認対象で、不要な編集はしない。
- `tests/comment_preview.rs` の公開preview、`src/tui.rs` のShell→TestBackend cell、`tests/tui_process.rs` の実bl→PTY／HTTPSを更新・拡張する。色に依存する旧新値再構成・TAB・末尾改行の全期待値を漏れなく整合する。`tests/tui_shell.rs` の配置と `tests/issue_comment.rs` のCLI／JSON不変も回帰確認する。既存fixture／stepsを再利用し、必要な入力と期待値だけを追加する。
- README／CONTRIBUTINGの該当するComment表示・再現説明と `docs/testing/manual-acceptance.md` の今後の受入手順・変更後の実測を整合する。新依存・Cargo設定・lockfile・exportの変更は不要。全callerはImplementation開始時の差分で再確認する。

#### 制約・対象外

元Issue #18と前節の制約・安全性を維持する。新しい配色設定・theme切替・端末検出／自動補正・色fallback用の新機構、永続状態、追加GET、全件取得、通知の廃止、API／CLI／JSONのnull変換、JSONの再帰変換、文字列のnull置換、他Issueのc/e・フォーム・editorは追加しない。NO_COLORは既存機構を用いる。

実Backlogへの書込み・検証データの準備／変更／復元は別の明示許可が必要であり、このPlan・Issueの承認に含めない。認証情報を別の場所から補わず、key・機密本文・raw responseを記録しない。実サービスへの意図的な障害・過大負荷、push／merge／release公開／deploy、既存変更の破棄を行わない。PlanningではImplementationを開始しない。

#### 完了条件

- 削除が指定した白文字＋暗赤背景＋取り消し線となり、追加・通常文字・NO_COLOR記号・変更改行・TAB・安全表示が回帰していない。利用者の実端末で削除文字が読み取れることを確認済み。RGBやcell assertの成功だけでは合格にしない。
- 既知／description／未知fieldの前後値自体がnullなら未設定。入れ子null・文字列null・空文字・0・falseと区別し、原データ・CLI・JSONが不変。
- 通常・focus・取得中・終端・取消のComment枠にアプリ生成の `|` がなく、必要な取得状態・通知と既存エラーpopupが残る。本文等の `|` は保持する。
- 既存のn/p/g/G・R成功時だけ交換・失敗／取消／古い応答による本文／選択／scroll／busyの非破壊、JST・固定行・14／6配置・resize／極小保護・10,000境界を維持し、総GET数を増やさない。
- 関連試験・正式チェック・変更後releaseのローカルHTTPS／PTY・必須の実端末／実Backlog受入を再実行して結果を記録する。未実施・失敗ならIssue #18は未完了とし、過去の281成功等やReview・Code Review Approveを流用しない。
- Plan・基本設計・Issue追補・該当説明・テスト・今後の受入手順が整合し、過去の履歴・未コミット部分実装を保持する。初版全体や後続Issueの完了と混同しない。

#### 動作確認方法

既存 `tdd` Skillによる、一つの失敗テスト→最小実装の縦sliceを使う。公開seamは前節と同じ。公開 `comment::preview`／`display` は値・style・原値非改変・安全表示（実端末の読みやすさは捕捉しない）、Shellの入力→TestBackendはtitle・cell装飾・配置（OS描画差は捕捉しない）、実bl→既存PTY／ローカルHTTPSは操作・通信・状態保持・復元（実APIの全データや実端末の可読性は捕捉しない）、CLIプロセスはstdout／JSON／終了コードの既存契約を確認する。内部関数の形やtask数に依存するテスト・新harnessは作らない。

- **色の回帰**：削除foreground／backgroundを期待RGBのliteralでassertし、取り消し線も確認する。全削除／部分置換／改行のみ削除・末尾改行／日本語・結合文字・emoji・TABを両モードで検証する。NO_COLORの全cell fg／bg Reset、削除／追加記号、変更改行の方向と元の改行、旧新値の内容再構成を維持する。色変更による内容欠落やsecret再構成を検出し、定数と同じ定数を比べるだけの試験にしない。
- **nullの回帰**：startDate、assigner等の既知field・description・未知fieldで、前だけ／後だけ／両方nullを試す。`詳細: 未設定 -> ""`、`詳細: "null" -> 未設定`、`START DATE: 2026-11-01 -> 未設定` を既知の期待値にする。入れ子object／arrayのnull・文字列null・空文字・0・false・巨大数値・本文同居を確認する。API入力Valueが前後で不変、CLI固定表示は従来のnull、JSONはnull型と文字列型を保持することをassertする。
- **titleの回帰**：100×24と40×12、focusあり／なし、idle／Loading／空一覧／No more comments／取消／失敗popupで、titleに生成 `|` がなく必要な状態／通知は消えないことを確認する。本文・履歴内の `|` は残る。R失敗時の本文／選択／scroll保持、R成功交換、取消、課題切替後の古い成功／失敗と新busyを既存試験で再確認する。
- **安全・境界**：API key・Span境界のsecret再構成、null表示やlabelと前後値の組立後再検査、外部ANSI／OSC無害化、TAB・変更改行・10,000／10,001の限定例外を既存境界で確認する。表示変換のためだけの追加通信なしを総GET数・HTTP記録で照合する。
- **実端末／実Backlog**：変更後releaseのhash、端末名／版・macOS・NO_COLORの有無、100×24／40×12／極小を記録し、削除文字と取り消し線が読めること・追加との区別・折返し／scroll・JST固定行・title／取得状態／通知・n/p/g/G/Rを利用者と確認する。profile origin `https://lazybl.backlog.com/` を確認し、変更後TUIでBLCLI-1の `839099888`／`839099889` の全削除／全追加と既存のコメント／本文なし履歴を確認する。開始日等のnull履歴は既存の読取り可能なデータがあれば使い、特殊値・未知field・null境界はharnessで検証できるため書込みによる準備を要求しない。必須の実Backlog受入自体はharnessで代替しない。読めない場合や認証不足・未実施は未完了として理由と再開条件を残す。
- 記録先は既存 `docs/testing/manual-acceptance.md`。過去の受入・hash・停止記録を消さず、今回の手順・結果・未確認事項を区別する。前節の必須受入を省略しない。

```sh
cargo test --locked --test comment_preview
cargo test --locked --lib tui::tests::
cargo test --locked --test tui_process comment_
cargo test --locked --test tui_shell --test issue_comment
cargo fmt --all -- --check
cargo test --locked --all-targets
cargo clippy --locked --all-targets -- -D warnings
cargo audit --file Cargo.lock
cargo build --locked --release --target aarch64-apple-darwin --bins
git diff --check
```

cargo-audit 0.22.2と既存正式コマンドを維持する。各終了コード・警告・失敗・環境制約を記録し、監査前後でlockfile不変を確認する。製品の変更後に全チェック・必要な独立Review・Code Reviewの承認を取り直す。TestBackend／PTYの成功と実端末の可読性受入を区別する。

#### Planningの保存・Work Item受渡し・終了

本Plan改訂全文をPlannotatorへ提出し、明示Approve後に同じ正式パスへ保存・readbackし、合意した基本設計箇所だけを整合する。正式Planのパスと全文を `taskflow-to-ticket` へ渡す。既存Issue #18一件を再利用し、残る実装・受入と今回の3点を同じ新しいImplementationセッションで確認する案を別途承認する。作業内容はBug Fixであり、既存Featureタイトルやlabelを変更する理由にはしない。

Plan承認・tracker設定承認・分割承認・最終追補本文と公開先の保存公開承認を分ける。新規Issue・label・依存関係は増やさず、既存タイトル・本文・OPEN状態・Blocked by／native関係・過去コメントを変更しない。明示承認した改訂コメントだけをIssue #18へ追記する。正式Plan保存・Work Item承認・公開とreadbackの成立後に、確定Issue参照・cwd・改訂Planと部分実装保持を明記した新しいRoot向けImplementation再開kickoffを提示して停止する。実装・Worker起動・merge／release／deployは行わない。

### Issue #21 再計画：課題更新成功後のComment自動再取得

#### 目的・正式入力・優先関係

[Issue #21](https://github.com/minorunakamura/backlog-cli/issues/21) の `REPLAN_REQUIRED` を受け、Detailのeによる課題更新成功後、Rを押さずにCommentの最新表示を取得できるようにする。正式Planは本ファイル `docs/plans/backlog-cli-initial.md` の改訂全文。基本設計 [課題の項目・作成・更新](../designs/backlog-cli.md#課題の項目作成更新) の「更新」、[課題コメント](../designs/backlog-cli.md#課題コメント)、[Issue #21の実Backlog受入追補](https://github.com/minorunakamura/backlog-cli/issues/21#issuecomment-6479184188)、`docs/testing/manual-acceptance.md` の「#21 入力優先修正後buildの利用者受入とComment更新への要望」「#21 Comment自動更新への仕様変更要求による停止」を参照する。

本節は旧契約の「課題更新後のCommentは自動更新せずRへ任せる」「成功後もComment選択・scrollを必ず保持する」だけを以下の範囲で置き換える。Issue #21の全更新項目・差分／解除・確認・独立送信・入力保持・安全表示・中断時確度と、必須の実端末／実Backlog受入は維持する。後続 [Issue #22](https://github.com/minorunakamura/backlog-cli/issues/22) のコメント追加、[Issue #23](https://github.com/minorunakamura/backlog-cli/issues/23) の既存コメント本文更新の動作・受入・依存関係は変更せず、機能を本Issueへ取り込まない。

#### 停止理由・確認済みの現状

- Issue #21本文は、正常な成功応答をDetailへ反映し、Issue ListとCommentを自動更新しない契約。#22はコメント追加後の最新ページ取得、#23は同じコメントの成功応答反映であり、#21の非更新契約を変更しない。利用者は今回の#21内でCommentも自動更新するよう要求し、Rootの質問で下記の表示・待機・失敗・開始条件・設計更新時期に合意した。
- `src/tui.rs` のwrite完了処理は、`src/tui/create.rs` の `update_display` で課題データと対象keyを検証し、対象一致時だけDetailへ反映する。`finished` のupdate成功は一覧再取得を起動せず、create成功だけが `refresh_after_create` へ進む。この区別を維持する。
- `src/tui/browse.rs` のComments operationは、Latestなら20件・ID降順・min/max境界なしで取得する。本文を持つ項目があれば既存の記法確認GETを行い、全項目の表示準備が成功した後にだけ交換する。世代と課題keyで反映を制限し、旧要求はruntimeがabort・joinする。`src/tui/comment.rs` のLatest交換は選択0・本文先頭で、正常な空ページは空表示となる。
- 更新開始時の `update_target` は未完了一覧・Detail・Comment要求を失効させる。読取り失敗は既存Comment内容・選択・scrollを消さず、書込み確度は別のprogressで保持する。既存作成結果のrefresh表示と通知はCreated／一覧専用なので、updateを作成後一覧更新へ誤接続しない。
- Planning開始時に変更済みの `CONTRIBUTING.md`, `README.md`, `docs/testing/manual-acceptance.md`, `src/api.rs`, `src/lib.rs`, `src/service.rs`, `src/tui.rs`, `src/tui/browse.rs`, `src/tui/create.rs`, `tests/tui_process.rs` と、untrackedの `tests/fixtures/tui_update.json`, `tests/fixtures/tui_update_steps.json` を保持する。commit／stash／破棄を独断で行わない。
- 旧buildの333件成功・独立Review・利用者受入とBLCLI-2の変更／復元は履歴であり、新しい動作の実装・検証済みを意味しない。最終Code Review gateは停止時点で未実施。Planningでは製品・テスト・fixture・これらの既存変更を編集せず、新仕様のテスト・実API接続・書込みも行わない。

#### 要件・決定事項

**開始条件と表示**

1. Detailのeからの課題更新で、対象と一致する正常な成功課題応答を処理してDetailへ反映できた場合だけ、同じ固定対象keyの最新Commentページを一度自動取得する。Detailは成功応答を使い、成功確認のための課題GETを追加しない。
2. HTTP 200後でもbody読取り・JSON decode・課題データ／対象keyの検証・表示準備に失敗した場合は、自動Comment取得を開始しない。従来どおりupdatedと「応答処理失敗・再送不要」を通知し、旧Detail・Commentを残して手動Rへ任せる。未更新・結果不明・未送信／無変更／取消でも自動取得しない。200観測済みの確度を下げない。
3. 自動取得の意味はCommentのRと同じ。count=20、order=desc、minId／maxIdなしの最新1ページだけを取得し、本文の記法確認と安全な表示準備も完了した成功時だけページを交換する。最新のコメントまたは本文なし変更履歴1件を選択し、本文scrollを先頭へ戻す。空ページは成功した空表示とし、古い内容を残す失敗とは区別する。
4. 更新による履歴そのもののIDを推測・検索しない。他者が直後に投稿すればそれが最新になる可能性を許容する。個別コメントGET、全件走査、polling、定期更新、自動retry、同じPATCHの再送を追加しない。表示のために必要な既存の記法確認GETを、書込みの成功確認GETと混同しない。

**結果画面・失敗・中断**

5. Detail反映直後からComment取得・表示準備中まで、更新結果画面に「更新済み・コメント表示更新中」を示す。メイン画面のfocusはDetailのまま。結果画面内の既存scroll・resizeは維持し、Enter／Escでは閉じず、背面へのfocus／選択変更・新しい編集・再送・破棄を受け付けない。送信済み操作を再実行できる表示にしない。
6. 取得完了後に「更新済み・コメント表示更新完了」または「更新済み・コメント表示更新失敗、旧表示保持・再送不要」を結果画面で区別する。成功・失敗どちらも完了後のEnter／EscでDetailへ戻る。取得中にCommentが表示可能なサイズでは、Escで読取りだけを取消せると誤案内しない。新しいEsc取消や背景継続操作は追加しない。
7. 取得または記法取得・表示準備の失敗では、更新済みDetailを巻き戻さず、旧Commentのページ内容・選択ID・本文scrollを保持する。元から空／未取得ならその状態を保持し、失敗を正常な空結果にしない。書込み確度はupdatedのまま、HTTP statusやエラーを安全化して通知し、手動のComment Rで表示を更新できると案内する。Rも従来どおり成功時だけ交換し、失敗なら保持する。
8. 自動取得中もCtrl+C／SIGINTは中断130。read taskをabort・joinし、端末を復元してupdated／再送不要を通知する。中断で旧Commentを交換・消去したり確度をunknown／not_updatedへ下げたりしない。終了後に表示の永続保存・復旧は行わない。取得待ちの極小時も中断・端末復元の安全性を維持する。

**失効と一覧非更新**

9. 更新開始前の一覧・Detail・Comment要求と予約済みpreviewを従来どおり失効させる。自動取得は対象keyと新しい読取り世代に結び付け、旧成功／失敗・取消／閉鎖後応答が現在の内容・通知・busyや後続要求の完了を上書きしない。自動取得開始のためにCommentを再selectして旧ページを消さない。結果待機中は通常操作へ漏らさず、終了後の手動R・課題切替・次の編集でも既存の失効境界を維持する。
10. Issue Listのタイトル・行データ・検索条件・total・ページ履歴・選択・scrollは成功／失敗とも自動変更しない。count／list再取得や成功課題の直接挿入・書換えは行わず、一覧の更新は一覧Rに任せる。Detail本文は従来の成功応答反映、Commentだけは上記の成功交換となる。コメント本文更新（#23）を最新表示へ移す動作へ変えない。

#### 実装方針・影響範囲

- 新しいImplementation Rootは、Issue #21本文・既存と今回の全改訂コメント、本節・基本設計、現在の全差分／untrackedと停止記録を直接確認する。既存の部分実装と旧観測を保持し、過去のチェック・Review・Code Review Approveを変更後の差分へ流用しない。
- `src/tui.rs` の課題更新完了点で、正常なDetail反映後にだけComment取得を接続する。`src/tui/browse.rs` の既存Comments／Latest、世代・busy・対象key・失敗保持・runtime取消を再利用し、完了／失敗を更新結果へ通知するための最小のメモリ内状態だけを使う。作成後一覧更新と混線させず、汎用refresh framework・新しいService／API公開入口・永続状態を作らない。
- `src/tui/create.rs` の更新結果・取得中表示・閉鎖ガード・完了通知を整合する。Created／一覧用の文言をupdateへ流用しない。必要なComment／Detailのキー案内はmodalの実際の操作に合わせる。`src/tui/comment.rs` と `src/tui/detail.rs` は既存Latest交換・scroll・安全表示を再利用し、不要な編集をしない。
- `tests/tui_process.rs` の全update成功経路・共有create/update入力試験・旧応答失効試験で自動取得と待機による影響を確認する。`tests/fixtures/tui_update.json` は旧ページ→新しい本文なし変更履歴／コメント等の応答sequenceへ整合し、`tui_update_steps.json` はUpdatedという途中表示だけで閉じず、自動取得の完了を待ってCommentの変化を確認する。
- READMEの利用説明、CONTRIBUTINGのrelease直接HTTPS／PTY手順・HTTP期待値、`docs/testing/manual-acceptance.md` の今後の手順・変更後実測を新しいImplementationで整合する。旧記録の「17 GET＋1 PATCH」「Comment保持」等を新結果に書き換えない。Comment GETと本文の有無に応じた既存記法GETを区別して実際のfixture経路を数え直し、総数を先に一律18等と決めない。
- 製品の変更中心は上記TUIの3ファイル・関連テスト／fixture／説明。API／Service／CLIの既存部分実装は保持・回帰確認するが、自動表示更新のための契約変更・新依存・Cargo設定／lockfile／export変更は不要。実装時に全callerと共有フォームを再確認し、範囲外の仕様変更が必要なら停止して再計画する。

#### 完了条件

- 上記の開始条件・最新1ページ・成功時だけ最新1件／本文先頭・空成功・対象／focus維持を、実blの表示とHTTP記録から確認できる。
- 取得中の結果画面・Enter／Esc非閉鎖・再送防止と、成功／失敗の完了通知・Detail復帰・Comment R回復が仕様どおり。失敗時は旧Comment選択／scrollを保ち、Detailの更新済み表示とupdated確度を維持する。
- 取得中のCtrl+C／SIGINTで130・updated／再送不要・端末復元を満たす。旧応答と失効後の成功／失敗が新表示・通知・busyを壊さない。
- PATCHは独立した明示sの一回だけ。自動取得は正常な対象応答を処理できた成功時だけで、Issue List非更新・一覧のR／ページ再訪・CLI／#22／#23の契約が回帰しない。
- Issue #21の既存全要件と必要な関連試験・正式チェック・変更後releaseの直接HTTPS／PTY確認・必須の実端末／実Backlog受入を満たし、Plan・基本設計・Issue追補・利用／開発説明・今後の受入手順が整合している。未実施・失敗・不安定な既存試験を隠さず記録し、旧buildの合格だけで完了としない。

#### 動作確認方法

既存 `tdd` Skillに従い、**実blの入力→画面／終了（既存PTY、必要なら公開Shell→TestBackend）とローカルHTTPSのHTTP送受信境界**で、一つの失敗テスト→最小実装の縦sliceを行う。これらは選択・scroll・待機・確度・通信・端末復元を捕捉するが、実IME／実端末の可読性／実APIの全データは保証しない。private関数形状・内部task数・独自harnessを固定しない。

- **成功**：古いページの非先頭コメントをscrollした状態から課題更新し、PATCH完了後に一回のLatest要求（20／desc／境界なし）、取得中は旧表示、完了後は新しい最新1件と本文先頭、Detail focus維持を確認する。本文なし履歴・通常本文と必要な記法確認GET・空ページを区別し、最新が今回以外の履歴／投稿でも特定ID探索をしない。
- **失敗と回復**：PATCH正常成功後のComment API拒否／通信失敗／不正応答、および必要な記法取得失敗を制御する。旧ページ・選択・非ゼロscroll、更新済みDetail、updated／再送不要の通知を確認し、完了後にComment R成功で交換・失敗で保持する。PATCH再送とcount／list再取得はゼロ。
- **待機・中断**：遅延Comment取得中にEnter／Esc・s／e／通常focusキーを送っても閉鎖・再送・背面変更を起こさず、結果scroll／resize・100×24／40×12／極小往復を維持する。Ctrl+CとSIGINTの両方で130・updatedと端末復元を確認し、200後のComment read失敗で書込み失敗へ戻さない。
- **開始しない経路**：確認Enter／無変更／編集取消・load失敗、PATCH拒否／結果不明、200後body／JSON／必須field／別key応答の失敗で、新しい自動Comment GETが増えないことを確認する。元の確度・旧Detail保持・独立s／一回PATCHを回帰検証する。
- **失効・一覧・隣接回帰**：更新開始前の遅延一覧／Detail／Comment成功・失敗を後着させ、自動取得の新busy／通知／交換を壊さないことを確認する。終了／失効後応答、取得後の課題切替・手動R・次の編集も確認する。成功／失敗とも旧一覧タイトル・検索／total・履歴・選択／scrollを保持し、訪問済みページ再訪でlist／countを呼ばない。共有作成フォーム・単行編集キー、Service／CLIの部分更新・解除・確度と既存コメント読取り・安全表示を再確認する。
- **実端末・実Backlog**：変更後releaseのSHA-256、端末名／版・macOS・IME／keyboard設定・NO_COLOR有無、100×24／40×12／極小、各操作・結果を記録する。日本語IMEとCtrl+A/B/Y、editor／resize・破棄・独立送信・終了復元を含む#21受入を新buildで再確認する。profile origin `https://lazybl.backlog.com/` を確認し、接続先・検証課題（既存候補BLCLI-2）・変更／解除／復元内容の別の明示許可を再開時に得る。許可範囲だけで変更・未指定項目保持・Detail反映と、**RなしでComment最新履歴が表示されること**をWeb等と照合し、必要な復元を確認する。一覧R前の旧一覧と、一覧R後の更新を区別して確認し、末尾が不可視なら見えたと推測しない。新しいデータ準備・権限・認証情報探索・書込み許可をPlan承認から導かない。
- 障害／応答逆転／空ページ／極小時中断等はharnessで再現し、実サービスへ障害や過大負荷を与えない。harness成功を必須実Backlog／実端末受入に代替しない。認証不足・未実施・失敗は未完了として理由と再開条件を残す。

```sh
cargo test --locked --test tui_process bl_update_
cargo test --locked --test tui_process bl_create_and_update_single_line_editing_
cargo test --locked --test tui_process bl_create_
cargo test --locked --test service issue_update_
cargo test --locked --test issue_update --test comment_add --test issue_comment
cargo test --locked --test tui_shell
cargo fmt --all -- --check
cargo test --locked --all-targets
cargo clippy --locked --all-targets -- -D warnings
cargo audit --file Cargo.lock
cargo build --locked --release --target aarch64-apple-darwin --bins
git diff --check
```

cargo-audit 0.22.2と既存正式コマンドを維持し、監査前後のlockfileハッシュ、DB／registry取得失敗・警告・終了コードを記録する。変更後releaseを更新済みCONTRIBUTINGの自己完結Python手順で直接起動し、HTTP method／path／formと呼出数、完了を待った表示、fake secret非表示・exit／termios復元を照合する。記録先は既存 `docs/testing/manual-acceptance.md`。旧hash・過去の合格・停止理由は履歴として保持し、新しい結果を追記する。既存検索PTYの待機raceも成功へ丸めず、初回失敗と再実行を区別する。過去のReview／Code Review承認を流用せず、変更後の差分に必要なReview・Code Review gateを受ける。

#### 制約・設計更新・Planningの受渡し

Plan全文の明示承認後、このPlanningで同じ正式パスへ保存・readbackし、基本設計「更新」の最終段落だけを上記契約へ整合する。他の基本設計・Domain境界・CLI／API・#22／#23・過去の観測は変更しない。README／CONTRIBUTING／今後の受入手順と製品・テストは新しいImplementationで更新し、それまでは旧Comment非更新記述より本節を優先する。GLOSSARY／ADR・別Plan・新状態管理文書は作らない。

新規依存、設定、永続cache／下書き／書込み履歴、汎用refresh機構、競合検出・自動merge、全件取得、更新履歴ID探索、自動retry／PATCH再送、#22／#23機能、push／merge／release／deployは対象外。実Backlog書込み・復元は個別の明示許可が必要。既存差分をcommit／stash／破棄せず、PlanningではImplementationを開始しない。

正式保存後に `taskflow-to-ticket` へ正式Planのパスと全文を渡す。既存Issue #21一件を再利用し、残る実装・受入と今回の動作変更を同じ新しいImplementationセッションで確認する案を別途承認する。内容はFeature（既存動作変更）。新規Issue・label・依存関係は増やさず、元タイトル・本文・OPEN状態・Blocked by／native関係・既存コメントを保持し、明示承認した改訂コメントで要件・受入条件・検証方法を追補する。

Plan承認・tracker設定・分割・最終追補本文／公開先の保存公開承認はそれぞれ別に扱う。正式Plan／基本設計の保存・readback、Work Itemの分割・本文／公開先承認と公開・readback後、確定Issue／追補参照で別の新しいRoot向けImplementation再開kickoffを提示して停止する。Worker起動・製品実装は行わない。


## 開発順序と受渡し

以下は技術的な順序であり、Work Itemsの分割承認を代替しない。各項目のテスト・説明はその機能と一緒に追加する。

1. Cargo/toolchain・lib/2bin・品質チェックの土台。
2. 厳密設定・安全表示・API client・書込み確度の共通基盤。
3. 候補/名前解決・検索/ページ・課題/コメント読取りと非対話CLI。
4. 課題/コメントの書込みサービスとCLI契約・エラー。
5. TUI基本shell・単行入力・端末復元・editor連携の最小経路、および入力部品の早期実端末確認。
6. TUIのプロジェクト/検索/一覧/プレビュー/コメント閲覧と状態保持。
7. TUIの作成・課題更新・コメント追加/本文更新・確認/結果表示。
8. 全体受入、README/CONTRIBUTING、GitHub Actions、配布tar.gz/SHA-256の確認。

各Work Itemは正式な本Planを参照し、新しいImplementationセッションで着手する。上流機能が未完成なのに下流を完了扱いにしない。設計変更が必要ならPlan Reviewへ戻り、承認を流用しない。

## 完了条件

- 基本設計と本Planのコマンド・flag・設定・19 API・画面操作・書込み確度・安全表示が実装され、後述の公開境界から検証できる。
- CLIはTTY/非TTYとも対話せず、JSONと終了コード契約、原文/未知field/大整数保持とsecret保護を満たす。
- TUIは通信中の操作・中断、古い応答の無視、ページ再訪、更新失敗時の保持、確認前の非送信、重複送信防止を満たす。
- 早期の入力部品確認と、完成後の実端末受入を両方実施し、環境・手順・結果を記録する。実IME未確認は自動試験合格で完了に置き換えない。
- Issue #17・#18・#20〜#23は、下記「実Backlogの段階的な受入」に従い、担当機能を実Backlogで確認してから完了とする。実環境確認の未実施・失敗をIssue #24へ先送りして完了扱いにしない。Issue #24は完成版の最終的な全体受入とする。
- ローカルでtest/rustfmt/Clippy/cargo-auditを実行し、失敗や確認不能を隠さない。既知脆弱性の無条件ignoreを追加しない。
- main pushとpull_requestを対象にGitHub Actionsの品質workflowを作成して設定確認する。GitHub上で未実行ならその旨を報告するだけで、実行のためのpush/PRを要求しない。
- 利用説明をREADME、開発/品質/CI/配布手順をCONTRIBUTINGへ分離して記載する。
- release buildのbl/bl-cli・README・secretなし設定例だけを `backlog-cli_darwin_arm64.tar.gz`へ梱包しSHA-256を生成する。利用者設定・toolchain・fixtureを混入させない。展開先の別ディレクトリで両binaryの起動を確認し、native依存が利用者環境で満たせることを確認する。公開はしない。

## 動作確認方法

### TDDと公開境界

機能ロジックは既存のtdd Skillに従い、一つの失敗テスト→最小実装の縦sliceで進める。本Planで確認するseamは次の4つ。内部のprivate関数名・細かいtask数を固定するテストは作らない。

| seam | 捕捉するもの / 捕捉しないもの |
| --- | --- |
| 実CLIプロセスのargv/stdin/stdout/stderr/exit | flag契約・非対話・JSON/終了コード / 実IME |
| 共通libの操作・検索・本文/安全表示の公開入口 | 既定値・変更判定・原文保持・変換/伏字 / 実端末の字幅 |
| APIのHTTP送受信境界 | method/path/header/form・paging・失敗/確度 / 実Backlogの全権限組合せ |
| TUIの入力→状態/表示、Ratatui TestBackendとPTY | 状態保持・古い応答・focus・layout・端末復元 / OSの実IME描画 |

UIの見た目・実IME・配布確認は無理にunit test化せず、実行可能な手動受入を組み合わせる。

### 自動試験

- 設定: XDG未設定/空/相対/絶対、profile/key欠落、cwd限定、未知キー/型不正、明示優先・既定値非流用、editor argv。
- CLI: 全13コマンドのhelpと主要成功/失敗、flag適用先/反復/排他/解除、ID0、stdin明示時だけ読取、空本文・不正UTF-8、同値更新非送信、JSON引数エラー。
- API: ローカルHTTPS harnessとテスト専用trustを使用し、製品にHTTP/TLS検証回避flagを追加しない。header認証・query非使用、form配列/空値/未指定、コメントPOST/PATCHの一回送信、所有者検証、4xx/429/5xx/不正JSON/切断/timeout/redirect非追従とretryなしを検証する。必要なテスト証明書・server crateだけdev dependencyにする。
- 確度: 送信直前、送信開始後、201/200 headers受信後body待ち、それぞれの中断/切断を制御して試す。成功status後のbody/decode/出力失敗でcreated/updatedが残ることを検証する。
- ページ: count/list同条件、count失敗、空ページ、totalとのずれ、再訪時の通信なし、ID境界除外・両方向/最古、content:null履歴。複数project横断・曖昧候補・権限制限はmockで検証する。
- 安全表示: 日本語/結合文字/emoji、HTML entity後の制御文字、ANSI/OSC、code/escape、未知macro/field、secretを含む未知key/値、巨大数値、水平scroll外のsecret、入力原文非改変、NO_COLOR/非TTY。
- TUI: 世代を逆転させた遅延応答、取消/画面閉鎖後応答、R失敗と成功時の交換、極小時の送信不可、入力キー衝突、フォーム値・ページ選択・scroll保持、作成/課題更新/コメント追加後の再取得失敗、editor失敗/UTF-8不正/resize/中断・一時file削除。

### 実端末・配布受入

Apple Silicon Macの実端末で、端末名/版・macOS・IME・対象buildを記録する。特定端末を製品の利用条件にしない。

早期の単行入力確認に加え、完成後に100x24以上/40x12以上/それ未満で操作、長い日本語・結合文字・emoji・変換候補、editor中resizeからの即時再描画、editor非0/起動失敗/中断、終了後に通常のshell操作ができることを確認する。状態・入力保持と伏字も目視する。自動試験で代替できない操作は実装担当と利用者で実施し、未実施のまま完了にしない。記録先は `docs/testing/manual-acceptance.md`とする。

### 実Backlogの段階的な受入（Issue #17以降の改訂）

実Backlogとの接続・権限・実データの差異をIssue #24で初めて発見しないよう、Issue #17以降の該当機能で実環境確認を必須にする。従来の「接続先の指定と、実サービスE2E未実施の記録」だけの扱いから、以下の担当Issueごとの完了条件へ改訂する。製品仕様・基本設計の責務・API範囲は変更しない。

接続先は **`https://lazybl.backlog.com/`** とし、実施前に使用profileの `spaceUrl` がこのoriginを指すことを確認する。製品の既定接続先や接続先制限にはしない。

| Work Item | そのIssueで行う確認 |
| --- | --- |
| [Issue #17](https://github.com/minorunakamura/backlog-cli/issues/17)：検索dialog | 実装したTUIから実際の候補を取得し、通常検索の条件適用・クリア・結果表示と横断検索の基本動作を確認する。複数project間の実検証は既存の環境制約により対象外とし、自動試験で確認する。 |
| [Issue #18](https://github.com/minorunakamura/backlog-cli/issues/18)：コメント閲覧 | 実装したTUIで実際のコメント・本文なし変更履歴を表示し、前後・最新・最古への移動と再読込みを確認する。 |
| [Issue #19](https://github.com/minorunakamura/backlog-cli/issues/19)：editor連携 | 実Backlogへの接続は必須にしない。既存のPTY・実端末によるeditor往復・resize・失敗/中断・端末復元の確認は維持する。 |
| [Issue #20](https://github.com/minorunakamura/backlog-cli/issues/20)：課題作成 | 許可済みのproject・内容で、TUIの入力・確認・独立送信を通して検証用課題を作成し、Backlog上の登録内容とTUIの結果表示を確認する。 |
| [Issue #21](https://github.com/minorunakamura/backlog-cli/issues/21)：課題更新 | 許可済みの検証用課題で、TUIから変更・解除を実施し、変更内容・未指定項目の保持と結果表示、RなしのComment最新表示を確認する。本Planの「Issue #21 再計画：課題更新成功後のComment自動再取得」に従い、一覧R前後の新旧表示を区別する。 |
| [Issue #22](https://github.com/minorunakamura/backlog-cli/issues/22)：コメント追加 | 許可済みの検証用課題で、TUIから本文のみの追加と課題項目変更を伴う追加を確認し、登録内容と成功後のコメント表示を確認する。 |
| [Issue #23](https://github.com/minorunakamura/backlog-cli/issues/23)：コメント本文更新 | 許可済みの自分の検証用コメントをTUIから更新し、本文の反映と同じコメントを表示し続けることを確認する。 |
| [Issue #24](https://github.com/minorunakamura/backlog-cli/issues/24)：全体受入 | 各Issueの実環境確認記録を照合したうえで、完成版の検索・閲覧・作成・更新を通した最終の全体受入を実施する。ここを初回の実接続確認にしない。既存の全体品質確認・実端末受入も維持する。 |

- 各機能の自動試験・ローカルHTTPS harness・実端末確認を置き換えず、実装したTUIと実API・権限・実データの結合確認を追加する。通信障害・応答逆転・rate limit等は従来どおりharnessで再現し、実サービスへ意図的に障害や過大な負荷を発生させない。
- 実Backlogへの書込みは、接続先・対象project/課題/コメント・作成/変更内容を確認した別の明示許可がある場合だけ行う。本PlanやIssueの承認、接続先の指定をその許可にしない。既存の設計検証用課題を無断で変更せず、検証データの準備や復元も許可された範囲だけで行う。削除APIや自動cleanupは追加しない。
- 書込み許可がない場合もharnessで書込み経路を確認できるが、必須の実環境確認の代替にはしない。認証・権限・検証データの不足、接続不能、確認の失敗があれば、そのIssueを未完了として理由・再開条件を報告する。未実施を成功として扱わず、Issue #24へ先送りして完了にしない。
- 記録先は既存の `docs/testing/manual-acceptance.md` とする。Issue番号・対象build・接続先・対象データの識別子・手順・結果・未確認事項を記録し、API keyや機密本文を記載しない。基本設計時のAPI Probeや別Issueの結果だけを、その担当機能の製品確認の代わりにしない。
- 新規Issueや依存関係は増やさず、上記の既存Issueの受入条件・検証手順を本節と整合させる。Issue #19、完了済みIssue、Tokio保守作業の要求・検証履歴は変更しない。Plan Review・保存後に、既存Issueの再利用範囲と最終本文の公開をそれぞれ確認して反映する。

正式チェック:
```sh
cargo fmt --all -- --check
cargo test --locked --all-targets
cargo clippy --locked --all-targets -- -D warnings
cargo audit --file Cargo.lock
cargo build --locked --release --target aarch64-apple-darwin --bins
```

cargo-audit自体は **0.22.2**、導入は `cargo install cargo-audit --version 0.22.2 --locked` にCONTRIBUTINGとCIの両方で固定する。監査はリポジトリ直下で `cargo audit --file Cargo.lock` を実行し、対象lockfileを生成・更新しない。advisory databaseは通常どおり更新し、警告・失敗と終了コードを報告する。配布物は `file`・`otool -L`・展開後のhelp/version起動・SHA-256照合でtarget、native依存、両binary、梱包物を確認する。

## 根拠・残るリスク

主要技術の版別資料:
- [Ratatui 0.30.2 Frame](https://docs.rs/ratatui/0.30.2/ratatui/struct.Frame.html)、[tui-input 0.15.5 Input](https://docs.rs/tui-input/0.15.5/tui_input/struct.Input.html)
- [Reqwest 0.12.28 ClientBuilder](https://docs.rs/reqwest/0.12.28/reqwest/struct.ClientBuilder.html)、[Response](https://docs.rs/reqwest/0.12.28/reqwest/struct.Response.html)、[retry::never](https://docs.rs/reqwest/0.12.28/reqwest/retry/fn.never.html)
- [serde_json 1.0.149 Cargo features](https://github.com/serde-rs/json/blob/v1.0.149/Cargo.toml)、[pulldown-cmark 0.13.5](https://docs.rs/pulldown-cmark/0.13.5/pulldown_cmark/)
- BacklogのAPI送信・解除形式・境界・権限と既存Probeの根拠は基本設計の「品質・配布」「主要参照」を参照する。

依存の同時ビルドと小さなassertは確認済みだが、製品全体の動作・脆弱性・配布物のnative依存・実端末IME/editor/resizeは未検証。入力部品は早期実端末確認まで候補とし、そこで実現できない場合は停止して再計画する。API競合・count/listのsnapshot不一致・実サービス複数project未検証等の既知の限界は基本設計のとおり維持する。
