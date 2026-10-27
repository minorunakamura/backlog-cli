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
RatatuiはCrossterm 0.29 backendを明示しています。tui-input 0.15.5は実端末確認前の
候補であり、まだ依存に追加していません。`bl-cli project list` と候補一覧3コマンド、
`issue list/mine` と共通libraryの候補/名前解決/検索を実装しています。
書込み・TUIの機能は後続Work Itemの範囲です。

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
`tests/service.rs` の公開 `Service::search` 試験では既定値適用・原文/巨大total保持と入力検証も確認します。
`tests/common/mod.rs` は既存CLI harnessの共有部分です。library試験も個別の子テストprocessで実行し、
そのprocessだけにTLS trustを渡すため、並列試験で環境変数を書き換えません。
`tests/safety.rs` は共通libraryの公開安全表示入口で、原文保持・Unicode・scalar/keyの伏字と
key衝突を検証します。内部private関数の固定テストはありません。
`tests/https_fixture.rs` はfixtureの逆引きを失敗させた別processから起動し、
証明書を検証したHTTPSリクエストが成功することを確認します。固定loopback serverの
初期化では不要な逆引きをせず、CIのresolver待ちに依存させません。

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

fixtureのstdoutの最初の行がport、`requests` がHTTP method/path/headerのJSONL記録です。
上の例は順に `{"items":[]}`、`ID`/`NAME` 見出し、`{"items":[]}` を出します。
記録は3 GETだけで、pathは `/api/v2/projects/a%2Fb%25%3F%23/issueTypes`、
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
fixtureは `routes` があればqueryを除いたpathでdispatchし、不明pathは404です。
`routes` がなければ従来どおり全pathに同じ応答を返します。各routeは通常のresponseと同じ形式です。
生成物はテスト専用で、実サービスに使わないでください。
`response.json` の `delay` はheaders送信後の秒数、`location` はredirect先、
`disconnect:true` は応答前の切断を制御します。実API keyは使わないでください。

現段階では実Backlogへの接続/書込み、実IME/TUI受入、配布tar.gz/SHA-256作成や公開は行いません。
editorの権限検査後のfile変更や、将来起動時のOSによる最終実行可否までは保証しません。
