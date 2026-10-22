# Backlog CLI 初版 実装計画

## 目的

人とAgentが同じ機能を使える、Taskflowに依存しないGo製CLI `bl`を作る。初版は課題の検索・閲覧・作成と、それに必要な一覧取得を対象とし、配布対象はApple Silicon Mac（darwin/arm64）とする。

入力設計は `docs/designs/backlog-cli.md`。特に「初期コマンドとAPI」「profileと設定」「検索とページ送り」「人向けの一覧・詳細UI」「本文記法と描画の境界」「通信・失敗時の方針」を要件の根拠とする。本Planは同文書の未決事項を具体化する。実装セッションは本Planと入力設計を読み、下記の明示した変更点については本Planを優先する。

## 要件・決定事項

### 1. 公開コマンドと入力

公開コマンドは次の7つ。help以外のコマンド階層省略alias、設定コマンドは追加しない。

| コマンド | 入力と動作 |
| --- | --- |
| `bl project list` | 参加プロジェクトの一覧。ローカルprojectで絞り込まない |
| `bl issue-type list` | 対象プロジェクトの種別一覧 |
| `bl priority list` | 優先度一覧 |
| `bl issue list [keyword]` | 任意の1位置引数keywordと検索条件で一覧 |
| `bl issue mine [keyword]` | 担当者self固定。それ以外はissue listと共通 |
| `bl issue view <issueIdOrKey>` | 1課題の取得と詳細表示 |
| `bl issue create` | プロジェクト・種別・優先度・タイトル・本文・担当者から作成 |

共通オプションは `--profile`、`--json`、`--no-input`。profile省略時はdefault。短縮名は入力設計の採用済みのものだけを使い、他の短縮名は初版で増やさない。

| 適用先 | オプション契約 |
| --- | --- |
| プロジェクトを使うコマンド | `--project/-p`：プロジェクトキーまたは正整数ID。単一値。特殊値allはissue list/mineだけ |
| create | `--issue-type` または `--issue-type-id`、`--priority` または `--priority-id` |
| create | `--title/-t`、`--body/-b` または `--body-file` |
| create/list | `--assignee/-a`、`--assignee-name`、`--assignee-id`。createは1人、listは反復OR |
| create | `--no-assignee`：設定の担当者既定値を解除。担当者系の明示指定と排他 |
| list/mine | `--status/-s` または `--status-id`、`--milestone/-m` または `--milestone-id`。いずれも反復OR |
| list/mine | `--limit`：既定20、1〜100。`--offset`：既定0、0以上 |

- 名前は完全一致。候補なし・複数一致ではエラーにし、推測しない。数字の名前も通常の名前オプションでは名前として扱う。直接IDは正整数とし、int64の範囲で検証する。
- `--assignee self`は選択profileのAPI keyの認証ユーザー。実名selfは `--assignee-name self`。listでは `-a self -a 山田` のように併用できる。名前系（assigneeとassignee-name）の併用はlistのみ許可し、同一IDは重複除去する。
- 同一項目の名前系とID系の混在は拒否する。createの担当者3形式は相互排他。反復する検索値はカンマ分割しない。単一値オプションを長短名混在も含め重複指定したらエラーとする。boolean共通flagの重複は標準parserの同値指定動作でよく、値オプションの重複検証と混同しない。
- mineでは担当者系オプションをすべて拒否する。別担当者・複数担当者の検索にはlistを使う。
- status allは単独指定で設定の状態条件を解除する。通常値・status-idと混在不可。実名allの状態はIDで指定する。
- project allは全参加プロジェクトを対象とし、空白のみではないkeywordまたはselfが必須。検索のローカル既定値を一切適用しない。明示status（allを含む）・milestone・self以外の担当者・担当者ID指定を拒否する。limit/offsetは利用できる。
- 通常のproject未指定は横断検索を意味しない。issue list/issue-type list/createでは、対話可能ならprojects APIの候補から選び、非対話なら入力不足エラーにする。
- createの必須項目はproject・issueType・priority・空白のみでないtitle。本文は任意。明示値、ローカル設定の順に適用し、種別・優先度をCLI独自に既定選択しない。
- `--body` と `--body-file`は、空文字指定も含め相互排他。ファイルはUTF-8本文として扱い、不正UTF-8は送信前に拒否する。`--body-file -`だけがstdinを読む。本文省略・空本文を許可し、本文データをトリミングしない。

