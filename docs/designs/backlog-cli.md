# Backlog CLI 基本設計

## 目的と範囲

人とAgentの両方が使える、Taskflowに依存しないBacklog CLIを作る。コマンド名は`bl`とし、課題関連から段階的に機能を追加する。

将来、Taskflowの`taskflow-to-ticket` Skillから、承認されたチケットをBacklogへ作成・再取得できることを目指す。ただし、今回そのSkillの変更や連携全体の実装は行わない。チケット分割・承認・依存順の判断はTaskflow側、指定された操作の実行と結果の出力はCLI側の責務とする。

この文書は議論で合意した基本設計と残る論点を引き継ぐものであり、実装計画・実装承認ではない。現時点ではコードの実装、実スペースへの接続、端末上の操作検証は行っていない。

## 採用する技術と責務

- 言語はGo。最初のビルド対象はApple Silicon搭載Macの`darwin/arm64`（`GOOS=darwin`、`GOARCH=arm64`）。配布物の名前には`darwin_arm64`表記を使える。
- Backlog API用SDKは使わず、REST API v2をHTTPSで直接呼ぶ。HTTP・URL・JSON処理にはGo標準ライブラリを使う。
- 対話UIはHuh、Markdownの端末向け描画はGlamour、詳細のスクロール表示はBubblesの`viewport`を採用する。外部pagerは使わず、将来も単体バイナリで配布する方針を重視する。Windows対応時の導入負担を避けるため、初版からCLI内viewerとする。ただしWindowsの動作保証は今回の範囲外。
- APIは課題・プロジェクト・優先度などの機能ごとに分け、共通clientが認証・HTTP通信・タイムアウト・APIエラーを扱う。全APIを一つのファイルへ集約しない。
- コマンドの入力と設定適用、API呼び出し、人向け表示とJSON出力を分ける。将来の機能も既存の共通処理を利用できる構成にするが、plugin機構や全API対応の汎用エンジンは作らない。

## 初期コマンドとAPI

公開コマンドは次の7つとする。

| コマンド | 用途 |
| --- | --- |
| `bl project list` | プロジェクトの一覧 |
| `bl issue-type list` | 対象プロジェクトの種別一覧 |
| `bl priority list` | 優先度一覧 |
| `bl issue list` | 課題の一覧・検索 |
| `bl issue mine` | 自分の担当課題の一覧・検索。`issue list --assignee self`相当のショートカット |
| `bl issue view` | 課題の詳細 |
| `bl issue create` | 課題の作成 |

内部利用を含め、対象は次の12 APIとなる。件数・状態・候補・本人情報の取得だけを理由に、公開コマンドを増やす必要はない。

| API | 用途 |
| --- | --- |
| `GET /api/v2/projects` | プロジェクト一覧 |
| `GET /api/v2/projects/:projectIdOrKey` | プロジェクトキーからIDの解決、本文記法の確認 |
| `GET /api/v2/projects/:projectIdOrKey/issueTypes` | 種別一覧、名前からIDの解決 |
| `GET /api/v2/projects/:projectIdOrKey/statuses` | 状態名からIDの解決 |
| `GET /api/v2/projects/:projectIdOrKey/users` | 担当者候補、名前からIDの解決 |
| `GET /api/v2/projects/:projectIdOrKey/versions` | マイルストーン名からIDの解決 |
| `GET /api/v2/users/myself` | 認証ユーザーの取得、`self`からIDの解決 |
| `GET /api/v2/priorities` | 優先度一覧、名前からIDの解決 |
| `POST /api/v2/issues` | 課題作成 |
| `GET /api/v2/issues/:issueIdOrKey` | 課題詳細、作成結果の確認 |
| `GET /api/v2/issues` | 課題検索・ページ取得 |
| `GET /api/v2/issues/count` | 同じ検索条件の全体件数 |

課題更新・コメント・親子関係の設定、期限・マイルストーン・カテゴリーの指定、カスタム属性は初期の作成範囲に含めない。担当者指定は作成範囲に加える。プロジェクト・種別・優先度は利用者が一覧で確認でき、状態・担当者・マイルストーンを含む必要な名前からIDへの解決は実行時に行う。API取得結果を設定ファイルへ保存する仕組みは設けない。

## profileと設定

### ユーザー共通設定

