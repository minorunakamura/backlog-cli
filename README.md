# backlog-cli

Go製のBacklog CLI `bl`。参加プロジェクト・優先度・プロジェクト内の課題種別の一覧と、課題の名前/ID条件・self・横断検索と対話一覧・ページ送り、Markdown/Backlog記法の課題の詳細viewer・固定表示・JSONでの原本取得、非対話での課題作成を実装しています。対象はApple Silicon Mac（darwin/arm64）です。

```sh
bl --help
bl project list
bl priority list --json --no-input
bl --profile work project list --json
bl issue-type list --project MYAPP
bl issue-type list -p 123 --json --no-input
bl issue list "ログイン" -p MYAPP --status-id 1 --status-id 2 --json
bl issue list -p 123 --assignee-id 456 --milestone-id 789 --limit 20 --offset 0
bl issue list -p MYAPP -s 未対応 -s 処理中 -a self -a 山田 -m v1.0
bl issue mine -p MYAPP --json
bl issue mine -p all
bl issue list "ログイン" -p all --json
bl issue view MYAPP-123
bl issue view MYAPP-123 --json
bl issue view 12345 --json --no-input
bl issue create -p MYAPP --issue-type タスク --priority 中 -t "ログインの修正" -a self --json
bl issue create -p 123 --issue-type-id 7 --priority-id 3 -t "修正" --body-file ticket.md
```

対話での課題作成、editor起動、配布archiveは後続の範囲です。未対応のコマンド・flagはエラーとし、黙って別条件で実行しません。仕様の全体は [正式Plan](docs/plans/backlog-cli-v1.md) を参照してください。

## 設定

共通設定の記入例は [examples/config.json](examples/config.json) です。`$XDG_CONFIG_HOME/backlog/config.json` に配置し、自分の接続先とAPI key環境変数名へ変更してください。`XDG_CONFIG_HOME` が未設定・空・相対パスなら `$HOME/.config/backlog/config.json` を読みます。

- `--profile` を省略すると `default` を選択します。存在しないprofileや空・未設定のAPI key環境変数はエラーです。別profile/keyへfallbackしません。
- API key本体は `apiKeyEnv` で指定した環境変数へ安全に設定してください。設定ファイル・コマンド引数・URLには記入しません。記入例に実値は含みません。
- `spaceUrl` はHTTPS origin（例: `https://example.backlog.com`）。末尾の `/` は正規化します。userinfo、query、fragment、ルート以外のpathは使えません。独自hostも利用できますが、通常のTLS証明書検証が必要です。
- 任意の `editor` は実行ファイルと引数の文字列配列。省略時は `["vim"]`、例は `["code", "--wait"]` です。空配列・空実行ファイル・NULはエラー。`VISUAL` / `EDITOR` は参照しません。将来の対話作成ではshellを介さず本文一時ファイルを最後の独立引数に渡す設定です。**今回の一覧・非対話createはeditorを起動しません。**

ローカル設定の例は [examples/settings.json](examples/settings.json)。必要なら実行時のcwd直下 `.backlog/settings.json` に配置します。親ディレクトリは探索しません。ローカル設定の不在は正常、読み取り失敗・JSON破損・型不正・未知キー・不正値はエラーです。共通設定の不在は設定エラーです。

ローカルの `project` は `issue-type list` / `issue list` / `issue mine` / `issue create` の既定値です（ローカル設定に `all` は指定不可）。`issue.create.{issueType,priority,assignee}` は作成時の名前の既定値です。課題検索には適用しません。`issue.list.status` はstatus名の既定値で、未設定・`[]` は状態絞り込みなし。非空ならプロジェクトの状態名から解決します。明示 `--status/-s` または `--status-id` はこの既定値全体を上書きし、`--status all` は状態条件を解除します。横断検索ではローカルproject・検索条件を適用しませんが、設定自体の検証は省略しません。**`project list` はローカルprojectで絞り込まず、APIの既定 `all=false` の参加プロジェクト全体を1回取得します。**

## 課題種別一覧のproject指定

`bl issue-type list --project/-p` はプロジェクトキーまたは正整数int64 IDの単一値です。長短名は同等で、混在も含め重複指定はエラーです。`all` は利用できません。