### 2. profile・設定とeditor

- 共通設定は `$XDG_CONFIG_HOME/backlog/config.json`。XDG_CONFIG_HOMEが未設定・空・相対パスなら `~/.config/backlog/config.json`。profileは `profiles` 配下の `spaceUrl` と `apiKeyEnv` を使う。
- API key本体は対応環境変数だけから読む。profile欠落・環境変数未設定または空ならエラー。profile/keyのfallbackはしない。
- ローカル設定はcwd直下の `.backlog/settings.json`だけ。親探索・profile変更・API候補の永続キャッシュを追加しない。
- ローカル構造は `project`、`issue.create.{issueType,priority,assignee}`、`issue.list.status`を維持。projectは文字列のキー/ID、名前の設定は文字列、statusは文字列配列。未設定または空status配列なら状態絞込みなし。
- 明示値が対応する既定値を上書きする。create既定値はlistには使わない。
- 設定の不在と、読み取り失敗・JSON破損・型不正・未知の設定キー・不正値を区別する。不正な設定を黙って無視しない。API key・profileを設定生成や自動補修で補わない。helpは設定やAPI接続なしで利用できる。
- 記入例を提供し手編集する方式を維持する。

**合意した入力設計への追加：** 共通設定のトップレベルに任意の `editor` を加える。`"editor": ["code", "--wait"]` のような実行ファイルと引数の文字列配列とし、profileやローカル設定には持たせない。未設定なら `["vim"]`。空配列・空の実行ファイル・NUL等の実行不能値は設定エラー。VISUAL/EDITORは参照しない。

理由は、利用者がCLIの設定ファイルでeditorを管理でき、環境変数設定を増やさずに済むこと。配列を `exec.CommandContext` 相当へ渡しshellを介さず、一時ファイルのパスを最後の独立引数として追加する。本文や外部データを実行コマンドへ連結しない。待機が必要なeditorには利用者が `--wait` 等を指定する。

editorは、必須入力不足で対話作成へ入ったときに本文が未指定なら起動する。明示本文は開き直さない。本文省略だけ、あるいはeditor設定の存在だけでは起動しない。必須入力がそろったcreateは本文なしでも即実行する。追加のeditor起動flagは作らない。

本文一時ファイルは安全な一時ディレクトリに0600で作り、通常終了・失敗・処理可能な中断で削除する。editor不在・起動失敗・非0終了・読み取り失敗ではPOSTしない。設定またはbody/body-fileの利用を案内する。強制終了時の一時ファイル消去までは保証しない。

**設計書更新の範囲とタイミング：** ユーザー確認済み。最初の実装Work Itemで `docs/designs/backlog-cli.md` の共通設定例とeditor選定規則だけを更新し、README・設定例にも反映する。本Planningでは元設計書を書き換えない。更新前の「editor選定規則は未決」の記述より本節を優先する。他の具体化は本Planを参照し、設計書の全面改訂はしない。

### 3. 対話の開始条件とUI

対話可能なのはstdin/stdoutがともにTTYで、json/no-inputがともに無効の場合だけ。

- createは必須入力不足のときだけ対話へ入る。projects/issueTypes/priorities/usersの候補と既定値を反映し、担当者なしも選択可能。タイトル・必要ならeditor本文入力後、選択profile/space/project・作成内容を確認してからPOSTする。確認キャンセルはPOSTしない。入力がそろったcreateには確認待ちを追加しない。
- 対話できない入力不足はエラー。stdin本文から値を暗黙補完しない。
- 課題一覧はHuhのスクロールSelectを親Bubble Tea Modelへ組み込む。課題キー・件名・状態・担当者を行表示し、「次ページ」「終了」も選択項目にする。次ページは続きがあるときだけ有効。
- 上下で選択、Enterで詳細/操作、nで即次ページ、qで即終了。初版は一覧内の文字フィルターを追加せず、HuhのFilter keyを無効化する。create入力等ではn/qを横取りしない。
- 一覧から詳細を開く際はissues detail APIを呼ぶ。詳細をqで閉じたら、元のページ・選択位置・一覧スクロールへ戻り、一覧を再取得しない。ページ内のSelectを保持し、値は公開 `Hovered()` 等で確認する。Form完了状態へ遷移させて一覧を無効化しない。
- detail取得/描画失敗時は安全なエラーを表示し、一覧状態を失わず戻れるようにする。後のページ取得失敗を「空ページ」「終了」に置き換えない。
- 詳細表示はissue viewと一覧遷移で共通。Glamourで整形しBubbles viewportでスクロール。上下/j/k、PgUp/PgDn、b/Space、g/Home、G/End、qを使う。短い本文でもqを待つ。view直実行は閉じたら終了、一覧経由は一覧へ戻る。
- WindowSizeMsgで選択領域を更新し、長い一覧行はセル幅で切り詰め、詳細は幅変更時に再描画してスクロール位置を範囲内へ調整する。極小端末でも負寸法・panicを起こさず、安全な最小寸法とサイズ案内を使う。
- json/no-input/非TTYではviewer・選択UIを起動しない。人向け一覧は1ページの固定出力、詳細は整形済み本文を1回出して終了する。非TTYの出力にはANSIを混ぜない。project/issue-type/priority一覧も固定の人向け表示でよく、未要求の選択機能を追加しない。

