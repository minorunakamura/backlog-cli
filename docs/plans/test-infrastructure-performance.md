# テスト基盤の計測に基づく高速化・安定化

## 目的

必要な検証能力を維持し、ローカル開発・CIのテスト待機と起動コスト、不安定性を減らす。実装と検証の反復がWorkerの30分枠を使い切る問題に対し、実時間待機の重複と全対象検証の重複を減らし、1セッションで扱える作業へ分ける。

製品・テスト基盤のImplementationは別の新しいセッションで行う。本Planningは、本Planの明示Approve・正式保存と、新規Work Itemsの分割・本文・保存先の個別承認／保存・公開までで終了する。

## 要件・決定事項

- 最小のテスト／fixture改善を採用する。公開API・製品コード・製品の設定や実行方式は変更しない。
- 実CLI・証明書検証付きHTTPS・製品の30秒timeoutを残したまま、5関数に分散した6つのtimeoutケースを最大6並列へ集約する。CPU数によって並列数が減らないよう、単一集約テスト内で6ケースを開始し全てjoinする。
- HTTPS fixtureのTLS handshakeによるhead-of-line blockingを改善する。PTYの既存失敗は、失敗経路の診断を先に整え、証拠で確認できた試験同期の不足だけを修正する。
- library／Service／HTTP／CLI／PTYの既存の保証を削除・一括置換しない。新依存、汎用framework、共有証明書、常駐server、永続状態、CI infrastructureは追加しない。
- ユーザーの最新判断に従い、**CIは設定確認まで**とする。改善後の実CI成功を完了条件にせず、未実行なら未確認と明記する。PlanningでCI起動・commit／push／PR作成は行わない。
- 性能値の妥当性は技術側が実測で判断する。以下の150秒は計測根拠に基づく技術目標であり、製品SLA・CIでの達成保証ではない。
- **今後追加するテストにも適用する恒久的な作成基準**をCONTRIBUTING.mdへ記載し、短いAGENTS.mdから案内する。検証する保証・適切な境界・実時間待機の必要性・focused確認を、新規テストの設計とReviewで確認する。文書とReviewによる運用であり、自動強制の仕組みを導入するものではない。
- 開発途中はfocused確認を使用する。各Work Itemでfullの性能計測3回・release確認一式を繰り返さず、最終差分固定後の全対象3回を性能測定と全体回帰に兼用する。時間枠不足は変更と証拠を保持して引き継ぎ、Workerのtimeout延長を解決策にしない。

## 正式入力・現状の保護

対象cwd: `/Users/minoru/Documents/mywork/cli-dev/backlog-cli`

一次資料:

- `docs/plans/backlog-cli-initial.md`「動作確認方法／TDDと公開境界」「自動試験」「正式チェック」。既存各機能の仕様・受入を変更しない。
- `docs/designs/backlog-cli.md`「全体構成と技術」「JSON・通信・書込み結果」「品質・配布」。HTTPS、header認証、body込み30秒、redirect／retry禁止、単一書込み、確度・中断・安全表示を維持する。
- `CONTRIBUTING.md`「品質確認」「テストと今回の範囲」「ローカルHTTPS harness」。個別のService子processは、TLS trust／環境変数の競合を防ぐために存在する。
- `.github/workflows/quality.yml`、`Cargo.toml`、`rust-toolchain.toml`。
- `docs/testing/manual-acceptance.md` のIssue #22に関する失敗・同期訂正・限定15秒化の記録。特に「取消前HTTP到着同期と未解消の8秒cutoff」「複合シナリオだけ有限15秒」を、過去の観測と試験方法の根拠として参照する。
- `tests/common/mod.rs`、`tests/fixtures/https_server.py`、`tests/fixtures/tty_shell.py`、`tests/https_fixture.rs`、`tests/service.rs`、各CLI suite、`tests/tui_process.rs`。

背景Issue: https://github.com/minorunakamura/backlog-cli/issues/22