- 明示指定 → cwdのローカル `project` の順で適用します。不正な設定を明示flagで隠すことはありません。
- キーはproject detail APIでIDへ解決し、issueTypes APIを取得します。直接IDは候補一覧・project detailを呼びません。
- 両方とも未指定なら、stdin/stdoutがともにTTYで、`--json` / `--no-input` がともにfalseのときだけ参加プロジェクト候補から選びます。他の場合は入力不足エラーです。暗黙の全プロジェクト検索はしません。
- 選択は上下・Enter、取消はEsc/Ctrl-C（exit 130）。候補のフィルターはありません。長い行はセル幅で省略し、極小端末ではサイズ案内を表示します。選択結果はこの呼出しだけで使い、設定へ保存しません。

## 課題検索と対話一覧

`bl issue list [keyword]` は任意の1位置引数をそのままkeywordにし、`--project/-p` は上の課題種別一覧と同じ明示 → local → TTY候補選択の順で決定します。未指定を横断検索として扱いません。直接project IDなら検索時のproject detailは呼びません（詳細を開くときは本文記法の確認に使います）。

- `--status-id` / `--assignee-id` / `--milestone-id` は正整数int64。反復すると各項目内でORになり、正規化後の同じIDは重複除去します。カンマ分割はしません。
- `--limit` は既定20、1〜100。`--offset` は既定0、0以上のint64です。project/limit/offset/profileなど単一値flagの重複は長短名混在も含めエラーです。
- `--status/-s`、`--assignee/-a` / `--assignee-name`、`--milestone/-m` はそれぞれprojectのstatuses/users/versions APIで名前を完全一致解決します。候補なし・複数一致は入力エラーです。数字やカンマを含む名前もそのまま名前として扱います。名前flagも反復OR、同じIDは重複除去します。同じ項目の名前系とID系は混在できません。直接ID指定だけならその項目の候補APIは呼びません。
- `--assignee self` は選択profileのAPI keyに対応する `/users/myself` のユーザーです。実名 `self` は `--assignee-name self` で指定します。`-a self -a 山田 --assignee-name self` のように名前系を併用できます。必要なAPIだけを1実行内で取得し、候補や本人情報を永続保存しません。
- `--status all` は単独指定で状態条件を解除します。通常の状態名・status ID・反復allと混在できません。実名 `all` の状態は `--status-id` で指定してください。
- `bl issue mine [keyword]` は `issue list --assignee self` と同じ設定・filter・count/list・表示・JSON・ページ送りを使います。明示の `--assignee/-a` / `--assignee-name` / `--assignee-id` は、self指定も含めすべて拒否します。
- `--project/-p all` はlist/mineだけで使える横断検索です。空白だけでないkeywordまたはselfが必須（`issue mine -p all` はkeyword不要）。project条件を省略して検索し、参加project一覧を別途取得しません。ローカルproject・検索statusは無視し、明示status（all含む）/status-id/milestone/milestone-id、self以外のassignee、assignee-name、assignee-idはkeywordがあっても拒否します。limit/offsetは利用できます。
- 確定した同じfilterで最初にcountを1回取得し、成功した場合だけlistを1回取得します。countにlimit/offset/sort/orderは渡さず、listには `count=limit` / `offset` / `sort=updated` / `order=desc` を明示します。count失敗を0件に置き換えません。
- stdin/stdoutがともにTTYで `--json` / `--no-input` がともにfalseなら、HuhのスクロールSelectで課題キー・件名・状態・担当者を表示します。上下/`j/k`で移動し、Enterで詳細または「前ページ」「次ページ」「終了」を選択します（前/次は可能時だけ、この順で課題行の後ろ）。`l`は課題行だけ詳細、`p/n`は即前/次ページ、`R`（Shift+r）は同じ検索の最新countとoffset=0ページを再取得、`q`は即終了0、Ctrl-C/context cancelは130です。文字フィルターはなく、これらのショートカットは一覧だけで有効です。長い日本語/emoji行をセル幅で省略し、resizeに追従します。20列×5行未満はサイズ案内（`q`で終了）です。
- 課題選択時はdetail APIを取得し、`issue view`と共通の詳細viewerを開きます。詳細の`h/q`は元のページ・選択・一覧スクロールへ戻り、list/countを再取得しません。詳細は開くたびに取得します。訪問したページは起動中のメモリだけに保持し、前後の再訪でも取得せず各ページの選択（操作項目を含む）・スクロールを復元します。初期`--offset`以前へは戻りません。詳細取得/描画・ページ取得の失敗は一覧に安全に通知し、現在ページ・履歴・位置を保持します（空ページや終了に置き換えません）。
- 一覧`R`は確定済みfilter・limitを使ってcount→list(offset=0)を取得し、表示データの検証まで成功したらtotal・先頭ページを一括交換して旧履歴を消します。初期offsetが非0でも0へ戻り、空結果も成功です。count/list/表示検証に失敗したら旧total・全履歴・ページ・位置を保ちます。取得中は重複取得・移動を抑止しますが、一覧`q`、詳細`h/q`、中断は有効です。
- 非TTY/JSON/no-inputではUIを起動せず、従来どおり1ページを出して終了します。全件自動取得はありません。空ページはtotalにかかわらずページ送り終了、非空なら `offset + 実取得count < 開始時total` の場合だけ「次ページ」/`n`が有効です（整数overflowは回避）。次offsetはlimitでなく実取得countを加算し、countは取り直しません。固定表示の `Next offset: N` は、続きのページを `--offset N` で明示取得するための案内です。
- countとlistは別snapshotです。途中の追加・削除・更新によって重複・取りこぼし・totalとの差が生じ得ます。一貫したsnapshotは保証しません。次のコマンド実行または一覧`R`でcountを取り直します。訪問ページは一覧`R`まで古い内容のままです。メモリ使用量は訪問ページ数に応じて増えます。`R`でも一貫したsnapshotや重複・取りこぼしの解消は保証しません。

