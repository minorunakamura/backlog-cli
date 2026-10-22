# backlog-cli

Go製のBacklog CLI `bl`。初回縦スライスとして、参加プロジェクトと優先度の取得だけを実装しています。対象はApple Silicon Mac（darwin/arm64）です。

```sh
bl --help
bl project list
bl priority list --json --no-input
bl --profile work project list --json
```

課題の検索・閲覧・作成、対話UI、editor起動、配布archive、CIはこのスライスには含みません。仕様の全体は [正式Plan](docs/plans/backlog-cli-v1.md) を参照してください。

## 設定

共通設定の記入例は [examples/config.json](examples/config.json) です。`$XDG_CONFIG_HOME/backlog/config.json` に配置し、自分の接続先とAPI key環境変数名へ変更してください。`XDG_CONFIG_HOME` が未設定・空・相対パスなら `$HOME/.config/backlog/config.json` を読みます。

- `--profile` を省略すると `default` を選択します。存在しないprofileや空・未設定のAPI key環境変数はエラーです。別profile/keyへfallbackしません。
- API key本体は `apiKeyEnv` で指定した環境変数へ安全に設定してください。設定ファイル・コマンド引数・URLには記入しません。記入例に実値は含みません。
- `spaceUrl` はHTTPS origin（例: `https://example.backlog.com`）。末尾の `/` は正規化します。userinfo、query、fragment、ルート以外のpathは使えません。独自hostも利用できますが、通常のTLS証明書検証が必要です。
- 任意の `editor` は実行ファイルと引数の文字列配列。省略時は `["vim"]`、例は `["code", "--wait"]` です。空配列・空実行ファイル・NULはエラー。`VISUAL` / `EDITOR` は参照しません。将来の対話作成ではshellを介さず本文一時ファイルを最後の独立引数に渡す設定です。**今回の2コマンドはeditorを起動しません。**

ローカル設定の例は [examples/settings.json](examples/settings.json)。必要なら実行時のcwd直下 `.backlog/settings.json` に配置します。親ディレクトリは探索しません。ローカル設定の不在は正常、読み取り失敗・JSON破損・型不正・未知キー・不正値はエラーです。共通設定の不在は設定エラーです。

ローカルの `project`、`issue.create.{issueType,priority,assignee}`、`issue.list.status` は将来のコマンドの既定値として読込・検証します。status未設定・`[]` は状態絞り込みなし。**`project list` はローカルprojectで絞り込まず、APIの既定 `all=false` の参加プロジェクト全体を1回取得します。**

## 出力と失敗

`--json` はstdoutへ改行終端のJSON文書1つだけを出します。APIの各objectの構造・未知フィールド・日本語・制御文字の値を保持し、secretだけを除去します。ページ送りはありません。

```json
{"items":[]}
```

JSONモードの失敗はstderrだけへ出し、stdoutに部分成功を出しません。例:

```json
{"error":{"code":"config_error","message":"selected profile is missing","httpStatus":null,"backlogErrors":[],"creation":null}}
```

`code` は `input_error` / `config_error` / `api_error` / `transport_error` / `response_error` / `output_error` / `cancelled`。HTTP応答を受けた場合は `httpStatus` を入れ、Backlogの `errors` 配列を `backlogErrors` に構造保持します。secretはエラー内でも除去します。今回は `creation` は常にnullです。

人向け出力は固定表示（project: `ID / KEY / NAME`、priority: `ID / NAME`）。ANSIと危険なC0/C1/DELを除去し、LF/TAB、日本語、emojiのZWJは保持します。空一覧では見出しだけを表示します。`--no-input` を受け付けますが、今回の一覧は元から入力待ちしません。helpは設定・APIなしで利用できます。

| 結果 | exit |
| --- | --- |
| 成功 | 0 |
| 入力・設定エラー | 2 |
| API・通信・応答・出力エラー | 1 |
| 中断 | 130 |

API keyは `Backlog-API-Key` headerだけで送信します。redirectは同一originも含め全拒否、CLI独自のretryなし、1リクエストのtimeoutは本文読み取りまで含め30秒です。エラーにURL/header/raw responseのdumpは出しません。stdoutのI/O障害では書込み済みの断片を撤回できないため、stderrとexitを必ず確認してください。

## 開発

Go **1.26.10** を `mise.toml` で固定しています。

```sh
mise trust
mise install
mise exec -- make tools
mise exec -- make fmt
mise exec -- make check
mise exec -- make test-race
mise exec -- make security
mise exec -- make build
./dist/bl --help
```

`make tools` は開発専用の `.tools/bin` へgolangci-lint **v2.9.0** とgovulncheck **v1.9.0** をGo 1.26.10でビルドします。golangci-lintに内包されるgofumpt **v0.9.2** とgoimports（x/tools **v0.42.0**）を使います。別版のformatterを重ねてインストールしません。バージョンは `.tools/bin/golangci-lint version` / `go version -m .tools/bin/golangci-lint` / `.tools/bin/govulncheck -version` で確認できます。

- `fmt`: `golangci-lint fmt ./...`
- `check`: `golangci-lint fmt --diff ./...`、`golangci-lint run ./...`、`go test ./...`、`go vet ./...`
- `test-race`: `go test -race ./...`
- `security`: `govulncheck -test ./...`
- `build`: `CGO_ENABLED=0 GOOS=darwin GOARCH=arm64 go build -trimpath -o dist/bl ./cmd/bl`

editorの保存時整形にも、このリポジトリのcwdから `mise exec -- make fmt`（または `.tools/bin/golangci-lint fmt ./...`）を使用してください。gopls等が使う別版formatterと混在させないよう設定します。特定editor向け設定ファイルは提供しません。

### 実APIなしの直接検証

公開実行境界は `internal/cli.Execute(ctx, args, stdout, stderr, client)` です。`client=nil` は通常のTLS検証、`httptest.NewTLSServer` の `server.Client()` を渡すと、そのmock証明書だけを信頼して実際のCobra→設定→HTTP→表示→exitを検証できます。clientのTransportを維持しつつtimeout30秒・redirect拒否は強制し、呼出元clientは変更しません。設定は `t.TempDir` / `t.Setenv` / `t.Chdir` で隔離します。

```sh
mise exec -- go test ./internal/cli -count=1 -v
mise exec -- go test ./internal/backlog ./internal/config ./internal/output -count=1 -v
mise exec -- go test ./cmd/bl -count=1 -v
```

CLIテストは認証/path、JSON原本、固定表示、未知flag/args、API拒否、不正JSON、redirect、出力失敗、本文読込中のcancel、未信頼TLS拒否を検証します。HTTP境界テストでrequest contextの30秒deadlineと本文読込のdeadline適用を確認します。`cmd/bl` テストは実バイナリのhelp/parser/config/stream/exitを検証します。実バイナリへmock独自CAを渡す本番用flag/debug機能はありません。darwinの実バイナリは通常のOS trustを使うため、self-signed mockは上の注入境界で検証してください。

実スペースへの接続は自動テストに含みません。