依頼文には未コミット変更・受入未完了の記述があったが、開始時の直接確認ではHEADは `a8b4cdcae02e1b3afd8beeb9db18bd452116238c`、git statusはclean、受入文書にも後の追記があった。本Planningはcommit等をしていない。計測前後の追跡／通常untrackedファイルのhashは不変だった。この差をIssue #22の完了認定に使わず、今回の作業を同Issueへ混ぜない。

Implementation開始時にHEAD・全差分・untracked・現在のチェックを再確認し、他者の変更を保持する。commit／stash／破棄を独断で行わない。対象コードが変われば現在の数値をそのまま新baselineとせず、影響する計測を取り直して区別する。過去の成功・Reviewを変更後の証拠に流用しない。

## 計測結果と原因の評価

### 条件・方法

2026-10-31 JST、Darwin arm64／macOS 26.7.1、10 logical CPU、32 GiB、rustc／cargo 1.99.1。親PATHのPythonは3.14.9、OpenSSL CLI 4.0.3。PTY driverは既存の隔離PATHにより `/usr/bin/python3` 3.9.6を使用する。両経路のPython／SSL差を比較時に記録する。

他のテスト・build計測を重ねず、`cargo test --locked --all-targets --no-run` の後、`/usr/bin/time -l cargo test --locked --all-targets` を事前に決めた最大3回、同じtree・既存のデフォルト並列で実行した。RUST_TEST_THREADSは未指定。build／wall／Cargo各suiteのfinished timeを分け、失敗を置き換えない。3回目の失敗で反復を終了した。

| 実行 | wall秒 | suite合計秒 | 結果 |
| --- | ---: | ---: | --- |
| no-run build | 8.90 | ― | 成功。既存targetを使ったincremental準備で、cold build値ではない |
| full-1 | 301.51 | 255.57 | 347成功・0失敗・実端末用2 ignored |
| full-2 | 256.10 | 255.71 | 347成功・0失敗・実端末用2 ignored |
| full-3 | 256.11 | 255.66 | exit101。tui_process 113成功／1失敗、後続tui_shell未実行 |

full-1のwallとsuite合計の差約46秒は初回固有の未特定コストであり、証明書生成や特定OS処理と断定しない。成功したwarm比較値はfull-2の256.10秒。失敗を含む3回を「成功したbaselineの中央値」とは扱わない。

### suite別の実測（秒）

CIは同HEADの既存Quality実行 https://github.com/minorunakamura/backlog-cli/actions/runs/41167569007 のログを読み取った値。macos-15-arm64 image `20261028.0489.1`、実行成功。ローカル値と混ぜて集計しない。

| suite | local-1 | local-2 | local-3 | CI |
| --- | ---: | ---: | ---: | ---: |
| lib | 6.38 | 6.50 | 6.46 | 9.30 |
| bl／bl-cli unit targets | 0.00 | 0.00 | 0.00 | 0.00 |
| body | 0.00 | 0.00 | 0.00 | 0.00 |
| candidate_list | 1.23 | 0.91 | 1.22 | 0.89 |
| cli_help_version | 0.81 | 0.70 | 0.86 | 0.03 |
| comment_add | 60.34 | 60.23 | 60.25 | 60.36 |
| comment_preview | 0.14 | 0.14 | 0.14 | 0.15 |
| comment_update | 30.29 | 30.27 | 30.26 | 32.34 |
| editor | 0.13 | 0.11 | 0.14 | 0.21 |
| https_fixture | 0.15 | 0.18 | 0.17 | 0.22 |
| issue_comment | 0.86 | 0.79 | 0.90 | 1.42 |
| issue_create | 30.17 | 30.14 | 30.14 | 30.17 |
| issue_search | 1.89 | 1.91 | 1.85 | 1.96 |
| issue_update | 30.16 | 30.14 | 30.18 | 30.11 |
| issue_view | 0.73 | 0.94 | 0.82 | 1.13 |
| project_list | 30.13 | 30.14 | 30.17 | 30.10 |
| safety | 0.00 | 0.00 | 0.00 | 0.00 |
| service | 4.00 | 3.88 | 4.11 | 4.49 |
| tui_input | 0.56 | 0.57 | 0.56 | 0.59 |
| tui_process | 57.26 | 57.81 | 57.43（失敗） | 175.25 |
| tui_shell | 0.34 | 0.35 | 未到達 | 0.35 |

