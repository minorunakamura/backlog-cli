# backlog-cli

Go製のBacklog CLI `bl`。参加プロジェクト・優先度・プロジェクト内の課題種別の一覧を実装しています。対象はApple Silicon Mac（darwin/arm64）です。

```sh
bl --help
bl project list
bl priority list --json --no-input
bl --profile work project list --json
bl issue-type list --project MYAPP
bl issue-type list -p 123 --json --no-input
```

課題の検索・閲覧・作成、課題一覧/detailの対話UI、editor起動、配布archive、CIはこのスライスには含みません。仕様の全体は [正式Plan](docs/plans/backlog-cli-v1.md) を参照してください。

## 設定

共通設定の記入例は [examples/config.json](examples/config.json) です。`$XDG_CONFIG_HOME/backlog/config.json` に配置し、自分の接続先とAPI key環境変数名へ変更してください。`XDG_CONFIG_HOME` が未設定・空・相対パスなら `$HOME/.config/backlog/config.json` を読みます。

- `--profile` を省略すると `default` を選択します。存在しないprofileや空・未設定のAPI key環境変数はエラーです。別profile/keyへfallbackしません。
- API key本体は `apiKeyEnv` で指定した環境変数へ安全に設定してください。設定ファイル・コマンド引数・URLには記入しません。記入例に実値は含みません。
- `spaceUrl` はHTTPS origin（例: `https://example.backlog.com`）。末尾の `/` は正規化します。userinfo、query、fragment、ルート以外のpathは使えません。独自hostも利用できますが、通常のTLS証明書検証が必要です。
- 任意の `editor` は実行ファイルと引数の文字列配列。省略時は `["vim"]`、例は `["code", "--wait"]` です。空配列・空実行ファイル・NULはエラー。`VISUAL` / `EDITOR` は参照しません。将来の対話作成ではshellを介さず本文一時ファイルを最後の独立引数に渡す設定です。**今回の一覧コマンドはeditorを起動しません。**

ローカル設定の例は [examples/settings.json](examples/settings.json)。必要なら実行時のcwd直下 `.backlog/settings.json` に配置します。親ディレクトリは探索しません。ローカル設定の不在は正常、読み取り失敗・JSON破損・型不正・未知キー・不正値はエラーです。共通設定の不在は設定エラーです。

ローカルの `project` は `issue-type list` の既定値です。`issue.create.{issueType,priority,assignee}`、`issue.list.status` は将来のコマンドの既定値として読込・検証します。status未設定・`[]` は状態絞り込みなし。**`project list` はローカルprojectで絞り込まず、APIの既定 `all=false` の参加プロジェクト全体を1回取得します。**

## 課題種別一覧のproject指定

`bl issue-type list --project/-p` はプロジェクトキーまたは正整数int64 IDの単一値です。長短名は同等で、混在も含め重複指定はエラーです。`all` は利用できません。

- 明示指定 → cwdのローカル `project` の順で適用します。不正な設定を明示flagで隠すことはありません。
- キーはproject detail APIでIDへ解決し、issueTypes APIを取得します。直接IDは候補一覧・project detailを呼びません。
- 両方とも未指定なら、stdin/stdoutがともにTTYで、`--json` / `--no-input` がともにfalseのときだけ参加プロジェクト候補から選びます。他の場合は入力不足エラーです。暗黙の全プロジェクト検索はしません。
- 選択は上下・Enter、取消はEsc/Ctrl-C（exit 130）。候補のフィルターはありません。長い行はセル幅で省略し、極小端末ではサイズ案内を表示します。選択結果はこの呼出しだけで使い、設定へ保存しません。

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

人向け出力は固定表示（project: `ID / KEY / NAME`、issue-type/priority: `ID / NAME`）。ANSIと危険なC0/C1/DELを除去し、LF/TAB、日本語、emojiのZWJは保持します。空一覧では見出しだけを表示します。`project list` / `priority list` は入力待ちしません。`issue-type list` はproject未指定時だけ上記条件で候補選択に入ります。helpは設定・APIなしで利用できます。

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