### 4. 検索・ページ送り

- 12 APIの範囲と機能単位の分割を維持する。名前解決にはproject detail/issueTypes/statuses/users/versions/priorities/myselfを必要時に利用する。直接ID指定の値は不要な候補APIを呼ばず渡す。project detailは記法確認・project解決に必要な場合に取得する。
- 検索filterを1つの値として確定し、countとlistへ同じprojectId[]/statusId[]/assigneeId[]/milestoneId[]/keywordを渡す。sort=updated、order=descをlistへ明示し、ソート指定は公開しない。
- countは検索開始時に1回取得する。失敗なら検索全体をエラーにし、0件で代替しない。countへlimit/offset/sort/orderは渡さない。
- listのcount=limit、offset=指定値。JSONとUIで同じ終了規則を使う。
- 実取得count=0ならtotalにかかわらず終了。非空なら `offset + count < total` のときだけ続きがある。計算は整数overflowを避ける。次offsetは現在offset+実取得count。
- totalをページ送りごとに取り直さない。全件自動取得を実装しない。データ変動による重複・取りこぼし、totalとの差は保証対象外とし、READMEに説明する。

### 5. JSON・エラー・作成結果

成功結果はstdoutに1つのJSON文書を出す。改行終端とし、UI/案内/ANSIを混ぜない。

| 結果 | JSON契約 |
| --- | --- |
| issue view/create | 当該GET/POSTのAPI応答オブジェクトの項目・構造・未知フィールドを保持し、CLIのurlを追加 |
| issue list/mine | `{"items":[<API課題+url>],"pagination":{"offset":0,"limit":20,"count":0,"total":0}}` |
| project/issue-type/priority list | `{"items":[<API応答の各オブジェクト>]}`。paginationは付けない |

- 空一覧は `items: []`。本文や外部文字は表示用変換を通さず、JSON stringの値として保持する。制御文字はJSON escapingで表現する。
- listのcountは実取得件数、totalは開始時の全体件数。hasMoreは付けない。
- JSONモードのエラーはstderrへ `{"error":{"code":"...","message":"...","httpStatus":null,"backlogErrors":[],"creation":null}}`。codeは `input_error`、`config_error`、`api_error`、`transport_error`、`response_error`、`output_error`、`cancelled` の固定集合。Backlog errorsはAPIのerrors配列を構造保持して入れ、HTTP応答がなければhttpStatus=null。creationはcreate以外ならnull、createなら下記object。
- `creation: {"state":"not_created|unknown|created","issueKey":null,"url":null}`。キー/URLを取得できた場合はstring。不明情報を捏造しない。JSON中でもsecretは必ずredactする。
- 入力/parserエラーもjsonモードでは上記形式。未知flagや排他違反を含むCLI境界テストでstdoutが空であることを確認する。
- 終了コードは成功0、入力/設定2、API/通信/応答/一般出力エラー1、作成成功確認後のdecode/出力等の失敗3、ユーザー中断130。中断時は130を優先しつつcreation.stateを別に保つ。
- 人向けエラーは同じ状態に基づく簡潔な表示とし、外部文字無害化・secret除去を通す。
- 成功JSONはメモリ上で構築/encodeしてから書く。取得失敗を部分成功JSONへしない。ただしstdoutのI/O障害では書込み途中の出力自体を撤回できず、stderrと終了コードで通知する。