## 課題の詳細viewer・固定表示とJSON取得

`bl issue view <issueIdOrKey> --json` は正整数int64 IDまたは課題キー（例: `MYAPP-123`）を検証・path encodeし、`GET /api/v2/issues/:issueIdOrKey` を1回だけ呼びます。API応答objectの構造・未知フィールド・大整数・本文のJSON string値を保持し、設定originと検証した応答issueKeyから作る `url` を追加します。secretは既存の出力処理で除去します。

- project指定は不要で、ローカルproject・検索条件・作成既定値は適用しません。設定自体の検証と `--profile` は他コマンドと共通です。
- JSON取得にproject/本文記法のAPIは呼びません。TTYの有無にかかわらず選択UI・viewer・editorを起動せず、stdinも読みません。
- `--json` なしでは、取得課題の `projectId` からproject detail APIを取得し、`textFormattingRule`（`markdown` / `backlog`）に応じて変換し、件名・URL・整形済み本文を表示します。**stdin/stdoutがともにTTYで `--no-input` もfalseのときだけ**、Bubbles viewportのCLI内viewerを起動します。短い本文でも自動終了せず、`q`で閉じるとコマンドが終了します。外部pagerや切替設定はありません。
- viewerは上下 / `j`・`k`、PgUp/PgDn / `b`・Space、先頭 `g`・Home / 末尾 `G`・Endで移動します。単独viewの`h`は無操作でヘルプにも表示しません。一覧経由だけ`h/q`で一覧へ戻ります。`R`は両方で同じ課題とproject記法を再取得し、応答時点の最新幅で安全に描画できた場合だけ内容を交換して先頭へ戻します。失敗時は旧内容・スクロール位置を保ち、安全に通知します。詳細`R`は背後の一覧を更新しません。Ctrl-Cまたはcontext cancelはexit 130です。幅のresizeでは共通の安全描画を再実行し、スクロール位置を範囲内へ調整します。20列×4行未満ではサイズ案内を表示し、`q`で閉じられます。
- `--no-input` または片側でも非TTYならviewerを起動せず、80列基準の整形済み詳細を1回固定出力して即終了します。stdoutが非TTYならANSIなしです。JSONは常に入力待ちなしで原本を返します。
- Markdown/GFMとBacklog記法の基本書式・通常改行を保持し、detailsは展開、色は本文保持、課題キーは設定originのURL、Wiki/Git/SVN/添付参照は説明text、目次は静的リスト、画像はalt/既知URL/IDだけを表示します。結合表はセル・見出し・captionを落とさず通常表へ簡略化します。外部画像や参照先は取得しません。
- code block/inline code/escapeを保護し、未知macroは説明表現、未知HTMLはタグを除いて本文を保持します。外部ANSI/危険な制御文字とentity復号による制御文字の復活を防ぎ、描画後もGlamour生成SGR以外の端末操作を除去します。JSON原本文にこの表示処理は適用しません。
- Backlog記法は行/inline scannerで見出し・list・強調・引用・code・checklist・表をMarkdownへ変換し、`{code}` / `{code:lang}` とU+005C escapeを保護します。Backlog原文のMarkdown専用記号は文字として保持します。
- 未知・欠落した `textFormattingRule` は推測せず表示エラー（exit 1、`--json` を案内）です。原本文の直接出力やMarkdownへの誤適用では代用しません。固定表示は従来どおり80列を基準に整形します。
- 入力不足・不正はexit 2、404・認証/権限等のAPI拒否は `api_error`、通信失敗は `transport_error`、不正JSON/object/issueKeyは `response_error`（いずれもexit 1）です。JSONエラーはstderrだけに出します。