profile単位で接続先スペースとAPI keyを読む環境変数名を管理する。API key本体は設定ファイルに保存しない。

保存先は`$XDG_CONFIG_HOME/backlog/config.json`。`XDG_CONFIG_HOME`が未設定・空なら`~/.config/backlog/config.json`とする。XDG仕様に従い、相対パスの環境変数は無効として標準の場所を使う。macOSでもこの配置を採用する。

```json
{
  "editor": ["code", "--wait"],
  "profiles": {
    "default": {
      "spaceUrl": "https://example.backlog.com",
      "apiKeyEnv": "BACKLOG_API_KEY"
    },
    "work": {
      "spaceUrl": "https://company.backlog.jp",
      "apiKeyEnv": "BACKLOG_WORK_API_KEY"
    }
  }
}
```

`--profile work`のように選択し、省略時は名前が`default`のprofileを使う。「現在のprofile」を切り替える別の永続状態は持たない。指定profileがない、または対応する環境変数が未設定・空ならエラーとし、別profile・別キーへfallbackしない。

### 作業ディレクトリの設定

カレントディレクトリの`.backlog/settings.json`だけを読む。親ディレクトリは探索せず、ここからprofileも変更しない。共通設定とローカル設定の両方について、初期版は記入例を提供し、利用者が手で編集する。設定用コマンドは作らない。

作成の既定値と検索の既定条件を分離する。将来の課題以外の機能と区別できるよう、課題用設定を`issue`にまとめる。

```json
{
  "project": "MYAPP",
  "issue": {
    "create": {
      "issueType": "タスク",
      "priority": "中",
      "assignee": "self"
    },
    "list": {
      "status": ["未対応", "処理中"]
    }
  }
}
```

この名前空間付きの構造を採用する。値は利用先の例ではなく、説明用のサンプルである。

- `project`はプロジェクト指定を必要とする機能の共通既定値。`bl project list`をそのプロジェクトだけに絞る意味ではない。
- 作成用の種別・優先度・担当者を、一覧の暗黙の絞り込みには使わない。例の`assignee: "self"`は作成時だけの既定値であり、CLI全体の暗黙の既定値ではない。
- 一覧のstatusは複数ならOR条件。未設定・空配列なら状態では絞り込まない。キーワードは検索ごとに入力する。
- 明示オプションをローカル設定より優先する。IDを直接指定する場合も、その項目の設定値より優先する。
- 設定なしと、壊れたJSON・不正な値・読み取り失敗を区別し、不正な設定を黙って無視しない。

## 作成と対話入力

作成項目はプロジェクト・種別・優先度・タイトル・本文・担当者とする。担当者は任意で、指定も設定もなければ担当者を指定せずに作成する。名前指定とID直接指定を用意する案で、名前は完全一致で解決する。存在しない・曖昧な候補から推測して選ばない。

`--assignee self`は、選択したprofileのAPI keyに対応する認証ユーザーを担当者にする。本人のID・名前を設定へ保存せず、`GET /api/v2/users/myself`で解決する。共用・botアカウントのAPI keyを使った場合も、`self`はその認証ユーザーを意味し、CLIを操作している人を推測しない。別の担当者を明示でき、設定の担当者既定値を解除して担当者なしにする手段も用意する。解除・直接ID指定の正確なオプションはPlanningで定める。

本文は次の3経路を受け付ける。`--body`と`--body-file`は同時指定不可とする。

```sh
bl issue create --title "修正" --body "修正内容"
bl issue create --title "修正" --body-file ticket.md
printf '修正内容\n' | bl issue create --title "修正" --body-file -
```

`--body-file -`は標準入力を意味する。指定なしで勝手に標準入力を読み始めない。

人のターミナルで`bl issue create`を実行して必要な入力が不足している場合は、設定の既定値を反映した対話入力を開始する。プロジェクト・種別・優先度はAPIの候補から選び、担当者はプロジェクトの候補から選ぶか未設定にできる。担当者の既定値があれば選択へ反映する。タイトルを入力し、本文はeditorで入力する。対話時は作成内容を確認してから登録する。

入力がそろっている場合は対話せず実行する。非対話環境で入力不足ならエラーとし、`--no-input`でターミナル上でも対話を禁止できる。`--json`実行でも選択UIや入力待ちを発生させない。