POST成功結果の追加GETは不要。201応答の課題データを結果に用い、人向け作成結果は作成成立・課題キー・URLを簡潔に示す。作成後に自動で詳細viewerを開かない。

作成状態の判定は次の原則とする。
1. 検証失敗・キャンセル等でPOST未実行が確実ならnot_created。Backlogから明示的な要求拒否を受けた場合も拒否として示す。
2. 201を受信したらcreated。本文decodeや出力失敗でunknown/not_createdへ戻さない。安全に検証できたレスポンスの課題キー/同一originのLocationから判明する情報があれば通知する。得られない場合も「作成済み、識別情報の取得失敗、再作成しない」を示す。
3. 送信された可能性があり成功/拒否を確認できない場合（切断、timeout、送信後中断、5xx等）はunknown。未作成と断定しない。
4. 非201の予期しない成功status等はresponse_errorとして確実性を保守的に扱い、自動再送しない。

### 6. 通信と安全境界

- API keyは公式推奨の `Backlog-API-Key` headerで送る。queryへ載せない。URL/headers/raw responseのdebug dumpを追加しない。エラーからkeyおよびその表現をredactする。
- spaceUrlはHTTPSのoriginとして検証する。userinfo/query/fragment/ルート以外のpathは拒否、末尾slashは正規化。特定の公開Backlogドメインだけに制限せず、設定された接続先を使う。TLS検証を無効化しない。
- redirectは全拒否。API keyの別hostへの送信とPOSTのredirect再送を防ぐ。CLIからretry、Idempotency-Key、X-Idempotency-Keyを追加しない。
- 1 API requestのtimeoutは本文readまで含め30秒。コマンド中断をcontextへ伝える。認証・権限・rate limit・通信・JSON破損を空結果に置き換えない。
- createは `application/x-www-form-urlencoded`、GETの配列条件は `[]` を含む正式な名前でURL encodeする。
- 動的なpath要素はエスケープし、任意URLをオプションから呼び出せる形にしない。課題URLは設定originと検証したissueKeyから構築する。Locationを無条件に信用しない。
- POSTは成否不明でも再送しない。429/5xxも含め自動retryはしない。利用者の手動再実行による重複リスクを明示する。
- 端末表示用の無害化でAPIへ送信する値やJSON原本を書き換えない。

### 7. 本文変換・無害化

記法はprojectの `textFormattingRule`（backlog/markdown）で分岐する。未知値は推測せず表示エラー。JSON取得に記法取得APIは不要。外部の画像・Wiki・repository・添付ファイルは取得しない。

構成は「原本文→表示用安全コピー/記法変換→Glamour→表示contentの安全確認→viewportまたは固定出力」。全体画面の端末制御は信頼済みUIライブラリに任せ、外部データへ同じ権限を与えない。

- MarkdownはGoldmarkの通常ASTを利用してコードblock/inline code/escapeを識別する。Backlog記法は行/inline単位の最小scannerで `{code}`/`{code:lang}`、U+005Cのescape等を保護して変換する。Glamour parserの改造、独自端末renderer、汎用言語エンジンは作らない。
- 入力設計の変換表をすべて対象とする。基本見出し/list/強調/引用/code/checklist/通常表、色の本文保持、details展開、issue link、Wiki/Git/SVN/添付参照、静的目次、画像の代替表示をfixtureで検証する。
- 複雑表は結合を再現せず、各セル本文・見出し・captionを落とさず通常表へ簡略化する。表delimiterのescapeやコード内の区切りを区別する。
- タグはcode/escape以外では露出させず、内容を保持する。未知macroは `未対応の記法（名前: foo、内容: …）` 等の説明表現へ変換する。未知HTMLタグはタグを除き本文を残す。入れ子/壊れた記法でも認識できない内容を黙って捨てない。
- URLが確かな場合だけリンクを作る。未知Wiki等のURLは推測しない。画像はfetchも表示もしない。
- 外部表示文字列全般（候補、タイトル、本文、ユーザー名、APIエラー）を、ANSI sequenceの除去とrune単位のC0/C1/DEL除去で無害化する。LF/TABは保持し、CRLFはLFへ正規化、その他のCR/BEL/BS等は除く。日本語UTF-8の継続byteを削除しない。Cfの一括削除でemojiのZWJ等を壊さない。
- `ansi.Strip`単独を安全策にしない。HTML entityから制御文字が復活する箇所では危険なentityを描画前に安全な文字表現へ置き換える。通常文・リンク先・inline code・fenced codeを別々に検証する。
- Glamourがinline codeのentityを復号することを考慮し、その箇所だけampersandの追加escape等で原文の文字を保持する。fenced codeへ同じ処理を一括適用しない。
- 描画後contentではSGR装飾のみ許可し、OSC（clipboard/link/title等）・端末操作CSI・未終端sequenceと残存C0/C1を遮断する。Glamour生成のSGRと、入力から除去すべき外部ANSIを処理段階で区別する。非TTYではSGRも除く。URLは可視テキストとして残す。
- 異常系fixtureには数値/入れ子entity、ESC/CSI/OSC52/DCS/8-bit・UTF-8 C1、壊れたsequence、code/escape/URL/日本語/emojiを含める。長い/未終端の記法でpanicや無制限ループを起こさないことも確認する。