同HEADのCI Test stepは431秒、うちコンパイル51.67秒、suite合計379.07秒。Clippyは約16秒、audit binaryはcache hitで導入skip、監査は約3秒。CIのCPU数／起動条件差を固定せず、ローカル150秒をそのままCI目標にしない。CI全体やcold buildの短縮を達成済みとはしない。

### Probeの結論

全て一時ディレクトリ・fake key・loopbackのみで実施し、製品・テストの追跡ファイルは変更していない。

1. **主な確定ボトルネックは実timeout待機の配置。** 5関数に計6回の30秒待機があり、Cargoが別suiteを順に実行することで合計約180秒を占める。6ケースを独立server／実CLIで同時に開始すると、group wall **30.16秒**、各ケース30.012〜30.019秒で、networkエラー、status、created／updated、要求数が全て期待どおりだった。delay32の中断試験はsignal後に終了するため、全てが32秒待つわけではない。
2. **残りの診断値は達成値ではない。** 上記5関数だけを意図して除いた診断はwall89.67秒だったが、PTYが1件失敗し後続suite未到達。正式合格・恒久skip案ではない。この値＋6ケース30.16秒の約120秒は目標設定の参考に限る。
3. **起動は現状の主因ではない。** 単独10回の中央値はserver ready99.9ms、別に同条件で計った証明書生成38.9ms、CLI help10.0ms、CLI＋HTTPS16.3ms、Python検証HTTPS3.7ms。証明書値は別生成の標本で、server readyとの差を厳密な内訳にしない。独立20ケースのfixture＋CLIを並列1／4／10で実行したwallは2.512／0.662／0.349秒。常駐化・証明書共有は利益と管理コストが釣り合わず見送る。
4. **TLS fixtureのhead-of-line blockingを制御確認。** silent TCP peerを0.8秒保持すると、別の検証HTTPSも0.811〜0.824秒待った（3回）。一時wrapperで `do_handshake_on_connect=False` としhandler側のI/Oへ握手を遅延すると、別要求は3.9〜5.4msで進行した（3回）。既存のlistening SSLSocketはaccept時にhandshakeすることをローカルstdlibの一次実装で確認した。TLS検証の無効化ではない。
5. **PTY全面改修は先行しない。** 既存comment PATCHシナリオ3回はwall2.485〜2.514秒。別のcProfile標本2.398秒のうち、screen_textは154回・0.460秒、select待ちは1.892秒。全履歴再構成のコストはあるが、cache／VT emulator全面改修は今回見送る。4つの現行PTY試験を同一binaryで並列1／4／10にすると38.868／16.734／16.788秒、全て成功。これは部分workloadの比較で、全suiteやCIの最適並列数の証明ではない。

Probe途中には、comment-update入力fixtureの必須field不足と、globで古いtest binaryを選んだ診断ミスがあった。該当値は採用せず、正しいfixture／Cargoログが指定する現行binaryでの標本だけを上記に記載した。失敗標本の成功への置換や製品testの無条件retryとは区別する。Implementationでもbinaryをglob先頭から推測しない。

### 観測した不安定性と未確定事項

- full-3: `bl_update_cancelled_loading_and_older_detail_responses_cannot_replace_a_new_form_or_success` が8秒cutoff。画面はSending、残りはComment refreshと終了。HTTP原記録は通常cleanupで失われており、原因は未確定。
- timeout除外診断: `bl_update_editor_failure_keeps_latest_raw_body_other_fields_and_focus` のUTF-8ケースが8秒cutoff。画面はeditorエラーを示すBody edit、残りはConfirm update以降。回収した要求は15 GET・書込みなし。該当stepはresizeとキーを同時に送るが、それだけで原因確定とはしない。
- 過去の初期Detail到着約5秒差、今回の2失敗、制御確認したTLS blockingを同じ原因と断定しない。実Backlogの遅延とも混同しない。