editorはユーザー共通設定のトップレベル`editor`に、実行ファイルと引数の文字列配列で指定する（例: `["code", "--wait"]`）。未指定なら`["vim"]`とし、`VISUAL`/`EDITOR`は参照しない。空配列・空の実行ファイル・NUL等の実行不能値は設定エラーとする。shellを介さず、本文一時ファイルのパスを最後の独立引数として渡す。profileやローカル設定にはeditorを持たせない。

本文を省略する場合の扱い、直接ID指定などの正確なオプション仕様は後続で具体化する。

## オプションの表記

長いオプション名と短縮名を併用する。人は短縮名、Agentやスクリプトは意味が明確な長い名前を使えるようにし、どちらも同じ設定適用・検証・動作とする。

| 長い名前 | 短縮名 |
| --- | --- |
| `--project` | `-p` |
| `--assignee` | `-a` |
| `--milestone` | `-m` |
| `--status` | `-s` |
| `--title` | `-t` |
| `--body` | `-b` |

同じオプションへ`-p`と`-P`の両方を割り当てず、すべてのオプションへ無理に短縮名を付けない。その他の短縮名・排他条件・反復指定などの厳密な契約はPlanningで定める。`bl list`などコマンド階層を省略する別名や、利用者が定義するalias機構は今回追加しない。

## 検索とページ送り

初版の検索条件はプロジェクト・キーワード・状態・担当者・マイルストーンとする。キーワードは位置引数、その他は`--project`、`--status`、`--assignee`、`--milestone`で指定する。取得制御には`--limit`、`--offset`を使う。

```sh
bl issue list "ログイン"
bl issue list "ログイン" --status 完了
bl issue list "ログイン" --status all
bl issue list "ログイン" --project all
bl issue list --assignee self --milestone "v1.0"
bl issue mine -p MYAPP -m "v1.0"
bl issue mine -p all
```

`bl issue mine`は担当者を`self`に固定する以外、`issue list`と同じ設定・検索オプション・表示・JSON出力・ページ送りを使う。省略時はローカル設定の`project`と`issue.list.status`を通常どおり適用する。`mine`専用の設定や独立した検索処理は持たない。別の担当者を検索するときは`issue list`を使う。

- `--assignee self`は作成時と同じ認証ユーザーを意味する。プロジェクト内では別の担当者も指定でき、候補の名前は完全一致で解決する。
- マイルストーンは対象プロジェクトの候補から名前を解決し、APIの`milestoneId[]`へ渡す。担当者は`assigneeId[]`へ渡す。
- `--status all`は、設定の状態条件を解除するCLI側の特別値。APIへ`all`を渡さず、状態フィルターを省略する。
- `--project all`は設定のプロジェクトを解除し、全参加プロジェクトを検索する。キーワード、または担当者`self`の指定を必要とする。`issue mine -p all`と`issue list -p all -a self`は同じ検索で、キーワードなしでも利用できる。両方の条件を併用することもできる。担当者もキーワードもない無条件の横断検索はエラーとする。
- 横断検索ではローカル設定の検索条件を適用しない。状態・マイルストーンなどプロジェクトごとの条件や、`self`以外の担当者指定は初版では扱わず、指定されたらエラーとする。件数・offsetなどの取得制御は可能。
- 全プロジェクト横断の1ページは、課題一覧APIへの1リクエストで取得できる。今回は別途全体件数も取得するため、検索は通常2リクエストとなる。名前からIDへの解決が必要な検索では、その通信は別途発生する。
- 一覧は既定20件。`--limit`をAPIの`count`、`--offset`を`offset`へ対応させる。APIの取得上限は1〜100件、offsetは0以上。
- 初期検索と明示的な一覧`R`ごとに、同じ確定済みfilterでcountを1回取得する。通常の前後移動・詳細の出入り・詳細Rでは取り直さない。countにはlimit/offset/sort/orderを渡さない。一覧Rは同じfilter・limitでcount→list(offset=0)と表示データの検証まで成功したら、新totalと先頭ページへ一括交換して旧履歴を捨てる。失敗時は旧total・全履歴・ページ・選択・スクロールを保つ。初期count失敗を0件に置き換えない。
- 実取得件数が0の空ページは、取得済みの全体件数にかかわらず終了として扱い、次ページへ進まない。空でないページは全体件数に基づいて続きの有無を判断し、続きがある場合だけ次ページの操作を有効にする。UIとJSON利用時で同じ終了規則を使う。
- 次offsetは現在offset+実取得件数（整数overflowを回避）。pはlimitを減算せず訪問履歴の境界を使う。初期`--offset`のページが履歴先頭で、それ以前は取得しない。訪問済みページだけ起動中のメモリへ保持し、再訪でlist/countを呼ばない。空/最終ページにも履歴があれば戻れる。一覧Rまで内容は古いままで、メモリは訪問ページ数に応じて増える。count/listは同一snapshotではなく、重複・取りこぼし・totalとの差は保証しない。