## 非対話の課題作成

`bl issue create` はproject・issue type・priority・空白だけでないtitleが必須です。明示flag → ローカル設定の順で適用し、種別・優先度をCLIが独自選択しません。**この段階ではTTYでも必須不足は `input_error`（exit 2）です。対話作成は後続Issueで実装します。** 必須が揃えば本文省略でも即実行し、確認待ち・editor起動・暗黙stdin読込はありません。

- `--project/-p` はキーまたは正整数int64 ID（`all` 不可）。`--issue-type` / `--priority` は完全一致名、`--issue-type-id` / `--priority-id` は正整数int64 IDです。同じ項目の名前とIDは排他、候補なし・複数一致はエラー。数字の名前も名前として解決します。直接IDなら不要な候補APIを呼びません。
- `--title/-t` は送信時にトリミングしません。単一値flagはすべて長短名混在も含め重複指定を拒否します。
- 担当者は任意の1人。`--assignee/-a self` は選択profileの認証ユーザー、通常名は完全一致、実名selfは `--assignee-name self`、直接IDは `--assignee-id` です。3形式は相互排他。`--no-assignee` は設定の担当者既定値を解除し、明示担当者と併用できません。
- 本文は `--body/-b` または `--body-file`。空文字指定も含め排他です。ファイルと明示 `--body-file -` のstdinをUTF-8検証し、改行や空白をトリミングせず送ります。本文省略・空本文を許可します。
- `application/x-www-form-urlencoded` で `POST /api/v2/issues` を1回だけ送信します。201応答そのものをJSON原本＋ `url` または人向けの作成成立・課題キー・URLとして返します。成功後の追加GET・viewer起動・retryはしません。
- 必須custom属性があるprojectではAPIに拒否される可能性があります。この段階ではcustom属性など作成項目を増やしません。

```sh
printf '修正内容\n' | bl issue create -p MYAPP --issue-type タスク --priority 中 -t 修正 --body-file - --no-assignee --json
```

## 出力と失敗

`--json` はstdoutへ改行終端のJSON文書1つだけを出します。APIの各objectの構造・未知フィールド・日本語・制御文字の値を保持し、secretだけを除去します。課題以外の一覧にはpaginationを付けません。

```json
{"items":[]}
```

課題一覧は検証したissueKeyと設定originから作る `url` を各API objectへ追加し、paginationのcountは実取得件数、totalは検索開始時件数です。`hasMore` は出しません。空ページも `items: []` です。

```json
{"items":[{"issueKey":"MYAPP-1","summary":"件名","url":"https://example.backlog.com/view/MYAPP-1"}],"pagination":{"offset":0,"limit":20,"count":1,"total":1}}
```

JSONモードの失敗はstderrだけへ出し、stdoutに部分成功を出しません。例:

```json
{"error":{"code":"config_error","message":"selected profile is missing","httpStatus":null,"backlogErrors":[],"creation":null}}
```

`code` は `input_error` / `config_error` / `api_error` / `transport_error` / `response_error` / `output_error` / `cancelled`。HTTP応答を受けた場合は `httpStatus` を入れ、Backlogの `errors` 配列を `backlogErrors` に構造保持します。secretはエラー内でも除去します。create以外は `creation: null`。createではparser/設定エラーも含め `{"state":"not_created|unknown|created","issueKey":null,"url":null}` を入れ、安全に取得できたキー/URLだけをstringにします。secretはこの識別情報からも除去します。

POST前・明示拒否（4xx）は `not_created`、切断・timeout・送信後中断・5xx・非201の予期しない成功/redirectは `unknown` です。**201ヘッダ受信時点で `created` とし、その後の本文読込/decode/出力障害でも戻しません。** レスポンスの検証済みキー、または同一originのLocationで判明した識別情報を通知します。取得できなければnullのままです。作成済みなら再作成しないでください。unknownの場合も未作成とは限らず、手動再実行は重複作成の危険があるため、作成状況を確認してください。自動再送や成否を調べる追加GETはありません。