## 制約・対象外

- API対象は入力設計記載の12のみ。SDK、Taskflow Skill変更・連携全体、課題更新、コメント、親子設定、作成時の期限/カテゴリー/マイルストーン/カスタム属性は追加しない。
- custom必須のプロジェクトは作成できない可能性がある。API拒否を正しく表示し、対応機能を自動追加しない。
- Intel Mac/Linux/Windowsの利用保証、外部pager、plugin/alias機構、取得候補の永続キャッシュ、全件自動取得、snapshot整合性は対象外。
- API keyの自動登録、設定用コマンド、現在profileの永続状態、自動再送・成否再解決手順を増やさない。
- Gitリポジトリ/remoteの新設・push、GitHub Release自動公開、署名/notarization、Homebrew formula作成は今回の範囲外。CI設定とローカル生成入口は作るが、公開を自動実行しない。
- 本Planningでproduction codeを書かない。Work Items公開後は停止し、Implementationは新しいセッションで実行する。

## 実装方針

### 構成と依存

現状は `docs/designs/backlog-cli.md` のみで、Go module、既存コード/テスト/CI、.gitはない。Go 1.26.10はローカルにインストール済みだが、このcwdのmise versionは未設定。既存のmodule/API互換性維持対象はない。

最小構成：
- `cmd/bl/main.go`：入出力・signal・exit codeの接続。
- `internal/cli`：Cobraのcommand tree、入力/設定適用、名前解決、実行の流れ。
- `internal/config`：共通/ローカル設定読込と検証。
- `internal/backlog`：共通HTTP clientと機能別APIファイル（projects/issues/users/priorities等）。全APIを1ファイルにしない。
- `internal/output`：JSON、人向け固定表示、secret除去/表示無害化。
- `internal/ui`：一覧/detail/createのUI、Bubble Tea Model。
- `internal/markup`：記法変換とGlamour描画の接続。
- `testdata` は各packageのfixtureに必要な箇所だけ。README、設定例、Makefile、CI設定、go.mod/go.sumを整備。

module名は初版のローカル内部用 `backlog-cli` とし、公開Go libraryは提供しない。層ごとのinterface一式、DI framework、repository層、generic API engineは作らない。HTTP client・I/O等のテストに必要な既存標準型/小さい境界だけを渡す。API原本はjson.RawMessage等で未知フィールドを落とさず扱う。

固定する実行/開発版：
- Go **1.26.10**。
- Huh **v0.8.0**、Bubble Tea **v1.3.10**、Bubbles **v0.21.1**、Glamour **v0.9.1**。
- Lip Gloss **v1.1.0**、x/ansi **v0.11.5**、x/term **v0.2.2**、Goldmark **v1.7.8**、Cobra **v1.9.1**。
- `golang.org/x/net v0.61.0`、その要求する `x/text v0.43.0` などは既知脆弱性を避けるため固定し、実際の解決結果をgo.mod/go.sumへ記録する。UIのversionだけ古いtransitive dependencyへ戻さない。
- golangci-lint **v2.9.0**。Go 1.26.10でビルドしたtoolを使用。内包formatterのgofumpt **v0.9.2**、goimports（x/tools **v0.42.0**）を使用し、別のformatter版を重ねてインストールしない。
- govulncheck **v1.9.0**を開発検証入口として固定。実行時依存にしない。