## 検証を置く層と維持する保証

| 境界 | 維持する検証 | 今回の扱い |
| --- | --- | --- |
| 共通library／Shell→TestBackend | 本文・原文保持、安全表示、Unicode、入力・layout・focus | 既存suiteを維持。純粋な表示は既存の軽い確認をfocusedに使う |
| Service公開入口＋HTTPS | 名前／ID、既定値、差分・no-op、API入出力・確度 | 子process／TLS trust隔離を維持。共有processでset_varする方式に変えない |
| HTTP送受信 | HTTPS・認証header・method/path/form・単一送信・redirect/retry禁止・timeout | 実通信を維持。共通fixtureのみ改善 |
| 実CLI | argv/stdin/stdout/stderr/exit、非対話、出力失敗・確度、中断 | CLIごとの経路を維持し、長いtimeoutケースの配置だけ集約 |
| 実TUI／PTY | 実キー・signal・resize・editor端末所有、復元、実HTTPと状態の結合 | 既存シナリオとassertを維持。証拠に基づく同期を限定修正 |
| 実端末／実Backlog | IME・字幅・実API権限／データとの結合 | 今回実施しない。fixture成功をこれらの保証に読み替えない |

Serviceのraw保持とCLIのsafe表示、TestBackendとPTYは責務が異なり、同じ入力があっても直ちに重複検証とはしない。現公開Shell APIだけでは任意の取得結果をinjectできないため、PTYの大規模移設には別設計が必要。今回は新しいtest専用公開APIやprivate構造固定テストを作らない。

## 実装方針

### 1. 共通HTTPS fixtureと失敗診断

- `tests/fixtures/https_server.py` のTLS handshakeをhandler側へ遅延し、一つの未完了handshakeがaccept全体を塞がないようにする。既存ThreadingHTTPServer、loopback動的port、検証可能な自己署名証明書、0700 root／0600秘密鍵、子process限定trust、kill／wait後cleanupを維持する。
- `tests/https_fixture.rs` から、silent TCP peerを開いたまま別の証明書検証付きHTTPS要求が完了することを検証する。peerを閉じてから成功する弱い試験にせず、有限の待機と必ず実行するcleanupを用いる。逆引き禁止の既存試験とtrustなし拒否も維持する。
- `tests/fixtures/tty_shell.py` はdeadline／例外経路のcleanup前にも、step index、経過時刻、送信stepの時刻、受信済みmethod／queryを除いたpath／ordinal・件数など、原因切り分けに必要な限定情報を出す。成功時JSONだけの拡張で済ませない。
- diagnosticsへheaders／body／API key／config／secretを含み得るqueryをdumpしない。新しい永続ログ管理や失敗root保存ライフサイクルは設けず、既存テスト出力を使う。

### 2. 実30秒timeoutの最大6並列集約

`tests/request_timeout.rs` を作り、単一集約テスト内の標準libraryのscoped threads等で6ケースを開始し、全てjoinしてケース別に成否を報告する。各ケースは独立Harness／server／CLIを使い、環境変数・応答sequence・受信記録を共有しない。最大並列数は6で固定し、全suiteの並列数や製品Tokio worker数は変えない。Cargoのsuite境界により、通常の正式実行ではこのgroupとPTY suiteを同時に実行しない。

| 移設元 | ケース／operation | 成功headers後の期待 | HTTP要求 |
| --- | --- | --- | --- |
| project_list::body_reading_is_inside_the_thirty_second_request_timeout | project list | network、httpStatus200、writeなし | projects GET 1回 |
| issue_create::create_body_timeout_retains_created_without_retry | issue.create／POST | network、201、created | issues POST 1回 |
| issue_update::update_body_timeout_keeps_updated_without_retry | issue.update／PATCH | network、200、updated | issue事前GET＋同対象PATCH各1回 |
| comment_update::success_body_timeout_retains_updated_without_retry | comment.update／PATCH | network、200、updated | 対象comment GET＋myself GET＋対象comment PATCH各1回 |
| comment_add::body_timeouts_keep_success_certainty_without_retry_for_post_and_patch | comment.add／POST | network、201、created | comments POST 1回 |
| 同上 | comment.add／PATCH | network、200、updated | issue事前GET＋同対象PATCH各1回 |