並び順は更新日時の降順に固定し、APIへ`sort=updated`・`order=desc`を明示する。初版にはソート項目・昇降順の指定を設けない。将来、`--asc`などによる向きの指定や、並べる項目の選択を追加することは可能とするが、正確なオプション名・契約は未決定であり、先行実装しない。種別・優先度・カテゴリーによる検索と、全件の自動取得も初期範囲に含めない。

## 人向けの一覧・詳細UI

一覧・詳細・選択候補・エラーなどの人向け出力では、件名・本文・ユーザー名など外部データ由来の端末制御文字を表示前に無害化する。CLIの表示処理がこの境界を担い、外部データの制御文字とGlamourなどが生成する表示用ANSIを区別する。通常の改行など表示に必要な構造は保ち、表示用の無害化でAPIへ送信する値やJSONの元文字列を変更しない。具体的な無害化方法とライブラリの対応範囲はPlanningで確認する。

### 一覧

Huhのスクロール可能な選択リストとし、課題キー・件名・状態・担当者を表示する。20件が端末に収まらなくても選択リスト内で移動できる形にする。独立した外部pagerは一覧に挟まない。

| キー | 操作 |
| --- | --- |
| 上下キー / `j/k` | 課題・操作項目を選択。`g`/Home・`G`/Endで先頭/末尾 |
| Enter | 選択した課題の詳細を開く、または操作を実行 |
| `l` | 選択中の課題だけ詳細。操作項目では無操作 |
| `p/n` | 即前/次ページ。存在しない側は無操作 |
| `R`（Shift+r） | 同じ検索の最新countとoffset=0ページを再取得 |
| `q` | 正常終了0。Ctrl-C/context cancelは中断130 |

課題行の後ろに「前ページ」（履歴あり時）、「次ページ」（続きあり時）、「終了」の順で選択項目を置く。更新項目は追加しない。文字フィルターは追加せずHuhのFilter keyを無効にする。create等の入力では一覧ショートカットを横取りしない。Enter/lは古いGetValueではなく現在のHovered項目で判断する。

ページごとのSelect・選択（操作項目を含む）・一覧スクロールを保持し、前後再訪で復元する。新規/空ページは先頭選択・先頭スクロール。resize後も選択を維持し、範囲内へスクロールを調整して選択を表示する。新規ページ取得/検証失敗では旧ページ・履歴・位置を残し、安全に通知する。

### 詳細

課題取得APIを呼び、`bl issue view`と一覧から開く詳細で共通の表示処理を利用する。本文はGlamourで整形し、Bubblesの`viewport`を使うCLI内viewerで先頭からスクロールして読む。外部`less`は使わず、本文の末尾にHuhのメニューを付ける方式にもしない。

Glamourが描画、`viewport`がスクロール、CLIが画面の切り替え・終了操作・端末サイズへの追従・一覧の状態保持を担当する。短い詳細でも自動終了させない。上下/`j/k`、PgUp/PgDn・`b`/Space、`g`/Home・`G`/Endでスクロールする。単独viewは`q`で終了0、`h`は無操作でヘルプにも出さない。一覧経由だけ`h/q`で戻る。Ctrl-C/context cancelは中断130。両方で`R`は同じ課題とproject記法を共通経路で再取得し、応答時点の最新幅で安全に描画できてから内容・rendererを交換して先頭へ戻る。通信・記法・描画失敗では旧詳細と位置を残し安全に通知する。背後の一覧は変えない。

一覧から開いた詳細を閉じたら、元のページ・選択・一覧スクロール位置へ戻る（詳細中のresize後も同様）。詳細は開くたびに取得し、詳細画面の履歴はキャッシュしない。戻るだけでは一覧を再取得しない。CLI全体の終了は一覧へ戻って行う。`bl issue view`から直接開いた場合は、詳細を閉じたらコマンドを終了する。