golangci-lint設定は `version: "2"`、`linters.default: standard`、`formatters.enable: [gofumpt, goimports]`。独自lint追加・無効化なし。editorはこの整形入口と同じ版を使えるようREADMEへ記載し、特定editor向け設定ファイルは増やさない。

### ビルドとCI

Makefileに以下の入口を用意する。Go version指定はプロジェクトのmise設定とCIで合わせる。toolの導入先は開発用に限定し、配布物へ同梱しない。

- `make fmt`：golangci-lint fmt ./...
- `make check`：golangci-lint fmt --diff ./...、golangci-lint run ./...、go test ./...、go vet ./...。
- `make test-race`：go test -race ./...。
- `make security`：govulncheck -test ./...。
- `make build`：CGO_ENABLED=0 GOOS=darwin GOARCH=arm64 go build -trimpath -o dist/bl ./cmd/bl。
- `make dist`：同じビルドから `backlog-cli_darwin_arm64.tar.gz` とSHA-256 checksumを生成。archiveにはbl、README、設定例を含める。API keyや利用者の設定を含めない。

GitHub Actionsは固定Go/tool版でtest/lint/race/securityとcross build/archive生成を行う。Linux CIで標準テストを動かしてもLinuxの動作保証とはしない。使用する公式Actionsは実装時に公式参照からcommit SHAを固定する。Release公開は行わない。GitHub repositoryが未準備で実CIが走らない場合はworkflow配置・ローカル検証までと実CI未実行を明記し、実行済みと主張しない。

### 調査・Probeで確認した事実

- 一時moduleで上記Go/UIセット・Cobra/x/netを解決し、darwin/arm64上でcompileおよびGo testのassertが成功した。
- Huh Selectを保持してdetail相当の処理後にUpdateを再開でき、Hoveredから値を取得できた。G/endの直後にGetValueだけを読むと旧値を返すケースを観測したため、選択取得方法を明示する。
- Glamourが `&#7;` をBELへ復号し、inline code内のentityも復号することを観測。inline codeのampersand追加escapeで複数の原文例が保持された。fenced codeは処理が異なる。
- 上記Probeに対するgovulncheck v1.9.0 -testで検出なし。ただし本番の全呼出し範囲や今後のadvisoryを保証する結果ではない。
- 依存のbuild/Model単体検証と、実端末での操作感・全本文変換/無害化の完成検証を区別する。後者はImplementationの受入条件とする。
- 実スペース接続は行っていない。

## 完了条件

1. 7コマンドと入力/設定優先順位・排他・名前/ID・self/all・複数担当者の契約が実装され、エラーも含めCLI経由で検証できる。
2. 同一検索filterで件数とページを取得し、空ページ終了、total保持、sort、非対話1ページ出力、mineとの同等性がテストされている。
3. 対話作成は候補/既定値/担当者なし/editor/確認を利用でき、非対話・jsonは入力待ちしない。POST未実行/成否不明/作成成立後失敗を区別し、自動再送しない。
4. 一覧→詳細→元の位置への復帰、日本語・長い行・resize・終了キーがApple Silicon Macの端末で確認されている。未実施ならUI受入未完了と報告する。
5. 入力設計の表示変換表とcode/escape/未知記法/複雑表/制御文字/entityのfixtureがあり、人向け出力の安全性とJSON原本保持が両立している。
6. 設定例・README・editorの基本設計追記、開発入口・CI・配布物生成が整備され、正式checksが通る。実CI未実行等は明確に残す。
7. API keyが設定例・出力・ログ・archiveへ漏れず、別origin redirect、切断後POST再送を禁止するテストが通る。
8. 実スペース確認は任意。未実施なら「実スペース未検証」と報告し、必須custom属性等による対象projectでの利用可能性を保証しない。

## 動作確認方法

### TDDと自動テスト

TDDを採用し、Implementationでは既存tdd Skillを読む。ユーザー確認済みのseamsは以下。1つの振る舞いのfailing test→最小実装を順次行い、private methodの呼出し回数や内部構造への依存を避ける。テストframeworkはGo標準testingとnet/http/httptestのみ。