全ケースで実 `bl-cli`、既存の引数・form・認証／HTTPS設定、delay32、exit1、stdout空、stderrのJSON形式・改行・必要なapiErrors、operation／outcome／httpStatusを維持する。要求method／path／件数を照合し、retry／追加GET／fallbackがないことも維持する。CLI開始直前からの各ケースの既存 `29秒以上40秒未満` を残す。

移設元5関数は新groupで同じ6ケースが実行されることを確認してから除く。テスト数の347への固定ではなく保証対応で移設漏れを検出する。signal中断試験や他のdecode／切断／拒否／表示失敗は移動・削除しない。新しい共通Service／transport抽象化は作らない。

最大6並列は資源数の上限であり、Harnessのport読取やCommand待機をハードな期限で打ち切る機構ではない。既存待機がhangする限界は残し、今回のための汎用watchdogは追加しない。group全体40秒以下という性能判定と、各ケース29〜40秒のassertは別に扱う。

### 3. PTY同期の診断と根拠のある限定修正

- 観測した上記2試験を、失敗情報を取れる状態で確認する。成功だけで解消とせず、対象要求・キー送信・editor復帰・resize／描画のどこまで進んだかを区別する。必要な最小の制御遅延や入力分割は試験目的を固定して行う。
- HTTP受信の同期不足が確認されたstepには、既存SHELL_REQUESTS_FILEを使ったmethod／path／要求ordinalの一致待ちを限定追加する。stickyな汎用get-receivedやbusy表示だけを「対象要求送信済み」の証拠にしない。requestsは一度読み、同じsnapshotから条件・件数を得る。
- HTTP受信は描画完了・成功headersのclient側観測・editor復帰・resize適用の証明にはしない。画面／editor／resizeの問題が確認されたstepだけ、必要な画面条件で操作を分離する。先に「受信待ちを足せば直る」「resizeが原因」と決めない。
- 遅延した旧成功／旧失敗が新しいdraft・busy・登録結果を上書きしないこと、否定の観測窓、旧要求の消費数、固定対象への単一書込み、Latest／旧List保持を維持する。1.3／1.4／1.6秒等の遅延・観測待ちを一律削除しない。
- 既定8秒、既存の複合1試験だけ15秒という現在の予算と、中断elapsed<5秒をまず維持する。前者は製品要件ではないが、一律延長で解消しない。根拠のないskip／ignore・成功までのretryはしない。
- 製品原因、fixtureだけでは保証を維持できない変更、根拠の得られない修正が必要になった場合は未完了として停止し、Plan Reviewを含む再計画へ戻る。未承認の製品改変や大規模seam再編へ拡張しない。

### 4. 今後のテスト作成基準・focused確認・正式チェック

`CONTRIBUTING.md` に、今回の移設説明だけでなく、今後の実装・テスト追加にも適用する「テストの設計・追加方針」を設ける。人とAgentが同じ基準を使い、次を恒久的なルールとする。

1. **まず保証と境界を選ぶ。** 入力検証・変換・表示・状態の規則は、既存の適切なlibrary／Shell→TestBackend境界で検証する。CLIのargv／stdin／JSON／exit、HTTPS認証・送受信、signal／PTY／editor／復元など、その境界自体を検証する場合は実processを使う。「小さいテストがよい」という理由で固有の保証を落とさない。
2. **既存の保証を再利用する。** 新しい入力組合せのたびに同じ30秒timeoutや同じ端末往復を増やさない。既存の共通transport試験が保証するものと、新機能で追加する固有の挙動を区別する。新しい独立したtimeout契約が本当に必要ならtimeout groupに配置し、最大6並列・実行予算を再評価する。ケース増加時も無制限spawnにはせず、必要な変更はその実装のPlan／Reviewで決める。将来用のqueue／frameworkは今回作らない。
3. **試験方法も設計する。** 新しい固定待機には理由と観測窓を記す。要求受信、headers観測、描画、editor復帰、resize適用を混同せず、可能なら目的に合う条件待ちを使う。否定の検証に必要な有限時間は残す。global環境変数・共有fixtureの競合、bounded cleanup、失敗診断も確認する。
4. **追加時の説明を残す。** 変更の説明・Reviewでは、追加／維持した保証、選んだテスト境界、既存検証との役割分担、待機／起動を増やす理由、focusedコマンドを簡潔に示す。新規テストの実行対象への登録漏れと、他suiteへの30秒待機の重複をReviewで確認する。新たな専用台帳や必須テンプレート管理は設けない。
5. **開発ループと正式確認を分ける。** 各変更途中はfocused、最終差分で正式チェックを行う。全対象結果を不要に繰り返す構成にしないが、変更後の必要な検証を過去の合格で代替しない。