人向け出力は固定表示（project: `ID / KEY / NAME`、issue-type/priority: `ID / NAME`、issue: `KEY / SUMMARY / STATUS / ASSIGNEE`）。ANSIと危険なC0/C1/DELを除去し、LF/TAB、日本語、emojiのZWJは保持します。空一覧では見出しだけを表示します。`project list` / `priority list` は入力待ちしません。`issue-type list` / `issue list` はproject未指定時だけ上記条件で候補選択に入ります。helpは設定・APIなしで利用できます。

| 結果 | exit |
| --- | --- |
| 成功 | 0 |
| 入力・設定エラー | 2 |
| API・通信・応答・出力エラー（作成成立確認前） | 1 |
| 作成成立確認後の本文読込・decode・出力等のエラー | 3 |
| 中断（他のexitより優先、creationは保持） | 130 |

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

### CI

[GitHub Actions](.github/workflows/ci.yml) はmainへのpush / pull request時にmacOS runnerで `make tools` → `make check` → `make test-race` → `make security` → `make build` を実行します。Goのバージョンは `go.mod` から読み、module/build cacheを利用します。実Backlog APIへの接続やAPI keyのSecrets設定は不要です。配布archive作成・リリース公開は行いません。

### 実APIなしの直接検証

公開実行境界は `internal/cli.ExecuteWithInput(ctx, args, stdin, stdout, stderr, client)` です。既存の `Execute(ctx, args, stdout, stderr, client)` も維持し、stdinには `os.Stdin` を使います。TTY判定は元streamのdescriptorで行い、UIへの出力ラッパーもdescriptorを保持してresizeを受け取ります。`client=nil` は通常のTLS検証、`httptest.NewTLSServer` の `server.Client()` を渡すと、そのmock証明書だけを信頼して実際のCobra→設定→HTTP→表示→exitを検証できます。clientのTransportを維持しつつtimeout30秒・redirect拒否は強制し、呼出元clientは変更しません。設定は `t.TempDir` / `t.Setenv` / `t.Chdir` で隔離します。

```sh
mise exec -- go test ./internal/cli -count=1 -v
mise exec -- go test ./internal/backlog ./internal/config ./internal/output ./internal/markup -count=1 -v
mise exec -- go test ./cmd/bl -count=1 -v
```

CLIテストは認証/path、JSON原本、固定表示、未知flag/args、API拒否、不正JSON、redirect、出力失敗、本文読込中のcancel、未信頼TLS拒否に加え、projectのkey/ID・長短flag・local優先順位・重複/all/不足拒否・API失敗を検証します。UI ModelのUpdate/Viewテストは選択・取消・候補無害化・filter無効・長い日本語/emoji・resize/極小サイズに加え、detailの移動キー・短い本文のq待機・親を終了しない閉鎖結果・再flow/offset clamp・中断/描画エラーを検証します。HTTP境界テストでrequest contextの30秒deadlineと本文読込のdeadline適用を確認します。`cmd/bl` テストは実バイナリのhelp/parser/config/stream/exitを検証します。実バイナリへmock独自CAを渡す本番用flag/debug機能はありません。darwinの実バイナリは通常のOS trustを使うため、self-signed mockは上の注入境界で検証してください。

課題検索の公開CLI/HTTP/出力テストでは、count/listのquery一致・順序/回数、名前の完全一致/不一致/曖昧さ、数字・カンマの名前、selfと実名self、名前/IDのOR・重複除去・混在拒否、単一値flag重複、limit/offset境界、設定優先/status all、mineとlistの出力一致、横断検索の許可/拒否と不要な候補APIの不在を確認します。候補API/count/list失敗・不正応答・中断、空ページ・totalとの差・int64上限、JSON原本/大整数/未知フィールドとURL、安全な固定表示も検証します。

課題取得のCLI/HTTP/出力テストはID/キー・path encode・profile、単一GETと不要なproject API/入力待ちの不在、JSON本文/日本語/ESC・未知field/大整数・応答キー由来のURL・secret除去、入力/args拒否、404/認証/通信/不正JSON/出力失敗/中断のstderrとexit、Markdown/Backlog固定表示・project記法取得・未知記法の安全な拒否・非TTY ANSI除去・入力待ちなしを検証します。変換/描画の公開境界では公式例fixture、code/escape、HTML/特殊参照/複雑表、entity/ANSI/C0/C1/壊れたsequence・長い未終端入力を検証します。

