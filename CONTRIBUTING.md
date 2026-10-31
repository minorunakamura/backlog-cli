# 開発

## 環境

対象はApple Silicon Mac（`aarch64-apple-darwin`）。RustupとXcode Command Line
Toolsを用意してください。`rust-toolchain.toml` にRust **1.99.1**、rustfmt、Clippyと
targetを固定しています。リポジトリ直下で次を実行すると必要なtoolchainが導入されます。

```sh
rustup show
cargo build --locked --bins
cargo install cargo-audit --version 0.22.2 --locked
cargo audit --version
```

単一Cargo packageに共通library (`src/lib.rs`) と `bl` / `bl-cli` を置きます。
主要依存は `Cargo.toml` で完全一致版を指定し、推移依存を含む `Cargo.lock` を追跡します。
RatatuiはCrossterm 0.29 backendを明示しています。共有単行入力はtui-input **=0.15.5**の
Crossterm公開APIを使用します。Herdr/Ghostty・macOS標準の日本語入力で早期実端末受入を行い、採用しました。全端末/IMEでの一致を保証するものではありません。`bl-cli project list` と候補一覧3コマンド、
`issue list/mine/view/create/update` / `issue comment list/view/add/update` と共通libraryの候補/名前解決/検索/詳細・コメント取得/追加/本文更新・課題作成/部分更新・Markdown/Backlog本文の安全表示を実装しています。
`bl` のプロジェクト選択・通常/横断検索ダイアログ・課題一覧ページ巡回/再取得・選択追従プレビュー/詳細・コメント閲覧・ブラウザ起動・focus・layout・端末復元も実装しています。
共有単行入力部品を製品のkeyword欄へ組み込み、開発専用の実端末確認経路も維持しています。
設定editor連携と共有libraryの本文編集、TUI課題作成・更新フォームを実装しています。
#20の実端末／実Backlog受入結果と#21の未実施事項は手動受入記録を参照してください。コメント追加フォームのローカル検証と実環境の未実施事項は末尾の#22節を参照してください。既存コメント本文編集は後続Work Itemの範囲です。

## 品質確認

ローカルとCIで以下を実行します。cargo-auditは **0.22.2** を上記手順で導入してください。
脆弱性や警告は無条件ignoreで隠さず、内容と終了コードを報告してください。
監査はリポジトリ直下で実行し、`Cargo.lock` を生成・更新しません。

```sh
cargo fmt --all -- --check
cargo test --locked --all-targets
cargo clippy --locked --all-targets -- -D warnings
cargo audit --file Cargo.lock
```

targetのrelease build確認:

```sh
cargo build --locked --release --target aarch64-apple-darwin --bins
```

`.github/workflows/quality.yml` は `main` へのpushとpull requestで上記4チェックを
macOS 15のApple Silicon runnerで実行し、同じ版のcargo-auditを導入します。
CIではcargo-auditのbinaryだけを `actions/cache` で再利用し、cache miss時に上記の固定版を
ソースからインストールします。cache keyはmacOS版・CPU architecture・cargo-audit版・
`rust-toolchain.toml` の内容で分け、復元後も `cargo-audit --version` が0.22.2であることを確認します。
advisory databaseや監査結果はcacheせず、更新と `cargo audit --file Cargo.lock` は毎回実行します。
初回・cache miss時はコンパイルが必要です。PRのcacheはそのPRにscopeされるため、mainのcacheが
作成されるまでは、別PRで再びcache missになる場合があります。
GitHub上で未実行ならその旨を記録し、確認のためだけのpushやPR作成は行いません。

## テスト設計とReviewの基準

### 保証から公開境界を選ぶ

追加・変更・Reviewでは、まずユーザーに必要な保証を言葉にし、既存のどのassertを再利用できるか確認してから、適切な公開境界を選びます。

- 純粋な入力検証・変換・表示は、既存libraryまたはShell→TestBackend境界で確認します。実CLIやPTYを起動して同じ組合せを繰り返す必要はありません。
- argv／stdin／stdout／stderr／exit／JSON、実HTTPSの認証・送受信には実processを使います。signal、端末入力の所有権・引渡し・復元には実process／PTYが必要です。`src/` 内のテストでも、実PTYを起動するものを純粋なunit testとは扱いません。
- 共通処理の全失敗種別・全field matrixは、その保証に適した境界へ置きます。機能ごとの端末試験は、固有の画面・draft・focus・送信・復元などを持つ代表ケースを優先します。
  代表化では残すassertと削る交差保証を説明してください。例えば共通editorのutf8／unlink検証を残しても、その失敗と各フォームのfocus／draftの結合確認は減ります。全field検証が残っても、各field不正とPATCH成功後の表示／確度の組合せまで完全代替したことにはなりません。

### 待機・起動と観測の成立条件

- 要求受信、client側のheaders観測、描画完了、editor復帰、resize適用は別の事象です。HTTP受信記録だけで後者を証明せず、必要な画面条件・marker・端末状態を併用します。件数だけを順序の証拠にしません。
- 否定確認や逆順応答で、後から送信・確度変更・旧表示の混入がないことを見る有限の観測窓は残します。全待機を受信gateへ置き換えるのではありません。
- 総HTTP件数を固定する場合は、その件数が保証する契約との関係を説明します。付随要求込みの総数と、対象method／pathの受信・回数は区別します。総数を外すなら、監視しなくなる付随要求の増減も明記します。単一書込み・retry禁止などの契約を弱めません。
- 新しい実時間待機・process／PTY起動を加える理由、既存試験との役割分担、focusedコマンドを変更説明・Reviewに短く残します。削減だけの変更に形式的なREDや不要な代替テストを増やしません。

### 実在テストへの対応とfocused例

以下はリポジトリ直下からコピーして使える例です。各コマンドは名前を完全一致で1試験選ぶ想定です。
実行結果の試験名・選択件数を確認し、**選択0件を成功としない**でください。関数の移動・改名時はfilterも確認します。

**純粋な変換・表示:** [tests/body.rs](tests/body.rs) の
`markdown_headings_and_nested_emphasis_keep_lines_and_unicode` は公開 `body::display` から改行・Unicode・原文保持を確認します。
[tests/tui_shell.rs](tests/tui_shell.rs) の
`shell_shows_all_five_panels_only_at_the_full_layout_boundary` は公開Shell→TestBackendで100x24とcompact境界の表示を確認します。
どちらも製品process／実端末は不要ですが、実signalや端末復元の保証にはなりません。

```sh
cargo test --locked --test body markdown_headings_and_nested_emphasis_keep_lines_and_unicode -- --exact --nocapture
cargo test --locked --test tui_shell shell_shows_all_five_panels_only_at_the_full_layout_boundary -- --exact --nocapture
```

**実CLI・HTTPS:** [tests/cli_help_version.rs](tests/cli_help_version.rs) の
`bl_cli_help_needs_neither_configuration_nor_a_terminal` は空設定・環境変数隔離・非TTYで実 `bl-cli` を起動し、help引数、stdout／stderr／exitを確認します（HTTPSは使いません）。
[tests/issue_view.rs](tests/issue_view.rs) の
`json_view_preserves_original_fields_and_builds_url_without_project_get` は実CLI＋証明書検証付きローカルHTTPSでJSON原文・構築URL・認証header・対象GETだけであることを確認します。
libraryの値比較だけでは、CLIの入出力や実認証・送受信は保証できません。

```sh
cargo test --locked --test cli_help_version bl_cli_help_needs_neither_configuration_nor_a_terminal -- --exact --nocapture
cargo test --locked --test issue_view json_view_preserves_original_fields_and_builds_url_without_project_get -- --exact --nocapture
```

**PTY固有のsignal・editor復元:** [src/tui.rs](src/tui.rs) の
`tui::tests::body_editor_roundtrip_and_failures_use_shared_terminal_handoff` は共通loop／guardから実PTY／editorへ引き渡し、cooked入力の専有、原文・他欄保持、一時file削除、中断・復元失敗を含む端末復元を確認します。
同fileの `tui::tests::shell_restores_when_sigint_arrives_during_entry_before_the_first_frame` は初回描画前のSIGINTと130終了・復元を確認します。
通常q終了やTestBackendの描画では代替できません。Cargoの完全一致filterには `tui::tests::` も含めます。

```sh
cargo test --locked --lib tui::tests::body_editor_roundtrip_and_failures_use_shared_terminal_handoff -- --exact --nocapture
cargo test --locked --lib tui::tests::shell_restores_when_sigint_arrives_during_entry_before_the_first_frame -- --exact --nocapture
```

### 開発途中と最終差分の検証