取得/更新中は重複リクエスト・選択変更・ページ移動・新規詳細表示を抑止する。一覧q、詳細q、一覧経由詳細h、中断は有効。閉鎖・中断・別詳細へ移動後の遅い応答は、所属する詳細インスタンスを確認しbusy・通知・contentの変更前に無視する。ヘルプ・極小端末案内を実操作に合わせ、長い行をセル幅内に収める。WindowSizeMsgで一覧領域と詳細の再flow/位置調整を行い、極小端末でも安全な最小寸法とサイズ案内を使う。

本文変換・Glamourによる描画とviewerを分けるが、複数pagerへの切り替え設定やplugin機構は作らない。単体バイナリ配布の方針は、本文入力時に利用者のeditorを起動することまで禁止する意味ではない。

対話操作は入出力がターミナルで、`--json`・`--no-input`のいずれも指定されていないときだけ行う。その他はviewerを起動せず、一覧実行は1ページを返して終了し、続きは明示的なoffsetで取得する。

## 本文記法と描画の境界

CLIはBacklog固有の記法を標準的なMarkdownや意味の分かる代替テキストへ変換し、Glamourが端末向けに描画する。Glamour自体の改造・parser拡張や、端末rendererの自作は行わない。

プロジェクトの本文記法に応じて処理を分ける。MarkdownにもBacklog固有の拡張があるため、Glamourにそのまま渡せる部分と、変換が必要な部分を区別する。

人向け表示に、記法としてのHTMLタグ・Backlogマクロをそのまま露出させない。ただしコード領域やエスケープされた記法は、説明対象の文字列として保持する。表示用の変換でBacklog上の本文を変更しない。

| 対象 | 表示方針 |
| --- | --- |
| 基本的な見出し・リスト・強調・引用・コード・チェックリスト・通常の表 | 必要な変換後、Glamourで描画。チェック操作で課題を更新する機能は持たない |
| 結合セルなどの複雑な表 | 初版は結合を再現しない。文字を落とさず通常の表へ簡略化する。完成済みの端末用表は自作しない |
| 色指定 | 装飾として処理し、本文の文字を残す。元の色の再現は必須にしない |
| 折りたたみ | タグを除き、見出しと内容を展開して表示 |
| 課題への特殊リンク | 接続先スペースから課題URLを組み立て、課題キーとリンクを表示 |
| Wikiへの特殊リンク | `Wiki: Home`など、参照内容を表示。正しいURLが分からなければ推測しない |
| Git・Subversion参照 | `Gitリビジョン: app@abcdefg`、`Subversionリビジョン: r11`などで表示 |
| Wiki添付ファイル参照 | `添付ファイル: sample.zip（ID: 11）`などで表示 |
| 目次 | `#contents`・`[toc]`の位置に、見出しから生成した静的な目次を表示 |
| 画像・サムネイル | 画像を取得・描画せず、代替テキスト・分かっているURL・添付IDを表示 |

これらの表示のために、Wiki・Git・添付ファイルの追加APIを初期範囲へ増やさない。正しいURLが既知なら利用する。未知の記法の具体的な安全な表示方法、複雑な表の簡略化などは未検証。

## JSON出力

`--json`は成功結果をstdoutへ、エラーをstderrへ出し、失敗時は非0の終了コードにする。stdoutへ案内文・選択UI・ANSI装飾を混ぜない。本文は描画前の元文字列を保持する。外部データの制御文字も元の文字列値として保持し、JSONのエスケープで表現する。人向け表示の無害化結果には置き換えない。

課題の取得・作成結果は、APIの項目名・構造を基本に課題URLの`url`を加えたオブジェクトとする案。一覧は`items`と`pagination`の形式とし、課題一覧のページ情報に全体件数を含める。

```json
{
  "items": [
    {
      "id": 12345,
      "issueKey": "MYAPP-123",
      "summary": "ログイン画面を修正する",
      "url": "https://example.backlog.com/view/MYAPP-123"
    }
  ],
  "pagination": {
    "offset": 0,
    "limit": 20,
    "count": 1,
    "total": 1
  }
}
```