公開実行境界は `internal/cli.ExecuteWithInput(ctx, args, stdin, stdout, stderr, client)` です。既存の `Execute(ctx, args, stdout, stderr, client)` も維持し、stdinには `os.Stdin` を使います。TTY判定は元streamのdescriptorで行い、UIへの出力ラッパーもdescriptorを保持してresizeを受け取ります。`client=nil` は通常のTLS検証、`httptest.NewTLSServer` の `server.Client()` を渡すと、そのmock証明書だけを信頼して実際のCobra→設定→HTTP→表示→exitを検証できます。clientのTransportを維持しつつtimeout30秒・redirect拒否は強制し、呼出元clientは変更しません。設定は `t.TempDir` / `t.Setenv` / `t.Chdir` で隔離します。

```sh
mise exec -- go test ./internal/cli -count=1 -v
mise exec -- go test ./internal/backlog ./internal/config ./internal/output -count=1 -v
mise exec -- go test ./cmd/bl -count=1 -v
```

CLIテストは認証/path、JSON原本、固定表示、未知flag/args、API拒否、不正JSON、redirect、出力失敗、本文読込中のcancel、未信頼TLS拒否に加え、projectのkey/ID・長短flag・local優先順位・重複/all/不足拒否・API失敗を検証します。UI ModelのUpdate/Viewテストは選択・取消・候補無害化・filter無効・長い日本語/emoji・resize/極小サイズを検証します。HTTP境界テストでrequest contextの30秒deadlineと本文読込のdeadline適用を確認します。`cmd/bl` テストは実バイナリのhelp/parser/config/stream/exitを検証します。実バイナリへmock独自CAを渡す本番用flag/debug機能はありません。darwinの実バイナリは通常のOS trustを使うため、self-signed mockは上の注入境界で検証してください。

実スペースへの接続は自動テストに含みません。

### 実APIなしの実端末受入（人間による確認が別途必要）

次のmock harnessは一時設定・専用TLS serverを自動作成し、終了時に削除します。実設定・実Backlog APIを使わず、25候補と長い日本語/ZWJ emoji・危険文字を返します。本番用debug flagや独自CA受入flagは追加していません。

```sh
mise exec -- go build -o /tmp/bl-terminal-mock ./internal/cli/testdata/terminal_mock.go
/tmp/bl-terminal-mock issue-type list
```

Apple Silicon Macの日本語端末で、(1) 上下で20件を超えて移動しEnter、`ID / NAME` 固定出力に選択projectの番号が現れる、(2) 長い日本語/👩‍💻が崩れず幅内で省略される、(3) resizeと10列×3行程度の極小サイズでpanicせずサイズ案内が出る、(4) EscとCtrl-Cそれぞれで元の端末へ戻り `echo $?` が130、を確認してください。実行し直すと必ず未指定状態から選び直します。`q`は取消キーではありません。

非対話gateと直接指定も同じmockで確認できます。

```sh
/tmp/bl-terminal-mock issue-type list --json          # exit 2、入力不足JSON（待機なし）
/tmp/bl-terminal-mock issue-type list --no-input      # exit 2、入力不足（待機なし）
/tmp/bl-terminal-mock issue-type list </dev/null      # exit 2、stdin非TTY
/tmp/bl-terminal-mock issue-type list | cat          # stdout非TTY、bl側exit 2（shellのpipe exitに注意）
/tmp/bl-terminal-mock issue-type list -p APP          # key→ID 42、固定出力
/tmp/bl-terminal-mock issue-type list -p 42 --json    # 大整数を保持したitems JSON
```

スクロールの再受入では、↓を1回ずつ押し、表示範囲内では候補を消さずカーソルだけが移動し、下端に達したときだけ表示範囲が1行ずつ動くことを確認してください。末尾でも候補行数が減って空白にならず、↑で先頭まで戻れ、resize後も選択が保持されることを確認します。27行以上の端末では25候補すべてが上下移動中も表示され続けることを確認してください。

任意のPython 3標準PTYによる自動smokeも用意しています（Goの正式checkにはPython不要）。両TTY/片側非TTY/両非TTY、json/no-input gate、選択と取消130、resizeを検証します。

```sh
python3 internal/cli/testdata/terminal_check.py /tmp/bl-terminal-mock
```

**PTY/Modelの成功は、人間による実端末の見やすさ・操作感の受入完了を意味しません。** 実スペース接続も未検証です。