課題作成のCLI/HTTP/出力テストはform/header、POST1回・追加GET/redirect/retryなし、既定値/明示値/profile/self、本文経路とUTF-8・非トリミング、入力/排他/重複拒否、切断・timeout・送信後中断・5xx・201の壊れJSON/本文読込/出力失敗とcreation/exit、未知field/大整数/secretを検証します。

実スペースへの接続は自動テストに含みません。

### 実APIなしの実端末受入（人間による確認が別途必要）

次のmock harnessは一時設定・専用TLS serverを自動作成し、終了時に削除します。実設定・実Backlog APIを使わず、25 project候補・既定3課題と長い日本語/ZWJ emoji・危険文字を返します。本番用debug flagや独自CA受入flagは追加していません。

```sh
mise exec -- go build -o /tmp/bl-terminal-mock ./internal/cli/testdata/terminal_mock.go
/tmp/bl-terminal-mock issue-type list
/tmp/bl-terminal-mock issue view APP-1              # 両TTYならviewer、qで閉じる
/tmp/bl-terminal-mock issue view APP-1 | cat        # ANSIなし、通常改行/日本語/👩‍💻を保持
/tmp/bl-terminal-mock issue view APP-1 --no-input   # 同じ固定表示、待機なし
/tmp/bl-terminal-mock issue view APP-1 --json       # 原本文のentity/ESCをJSON値として保持
# 以下の環境変数はmock harness専用（既定markdown）。本番CLIの設定/flagではありません。
BL_TERMINAL_MOCK_RULE=backlog /tmp/bl-terminal-mock issue view APP-1
BL_TERMINAL_MOCK_RULE=backlog /tmp/bl-terminal-mock issue view APP-1 --no-input | cat
BL_TERMINAL_MOCK_RULE=backlog /tmp/bl-terminal-mock issue view APP-1 --json
# mock専用: 短い/長い詳細（RULE=backlogとの併用も可能）
BL_TERMINAL_MOCK_BODY=short /tmp/bl-terminal-mock issue view APP-1
BL_TERMINAL_MOCK_BODY=long /tmp/bl-terminal-mock issue view APP-1
# mock専用: 装飾をまたぐダミーsecret。80→20→36→80列で本文の[REDACTED]維持を確認（RULE=backlogも対象）
BL_TERMINAL_MOCK_BODY=styled-secret /tmp/bl-terminal-mock issue view APP-1
/tmp/bl-terminal-mock issue view APP-1 </dev/null  # stdin非TTY、固定表示で即終了
```

issue viewは両記法で、見出し・連続する通常改行・展開されたdetails・参照説明・結合表の全セル/見出し/caption・codeのentity原文字列が読めること、画像を取得しないこと、clipboard/title/画面消去等が起きずsecretが出ないこと、JSON/no-input/片側非TTYでは待機せず固定表示/JSONで終了することを別途確認してください。

詳細viewerは両記法の短い/長い本文で、(1) 短い本文でも画面が残り`q`で終了0、(2) 上下・`j/k`、PgUp/PgDn・`b`/Space、`g`/Home・`G`/Endで移動でき、先頭末尾で行が欠けない、(3) 日本語・👩‍💻・長い行・件名・URLが読める、(4) 幅を80→36→80列などへ変えて再flowし、末尾で高さを増やしても空白へ飛ばない、(5) 10列×3行でサイズ案内、元のサイズへ戻して読み続けられる、(6) `q`/Ctrl-Cで元の画面と端末入力が復元され、`echo $?`が0/130、を人間の実端末で確認してください。一覧経由では詳細の`q`は一覧へ戻り、一覧の`q`でコマンドを終了します。

### 対話一覧の主手順（1コマンド）

Apple Silicon Macの日本語対応端末で、リポジトリ直下から次の1コマンドで起動します。build済みバイナリ・実設定/API keyは不要です。最初は80列×12行程度にして、25件が画面に収まらない高さで確認してください。

```sh
BL_TERMINAL_MOCK_ISSUES=55 BL_TERMINAL_MOCK_BODY=long mise exec -- go run ./internal/cli/testdata/terminal_mock.go issue list -p 42 --limit 25
```