課題フィールドは説明用の抜粋。`count`は実取得件数、`total`は検索開始時に取得した全体件数。`hasMore`は付けず、`count > 0 && offset + count < total`で続きを判断できる。`count == 0`は`total`にかかわらず終了を意味する。件数取得失敗を0件として扱わない。

課題以外の一覧を含むJSONの厳密なフィールド契約と、エラー出力の具体的な形式は未確定。

## 通信・失敗時の方針

- API key認証は公式仕様に従う。キーを含むURLをログ・エラーへそのまま出さない。
- 課題作成は公式資料に従いform形式で送信する。通信にはタイムアウトを設け、HTTPステータスとBacklogのエラー内容を確認する。
- 課題作成の通信が途切れて成否不明でも、自動再送しない。重複作成を避けるため、状況を明示する。
- POSTの成功を確認した時点で作成成立とし、その後の再取得・表示が失敗しても、作成自体の失敗や成否不明として扱わない。作成成立と後続処理の失敗を区別し、取得済みの課題キー・URLを伝えて再作成を促さない。後続処理の失敗を理由にPOSTを再送しない。再取得の要否と、JSON・終了コードを含む具体的な通知契約はPlanningで定める。
- 認証・権限・通信失敗を、空の結果や別の設定へのfallbackで隠さない。

## 開発時の品質ツール

- linterはgolangci-lint v2系を採用し、`linters.default: standard`で始める。初版では独自の追加・無効化ルールを設けず、必要性が分かった時点で見直す。
- formatterはgofumptとgoimportsを採用する。gofumptがgofmt互換のより厳密な整形、goimportsが不足・未使用importの追加・削除を担当する。gofmtを別の必須整形工程として重ねない。
- golangci-lint v2では`formatters`に両formatterを指定でき、`fmt`で整形、`run`でlintと整形違反の検出ができる。実行入口・CIへの組み込み・editorとの整合はPlanningで具体化する。
- これらは開発用ツールであり、配布した`bl`の実行時依存にはしない。Goとの互換性を確認してバージョンを固定し、ローカルとCIでそろえる。`standard`の内容はgolangci-lintのバージョンに依存するため、更新時に確認する。
- テストはGo標準の`go test`・`testing`を採用し、HTTP通信のテストには`net/http/httptest`を使う。初版では追加のテストフレームワークを導入しない。検証範囲と実行条件はPlanningで具体化する。

## Planningへの引き継ぎ

主要な責務、初期機能、設定構造、viewer方式、検索範囲・並び順は合意済みであり、Planningへ進める状態とする。以下の具体化・検証は後続で扱えるため、基本設計の確定条件にはしない。この整理はPlanningの実行・実装計画の承認・実装開始を意味しない。

### Planningで具体化すること

- CLIオプションの厳密な契約。名前・ID指定の識別、特殊値`self`と同名のユーザーの指定、担当者既定値の解除、反復指定・排他条件、プロジェクト未指定時の入力規則を定める。採用済みの設定構造・短縮名・`mine`の意味は変更しない。
- JSON出力の厳密なフィールド契約とエラー形式。成立済みの作成と後続の取得・表示失敗を区別する通知内容・終了コード、作成後の再取得の要否を定める。
- 人向け出力の制御文字無害化方法、ライブラリが処理する範囲とCLIが補う範囲を確認する。外部データの制御文字と表示用ANSIの区別、通常の改行の保持、JSONの元文字列保持を満たす。
- editor選定規則と本文省略の扱い、通信タイムアウト値。
- Go・Huh・Glamour・Bubblesと関連するBubble Teaの互換バージョン、コマンド解析ライブラリ、実装構成、テストの検証範囲・実行条件、ビルド・配布の詳細。採用済みのgolangci-lint・gofumpt・goimportsについて、互換バージョンの固定と実行方法・CIへの組み込みを定める。将来の検索条件追加に備える汎用エンジンや、未要求の設定・オプションは作らない。

### 未検証事項・既知の制約