開発途中は変更箇所と受け皿となる既存試験をfocusedで確認します。最終差分では上記[品質確認](#品質確認)の正式チェックとrelease buildを行い、focusedで代替しません。
既存manual用2 ignoredは実端末確認用として区別し、新しいskip／ignore、成功までの無変更retry、timeout延長や期待の無差別緩和で失敗を隠しません。
固定反復や性能閾値は、必要な問いと予算がある場合にだけ別途決めます。無意味なRED・反復・性能ノルマを保証確認の代替にしません。
失敗時はその実行を停止してassert・安全な診断と差分の関係を確認し、後の成功で失敗記録を置き換えません。無関係の既存失敗・製品原因・原因不明なら、対象を無断で広げず未完了として報告します。

#57の最終検証は[改訂Plan「動作確認方法」](docs/plans/test-infrastructure-performance.md#動作確認方法)と[優先追補](https://github.com/minorunakamura/backlog-cli/issues/57#issuecomment-6545857516)に従い、文書を含む最終差分で正式チェックを各1回行います。
cargo-audit 0.22.2・通常DB更新・監査前後のCargo.lock hash不変を確認し、全体試験は `/usr/bin/time -l cargo test --locked --all-targets` の同じ1回からwall／suite時間を参考値として記録します。旧workloadとの厳密な速度比較・中央値・flake不存在の証明にはしません。
HEAD／diff・環境・コマンド・終了コード・警告・成功／失敗／ignored・未実施事項を[手動受入記録](docs/testing/manual-acceptance.md)へ追記し、過去の成功を新差分の証拠へ流用しません。コード／fixtureが変わった場合は影響する検証を取り直します。
今回は固定3回・10反復・性能閾値・別途release直接再演を要求しませんが、正式release buildと既存の実30秒timeoutの6ケース・各CLIの時間assertは維持します。
実CIは既存workflowとの設定整合までとし、実CI・別途release直接操作・実端末／IME・実Backlogは今回未実施と記録します。既存release再現手順・fixturesや他の機能Issueの受入条件は変更しません。

## テストと今回の範囲

承認済み [Plan](docs/plans/backlog-cli-initial.md) と
[基本設計](docs/designs/backlog-cli.md) を参照してください。
新しい振る舞いは公開境界を一つの失敗テスト→最小実装の縦sliceで検証します。
初期基盤の `tests/cli_help_version.rs` は、実CLIプロセスのargv/stdin/stdout/stderr/exitを
境界に、環境変数を消し、空のHOME/XDG_CONFIG_HOME/cwdと非TTYでhelp/versionを確認します。
内部private関数の名前や実装詳細はテストに固定しません。

`tests/project_list.rs` は実CLIとローカルHTTPS送受信境界で、設定の厳密検証・認証header・
固定表示/JSON・redirect/retry禁止・Ctrl+C・出力失敗を確認します。
PTYでも入力を与えず完了し、NO_COLORと端末制御/secret保護を確認します。
`tests/candidate_list.rs` は3一覧のflag適用先、project既定値/明示優先、固定表示/JSON、
空一覧と不正応答、global/profile、不要GETなし、secret保護を実CLIで確認します。
`tests/service.rs` は公開 `Service::candidates/resolve/myself` から、全候補のpath/header、
完全一致/曖昧さ/数値名、巨大ID、直接IDと不正ID、resolution 0、self、API失敗伝播、cacheなしを確認します。
`tests/issue_search.rs` は実CLI/HTTP境界で通常・mine・複数project横断、既定値/解除、
OR/AND・名前/ID排他・重複除去・巨大ID、count/list同条件と1ページのみ、空ページ/total差、
失敗/不正応答、JSON原文と安全表示、TTYでの非対話を確認します。
`tests/issue_view.rs` は実CLI/HTTP境界で全詳細項目・nullable項目・複数milestone・Markdown/Backlog表示と原文fallback、
未知field/巨大整数/通常文字列・secret/key衝突・危険制御、ID/keyと構築URL、JSONのproject GETなし、
不正応答/API失敗・出力失敗・help/global flags・TTYでの非対話を確認します。
`tests/issue_comment.rs` は実CLI/HTTPS境界でlist/view、降順1ページ・exclusive min/max・同時境界・空ページ、
limit/ID/順序の入力検証、本文なし履歴と本文/履歴同居、既知/未知fieldの前後値、安全表示、JSON原文/大整数と
issue/project GETなし、全件null履歴はissue/projectが403でもコメントGETだけで安全表示できること、
文字列本文が混在すれば記法取得の失敗を隠さないこと、API/不正応答・出力失敗・help/global flags・TTY非対話を確認します。
`tests/service.rs` の公開 `Service::comment_list/comment_view/issue_text_formatting_rule` 試験でも、
安全表示前の本文・履歴・未知field/巨大ID保持、入力検証時の通信なし、対象課題のprojectIdだけでの記法取得、
API失敗の伝播を確認します。
`tests/issue_create.rs` は実CLI/HTTPS境界で必須入力・既定値/明示優先/担当者解除、名前/self・巨大ID・不要候補なし、
実在日付・milestoneの重複除去、literal/file/明示stdinのUTF-8・空本文・原文form・排他/単一flag反復を確認します。
実際のcreate commandだけに引数・設定エラーのwriteを付け、readの値tokenを誤認しないことも検証します。
単一POST・redirect/retry/確認GETなし、信頼できる4xx拒否と不正拒否/5xx/切断、201後のbody切断/decode/必須field/
出力失敗・redaction key衝突での確度保持、送信前/送信開始後/201 headers後のCtrl+Cを確認します。
stdin待ち中断・stdin非明示時の非読取・global/profile・TTY非対話も実processで試験します。
`tests/service.rs` の公開 `Service::issue_create` 試験は、送信本文と返却object/巨大整数の原文保持・URL追加、
既定値/直接ID・解除・入力検証前のPOSTなし、201後と名前解決時のエラー確度を確認します。
`tests/issue_update.rs` は実CLI/HTTPS境界で最新GETと単一PATCH、明示入力だけの差分form、設定非流用、
名前/直接ID/self・resolution 0と解除、日付のcalendar比較、milestone集合の置換/重複除去/順序不問と空配列解除を確認します。
全項目同値/未指定/null・空本文のno-op、title/日付/flag/UTF-8検証、literal/file/明示stdin・空入力/本文省略、
非明示stdinの非読取、global/profile/help・TTY非対話、未知field/巨大整数/原文と構築URL・安全表示を検証します。
事前エラーはPATCHなし、具体的4xx拒否・切断/5xx/redirect・200後のbody/decode/必須field/出力エラー・key衝突、
stdin/事前GET/送信後/200 headers後のCtrl+Cでも確度を保持し、retry/redirect/確認GETがないことを確認します。
200後の中断は `patch-headers-sent` markerを待ち、事前GETの200 markerとは区別します。
`tests/service.rs` の公開 `Service::issue_update` 試験でも原文/巨大ID・URL、比較に使うIDの検証と未使用field非要求、
直接ID/部分更新・no-op・入力検証時の通信なし・日付順序の独自制限なし・確度を確認します。
`tests/comment_add.rs` は実CLI/HTTPS境界で本文のみ/同値指定時の単一POST、実変更時の最新GET+単一PATCHを検証します。
UTF-8原文のliteral/file/明示stdin・空/未指定本文、名前/ID/解除の排他と単一指定、resolution ID 0、対象projectでの名前解決とself、
create既定値非流用・状態に伴う完了理由の自動変更なし、同値/変更なしの非書込み、raw data/未知field/巨大整数とurl非追加を確認します。
引数/config/本文/事前GET/名前解決失敗時はcomment.add/not_created、実方式選択後はPOST/PATCHそれぞれの確度語彙を確認します。
4xx拒否・切断/5xx/redirect・201/200後のbody/decode/必須field/出力失敗・key衝突と、stdin待ち/GET/送信後/成功headers後の
Ctrl+Cでも確度を保持し、retry/権限fallback/二重書込み/コメントID補完GETがないことをmethod/path/formと件数で照合します。
`tests/service.rs` の公開 `Service::comment_add` 試験でも原文/secret・巨大整数、未使用field非要求、ID0の解除、
入力検証時の通信なしと確度、返却dataのurl非追加を確認します。
`tests/comment_update.rs` は実CLI/HTTPS境界で最新コメント・認証ユーザー取得、任意精度の投稿者ID検証と所有者確認、
他者コメント/content:null履歴/同値の非書込み、必須かつ排他的な本文入力、課題項目flag拒否を確認します。
literal/file/明示stdinのUTF-8・空入力拒否・空白改行原文保持、非明示stdinの非読取、設定非流用、global/profile/help・TTY非対話、
contentだけの単一PATCH、元object/未知field/巨大整数と安全表示、引数/config/事前GET/権限拒否時の確度を検証します。
200後のbody切断/decode/必要field/出力失敗・key衝突、stdin/事前GET/送信後/200 headers後のCtrl+Cでも確度を保持し、
retry/redirect/fallback/余分なGETがないことを照合します。既存のwrite確度処理とfixtureをそのまま使います。
`tests/service.rs` の公開 `Service::comment_update` 試験でも入力検証時の通信なし、巨大対象ID、原文/secret・未知field/巨大整数保持、
返却objectのurl非変更・追加の事後所有者field非要求を確認します。
`tests/body.rs` は公開 `body::display/display_with_origin` からMarkdown/Backlogの見出し/list/強調/引用/code/checklist/通常表、
複合記法・通常改行/空行/TAB・code/escapeの保護、可視link/画像/参照、HTML/未知macroの内容保持、
難しい表/未知記法の原文fallbackを確認します。Backlogの課題linkは設定origin/keyの共通検証・URL構築を使い、
不正origin/key・code/escape内のキーを展開しないこと、Wiki/Git/SVN/添付の名前/revision/ID保持も検証します。
原文非改変、日本語/結合文字/emoji、
entityや表示変換で現れるsecret・制御文字への最終safetyも検証します。
CLI試験では非TTY/NO_COLORとPTYの装飾なし、画像/参照先の追加GETなし、JSON原文とproject GETなしを確認します。
`tests/service.rs` の公開 `Service::search` 試験では既定値適用・原文/巨大total保持と入力検証も確認します。
同fileの `Service::issue_view/project_text_formatting_rule` 試験では表示前の原文保持・URL付加、
記法確認の独立性・巨大project ID・不正記法応答/API失敗のkind/httpStatus/exitを確認します。
`tests/common/mod.rs` は既存CLI harnessの共有部分です。library試験も個別の子テストprocessで実行し、
そのprocessだけにTLS trustを渡すため、並列試験で環境変数を書き換えません。
`tests/safety.rs` は共通libraryの公開安全表示入口で、原文保持・Unicode・scalar/keyの伏字と
key衝突を検証します。内部private関数の固定テストはありません。
`tests/https_fixture.rs` はfixtureの逆引きを失敗させた別processから起動し、
証明書を検証したHTTPSリクエストが成功することを確認します。固定loopback serverの
初期化では不要な逆引きをせず、CIのresolver待ちに依存させません。

`tests/tui_shell.rs` は公開TUI入力→描画境界とTestBackendで100x24/99x24/100x23/40x12/極小・0サイズ、
左右focus/Enter/Esc、非実装キーの保護、NO_COLORを確認します。
`tests/tui_process.rs` と `tests/fixtures/tty_shell.py` は実 `bl` をPTYで起動し、片側だけTTYの拒否、
profile/help、正式な初期データと空一覧、初期API/不正応答エラー、secret/ANSI安全化、
headers前/body待ちのq・raw Ctrl+C・SIGINT終了、termios/alternate screen/cursor復元を確認します。
プロジェクトの移動/確定・設定project不在/巨大ID・実取得数offset/空/total境界・訪問済みページの
選択/scrollと再通信なし・Project/Issue R成功/失敗・同project再確定/別project切替・
遅延した成功/失敗と新busy・Esc取消も、実キー入力と描画/HTTPS記録から検証します。
共通serviceの公開resolve_search/search_resolved/search_page境界では、条件の再解決なし・
list-only取得・count失敗非隠蔽・JSON原文/巨大数保持を検証します。
macOSの一時的なPENDIN（再表示待ち）ビットだけはtermios比較から除外します。
`src/tui.rs` のPTY試験はtest binaryのWrite境界に描画エラー/panic/復元失敗を注入し、
復元の継続・属性reset・秘匿済みエラーを検証します。同じ境界で初回描画前のSIGINTと
開始済みblocking処理を再現し、端末復元とprocessの応答性も確認します。製品のdebug flagはありません。
実端末の目視/IME確認をこれらで代替しません。shellの手動手順とpending記録は
[docs/testing/manual-acceptance.md](docs/testing/manual-acceptance.md) を参照してください。

```sh
cargo test --locked --test tui_shell --test tui_process
cargo test --locked --lib tui::tests::shell_ -- --nocapture
```

### TUI課題作成 (#20): releaseの直接ローカル検証

`tests/tui_process.rs` の `bl_create_` と `bl_new_create_interrupt_` は製品PTY/HTTPS境界で、
既定値/解除・複数milestone・必須値/実在日付・単行原文/全欄伏字・全確認本文・editor/resize/失敗・
差分だけの破棄確認・確認Enterの非POSTと独立送信・busy中の操作禁止・count/listの原子的交換/履歴保持・
取消した候補取得の世代・Ctrl+C/SIGINTの3段階の確度を確認します。
タイトル／開始日／期限日は単行入力の標準編集がフォームshortcutより優先です。Ctrl+A/B/Yは先頭移動／左移動／貼戻しで、
空欄や端で変化しなくてもfocusは移りません。Ctrl+Y/A/Bで種別／担当者／本文へ移るには先にTab等で入力欄外へ出ます。
Ctrl+T/Pなど編集キーでない既存shortcut、Tab/Shift+Tab、Enter/Esc、候補popupは従来どおりです。
`tests/service.rs` の `create_task_abort_joins_without_detached_http_or_later_certainty_changes` は、
公開Serviceと共有CreateProgressから、送信前/送信後/201 headers後のabort+joinと、回収後に確度が変わらないことを確認します。
HTTP futureは外側の操作taskが直接所有します。新しいtaskを切り離したままにせず、TUI終了/panic/描画失敗でも
write taskをabort+joinしてから確度を読み、端末復元を試みます。既存CLIの確度試験も維持します。

```sh
cargo test --locked --test tui_process bl_create_ -- --nocapture
cargo test --locked --test tui_process bl_new_create_interrupt_ -- --nocapture
cargo test --locked --test service create_task_abort_joins -- --nocapture
```

以下は **cargo testとは別に、実release `bl` を直接起動する** 自己完結のlocalhost HTTPS + PTY検証です。
リポジトリ直下でrelease build後に実行してください。実設定／実keyを読み込まず、実サービスへ接続しません。
Python/OpenSSL以外の追加依存は不要です。一時rootとserverは最後に回収します。

```sh
cargo build --locked --release --target aarch64-apple-darwin --bins
python3 - <<'PY'
import json, os, pathlib, subprocess, sys, tempfile
repo = pathlib.Path.cwd()
bl = repo / 'target/aarch64-apple-darwin/release/bl'
with tempfile.TemporaryDirectory(prefix='bl-create-local-') as directory:
    root = pathlib.Path(directory)
    routes = {
        '/api/v2/space': {'body': json.dumps({'name': 'Local create check'})},
        '/api/v2/projects': {'body': json.dumps([{'id': 1, 'projectKey': 'LOCAL', 'name': 'Local project'}])},
        '/api/v2/issues/count': {'body': '{"count":0}'},
        'GET /api/v2/issues': {'body': '[]'},
        'POST /api/v2/issues': {'status': 201, 'delay': 0.3, 'body': '{"id":9,"issueKey":"LOCAL-9"}'},
        '/api/v2/projects/1/issueTypes': {'body': '[{"id":2,"name":"Task"}]'},
        '/api/v2/priorities': {'body': '[{"id":3,"name":"Normal"}]'},
        '/api/v2/projects/1/users': {'body': '[{"id":4,"name":"Alice"}]'},
        '/api/v2/projects/1/versions': {'body': '[{"id":5,"name":"First"},{"id":6,"name":"Second"}]'},
    }
    (root / 'response.json').write_text(json.dumps({'routes': routes}))
    server = subprocess.Popen([sys.executable, str(repo / 'tests/fixtures/https_server.py'), str(root)], stdout=subprocess.PIPE, text=True)
    try:
        origin = 'https://localhost:' + server.stdout.readline().strip()
        (root / 'backlog').mkdir()
        (root / '.backlog').mkdir()
        (root / 'backlog/config.json').write_text(json.dumps({
            'editor': ['/usr/bin/python3', str(repo / 'tests/fixtures/editor.py'), str(root / 'editor-record'), 'body-file'],
            'profiles': {'local-check': {'spaceUrl': origin, 'apiKeyEnv': 'TEST_KEY'}}}))
        (root / '.backlog/settings.json').write_text(json.dumps({'issue': {'create': {'issueType': 'Task', 'priority': 'Normal', 'assignee': 'Alice'}}}))
        body = ' Raw body\n\n'
        (root / 'editor-body').write_text(body)
        steps = [
            {'wait': 'Local project', 'keys': '\r'},
            {'wait': 'total 0', 'keys': 'lc'},
            {'wait': 'Assignee: Alice', 'keys': ' Raw title \t\x01'},
            {'wait': 'Assignee: Alice', 'keys': '\x7f\t2024-02-29\t2024-03-01\t\r'},
            {'wait': 'Milestones candidates', 'keys': ' j \r'},
            {'wait': 'Milestones: First', 'keys': '\t\r'},
            {'wait': 'EDITOR READY', 'keys': '\n'},
            {'wait': 'Body editor finished', 'keys': '\t\r'},
            {'wait': 'Confirm create', 'keys': '\r'},
            {'wait': 'Confirm create', 'keys': 's'},
            {'wait': 'Sending', 'keys': 's\r\x1bq\x14bad'},
            {'wait': 'Created; list refreshed', 'keys': '\r'},
            {'wait': 'Issue List', 'keys': 'q'},
        ]
        (root / 'shell-steps.json').write_text(json.dumps(steps))
        env = {'HOME': directory, 'XDG_CONFIG_HOME': directory, 'TEST_KEY': 'test-secret', 'NO_COLOR': '1',
               'PATH': '/usr/bin:/bin', 'SSL_CERT_FILE': str(root / 'tls/localhost-cert.pem'),
               'SHELL_STEPS_FILE': str(root / 'shell-steps.json'), 'SHELL_REQUESTS_FILE': str(root / 'requests')}
        run = subprocess.run([sys.executable, str(repo / 'tests/fixtures/tty_shell.py'), 'scripted', str(bl), '--profile', 'local-check'],
                             cwd=root, env=env, capture_output=True, text=True, timeout=15)
        assert run.returncode == 0, run.stderr
        result = json.loads(run.stdout)
        assert (result['code'], result['restored'], result['stderr']) == (0, True, ''), result
        assert len(result['snapshots']) == len(steps), result
        assert result['request_counts'][8] == result['request_counts'][9], 'confirmation Enter sent a request'
        assert 'Profile: local-check' in result['snapshots'][9]
        assert 'Raw body' in result['snapshots'][9]
        requests = [json.loads(line) for line in (root / 'requests').read_text().splitlines()]
        posts = [r for r in requests if r['method'] == 'POST']
        assert len(posts) == 1, requests
        assert posts[0]['form'] == {'projectId': ['1'], 'summary': [' Raw title '], 'issueTypeId': ['2'], 'priorityId': ['3'],
            'description': [body], 'startDate': ['2024-02-29'], 'dueDate': ['2024-03-01'], 'milestoneId[]': ['5', '6']}, posts
        assert sum(r['path'].startswith('/api/v2/issues/count?') for r in requests) == 2
        assert sum(r['method'] == 'GET' and r['path'].startswith('/api/v2/issues?') for r in requests) == 2
        assert 'test-secret' not in result['stdout']
        print('PASS: release bl; confirm Enter=0 POST; explicit send=1 POST; raw form/default clear/milestones/editor; refresh=2 count+2 list; exit=0; termios restored')
    finally:
        server.terminate()
        server.wait(timeout=3)
PY
shasum -a 256 target/aarch64-apple-darwin/release/bl target/aarch64-apple-darwin/release/bl-cli
```

期待結果は最後の`PASS:`と両binaryのSHA-256です。これはPTYでのローカル検証であり、実端末の表示・IMEや
`https://lazybl.backlog.com/`への必須受入ではありません。対象buildと実測結果／pendingは
[手動受入記録](docs/testing/manual-acceptance.md#issue-20-tui課題作成)へ記録します。
実サービスの作成は接続先・project・内容について別の明示許可を得てからにしてください。

### 単行入力部品 (#14)

`tests/tui_input.rs` は公開 `tui::SingleLineInput` のevent→value/draw境界で、
日本語/結合文字/ZWJ emojiのgrapheme編集・実cursor位置・長文水平scroll（u16列上限超も含む）、
resize/focusでの原文保持、貼付けCRLF/CR/LF/TABのspace正規化、表示範囲外のsecret/制御文字による
全欄伏字と固定cursor、極小時の編集/cursor保護を確認します。空secretは伏字判定から除外します。
`src/tui.rs` の入力shell試験は、通常文字/編集キーとglobal操作の衝突、Ctrl+C優先、focus/resize、
実Crossterm bracketed paste→安全表示→終了/復元を同じ入力部品とloop/guardで検証します。
通常shellのキーと端末復元試験も維持します。

```sh
cargo test --locked --test tui_input --test tui_shell --test tui_process
cargo test --locked --lib tui::tests:: -- --nocapture
```

実IME/目視は通常の自動テストでは実行せず、明示起動するignored testを実端末で使用します。
実API・設定file・実keyは不要で、製品CLIへのdebug flag/commandや永続状態はありません。

```sh
cargo test --locked --lib tui::tests::manual_single_line_input_terminal -- --ignored --exact --nocapture --test-threads=1
```

stdin/stdoutの両方がTTYである必要があります（pipeやリダイレクト不可）。
入力中はq/h/l/j/kも文字で、Tab/Shift+Tabでfocus切替、Escで編集を離れた後qで終了、Ctrl+Cは常に中断です。
40x12未満では編集せずq/Ctrl+Cだけを受け付けます。bracketed pasteはguardが開始/解除します。
手動testは製品と同じ端末loop/guardを使いますが、初期API取得だけを省き、入力原文をdebug出力しません。
再実施手順・環境・**実IMEのユーザー確認結果と未実施の個別操作**は
[manual-acceptance.md](docs/testing/manual-acceptance.md#単行入力部品-14-早期実端末確認)を参照してください。
自動PTY成功と実IMEのユーザー確認を区別して記録します。

### ローカルHTTPS harness

テスト実行にはPython 3（標準libraryのhttp.server/ssl/pty）とOpenSSL CLIが必要です。
CIもTest前に次の利用可否を確認します。追加のPython packageや固定PEMは不要です。

```sh
python3 -c 'import http.server, ssl, pty; print(ssl.OPENSSL_VERSION)'
openssl version
```

再実行はリポジトリ直下から:

```sh
cargo test --locked --test project_list
cargo test --locked --test safety
cargo test --locked --test candidate_list
cargo test --locked --test service
cargo test --locked --test issue_search
cargo test --locked --test issue_view
cargo test --locked --test issue_comment
cargo test --locked --test issue_create
cargo test --locked --test issue_update
cargo test --locked --test comment_add
cargo test --locked --test comment_update
cargo test --locked --test body
cargo test --locked --test https_fixture
# 実30秒のbody timeout全6ケースだけを再確認
cargo test --locked --test request_timeout -- --nocapture
# libtest並列数に依存せずgroup内で6並列となることの確認
cargo test --locked --test request_timeout -- --nocapture --test-threads=1
```

各CLI suiteのfocused確認から、実30秒のbody timeoutだけを `tests/request_timeout.rs` へ移設しています。
単一テスト内でproject list、issue.create、issue.update、comment.update、comment.addのPOST／PATCHの
6ケースを最大6並列で開始し、panicしたケースがあっても全てjoinしてケース名で結果を報告します。
各ケースは独立Harness／HTTPS server／実CLI／子process限定trustを使い、delay32、各CLIの29秒以上40秒未満、
statusとwrite確度、method／path／form／認証と要求件数を確認します。readではwriteなしも確認します。
最大6並列は資源数の上限でありhard deadlineではなく、既存Harnessのport読取やCommand待機のhang限界は残ります。
開発途中は変更対象のfocused確認を使い、timeout保証自体の確認には上記groupを実行してください。
正式な `cargo test --locked --all-targets` は毎回この全6ケースも含み、focused確認で代替しません。
group wallは参考値であり、40秒以下という性能合否は設けません。各CLIの29秒以上40秒未満という既存assertは維持します。

harnessは `tests/fixtures/https_server.py` をloopbackの動的portへ起動します。
serverごとに一度、OpenSSLでlocalhostのSANと1日の有効期限を持つ自己署名証明書と
秘密鍵を生成し、そのserverへの複数CLI呼出しで共有します。固定の鍵/証明書は保存しません。
生成先はリポジトリ外の一時root内の `tls/`（directoryは0700、秘密鍵は0600）です。
Reqwestの `rustls-tls-native-roots` featureを既存のwebpki rootsと併用し、
子CLI/子libraryテストprocessだけの `SSL_CERT_FILE` に `tls/localhost-cert.pem` をtrust anchorとして指定します。
TLS検証の無効化や製品のHTTP/test flag、システムtrustへの登録はありません。
trustなしで接続が失敗する試験もあります。
Rust harnessが一時rootを所有し、終了時だけでなくserver起動/生成失敗時も削除します。
Python側も生成/起動失敗時に `tls/` を削除します。serverをkillする通常の終了では
Rust側がserverの停止・回収後に鍵・設定・応答・受信記録をまとめて削除します。

直接release CLIで再現する場合も、親shellが一時rootの削除を担当します。
上記release build後、リポジトリ直下から実行してください:

```sh
(
  cli="$PWD/target/aarch64-apple-darwin/release/bl-cli"
  root=$(mktemp -d)
  server=
  trap 'if [ -n "$server" ]; then kill "$server" 2>/dev/null || :; wait "$server" 2>/dev/null || :; fi; rm -rf "$root"' EXIT
  printf '%s\n' '{"body":"[]","status":200}' > "$root/response.json"
  python3 tests/fixtures/https_server.py "$root" > "$root/port" &
  server=$!
  while [ ! -s "$root/port" ]; do
    kill -0 "$server" 2>/dev/null || exit 1
    sleep 0.1
  done
  mkdir "$root/backlog"
  printf '{"profiles":{"default":{"spaceUrl":"https://localhost:%s","apiKeyEnv":"TEST_KEY"}}}\n' "$(cat "$root/port")" > "$root/backlog/config.json"
  run_cli() {
    (cd "$root" && env -i HOME="$root" XDG_CONFIG_HOME="$root" TEST_KEY=test-secret NO_COLOR=1 \
      SSL_CERT_FILE="$root/tls/localhost-cert.pem" "$cli" "$@")
  }
  run_cli issue-type list --project 'a/b%?#' --json
  run_cli priority list
  run_cli resolution list --json
  cat "$root/requests"
)
```

同じsubshell内で更新を直接再現するには、`cat "$root/requests"` の前へ次を追加します。
成功時は最新GETと、本文解除・milestone置換だけのPATCHが1回ずつ記録されます。

```sh
  cat > "$root/response.json" <<'JSON'
{"routes":{"GET /api/v2/issues/P-1":{"body":"{\"projectId\":42,\"description\":\"old\",\"milestone\":[{\"id\":5}]}"},"PATCH /api/v2/issues/P-1":{"body":"{\"id\":1,\"issueKey\":\"P-1\",\"unknown\":123456789012345678901234567890}"}}}
JSON
  run_cli issue update P-1 --body '' --milestone-id 8 --milestone-id 9 --json
  # 200 headers後のbody失敗: exit 1、write.outcomeはupdatedのまま。
  cat > "$root/response.json" <<'JSON'
{"routes":{"GET /api/v2/issues/P-1":{"body":"{\"projectId\":42,\"description\":\"old\"}"},"PATCH /api/v2/issues/P-1":{"body":"not json"}}}
JSON
  run_cli issue update P-1 --body '' --json
```

fixtureのstdoutの最初の行がport、`requests` がHTTP method/path/header/body/formのJSONL記録です。
POST/PATCHのformは空値・反復array keyを保持した配列値で記録します。`truncate:true` はContent-Lengthより短いbodyを返します。
上の一覧3コマンドの例は順に `{"items":[]}`、`ID`/`NAME` 見出し、`{"items":[]}` を出します。
一覧3コマンドだけを実行した場合、記録は3 GETだけで、pathは `/api/v2/projects/a%2Fb%25%3F%23/issueTypes`、
`/api/v2/priorities`、`/api/v2/resolutions`。myself/project/space取得は発生しません。
`response.json` のbodyをAPI配列のJSON文字列に置き換えれば非空表示も再現できます。
検索の成功・失敗を直接確認するには、同じsubshellの `cat "$root/requests"` の前へ次を追加できます。

```sh
  cat > "$root/response.json" <<'JSON'
{"routes":{"/api/v2/issues/count":{"body":"{\"count\":43}"},"/api/v2/issues":{"body":"[{\"id\":1,\"issueKey\":\"P-1\",\"summary\":\"修正\",\"status\":{\"name\":\"未対応\"}}]"},"/api/v2/users/myself":{"body":"{\"id\":7}"}}}
JSON
  run_cli issue list --project 123 --limit 2 --offset 8 --json
  run_cli issue mine --project all --json
  # count失敗: 終了1、stdout空、stderrにpermission JSON。listは取得しない。
  printf '%s\n' '{"routes":{"/api/v2/issues/count":{"status":403}}}' > "$root/response.json"
  run_cli issue list --project 123 --json
```

成功時はそれぞれ1件のitemsと `pagination`（count=1、total=43）を返します。
`requests` のcount/list両方に同じfilterとsort/orderがあり、listだけcount/offsetを持ちます。
fixtureは `routes` があれば `"PATCH /api/v2/issues/P-1"` のようなmethod + queryを除いたpathを優先し、
次に従来のpathだけのrouteでdispatchします。不明pathは404です。
`routes` がなければ従来どおり全pathに同じ応答を返します。各routeは通常のresponseと同じ形式です。
生成物はテスト専用で、実サービスに使わないでください。

### release `bl` の直接PTY確認 (#15)

上の一時root/serverを作るsubshellの `cat "$root/requests"` の前に、以下を入れて再利用できます。
実Backlogへは接続せず、同じ検証TLS/設定でrelease `bl` を直接起動します（test binaryではありません）。

```sh
  tui="$(dirname "$cli")/bl"
  driver="$PWD/tests/fixtures/tty_shell.py"
  mkdir -p "$root/.backlog"
  printf '%s\n' '{"project":"B"}' > "$root/.backlog/settings.json"
  cat > "$root/response.json" <<'JSON'
{"routes":{
  "/api/v2/space":{"body":"{\"name\":\"Actual space\"}"},
  "/api/v2/projects":{"body":"[{\"id\":2,\"projectKey\":\"B\",\"name\":\"Beta\"}]"},
  "/api/v2/issues/count":{"sequence":[{"body":"{\"count\":3}"},{"body":"{\"count\":2}"}]},
  "/api/v2/issues":{"offsets":{
    "0":{"body":"[{\"id\":1,\"issueKey\":\"B-1\",\"summary\":\"First page\",\"status\":{\"name\":\"open\"}}]"},
    "1":{"body":"[{\"id\":2,\"issueKey\":\"B-2\",\"summary\":\"Second page\",\"status\":{\"name\":\"open\"}}]"}
  }}
}}
JSON
  cat > "$root/steps.json" <<'JSON'
[{"wait":"First page","keys":"ln"},{"wait":"Second page","keys":"p"},
 {"wait":"First page","keys":"R"},{"wait":"total 2","keys":"q"}]
JSON
  (cd "$root" && env -i HOME="$root" XDG_CONFIG_HOME="$root" TEST_KEY=test-secret NO_COLOR=1 \
    SSL_CERT_FILE="$root/tls/localhost-cert.pem" SHELL_STEPS_FILE="$root/steps.json" \
    SHELL_REQUESTS_FILE="$root/requests" python3 "$driver" scripted "$tui")
```

出力JSONの `code:0`、`restored:true`、空 `stderr`、`browse_request_counts:[4,5,5,7]` と各 `snapshots` を確認します。
`request_counts` はプレビュー/記法GETも含む全通信、`browse_request_counts` は一覧操作の通信数です。
`screen` は終了前の100x24描画、`stdout` はraw端末出力です。driverの簡易描画復元はcursor-addressed出力のみが対象で、
実端末の目視/IME受入を代替しません。対話操作を直接確認するなら同じ環境変数で `"$tui"` をTTY上で起動します。
stepsの `wait` は現在画面の部分一致、`keys` は送信文字列（Enterは `\r`、Escは `\u001b`）、
`pause` は一致後の待機秒数（既定0.08）です。routeの `sequence` は同じpathの応答を順番に返し末尾を反復、
`offsets` はlistのoffset文字列から応答を選び、不在なら空一覧です。`header_delay`/`delay` と組み合わせて
R失敗/取消/応答逆転を再現できます。各自動試験の具体的な組合せは `tests/tui_process.rs` を参照してください。


### プレビュー/詳細とブラウザ (#16)

`tests/tui_process.rs` は同じPTY/HTTPS境界で、約150ms debounceの実時間/通信回数、連続選択・対象切替直後の旧本文非表示、
遅延した成功/失敗による新busyの非破壊、Enter再GETなし、全項目、Markdown/Backlog/未知記法fallbackと安全表示、
Detail Rのissue/project両方成功時だけの交換・失敗/取消/閉鎖後の内容/scroll/focus保持と一覧非更新を確認します。
ブラウザはテスト用PATHの`open` stubでargvを記録し、APIの任意URL非使用、不正キー/secret URLの非起動、失敗時の選択保持を確認します。
実ブラウザを開かず、製品flagは追加しません。`src/tui.rs` のTestBackend試験で全scrollキー、右側focus往復、
Unicodeのwrap/resize/clampとscroll上限でのoverflow防止を確認します。
再取得/ブラウザ失敗の中央popup（ERROR見出し/ボタンなし）、40x12での折返し、赤/太字とNO_COLORの枠/太字、
修飾なしEnter/Esc/Spaceの通知だけのdismiss・次キーの通常動作、j/k/h/l・Ctrl+Oの通知消去と通常動作を確認します。
既存の通常dispatchに進むShift付きキーは通知を消して本来の動作を行い、拒否するmodifier/releaseやresize/極小復帰では通知を保持します。
PTYではDetail失敗後の本文/scroll/一覧保持、Rでの通知消去と再取得（再失敗なら新通知）、選択変更での消去・古い応答の非破壊・Ctrl+C=130と端末復元を照合します。
一覧だけを扱う試験のfixtureも、自動previewの正常応答を返し、意図しない404通知が操作を遮らないようにします。
Ratatui **0.30.2** の `unstable-rendered-line-info` featureだけを有効にし、描画と同じ `Paragraph`/`Wrap`/内幅で行数を計算します。
依存更新時はこのunstable APIを再確認してください。スクロール開始位置の上限は65,535折返し行で、
これを超える本文末尾へは到達できない場合があります（原文は変更しません）。

release `bl` の直接確認には、上のローカルHTTPS用subshellでserver/設定を作り、次を実行できます。
`repo` と `tui` はcwdを変える前に設定します。fixtureは実Backlogへ接続/書込みを行いません。

```sh
  repo="$PWD"
  tui="$repo/target/aarch64-apple-darwin/release/bl"
  cp "$repo/tests/fixtures/tui_detail.json" "$root/response.json"
  cp "$repo/tests/fixtures/tui_detail_steps.json" "$root/steps.json"
  mkdir -p "$root/.backlog" "$root/bin"
  printf '%s\n' '{"project":"B"}' > "$root/.backlog/settings.json"
  cat > "$root/bin/open" <<'SH'
#!/bin/sh
printf '%s\n' "$#" "$@" >> open-args
exit 7
SH
  chmod 700 "$root/bin/open"
  (cd "$root" && env -i HOME="$root" XDG_CONFIG_HOME="$root" TEST_KEY=test-secret NO_COLOR=1 \
    PATH="$root/bin:/usr/bin:/bin" SSL_CERT_FILE="$root/tls/localhost-cert.pem" \
    SHELL_STEPS_FILE="$root/steps.json" SHELL_REQUESTS_FILE="$root/requests" \
    python3 "$repo/tests/fixtures/tty_shell.py" scripted "$tui")
  cat "$root/open-args"
```

成功基準は `code:0`、`restored:true`、空stderr、10 GETのみ、`open-args`は引数数1と設定originの`/view/B-3`のみです。
初期連続選択はB-3だけを取得し、100x24/40x12のGで`Local body line 30`へ到達、R失敗時はその位置を保持、
次のRは通知を消して再取得し、成功時に先頭へ戻ります。そのままGで`Refreshed body [REDACTED]`を表示します。
Ctrl+O失敗も中央popupに一度だけ表示され、B-3の選択を保持します。
`stdout` に `test-secret` や外部OSCが出ないことも確認してください。
`tests/fixtures/tui_detail*.json` を使う自動試験も同じdebug製品binaryでこの手順を走らせます。
このsequence fixtureは自動手順用です。自由な手動操作ではリクエスト回数で応答が進まないよう、成功/失敗の応答段階を固定してください。
PTY driverのstepは `resize:[高さ,幅]` と、stub完了等の同期用 `wait_file:"root内の相対path"` も使用できます。
`sent_at`/HTTP記録の`at`は時刻秒、実時間debounceの照合用です。簡易画面復元は実端末の目視/IMEを代替しません。

コメントpagingはrouteに `"commentItems":[コメントobject,...]` を指定して再現できます。
fixtureはqueryの `minId` / `maxId` を正整数として比較して境界自身を除外し、IDでsortし、`count` 件へ切り詰めます。
`tests/issue_comment.rs` はID 1〜5の同じ集合からminのみ・maxのみ・両方・空ページの実結果と送信queryを照合します。
固定bodyを返すだけでは境界除外の検証として扱いません。
`response.json` の `header_delay` はheaders前、`delay` はheaders送信後の秒数、`location` はredirect先、
`disconnect:true` は応答前の切断を制御します。`requests` / `headers-sent` markerで中断するphaseを合わせます。
`get-headers-sent` / `post-headers-sent` / `patch-headers-sent` も生成するため、事前GETと書込みheadersを区別できます。
実API keyは使わないでください。

このローカルharnessは実Backlogへの接続/書込み、実IME/TUI受入、配布tar.gz作成や公開を行いません。
Issue #17以降の該当機能では承認済みPlanの実Backlog受入が別途必須です（#19は実Backlog接続不要）。未実施を完了扱いにしません。
editorの権限検査後のfile変更や、起動時のOSによる最終実行可否までは保証しません。

### 検索ダイアログ (#17)

`tests/tui_process.rs` の `search_dialog_` / `cross_project_dialog_` は実製品 `bl` + PTY/HTTPS境界で、
設定初回/適用済み値/clear/reopen/R、4条件複数選択/巨大ID、編集時count/listなし・適用時query一致、
失敗/取消時のdraft・一覧/履歴/選択保持、古いcandidate/applyの成功/失敗と新busy非破壊、
横断keyword/self/空白拒否/複数project/通常復帰/c案内を検証します。
中央72x18/38x10と重ねpopup、一行候補の最小縦scroll、左右のgrapheme安全な全文確認、
標準編集優先/Enterの役割/Repeat・Release・modified Enter非適用、成功/空候補のdialog内再利用、
GET回数・取得中重複なし・clear/cache hit/閉鎖後の古い応答無効化・貼付け全欄伏字/原文保持・
tiny時編集禁止・Ctrl+C優先/端末復元も同じ境界で確認します。実IME/実サービス権限は保証しません。

```sh
cargo test --locked --test tui_process search_dialog_
cargo test --locked --test tui_process cross_project_dialog_
cargo test --locked --lib tui::tests::search_dialog_
```

release製品を直接再現する場合は、release build後に以下を実行できます（実API key不要、GETのみ）。
一時rootのHTTPS trustは子processだけに渡し、設定・fake key・TLSはsubshell終了時に削除します。

```sh
(
  repo="$PWD"
  tui="$repo/target/aarch64-apple-darwin/release/bl"
  root=$(mktemp -d)
  server=
  trap 'if [ -n "$server" ]; then kill "$server" 2>/dev/null || :; wait "$server" 2>/dev/null || :; fi; rm -rf "$root"' EXIT
  cp tests/fixtures/tui_search.json "$root/response.json"
  cp tests/fixtures/tui_search_steps.json "$root/steps.json"
  mkdir -p "$root/backlog" "$root/.backlog"
  python3 tests/fixtures/https_server.py "$root" > "$root/port" & server=$!
  while [ ! -s "$root/port" ]; do
    kill -0 "$server" 2>/dev/null || exit 1
    sleep 0.1
  done
  printf '{"profiles":{"default":{"spaceUrl":"https://localhost:%s","apiKeyEnv":"TEST_KEY"}}}\n' "$(cat "$root/port")" > "$root/backlog/config.json"
  printf '%s\n' '{"project":"B","issue":{"list":{"status":["open"]}}}' > "$root/.backlog/settings.json"
  (cd "$root" && env -i HOME="$root" XDG_CONFIG_HOME="$root" TEST_KEY=test-secret NO_COLOR=1 \
    SSL_CERT_FILE="$root/tls/localhost-cert.pem" SHELL_STEPS_FILE="$root/steps.json" \
    SHELL_REQUESTS_FILE="$root/requests" python3 "$repo/tests/fixtures/tty_shell.py" scripted "$tui")
  cat "$root/requests"
)
```

成功基準は `code:0`、`restored:true`、空stderr、26 snapshots、28 GETのみ、count/list各5回です。
各countとlistはcount/offset以外が同一queryです。最初は設定open、次はkeyword+2状態+Alice+巨大IDのv1、
clear後はstatus/keywordなし、横断ではselfのみ・project/statusなし、最後は具体Bの通常openへ戻ります。
snapshots[9]と[10]は状態・担当者・マイルストーンの値欄が空欄（`Any`や旧選択値なし）、
clearのcount/listにkeyword/statusId/assigneeId/milestoneIdがないことを照合します。
[13]は認証ユーザー候補「自分」（簡易VT復元では `自 分`）、
[14]は `Alpha result` と `Beta result`、[18]は `Returned issue` です。内部値selfはassigneeId=7に解決されます。
[18]でpreview完了を待ってから100x24へ戻しL→Detail、l→Comment、H→Issue、L→Comment、
40x12へresize後もComment、H→Issue、L→Commentを[19]〜[25]で確認します。
`request_counts[18..26]` がすべて同じ（総GET数28）で、移動だけによるGET増加がないことを照合します。
Detailを除外する `browse_request_counts` だけで追加GETゼロを判断しないでください。
raw stdoutに `test-secret` がないことも確認してください。順序で応答が進むfixtureなので自由操作には使わず、
実端末で任意操作する場合は応答を固定してください。実Backlogと実IME/キー衝突の必須受入手順・未実施記録は
[manual-acceptance.md](docs/testing/manual-acceptance.md#検索ダイアログ-17) にあります。

メインの大文字H/L（NONEまたはSHIFTのPress）は左/右の最終focusへ復帰し、同じ側はno-opです。
初期左Project/右Detail、内部h/l・Tab/Shift+Tab・Enter/Esc・検索成功の確定focusも記憶します。
Issue Enterは常にDetail、右Escは読取り取消+Issueのままです。resize/project/通常・横断検索で記憶をresetしません。
新キーは通信や選択/条件/履歴/scrollを変えず、空状態・読取り中も有効です。dialog/popup/適用中/極小では背面に流さず、
入力中のH/Lは文字。Ctrl/Alt等併用・Repeat/Releaseの拒否では通知も不変、有効キーは同側no-opでも通知を消します。
`cargo test --locked --test tui_shell` と `cargo test --locked --test tui_process region_` で
公開Shell/実blの復帰・入力保護・通知・総GET数・遅延R継続・scroll/選択/履歴・Ctrl+C130/復元を再確認できます。

検索は初期キーワードfocusで、Tab（逆はShift+Tab）により状態→担当者→マイルストーンへ移ります。
キーワード内のCtrl+A/E/B/F/K/U/W/H/Y等は標準編集です。欄外だけCtrl+K/S/A/Gが直接focus、Ctrl+Uが全clearです。
修飾なしPress Enterはキーワード/閉じた項目では検索、候補内では閉じるだけ、候補失敗中ではretryだけです。
Ctrl+F検索と条件全文のPgUp/PgDn操作はありません。長い値は左右で確認します。
選択値はラベルの右に一行、未選択の値欄は空欄です（一覧summaryのkeyword `Any`と横断の「利用不可」は別です）。
fixtureには長い日本語/結合文字/emoji候補も含みます。100x24の最大72x18、40x12の38x10とpopupを確認してください。
Shell→TestBackend試験は100x12も含め、ラベル右の空欄/選択値・左右全文・focus印・keyword cursor復帰を確認します。
横断時はh/lまたはTab→Project→具体project Enterで通常復帰/作成先確定、cは同じ案内のみ、nは次ページです。
実IMEでは変換確定Enter非適用→次Enter一回適用を製品で必須確認します。端末が通常Enterとして送る
IME確定をアプリは識別できないため、独自の時間/文字種推測はせず、失敗なら未完了として判断へ戻してください。

## TUIコメント閲覧の検証 (#18)

```sh
cargo test --locked --test comment_preview
cargo test --locked --lib tui::tests::
cargo test --locked --test tui_process comment_
cargo test --locked --test tui_shell --test issue_comment
```

`tests/tui_process.rs` は実`bl`+PTY+ローカルHTTPSで、20件のID境界paging、
両方向の隣接取得・exclusive境界・巨大ID・空終端・最古への1 GET、null履歴／本文同居・既知／未知fieldと
任意JSON、R失敗の選択／scroll保持と成功交換、選択変更の即時消去、遅延成功／失敗・新busy・Esc取消を確認します。
Shellの入力／TestBackend試験はCommentの固定作者／JST／空行、100×24の14／6配置と本文Rect共用、
j/k・Unicode折返し・長名省略と全文／resize再判定・clamp・focus保持・g/Gの本文scroll非実行を確認します。
削除のliteral RGB fg(255,255,255)／bg(128,0,0)／取り消し線、従来の追加Black／Green、NO_COLORの全cell fg／bg Resetと記号をcell単位で検証します。
本文TABの`⇥`可視化（通常Comment／Detail／固定行overflow全文も含む）と変換後secret再検査、
色あり差分の変更改行`↵`の赤bg＋取り消し線／緑bgと実改行保持も公開preview・描画cellで確認します。
`tests/comment_preview.rs` の公開preview試験ではJSTのoffset・年/月/日跨ぎ・閏日・小数秒・不正値、
行＋grapheme差分・Unicode／空白／TAB／末尾改行の内容再構成、空/同値/非文字列、既知／description／未知fieldの前だけ・後だけ・両方nullの`未設定`表示、
入れ子null／文字列null／空文字／0／false／巨大数値と原値不変、9,999／10,000／10,001 scalar、
伏字化前countと組立secretの最終投影再検査を確認します。通常本文／CLI／JSONには超過例外を広げません。
`tests/issue_comment.rs`はCLIの従来のnull表示とJSONのnull／文字列型保持を確認します。
枠titleはidle／focusで余分な空白・`|`なし、Loading／空一覧／終端／取消／失敗popupと通知消去を既存Shell／PTYで検証します。
本文・履歴の`|`、R失敗のscroll保持・成功交換・古い応答の非破壊、総GET数は維持します。
比較は既存取得taskで準備し、UI draw／completeでは比較しません。50msは共有deadlineによる近似の目安で、厳密な時間上限ではありません。
依存追加は`time =0.3.55`（default-features=false, parsing/formatting）と`similar =2.7.0`（unicode）のみ。
新しいtimezone設定・cache・永続化はありません。Paragraphのscrollは既存のu16（65,535 wrapped行）上限です。
Service公開`comment_page`は`CommentOrder::Asc/Desc`を受け、既存`comment_list`は降順固定のままです。
共通`comment` formatterの安全表示を利用し、TUI previewとCLI固定表示は分離します。null履歴だけのページに記法GETは不要です。
一覧試験のpreview fixtureには明示的な空comments応答を置き、server全体の404は隠しません。
`browse_request_counts`／`browse_requests`はpreview（commentsも含む）を除外する一覧向け計数です。
コメント／focusの通信数は総`request_counts`とHTTPS受信記録で照合してください。

### release blのローカルHTTPS再現

実サービス／利用者config／API keyを使用しません。上記release build後、リポジトリ直下から実行します。

```sh
(
  repo="$PWD"
  tui="$repo/target/aarch64-apple-darwin/release/bl"
  root=$(mktemp -d)
  server=
  trap 'if [ -n "$server" ]; then kill "$server" 2>/dev/null || :; wait "$server" 2>/dev/null || :; fi; rm -rf "$root"' EXIT
  cp "$repo/tests/fixtures/tui_comment.json" "$root/response.json"
  cp "$repo/tests/fixtures/tui_comment_steps.json" "$root/steps.json"
  python3 "$repo/tests/fixtures/https_server.py" "$root" > "$root/port" & server=$!
  while [ ! -s "$root/port" ]; do
    kill -0 "$server" 2>/dev/null || exit 1
    sleep 0.1
  done
  mkdir -p "$root/backlog" "$root/.backlog"
  printf '{"profiles":{"default":{"spaceUrl":"https://localhost:%s","apiKeyEnv":"TEST_KEY"}}}\n' "$(cat "$root/port")" > "$root/backlog/config.json"
  printf '%s\n' '{"project":"B"}' > "$root/.backlog/settings.json"
  (cd "$root" && env -i HOME="$root" XDG_CONFIG_HOME="$root" TEST_KEY=test-secret NO_COLOR=1 \
    SSL_CERT_FILE="$root/tls/localhost-cert.pem" SHELL_STEPS_FILE="$root/steps.json" \
    SHELL_REQUESTS_FILE="$root/requests" \
    python3 "$repo/tests/fixtures/tty_shell.py" scripted "$tui")
  cat "$root/requests"
)
```

期待結果: `code:0`、`restored:true`、空stderr、13 GETのみ（comments 3、issue 3、project記法3、
space/projects/count/list各1）。初期Comment 2からnでComment 1へ、jを15回で本文scrollし、R失敗通知をEnterで消しても
その選択／scrollを保持、次のR成功でID 3の最新本文先頭へ交換します。Endは本文末尾の既知／未知履歴へ移動し（metadataは出しません）、
H/LでIssueとCommentを往復してもscrollと総GET数を保持します。外部secret／OSCは出ず、`[REDACTED]`を表示します。
このsequenceは自動手順用です。自由操作では応答段階を固定してください。PTY簡易画面は実端末の字幅／IME確認の代替ではありません。
最新buildでの実端末と`https://lazybl.backlog.com/`の必須受入結果・未確認事項は[manual-acceptance.md](docs/testing/manual-acceptance.md)を参照してください。

### editor連携 (#19)

`src/editor.rs` の公開 `edit_file`／`edit_body` がshellなしのargv実行を共有します。
端末の引渡し／復帰は既存TUI loopとTerminalGuardだけが所有します。
`tempfile =3.27.0` は承認済みPlanの本人限定一時file用で、lock追加は同crateと推移依存fastrand 2.5.0のみです。
既存依存の版は変更していません。依存変更時のaudit／lock hash前後比較も正式チェックに含めます。
P1 correctionでは既存lock解決済み`rustix =1.1.6`の公開`termios` APIを直接利用します。
Crosstermの公開APIではeditorへの引渡しをまたぐ起動前snapshotを保持できないためです。
単一guardが起動前のtermiosを保持し、editor停止／終了後、130終了／TUI再enterより前に復元します。
独自FFI／stty子process／別の端末管理層は追加せず、lock差分は直接依存の辺1行のみです。

```sh
cargo test --locked --test editor --test tui_shell
cargo test --locked --test tui_process settings_
cargo test --locked --lib body_editor_roundtrip_and_failures
```

公開library試験は原文・0600／所有者・literal argv・正常交換・起動失敗／非0／読取失敗／不正UTF-8／中断時保持と削除、
同時readyの親中断優先を検証します。実bl＋HTTPS／PTYは設定選択、未作成の場所案内、保存の非巻戻し、実行中config不変、
cooked入力の専有、resizeから追加キーなしの再描画、元の選択／ページ／scroll／focus保持と130／termios復元を検証します。
本文は後続フォームを作らず、cfg(test)限定ignored経路を同じloop／guardへ接続してPTYで確認します。
従来7経路にraw／noechoを残す正常・非0・子SIGTERM・親SIGINTの4経路と、editor後の復元失敗2経路を加えています。
起動前後の全termios項目一致（既存のmacOS一時PENDIN除外のみ）、元本文・他欄・temp削除、
復元失敗時に元のeditorエラー／中断130も保持して通知することをassertします。
`tty_shell.py`のscripted stepの`signal`は親へ送信、`remove`は起動後の実行file不在を再現するテスト境界です。

#### release製品の設定編集を直接PTYで再実施

リポジトリ直下から実行します。loopback HTTPS・一時config・fake keyだけを使い、利用者設定を開きません。
設定編集は本文のようなUTF-8再読取をしないので、保存内容の検証は再起動時です。不正UTF-8／読取失敗は下の本文経路で確認します。

```sh
cargo build --locked --release --target aarch64-apple-darwin --bins
python3 - "$PWD" <<'PY'
import json, os, pathlib, subprocess, sys, tempfile
repo = pathlib.Path(sys.argv[1])
binary = repo / 'target/aarch64-apple-darwin/release/bl'
for mode in ['settings', 'nonzero', 'interrupt', 'spawn',
             'raw-settings', 'raw-nonzero', 'raw-signal', 'raw-interrupt']:
    outcome = mode[4:] if mode.startswith('raw-') else mode
    with tempfile.TemporaryDirectory(prefix='backlog-editor-') as name:
        root = pathlib.Path(name)
        (root / 'backlog').mkdir()
        (root / 'response.json').write_text(json.dumps({'routes': {
            '/api/v2/space': {'body': '{"name":"Editor local test"}'},
            '/api/v2/projects': {'body': '[]'}}}))
        server = subprocess.Popen([sys.executable, str(repo / 'tests/fixtures/https_server.py'), name],
                                  stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        try:
            port = int(server.stdout.readline())
            argv = [sys.executable, str(repo / 'tests/fixtures/editor.py'), str(root / 'record'), mode]
            steps = [{'wait':'Editor local test', 'keys':'L*\r'}]
            if mode == 'spawn':
                program = root / 'editor'
                program.write_text('#!/usr/bin/python3\n')
                program.chmod(0o700)
                argv = [str(program)]
                steps[0]['remove'] = str(program)
            config = {'editor': argv, 'profiles': {'default': {
                'spaceUrl': f'https://localhost:{port}', 'apiKeyEnv': 'TEST_KEY'}}}
            (root / 'backlog/config.json').write_text(json.dumps(config))
            if outcome == 'interrupt':
                steps += [{'wait':'EDITOR READY', 'signal':'SIGINT'}]
            else:
                if mode != 'spawn':
                    steps += [{'wait':'EDITOR READY', 'resize':[12,40], 'keys':'qjk\n'}]
                notice = {'settings':'Settings editor closed; restart',
                          'nonzero':'Editor did not exit successfully',
                          'signal':'Editor did not exit successfully', 'spawn':'Cannot run editor'}[outcome]
                steps += [{'wait':notice, 'keys':'\x1b'}, {'wait':'> Issue Detail', 'keys':'q'}]
            (root / 'steps.json').write_text(json.dumps(steps))
            env = dict(HOME=name, XDG_CONFIG_HOME=name, TEST_KEY='test-secret', NO_COLOR='1',
                       PATH='/usr/bin:/bin', SSL_CERT_FILE=str(root / 'tls/localhost-cert.pem'),
                       SHELL_STEPS_FILE=str(root / 'steps.json'), SHELL_REQUESTS_FILE=str(root / 'requests'))
            run = subprocess.run([sys.executable, str(repo / 'tests/fixtures/tty_shell.py'),
                                  'scripted', str(binary)], cwd=name, env=env, capture_output=True, text=True)
            assert run.returncode == 0, run.stderr
            result = json.loads(run.stdout)
            assert result['code'] == (130 if outcome == 'interrupt' else 0), result
            assert result['restored'] and not result['termios_diff'], result
            assert ('interrupted' in result['stderr'] if outcome == 'interrupt' else not result['stderr']), result
            if mode != 'spawn':
                record = json.loads((root / 'record').read_text())
                assert record['cooked'] and record['echo'], record
                if mode.startswith('raw-'):
                    assert record['raw_noecho'], record
                if outcome == 'interrupt':
                    assert subprocess.run(['/bin/kill','-0',str(record['pid'])], capture_output=True).returncode != 0
                else:
                    assert record['input'] == 'qjk\n', record
                    expected = record['original'] if outcome == 'signal' else '{"saved":true}'
                    assert (root / 'backlog/config.json').read_text() == expected
            print(mode, 'exit=', result['code'], 'restored=', result['restored'], 'snapshots=', len(result['snapshots']))
        finally:
            server.terminate()
            _, errors = server.communicate(timeout=5)
            assert not errors, errors
PY
```

#### 実端末：製品設定編集と本文の最小手順

**PTYは目視受入の代替ではありません。** 端末名／版・macOS／IME・TERM／NO_COLOR・対象build hashを
[manual-acceptance.md](docs/testing/manual-acceptance.md#issue-19-editor連携)へ記録します。

製品設定編集は同文書冒頭「ローカルHTTPSを使う準備」の専用bash／一時root／`run_bl`を使用します。
そのセットアップで作る共通configのeditor省略時はvimです。利用者のconfigで試す必要はありません。
`run_bl`で起動し、`*`→上下／j/kで2対象循環→Esc取消→再度`*`→Enterで共通configを開きます。
vim中に100x24→40x12へresizeして`:q!`。追加キーを押さず、新サイズの元focusへ戻ることを目視します。
cwd settingsは不在のまま`*`→j→Enterで場所案内だけになることを確認します。
既存settingsを開く確認はTUI終了後に試験担当が`mkdir -p "$root/.backlog"; printf '{}\n' > "$root/.backlog/settings.json"`
でfixtureを準備して再起動します（製品による生成ではありません）。保存後も起動中の設定は変わらず、再起動後だけ検証します。

本文の実端末確認は次のignored testだけに存在します。製品にdemo画面／flagはありません。

```sh
cargo test --locked --lib tui::tests::manual_body_editor_terminal -- --ignored --exact --nocapture --test-threads=1
```

eでvimへ渡す原文は`  原文 test-secret\r\n`、別欄は`other-value é 👩‍💻`です（すべてfixture）。
Tabで別欄編集、Escで欄外、eで本文編集。空白／改行／日本語を保存して戻り、別欄の値とfocusが保持されること、
失敗では元本文表示が残ること、表示側だけsecretが伏字になることを確認します。100x24／40x12／それ未満も試します。
本文の一時fileは本人限定、戻った時点で削除されます。editor独自swap／backupは対象外です。

失敗を手動再現する時は、このtestだけが読む`BL_TEST_EDITOR_ARGV`を使えます。
例: `BL_TEST_EDITOR_ARGV='["/usr/bin/false"]'`（非0）、`'["/no/such/editor"]'`（起動失敗）、
`'["/usr/bin/python3","-c","import pathlib,sys; pathlib.Path(sys.argv[-1]).write_bytes(bytes([255]))"]'`（不正UTF-8）、
`'["/usr/bin/python3","-c","import pathlib,sys; pathlib.Path(sys.argv[-1]).unlink()"]'`（読取失敗）を上のcargoコマンドへ付けます。
この変数を製品blが読むことはありません。正常・失敗とも通知をEscで閉じ、欄外qで終了してください。

中断の正確な終了コードを観測する時はcargo wrapperを避けてtest binaryを直接起動します。

```sh
test_bin=$(cargo test --locked --lib --no-run --message-format=json | python3 -c 'import json,sys; print([r["executable"] for r in map(json.loads,sys.stdin) if r.get("executable")][0])')
before=$(stty -g)
"$test_bin" --exact tui::tests::manual_body_editor_terminal --ignored --nocapture --test-threads=1
code=$?; printf 'test binary exit=%s\n' "$code"
[ "$(stty -g)" = "$before" ] && echo 'termios unchanged'
```

editor中に別端末から**親のbl／test binaryのPID**へ`kill -INT <PID>`し、子停止・一時file削除・通常shell復元・130を確認します。
editor自身だけの非0／シグナル終了は通知して元画面へ戻る別経路です。cargo経由でtest processが130終了するとcargoの終了は101になるため混同しません。
終了後は実shellで文字入力・削除・cursor／表示属性を目視します。SIGKILL・切断後の復元／削除は保証対象外です。

### TUI課題更新 (#21): releaseの直接ローカル検証

`tests/tui_process.rs` の `bl_update_` は実bl + PTY/HTTPS境界で、最新の全初期値・create既定値非流用、
全10項目の変更／解除・resolution ID 0・milestone集合、未編集／同値非送信、変更だけの確認・独立sの1PATCH、
入力／破棄確認／editor失敗とresize／極小／キー衝突、安全表示の原文保持を確認します。
作成と共用のタイトル／日付入力ではCtrl+A/B/Y等の標準編集を優先し、focus shortcutは入力欄外で使います。
`bl_create_and_update_single_line_editing_` は両フォームの編集／no-op時focus保持と欄外shortcutを同じ製品境界で確認します。
一覧タイトル・条件・total・ページ履歴・選択／scrollの非更新、成功時だけのComment Latest交換と失敗時の選択／scroll保持、
取得中のmodal／極小q保護・Ctrl+C／SIGINT・手動R回復、取消loadと古い一覧／Detail／Comment成功・失敗応答の世代失効、
送信前／送信開始後／200 headers後のCtrl+CとSIGINT、拒否／unknown／200後のbody・decode・必須field失敗も照合します。
共通Serviceの既存 `issue_update_`・CLI `tests/issue_update.rs` とcommentの回帰試験は、部分指定時に未使用fieldを要求せず、
候補の名前解決・事前検証・既存の確度を維持することを確認します。prepare/sendはcrate内のopaqueな更新値だけを渡し、
任意formの公開APIを追加していません。HTTP futureは既存の直接所有を維持し、終了時はabort+join後に確度を読みます。

```sh
cargo test --locked --test tui_process bl_update_ -- --nocapture
cargo test --locked --test service issue_update_
cargo test --locked --test issue_update --test comment_add --test issue_comment
cargo test --locked --test tui_process bl_create_and_update_single_line_editing_
cargo test --locked --test tui_process bl_create_
cargo test --locked --test tui_shell
```

以下は **cargo testとは別にrelease製品blを直接起動する** 自己完結のlocalhost HTTPS/PTY手順です。
リポジトリ直下で実行してください。利用者config／実key／実サービスを使わず、一時rootとserverを最後に回収します。
fixtureは初期Detail／Comment記法用の2 issue GETと、e開始時の最新issue GETを区別します。
PATCH後は本文なし履歴へのLatest GET 1回だけで、記法GETは不要です（本文ありの応答なら既存のissue＋project GETが必要）。
このfixtureの実経路は18 GET＋1 PATCHです。自由操作ではなく固定手順専用で、途中のUpdated:でなくComment更新完了を待ちます。

```sh
cargo build --locked --release --target aarch64-apple-darwin --bins
python3 - <<'PY'
import json, pathlib, subprocess, sys, tempfile
repo = pathlib.Path.cwd()
binary = repo / 'target/aarch64-apple-darwin/release/bl'
with tempfile.TemporaryDirectory(prefix='bl-update-local-') as directory:
    root = pathlib.Path(directory)
    (root / 'response.json').write_bytes((repo / 'tests/fixtures/tui_update.json').read_bytes())
    (root / 'steps.json').write_bytes((repo / 'tests/fixtures/tui_update_steps.json').read_bytes())
    (root / 'backlog').mkdir()
    (root / '.backlog').mkdir()
    server = subprocess.Popen([sys.executable, str(repo / 'tests/fixtures/https_server.py'), directory],
                              stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    try:
        origin = 'https://localhost:' + server.stdout.readline().strip()
        (root / 'backlog/config.json').write_text(json.dumps({'profiles': {'default': {
            'spaceUrl': origin, 'apiKeyEnv': 'TEST_KEY'}}}))
        (root / '.backlog/settings.json').write_text(json.dumps({'project': 'B'}))
        env = dict(HOME=directory, XDG_CONFIG_HOME=directory, TEST_KEY='test-secret', NO_COLOR='1',
                   PATH='/usr/bin:/bin', SSL_CERT_FILE=str(root / 'tls/localhost-cert.pem'),
                   SHELL_STEPS_FILE=str(root / 'steps.json'), SHELL_REQUESTS_FILE=str(root / 'requests'))
        run = subprocess.run([sys.executable, str(repo / 'tests/fixtures/tty_shell.py'), 'scripted', str(binary)],
                             cwd=root, env=env, capture_output=True, text=True, timeout=15)
        assert run.returncode == 0, run.stderr
        result = json.loads(run.stdout)
        assert (result['code'], result['restored'], result['termios_diff'], result['stderr']) == (0, True, [], ''), result
        assert len(result['snapshots']) == 10, result
        assert 'Latest title' in result['snapshots'][2], result
        assert result['request_counts'][4] == result['request_counts'][5], 'confirmation Enter sent HTTP'
        for text in ['Changed title', 'Old list title', 'TITLE: Latest title -> Changed title']:
            assert text in result['snapshots'][8], result
        requests = [json.loads(line) for line in (root / 'requests').read_text().splitlines()]
        patches = [r for r in requests if r['method'] == 'PATCH']
        assert len(requests) == 19 and len(patches) == 1, requests
        assert sum(r['method'] == 'GET' for r in requests) == 18, requests
        assert patches[0]['form'] == {'summary': ['Changed title'], 'description': [''], 'assigneeId': [''],
            'resolutionId': ['0'], 'milestoneId[]': ['5', '6']}, patches
        for prefix in ['/api/v2/issues/count?', '/api/v2/issues?']:
            assert sum(r['path'].startswith(prefix) for r in requests) == 1, requests
        assert sum(r['method'] == 'GET' and r['path'] == '/api/v2/issues/B-1' for r in requests) == 3, requests
        assert sum(r['path'] == '/api/v2/issues/B-1/comments?count=20&order=desc' for r in requests) == 2, requests
        patch_index = requests.index(patches[0])
        assert [r['path'] for r in requests[patch_index + 1:]] == ['/api/v2/issues/B-1/comments?count=20&order=desc'], requests
        assert all(r['method'] == 'GET' for r in requests[patch_index + 1:]), requests
        assert result['request_counts'][8] == result['request_counts'][9] == 19, result
        assert 'test-secret' not in result['stdout'], result
        pathlib.Path('/tmp/issue21-direct-result.json').write_text(json.dumps(result, ensure_ascii=False, indent=2))
        print('PASS: release bl; latest values; confirm Enter=0 PATCH; explicit s=1 PATCH; clears/ID0/milestones; 18 GET+1 PATCH; old list retained; latest history without R; exit=0; termios restored')
    finally:
        server.terminate()
        _, errors = server.communicate(timeout=3)
        assert not errors, errors
PY
shasum -a 256 target/aarch64-apple-darwin/release/bl target/aarch64-apple-darwin/release/bl-cli
```

期待結果は最後の`PASS:`と両binaryのSHA-256です。raw結果は`/tmp/issue21-direct-result.json`です（fake keyだけのfixture）。
これは実端末の字幅／IME／目視や実Backlog書込みの代替ではありません。
接続先・対象・変更／解除内容の別の明示許可を得た実Backlog受入と実端末確認は
[manual-acceptance.md](docs/testing/manual-acceptance.md#issue-21-tui課題更新)へ記録し、未実施を成功扱いにしません。

### TUIコメント追加 (#22): releaseの直接ローカル検証

Comment focusの修飾なし`c`から本文・状態・担当者・完了理由だけを編集します。Tab順は本文→状態→担当者→完了理由→確認→取消。
候補と解除、本文editor、確認から独立した`s`、取消／破棄確認は課題フォームと共用です。状態と完了理由は独立で、ID 0と解除を区別します。
opaqueなprepared commentに共通Serviceが確定したmethod/formを保持し、確認と送信で再解決・追加の確認GETをしません。
本文だけならPOST comments、実際の課題項目変更があれば非空本文をcommentとして1 PATCH、本文なしならcommentは送りません。
正常応答後だけ既存のLatest comments読取り／表示準備を一度行い、すべて成功後に交換します。空ページも成功です。
PATCHの正常な対象課題はDetailへ反映しますが、Listの行・タイトル・条件・total・履歴・選択・scrollは変更しません。
表示更新失敗はCreated/Updatedを保ち、旧Commentと「登録済み・コメント表示更新失敗／再送不要／R」を表示します。
送信／表示再取得中のmodalは閉じず、Ctrl+C／SIGINTではabort+join後のcomment.add確度と端末復元を通知します。
既存コメント本文の編集（#23）は追加していません。

```sh
cargo test --locked --test tui_process bl_comment_add_ -- --nocapture
cargo test --locked --test service --test comment_add --test issue_comment --test issue_update --test tui_shell
cargo test --locked --test tui_process bl_create_
cargo test --locked --test tui_process bl_update_
```

次は**cargo testと別のrelease製品**を起動する自己完結のlocalhost HTTPS/PTY検証です。
実設定・実key・Backlogへの接続は不要です。2ケースそれぞれ新規server／一時rootを作り、最後に回収します。
本文ありの表示更新なのでLatest GETの後に既存のissue＋project記法GETも行います（計17 GET＋1 POST/PATCH）。
固定fixtureは初期Detailと初期Comment記法のGET、フォーム開始時の最新GETを区別します。
原文送信と安全表示、確認Enterの非送信、単一書込み、Comment／Detailと旧List、終了0とtermios復元をassertします。

```sh
cargo build --locked --release --target aarch64-apple-darwin --bins
python3 - <<'PY'
import json, pathlib, subprocess, sys, tempfile
repo = pathlib.Path.cwd()
binary = repo / 'target/aarch64-apple-darwin/release/bl'
for method in ['post', 'patch']:
    with tempfile.TemporaryDirectory(prefix='bl-comment-add-local-') as directory:
        root = pathlib.Path(directory)
        (root / 'response.json').write_bytes((repo / 'tests/fixtures/tui_comment_add.json').read_bytes())
        (root / 'steps.json').write_bytes((repo / f'tests/fixtures/tui_comment_add_{method}_steps.json').read_bytes())
        (root / 'backlog').mkdir()
        (root / '.backlog').mkdir()
        body = ' Raw #22 日本語\n test-secret\x1b]52;c;unsafe\x07\n'
        (root / 'editor-body').write_text(body)
        server = subprocess.Popen([sys.executable, str(repo / 'tests/fixtures/https_server.py'), directory],
                                  stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        try:
            origin = 'https://localhost:' + server.stdout.readline().strip()
            (root / 'backlog/config.json').write_text(json.dumps({'editor': [sys.executable,
                str(repo / 'tests/fixtures/editor.py'), str(root / 'editor-record'), 'body-file'],
                'profiles': {'default': {'spaceUrl': origin, 'apiKeyEnv': 'TEST_KEY'}}}))
            (root / '.backlog/settings.json').write_text(json.dumps({'project': 'B', 'issue': {'create': {
                'issueType': 'DO NOT USE', 'priority': 'DO NOT USE', 'assignee': 'DO NOT USE'}}}))
            env = dict(HOME=directory, XDG_CONFIG_HOME=directory, TEST_KEY='test-secret', NO_COLOR='1',
                       PATH='/usr/bin:/bin', SSL_CERT_FILE=str(root / 'tls/localhost-cert.pem'),
                       SHELL_STEPS_FILE=str(root / 'steps.json'), SHELL_REQUESTS_FILE=str(root / 'requests'))
            run = subprocess.run([sys.executable, str(repo / 'tests/fixtures/tty_shell.py'), 'scripted', str(binary)],
                                 cwd=root, env=env, capture_output=True, text=True, timeout=15)
            assert run.returncode == 0, run.stderr
            result = json.loads(run.stdout)
            assert (result['code'], result['restored'], result['termios_diff'], result['stderr']) == (0, True, [], ''), result
            assert result['request_counts'][4] == result['request_counts'][5], 'confirmation Enter sent HTTP'
            assert 'Latest owner (4)' in result['snapshots'][1], result
            assert '0 chars' in result['snapshots'][1], 'description copied into comment'
            assert 'LATEST REGISTERED' in result['snapshots'][7], result
            assert ('Changed title' if method == 'patch' else 'TITLE: Detail B-1') in result['snapshots'][8], result
            assert 'Old list title' in result['snapshots'][9], result
            requests = [json.loads(line) for line in (root / 'requests').read_text().splitlines()]
            writes = [r for r in requests if r['method'] != 'GET']
            assert len(requests) == 18 and len(writes) == 1, requests
            assert sum(r['method'] == 'GET' for r in requests) == 17, requests
            write = writes[0]
            assert write['method'] == method.upper(), write
            assert write['path'] == '/api/v2/issues/B-1' + ('/comments' if method == 'post' else ''), write
            expected = {'content': [body]} if method == 'post' else {
                'statusId': ['2'], 'assigneeId': [''], 'resolutionId': ['0'], 'comment': [body]}
            assert write['form'] == expected, write
            after = requests[requests.index(write) + 1:]
            assert [r['path'] for r in after] == ['/api/v2/issues/B-1/comments?count=20&order=desc',
                '/api/v2/issues/B-1', '/api/v2/projects/2'], requests
            assert all(r['method'] == 'GET' for r in after), requests
            for prefix in ['/api/v2/issues/count?', '/api/v2/issues?']:
                assert sum(r['path'].startswith(prefix) for r in requests) == 1, requests
            assert 'test-secret' not in result['stdout'] and '\x1b]52;' not in result['stdout'], result
            record = json.loads((root / 'editor-record').read_text())
            assert record['original'] == '' and record['mode'] == 0o600 and record['cooked'], record
            assert not pathlib.Path(record['path']).exists(), record
            pathlib.Path(f'/tmp/issue22-direct-{method}-result.json').write_text(json.dumps(result, ensure_ascii=False, indent=2))
            pathlib.Path(f'/tmp/issue22-direct-{method}-requests.json').write_text(json.dumps(requests, ensure_ascii=False, indent=2))
            print(f'PASS: release bl {method}; 17 GET+1 {method.upper()}; confirm Enter=no send; raw body preserved/safe display; latest Comment; Detail/old List; exit=0; termios restored')
        finally:
            server.terminate()
            _, errors = server.communicate(timeout=3)
            assert not errors, errors
PY
shasum -a 256 target/aarch64-apple-darwin/release/bl target/aarch64-apple-darwin/release/bl-cli
```

期待結果は2つの`PASS:`とbinaryのSHA-256です。観測結果とfake-key HTTP記録は`/tmp/issue22-direct-{post,patch}-{result,requests}.json`です。
実端末のIME／字幅／目視／editor復帰と実Backlogでの追加・状態変更・初期状態への復元は別の必須受入です。
接続先・対象・内容の明示許可の下でRoot／利用者が実施し、[手動受入記録](docs/testing/manual-acceptance.md)へ記録します。
未実施を成功やIssue完了と扱いません。