新規 `AGENTS.md` は、このCONTRIBUTINGのテスト方針をテスト追加・変更前に読むよう案内する短い入口に限定する。上記ルールや製品仕様の二重管理、新しいAgent／Extension／workflow設定は作らない。既存のAGENTS.mdがImplementation開始時に見つかれば上書きせず、既存規約に合わせ最小限に統合する。将来のAgentが必ず規約を守ることを技術的に強制・保証はしない。

同じ節に、純粋library／Service／CLI／PTY／timeout groupのfocused例と、正式all-targetsの違いを記す。focusedは開発ループ用であり、全対象の代替ではない。timeoutを通常の各CLI focused確認から外すのは、新suiteへ保証を移設したためであり、正式all-targetsでは毎回全6ケースを実行する。

既存の正式コマンド・CI trigger・runner・監査方針は維持する。`.github/workflows/quality.yml` の全対象選択・Python SSL／PTY・OpenSSL前提との整合を確認する。変更が不要なら編集しない。CI cache／sharding／runner追加、監査DBの固定、監査結果cacheは行わない。

今回の実測と失敗・未確認事項は既存 `docs/testing/manual-acceptance.md` へ今回の新Work Itemの記録として追記し、Issue #22の過去の失敗・訂正・受入記録を書き換えない。大きい初期Plan・基本設計・READMEの製品契約を変更する必要はない。

## 作業単位と30分枠への対応

技術的な順序は、(1) TLS blocking回帰とfixture改善・失敗診断、(2) timeout集約、(3) 証拠に基づくPTY限定修正、(4) 最終計測・回帰・今後のテスト作成基準と導線の文書整合。実際のWork Itemsの分割・依存関係・本文はPlan承認後にticket Skillで別途確認する。並行writerは使わず、同じcwdの変更は順に扱う。

- 中間作業は関連focused確認と必要なチェックを行う。共通fixtureの影響確認として全対象を実行する場合も、その差分の1回とし、各項目で性能full3回・release一式を繰り返さない。
- 最終性能・回帰確認は、速いtimeout groupを導入しPTYの限定修正を終えた最終差分で行う。正式full3回・重点10反復・release確認を各Workerへ重複依頼しない。
- 最終検証自体も独立した小さいWork Itemにできる。途中で差分が変われば影響する検証は無効で、旧結果を流用しない。
- Worker枠内に収まる見込みがなくなったら、変更済みファイル・実行中process・終了済み検証／ログ・未完了検証を報告して引き継ぐ。30分以内の実装完了そのものを保証するものではなく、harness設定・新しい永続workflow状態は追加しない。

## 完了条件