- Huhの一覧・ショートカット・CLI内viewerからの復帰、日本語表示とリサイズを組み合わせた操作感は実端末で確認する。Windows対応は別途検証が必要であり、Intel Mac・Linux・Windowsの動作保証は今回の範囲外。
- 表示用変換の境界事例。コード・エスケープを壊さず、タグやマクロを人向けの表現へ変換できるかを確認する。ブラウザと同じ見た目は要求しない。
- 利用先プロジェクトの本文記法・必須カスタム属性は未確認。利用先情報または実スペースでの検証で確認する。必須カスタム属性がある場合、現在の作成項目だけでは登録できない可能性がある。必要と判明しても、カスタム属性対応を初版へ自動追加せず、対象プロジェクトでの利用可否と範囲を見直す。
- 件数取得とページ取得は同じスナップショットではない。途中の追加・削除・並び順に影響する更新により、totalとの差やoffsetによる重複・取りこぼしが生じ得る。全件の一貫したスナップショットは保証しない。

期待する結果は、利用者がIDや多数のオプションを覚えずに検索・閲覧・作成でき、Agentは対話待ちなしで同じ操作を実行できること。後続ではこの観点と、誤った接続先への書き込み・API key漏えい・作成の重複を防げることを確認する。

## 根拠

- Taskflowの参照Skill：プロジェクトルートから`../../pi-dev/pi-taskflow/skills/taskflow-to-ticket/SKILL.md`。本文とGitHub設定・チケットテンプレートを確認した。今回その手順は実行していない。
- Backlog公式API：[課題追加](https://developer.nulab.com/ja/docs/backlog/api/2/add-issue/)、[課題取得](https://developer.nulab.com/ja/docs/backlog/api/2/get-issue/)、[課題一覧](https://developer.nulab.com/ja/docs/backlog/api/2/get-issue-list/)、[課題数](https://developer.nulab.com/ja/docs/backlog/api/2/count-issue/)。
- Backlog公式API：[プロジェクト一覧](https://developer.nulab.com/ja/docs/backlog/api/2/get-project-list/)、[プロジェクト情報](https://developer.nulab.com/ja/docs/backlog/api/2/get-project/)、[種別一覧](https://developer.nulab.com/ja/docs/backlog/api/2/get-issue-type-list/)、[状態一覧](https://developer.nulab.com/ja/docs/backlog/api/2/get-status-list-of-project/)、[優先度一覧](https://developer.nulab.com/ja/docs/backlog/api/2/get-priority-list/)。
- Backlog公式API：[認証ユーザー情報](https://developer.nulab.com/ja/docs/backlog/api/2/get-own-user/)、[プロジェクトユーザー一覧](https://developer.nulab.com/ja/docs/backlog/api/2/get-project-user-list/)、[バージョン（マイルストーン）一覧](https://developer.nulab.com/ja/docs/backlog/api/2/get-version-milestone-list/)。本文を確認し、担当者・マイルストーンの解決に利用できることを確認した。課題追加APIの`assigneeId`、課題一覧APIの`assigneeId[]`・`milestoneId[]`・`sort`・`order`も確認した。
- Backlog公式ヘルプ：[Backlog記法](https://help-center.backlog.com/テキスト整形ルール（Backlog記法）-6a1d4d7f3abb3ada78c5658b)、[Markdown記法](https://help-center.backlog.com/テキスト整形のルール（Markdown記法）-6a1d4d7f3abb3ada78c5658a)。本文を確認し、表示変換を検討した。
- [XDG Base Directory Specification 0.8](https://specifications.freedesktop.org/basedir/latest/)。ユーザー設定の保存先に関する規則を参照。
- [Glamour](https://github.com/charmbracelet/glamour)、[Huh](https://github.com/charmbracelet/huh)、[Bubbles](https://github.com/charmbracelet/bubbles)、[viewportのソース](https://github.com/charmbracelet/bubbles/blob/main/viewport/viewport.go)。README・関連API資料・viewportのソースを確認した。Glamourが端末向け描画、viewportがスクロール表示を担当できる。採用バージョンは未固定。
- [lessの配布案内](https://www.greenwoodsoftware.com/less/download.html)と手元のless 668のmanを比較材料にした。Windows向けの実装もあるが、外部依存なしの配布を重視して採用しなかった。
- 開発ツールの公式資料：[golangci-lint設定](https://golangci-lint.run/docs/configuration/file/)、[CLI](https://golangci-lint.run/docs/configuration/cli/)、[gofumpt](https://github.com/mvdan/gofumpt)、[goimports](https://pkg.go.dev/golang.org/x/tools/cmd/goimports)。golangci-lint v2の`standard`とformatter設定、整形・検出の分担を確認した。採用する具体的なバージョンは未固定。