| 操作（順番に実施） | 期待結果 |
| --- | --- |
| 起動直後にp | offset 0、APP-1選択のまま。「前ページ」はない |
| jを23回、kを1回、jを1回 | APP-24が選択され、一覧がスクロール。日本語/emoji・状態・担当者と行幅が崩れない |
| l | APP-24の長い詳細。hで一覧に戻れる案内とRの更新案内がある |
| j/k、Space/b、G/g、End/Home | 本文を上下・ページ・先頭末尾へ移動でき、一覧は変化しない |
| 詳細でR | APP-24を再取得して詳細先頭へ戻る。固定mockは同じ内容だが、表示が残り操作できる。API再取得の回数/新内容は自動テストで別途確認 |
| h | offset 0のAPP-24選択・一覧スクロールへ戻る |
| Enter、詳細でq | 同じAPP-24が開き、qでも同じ一覧位置へ戻る。CLIは終了しない |
| n、jを9回 | offset 25、新規ページ先頭APP-26からAPP-35へ。前/次ページ項目がある |
| p、n | pでoffset 0のAPP-24位置へ、nでoffset 25のAPP-35位置へ。再取得せず各ページの位置を復元 |
| G、k、Enter | 「終了」の1つ前の「次ページ」を実行しoffset 50へ。APP-51〜55が表示され、次ページ項目はない |
| n、p、Enter | nは最終ページで無操作。pでoffset 25の「次ページ」選択へ戻り、Enterで保持済みoffset 50へ戻る |
| 一覧でR | offset 0・APP-1・一覧先頭に戻る。前ページ履歴は消える |
| n、p | nで新たにoffset 25を取得しAPP-26先頭へ。pでoffset 0へ戻る（古いAPP-35の位置は復元しない） |
| APP-24等へ移動しl、80→36→20→80列へresize、h | 詳細は再flowし、戻った一覧も選択が維持され表示可能範囲へ調整される |
| 10列×3行へ縮小後、元サイズへ戻す | 安全なサイズ案内となり、復元後も選択・操作が保たれる |
| 一覧でG、l | 「終了」選択のまま。lで終了せず、detail APIも呼ばない |
| q | CLI終了0、元の画面・端末入力が復元される |


同じ起動コマンドで再起動しCtrl-Cも確認してください。終了130と元の画面・端末入力への復元を確認します（正常終了と中断は別プロセスで確認）。人間が各行の期待結果を確認し、受入記録に残す必要があります。自動PTY成功を見やすさ・操作感の受入とは扱いません。

### 一覧の補助検証（主手順とは別）

以下は上の補助build済み `/tmp/bl-terminal-mock` を使います。mock専用環境変数であり本番CLIの設定/flagではありません。

```sh
BL_TERMINAL_MOCK_ISSUES=55 BL_TERMINAL_MOCK_RULE=backlog /tmp/bl-terminal-mock issue mine -p 42 --limit 25
BL_TERMINAL_MOCK_ISSUES=55 BL_TERMINAL_MOCK_FAIL_DETAIL=APP-24 /tmp/bl-terminal-mock issue list -p 42 --limit 25
BL_TERMINAL_MOCK_ISSUES=55 BL_TERMINAL_MOCK_RULE=unknown /tmp/bl-terminal-mock issue list -p 42 --limit 25
BL_TERMINAL_MOCK_ISSUES=55 BL_TERMINAL_MOCK_FAIL_NEXT=1 /tmp/bl-terminal-mock issue list -p 42 --limit 25
```

API/記法/次ページ失敗時の安全な通知と保持、単独viewのh無効・R、短い詳細、mine、非0offset、空ページ、非TTY/JSON/no-inputはModel/CLI/HTTPテストと補助PTYでも確認します。一覧/詳細Rの新内容への交換・更新失敗保持は応答切替可能なModel/HTTPテストで確認します。必要ならmock専用 `BL_TERMINAL_MOCK_TRACE=1` を併用してAPI path/offsetをstderrへ記録できます（key/応答dumpなし）。通常移動/詳細復帰ではcountなし、一覧Rごとにcount1回→list(0)、詳細Rごとに同じ課題GET→project取得です。主手順1回で全異常系を確認できるわけではありません。

### project候補の実端末手順

