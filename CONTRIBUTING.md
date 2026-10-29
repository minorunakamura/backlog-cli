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
`bl` のプロジェクト選択・通常/横断検索ダイアログ・課題一覧ページ巡回/再取得・選択追従プレビュー/詳細・ブラウザ起動・focus・layout・端末復元も実装しています。
共有単行入力部品を製品のkeyword欄へ組み込み、開発専用の実端末確認経路も維持しています。
TUIのコメント取得・書込みフォーム・editorは後続Work Itemの範囲です。

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

## テストと今回の範囲

承認済み [Plan](docs/plans/backlog-cli-initial.md) と
[基本設計](docs/designs/backlog-cli.md) を参照してください。
公開境界を一つの失敗テスト→最小実装の縦sliceで検証します。
初期基盤の `tests/cli_help_version.rs` は、実CLIプロセスのargv/stdin/stdout/stderr/exitを
境界に、環境変数を消し、空のHOME/XDG_CONFIG_HOME/cwdと非TTYでhelp/versionを確認します。
内部private関数の名前や実装詳細はテストに固定しません。

`tests/project_list.rs` は実CLIとローカルHTTPS送受信境界で、設定の厳密検証・認証header・
固定表示/JSON・redirect/retry禁止・body込み30秒timeout・Ctrl+C・出力失敗を確認します。
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
出力失敗・redaction key衝突での確度保持、送信前/送信開始後/201 headers後のCtrl+Cとbody込み30秒timeoutを確認します。
stdin待ち中断・stdin非明示時の非読取・global/profile・TTY非対話も実processで試験します。
`tests/service.rs` の公開 `Service::issue_create` 試験は、送信本文と返却object/巨大整数の原文保持・URL追加、
既定値/直接ID・解除・入力検証前のPOSTなし、201後と名前解決時のエラー確度を確認します。
`tests/issue_update.rs` は実CLI/HTTPS境界で最新GETと単一PATCH、明示入力だけの差分form、設定非流用、
名前/直接ID/self・resolution 0と解除、日付のcalendar比較、milestone集合の置換/重複除去/順序不問と空配列解除を確認します。
全項目同値/未指定/null・空本文のno-op、title/日付/flag/UTF-8検証、literal/file/明示stdin・空入力/本文省略、
非明示stdinの非読取、global/profile/help・TTY非対話、未知field/巨大整数/原文と構築URL・安全表示を検証します。
事前エラーはPATCHなし、具体的4xx拒否・切断/5xx/redirect・200後のbody/decode/必須field/出力エラー・key衝突・timeout、
stdin/事前GET/送信後/200 headers後のCtrl+Cでも確度を保持し、retry/redirect/確認GETがないことを確認します。
200後の中断は `patch-headers-sent` markerを待ち、事前GETの200 markerとは区別します。
`tests/service.rs` の公開 `Service::issue_update` 試験でも原文/巨大ID・URL、比較に使うIDの検証と未使用field非要求、
直接ID/部分更新・no-op・入力検証時の通信なし・日付順序の独自制限なし・確度を確認します。
`tests/comment_add.rs` は実CLI/HTTPS境界で本文のみ/同値指定時の単一POST、実変更時の最新GET+単一PATCHを検証します。
UTF-8原文のliteral/file/明示stdin・空/未指定本文、名前/ID/解除の排他と単一指定、resolution ID 0、対象projectでの名前解決とself、
create既定値非流用・状態に伴う完了理由の自動変更なし、同値/変更なしの非書込み、raw data/未知field/巨大整数とurl非追加を確認します。
引数/config/本文/事前GET/名前解決失敗時はcomment.add/not_created、実方式選択後はPOST/PATCHそれぞれの確度語彙を確認します。
4xx拒否・切断/5xx/redirect・201/200後のbody/decode/必須field/出力失敗・key衝突・30秒timeoutと、stdin待ち/GET/送信後/成功headers後の
Ctrl+Cでも確度を保持し、retry/権限fallback/二重書込み/コメントID補完GETがないことをmethod/path/formと件数で照合します。
`tests/service.rs` の公開 `Service::comment_add` 試験でも原文/secret・巨大整数、未使用field非要求、ID0の解除、
入力検証時の通信なしと確度、返却dataのurl非追加を確認します。
`tests/comment_update.rs` は実CLI/HTTPS境界で最新コメント・認証ユーザー取得、任意精度の投稿者ID検証と所有者確認、
他者コメント/content:null履歴/同値の非書込み、必須かつ排他的な本文入力、課題項目flag拒否を確認します。
literal/file/明示stdinのUTF-8・空入力拒否・空白改行原文保持、非明示stdinの非読取、設定非流用、global/profile/help・TTY非対話、
contentだけの単一PATCH、元object/未知field/巨大整数と安全表示、引数/config/事前GET/権限拒否時の確度を検証します。
200後のbody切断/decode/必要field/出力失敗・key衝突・30秒timeout、stdin/事前GET/送信後/200 headers後のCtrl+Cでも確度を保持し、
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
# 30秒のbody timeoutだけを再確認
cargo test --locked --test project_list body_reading_is_inside_the_thirty_second_request_timeout -- --exact
```

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

成功基準は `code:0`、`restored:true`、空stderr、9 GETのみ、`open-args`は引数数1と設定originの`/view/B-3`のみです。
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
Issue #17以降の担当機能では承認済みPlanの実Backlog受入が別途必須です。未実施を完了扱いにしません。
editorの権限検査後のfile変更や、将来起動時のOSによる最終実行可否までは保証しません。

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