1. 実30秒timeoutの6ケースが独立環境・最大6並列で毎回の正式all-targetsに含まれ、各固有のHTTP経路・status・確度・入出力・時間・単一送信の保証を保っている。
2. silent TCP peerを保持したまま別の検証HTTPSが進行する回帰試験が成功する。逆引き不要・trustなし拒否・認証とfixture cleanupが回帰しない。
3. PTYのdeadline／例外経路で安全な診断を得られる。今回の2失敗について観測と修正根拠を記録し、証拠のある同期だけを改善する。根拠未確定のまま「解消」としない。
4. 同じ最終差分の固定反復で下記の性能・安定性条件を満たす。失敗／未実施を隠さず、未達はtimeout・期待を緩めず再検討する。
5. 最終の正式チェックと変更後treeのrelease localhost HTTPS／PTY代表確認が成功し、focused／正式の説明とCI設定が整合する。実CIは設定確認まで、未実行を明記する。CONTRIBUTINGに今後のテスト作成基準があり、AGENTS.mdから参照できる。検証境界・重複した実時間待機の回避・実processを残す条件・追加時のReview観点が、今回限りの移設手順ではなく将来の実装でも利用できる。
6. 製品ソース・公開契約・依存／lockfile・実行方式が不変。Issue #22や他機能の完了判定・受入を今回へ取り込んでいない。

## 動作確認方法

### TDDと公開seam

新しいfixtureの振る舞い・同期判断には既存 `tdd` Skillを使う。一つの失敗試験→最小修正の縦sliceで行う。事前合意するseamは以下で、PlanのApproveにこの検証方針を含む。

- Python HTTPS fixtureの起動→証明書検証付き要求／受信記録：TLS blocking・認証／HTTPを捕捉するが、実Backlogの権限は保証しない。
- PTY driverのstep入力→子process入力／結果・失敗診断：未成立条件で進まないこと、例外時の証拠・cleanupを捕捉するが、実IMEは保証しない。
- 実bl-cli／実blの既存公開process境界：timeout確度、単一書込み、画面・中断・復元を捕捉する。

timeoutケースの単なる移設に形式的な新規REDを要求せず、6ケースの保証対応と既存assertの維持を検証する。private関数名・内部thread/task数を製品テストで固定しない。

### 今後のテスト作成基準の確認

文書Reviewで、(a) 純粋な入力検証／表示、(b) CLI引数／JSON契約、(c) PTY signal／editor復元の3種類を既存テストの具体例へ対応させる。各例で、追加する保証・使用する既存境界・focusedコマンド・実processを残す理由または不要な理由を文書から判断できることを確認する。例示のために新機能や不要な重複テストを実装しない。AGENTSのリンクを辿って正しい節へ到達でき、CONTRIBUTING・正式all-targets・CIの説明に矛盾がないことも確認する。

### 性能比較

同等のApple Silicon環境・固定Rust／依存・同じworkloadで、競合するbuild／テストを止めて行う。HEAD・diff／lock hash、CPU／OS、親PythonとPTY Python、SSL／OpenSSL、RUST_TEST_THREADS、実コマンド、終了コードを記録する。

1. `cargo test --locked --all-targets --no-run` のbuild時間を別記録する。binaryのwarm-upには必要なら `cargo test --locked --all-targets -- --list` を使い、実行対象一覧も確認する。最初の実行にだけ大きい起動差があれば捨てずに別記する。
2. 最終差分を固定し、`/usr/bin/time -l cargo test --locked --all-targets` を**事前固定の3回**実行する。3回全ての成功を要求し、wall中央値・範囲・suite時間・失敗／ignored数を記録する。途中で失敗したら停止し、成功標本だけに取り替えない。修正後は別差分の測定組として扱う。
3. ローカルのwarm wall中央値 **150秒以下**、かつtimeout groupのwall **40秒以下**を成功閾値とする。成功warm baseline256.10秒に対し約41％以上の短縮。残り診断約89.67秒＋集約30.16秒を参考に約30秒の余裕を設けた目標で、約120秒を実現済みとはしない。
4. no-run build、初回起動、suite合計、Test step／job全体を混ぜない。変更後CIは設定だけを確認し、既存CIの379.07秒／Test431秒とローカル値を直接合否比較しない。将来の自然なCI結果を比較する場合もbuildとテストを分離する。

### 安定性・回帰

次の3試験を同じ最終差分・現行Cargo binaryで、1回のfocused実行にまとめた固定10反復を行う。例えばcargo testの複数filterを利用し、毎回3試験が選択されたことを確認する。focusedの並列は3とし、正式all-targetsのデフォルト並列は変更しない。