| 境界 | 確認するもの / 含めないもの |
| --- | --- |
| CLI実行関数/実行バイナリのargs・streams・exit | help、7コマンド、長短flag同等、未知flag/不足/排他/反復、jsonの純度、TTY条件別のUI/editor非起動、mine共通動作。実端末の見た目は別 |
| 設定の公開読込 | t.TempDir/t.SetenvでXDG絶対/相対、default/明示profile、missing/invalid/permission、cwdのみ、優先順位、editor配列。実利用者の設定を書き換えない |
| HTTP clientの公開操作 | httptest serverでpath/query/header/form、配列・count/list filter一致、status/errors/不正JSON/timeout/cancel、redirect拒否・secret除去。実APIの権限/必須属性は別 |
| 本文変換/描画と表示無害化の公開入口 | 公式例から作る期待値、両記法、code/escape/entity/タグ/表/未知macro、危険ANSIの不在、JSON原文字列と日本語保持。端末固有幅の操作感は別 |
| UI ModelのUpdate/View | 合成KeyMsg/WindowSizeMsgでページ・選択保持、n/q、空ページ終了、resize、エラー復帰。実端末の描画・editor切替は別 |

必須異常系：
- POSTの記録後に接続切断、201後の壊れたJSON、送信後中断、5xxをmockで再現。creationと終了コードが一致し、POSTは1回だけ。
- 必須値指定・本文なし・editor設定ありでもeditorを起動しない。
- editorの異常終了・本文読み取り失敗・作成確認キャンセルでPOSTしない。一時ファイルは削除。
- count失敗を0件扱いしない。total>0でも空pageなら停止。非空最後page、非0offset、データ変動相当の境界、整数上限を確認。
- 入力がそろったnonTTY/json/no-inputをタイムアウト付きで確認し、選択UI・viewer・stdin暗黙読込が発生しない。
- 入力・API応答にsecretと端末制御文字を含め、stderr/候補/本文の安全性とJSON値の保持を確認。
- `go test -race` と長い/壊れた記法の境界テストを実施する。端末GoldenのANSI全体一致だけで安全性を代用しない。

### 実端末の受入

darwin/arm64の日本語が表示できるターミナルで、mock応答を用いる自動/手動操作手順を用意する。実APIなしでも一覧20件を超える移動・next・q、長い日本語/emoji、短い/長い詳細、上下/ページ/先頭末尾、一覧復帰の位置、resize/極小端末、editor起動/キャンセルを確認する。PTYテストを使う場合も実端末の見やすさを確認したことにしない。

### 任意の実スペース受入

利用者が選択profile/space/projectと参照・作成内容を確認して明示的に許可した場合だけ行う。API keyを会話やfixtureへ貼らない。候補・自分の担当検索・詳細・本文記法を確認し、作成は同意した内容1件だけ。結果key/URLで作成成立を確認し、自動削除・自動再送はしない。未実施/作成拒否/必須custom属性の存在はそのまま報告し、機能を追加しない。

## 主要な一次参照

- 入力設計：`docs/designs/backlog-cli.md`（上記の根拠節と公式APIリンク一覧）。
- [API認証](https://developer.nulab.com/docs/backlog/auth/)、[課題追加](https://developer.nulab.com/docs/backlog/api/2/add-issue/)、[課題一覧](https://developer.nulab.com/docs/backlog/api/2/get-issue-list/)、[課題数](https://developer.nulab.com/docs/backlog/api/2/count-issue/)、[プロジェクト](https://developer.nulab.com/docs/backlog/api/2/get-project/)。
- [Huh v0.8.0 Select](https://github.com/charmbracelet/huh/blob/v0.8.0/field_select.go)、[Bubbles v0.21.1 viewport](https://github.com/charmbracelet/bubbles/blob/v0.21.1/viewport/viewport.go)、[Glamour v0.9.1](https://github.com/charmbracelet/glamour/tree/v0.9.1)。
- [Backlog記法](https://support-ja.backlog.com/hc/ja/articles/360035641594)、[Markdown記法](https://help-center.backlog.com/テキスト整形のルール（Markdown記法）-6a1d4d7f3abb3ada78c5658a)、[Backlog escape](https://backlog.com/ja/enterprise-help/userguide/userguide1862/)。
- [golangci-lint v2.9.0](https://github.com/golangci/golangci-lint/releases/tag/v2.9.0)、[固定設定reference](https://github.com/golangci/golangci-lint/blob/v2.9.0/.golangci.reference.yml)、[Go vulnerability database](https://vuln.go.dev/)。