Apple Silicon Macの日本語端末で、(1) 上下で20件を超えて移動しEnter、`ID / NAME` 固定出力に選択projectの番号が現れる、(2) 長い日本語/👩‍💻が崩れず幅内で省略される、(3) resizeと10列×3行程度の極小サイズでpanicせずサイズ案内が出る、(4) EscとCtrl-Cそれぞれで元の端末へ戻り `echo $?` が130、を確認してください。実行し直すと必ず未指定状態から選び直します。`q`は取消キーではありません。

非対話gateと直接指定も同じmockで確認できます。

```sh
/tmp/bl-terminal-mock issue create -p 42 --issue-type-id 7 --priority-id 3 -t 修正 --json # exit 0、作成JSON
/tmp/bl-terminal-mock issue create -p APP --issue-type 'タスク👩‍💻 (project 42)' --priority 中 -t 修正 -a self # 作成成立・key・URL
/tmp/bl-terminal-mock issue create --json             # exit 2、creation not_created（TTYでも待機なし）
/tmp/bl-terminal-mock issue-type list --json          # exit 2、入力不足JSON（待機なし）
/tmp/bl-terminal-mock issue-type list --no-input      # exit 2、入力不足（待機なし）
/tmp/bl-terminal-mock issue-type list </dev/null      # exit 2、stdin非TTY
/tmp/bl-terminal-mock issue-type list | cat          # stdout非TTY、bl側exit 2（shellのpipe exitに注意）
/tmp/bl-terminal-mock issue-type list -p APP          # key→ID 42、固定出力
/tmp/bl-terminal-mock issue-type list -p 42 --json    # 大整数を保持したitems JSON
/tmp/bl-terminal-mock issue list -p 42 --limit 2 --no-input # 固定2件、Next offset: 2
/tmp/bl-terminal-mock issue list -p APP --offset 2 --json # key解決後、最終1件とpagination
/tmp/bl-terminal-mock issue list -p 42 --offset 3 --json  # total=3でも空items、終了
/tmp/bl-terminal-mock issue list -p APP -s 未対応 -s 処理中 -m v1.0 -a self -a 山田 --json
/tmp/bl-terminal-mock issue list -p 42 --assignee-name self --json # 実名self（ID 11）、本人はID 10
/tmp/bl-terminal-mock issue list -p 42 -s 123 -m a,b -a 123 --json # 数字/カンマも名前
/tmp/bl-terminal-mock issue list -p 42 --status all --json
/tmp/bl-terminal-mock issue mine -p APP --limit 2 --offset 1 --json
/tmp/bl-terminal-mock issue mine -p all --json
/tmp/bl-terminal-mock issue list "ログイン" -p all --json
/tmp/bl-terminal-mock issue list -p 42 -s 重複 --json # exit 2、複数一致
/tmp/bl-terminal-mock issue list -p 42 -m missing --json # exit 2、候補なし
/tmp/bl-terminal-mock issue mine -p 42 -a self --json # exit 2、担当者flag拒否
/tmp/bl-terminal-mock issue list "ログイン" -p all -s all --json # exit 2、横断の状態条件拒否
```

project候補スクロールの再受入では、↓を1回ずつ押し、表示範囲内では候補を消さずカーソルだけが移動し、下端に達したときだけ表示範囲が1行ずつ動くことを確認してください。末尾でも候補行数が減って空白にならず、↑で先頭まで戻れ、resize後も選択が保持されることを確認します。27行以上の端末では25候補すべてが上下移動中も表示され続けることを確認してください。

任意のPython 3標準PTYによる自動smokeも用意しています（Goの正式checkにはPython不要）。両TTY/片側非TTY/両非TTY、json/no-input gate、選択と取消130、resizeに加え、両記法のdetailのq待機・移動キー/幅変更の再描画・極小サイズ・Ctrl-C/context cancel 130・非TTY ANSIなし・JSON原本を検証します。一覧はlist/mineの20件超の移動・l/Enter詳細再選択とh/q復帰・p/nの訪問ページ復元・offset 0/25/50・一覧Rでcount追加1回と先頭/履歴リセット・詳細RのAPI再取得・最終page停止・detail/描画/next失敗時の継続・resizeを検証します。project選択/一覧/詳細へのOS SIGINT・SIGTERMをそれぞれ3回反復し、終了130、端末モードと代替画面の復元も確認します。

```sh
python3 internal/cli/testdata/terminal_check.py /tmp/bl-terminal-mock
```

**PTY/Modelの成功は、人間による実端末の見やすさ・操作感の受入完了を意味しません。** 実スペース接続も未検証です。