- `bl_update_cancelled_loading_and_older_detail_responses_cannot_replace_a_new_form_or_success`
- `bl_update_editor_failure_keeps_latest_raw_body_other_fields_and_focus`
- `bl_comment_add_cancelled_load_and_reversed_detail_or_comment_reads_cannot_replace_new_draft_or_registration`

10反復全て0失敗を要求するが、統計的なflake率ゼロの証明とはしない。途中失敗は停止・証拠保持し、無条件再試行しない。必要な制御遅延試験で旧成功／旧失敗の順序・消費数と、新draft／Latest／旧表示保持を確認する。

HTTPS fixtureの新回帰、既存trust拒否・認証・retry禁止、実CLIの各書込みとService、TUIのheaders前／body待ち／登録後refresh中の中断、Ctrl+Cバイト／実SIGINT、abort＋join後の確度、termios／alternate screen／cursor復元を既存focused試験と正式全対象で確認する。片側TTY拒否・editor／resize・安全表示も維持する。

最終差分で以下を実行する。testの3回は上記の性能測定と同じ実行であり、別にもう3回追加しない。

```sh
cargo fmt --all -- --check
cargo test --locked --all-targets
cargo clippy --locked --all-targets -- -D warnings
cargo audit --version
cargo audit --file Cargo.lock
cargo build --locked --release --target aarch64-apple-darwin --bins
git diff --check
```

cargo-audit 0.22.2、通常のDB更新、lockfile監査前後のhash不変を確認し、警告・失敗を報告する。監査やrelease buildの結果を過去の合格で代替しない。変更後treeでbuildしたreleaseを、更新した共通fixtureと既存CONTRIBUTINGの自己完結localhost HTTPS／PTY手順で直接起動し、POST／PATCH代表の単一送信・原文／安全表示・Latest／旧List・exit／復元を確認する。実Backlog通信・データ準備・書込みは一切行わない。

## 制約・対象外・残るリスク

- HTTPS／認証／secret保護、単一書込み、redirect／retry禁止、成功確度、中断・端末復元の保証を弱めない。TLS検証回避、HTTP化、製品test flag、全体のtimeout延長、根拠のないskip／ignoreを追加しない。
- 製品コード、API／Service／CLI／TUI公開入口、runtime worker数、依存、Cargo export／feature、既存基本設計・初期Planの製品契約は変更しない。必要なら停止・再計画する。
- 証明書共有、常駐server、全PTYの軽い境界への移設、clock／transport抽象化、VT emulator／screen cache、CI cache／sharding、Worker設定の変更は対象外。
- 根拠を得たTLS blocking改善は、元の約5秒差や観測2失敗の同一原因を証明しない。PTYの正確な原因が未確定であること、初回wall差の未特定コストを引き継ぐ。
- 最大6並列はhardな終了期限ではない。既存のprocess起動／port読取待機のhang、任意のOS負荷、全端末・IMEの差までは解消・保証しない。
- CIの設定整合とローカル成功だけで、変更後の実CI成功や特定のCI短縮率を断言しない。実CI未実行を残す。新しい実端末／実Backlog受入を本作業の必須にせず、旧受入を今回の実測へ流用しない。
- Planning成果物は本ファイルと新Work Items。Issue #22の状態・本文・コメントを変更せず、Implementation・commit・merge・release・deployを自動開始しない。

## 承認・保存・受渡し

明示Approve後、この全文を `docs/plans/test-infrastructure-performance.md` に正式保存してreadbackする。途中のtool出力・Scout／Oracle会話・一時Probe scriptを正式入力として引き継がず、必要な結論は本Planと解決可能な一次参照に集約する。

その後 `taskflow-to-ticket` Skillを明示的に読み、本Planの正式パスと全文を渡す。既存tracker設定の確認はticket Skillだけで行い、別の新Work Itemsとして、分割・依存関係と最終本文／保存公開先をそれぞれ承認してから保存・公開する。各項目から本Planを参照可能にする。一部だけの保存・公開ではPlanning完了とせず、全項目のreadback確認後に停止する。Implementationは別の新しいセッションで行う。
