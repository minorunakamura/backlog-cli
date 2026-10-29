# Backlog 基本設計 — Rust / bl・bl-cli

## 目的と範囲

人とAgentの両方が使える、Taskflowに依存しないBacklogツールをRustで新規開発する。人向けの統合TUIは`bl`、Agent・スクリプト向けの非対話CLIは`bl-cli`として提供する。両バイナリは設定・API通信・検索・書込み結果の判定・本文変換・安全表示を共有する。

初版はプロジェクトの選択、課題の検索・閲覧・作成・更新、課題コメントの一覧・個別閲覧・追加・更新を対象とする。コメント削除は含めない。IDや多数のオプションを覚えずに人が操作でき、Agentは対話待ちなしで同じAPI操作を実行できることを目指す。

この文書は、製品の機能・主要構成・責務・外部との契約を定める基本設計であり、実装承認ではない。具体的な型・関数・API呼出し、threadやtaskの構成、実装手順は扱わない。実装計画は本設計を入力として別途策定する。

## 全体構成と技術

言語はRust。単一のCargo packageに共通libraryと`bl`・`bl-cli`の2つのbinary targetを置く。両実行ファイルから共通libraryを直接利用し、内部RPC・FFI・常駐サービスは設けない。

| 構成要素 | 責務 |
| --- | --- |
| TUI（bl） | 画面・入力・選択状態を管理し、人の操作を課題・コメントの閲覧と書込みへつなぐ |
| CLI（bl-cli） | 引数・明示stdinを受け取り、固定表示またはJSONと終了コードを返す |
| 設定・機能の共通処理 | 接続先と既定値の適用、名前解決、検索条件、ページ情報、書込み結果の判定を扱う |
| API通信 | 認証・HTTPS・timeout・APIエラーを共通に扱い、APIごとの入出力を検証する |
| 本文変換・安全表示 | 原文を保ったまま表示用の変換とsecret保護を行い、両実行ファイルへ提供する |
| 外部アプリ連携 | TUIからのeditor利用と既定ブラウザ起動を扱う。API通信や設定内容の補修は行わない |

TUIとCLIは同じ機能上の規則を利用し、入力・表示方法だけを分ける。通信中もTUIの終了・中断・画面操作を妨げず、遅い応答で新しい表示を上書きしない。APIは機能単位に整理し、汎用frameworkや全API対応エンジンは作らない。

### 主要技術

| 用途 | 技術 |
| --- | --- |
| TUI・端末入出力 | Ratatui・Crossterm |
| CLI引数解析 | Clap |
| 非同期処理・HTTPS | Tokio・Reqwest・Rustls |
| 設定・JSON | Serde・serde_json |
| Markdown解析 | pulldown-cmark |

Backlog SDKは使わずREST API v2を直接呼ぶ。端末の差分描画や入力プロトコルを独自実装せず、各ライブラリの公開機能を利用する。具体的な依存版や入力部品の選定は後続で行い、forkやpatchを前提にしない。

初版の配布・動作保証対象はApple Silicon Mac（`aarch64-apple-darwin`）。利用者にRust toolchainを要求せず、外部pager・Markdown表示コマンドなしで利用できるバイナリを配布する。

### 端末と外部editor

設定編集と本文編集はTUIから同じeditor連携を利用する。editor利用中はTUIを一時中断し、終了後はフォーム値・選択・検索・スクロールを保って元の画面へ戻る。editor中に端末サイズが変わっても、追加のキー操作なしで現在のサイズに合う表示へ戻る。

正常終了・editor失敗・処理可能な中断では、端末を通常利用できる状態に戻す。復元失敗を成功として通知しない。SIGKILL、端末切断、復元操作そのものの失敗後の正常利用は保証しない。

## 実行ファイル

### bl — 人向けTUI

`bl [--profile <name>]`で起動する。profile省略時は`default`。stdinとstdoutの両方がTTYであることを要求し、暗黙に別の端末を開かない。helpは設定やAPI接続なしで利用できる。

### bl-cli — 非対話CLI

TTYでも選択UI・viewer・editor・確認待ちを起動しない。入力不足はエラーにする。`--body-file -`が明示された場合だけstdinを読む。

| コマンド | 用途 |
| --- | --- |
| `bl-cli project list` | 全参加プロジェクトの一覧 |
| `bl-cli issue-type list` | 対象プロジェクトの種別一覧 |
| `bl-cli priority list` | 優先度一覧 |
| `bl-cli resolution list` | 完了理由一覧 |
| `bl-cli issue list [keyword]` | 課題の一覧・検索 |
| `bl-cli issue mine [keyword]` | 認証ユーザー担当の課題検索 |
| `bl-cli issue view <issueIdOrKey>` | 課題の詳細 |
| `bl-cli issue create` | 課題の作成 |
| `bl-cli issue update <issueIdOrKey>` | 課題の部分更新 |
| `bl-cli issue comment list <issueIdOrKey>` | コメント一覧 |
| `bl-cli issue comment view <issueIdOrKey> <commentId>` | コメントの詳細 |
| `bl-cli issue comment add <issueIdOrKey>` | コメント追加・状態／担当者／完了理由の変更 |
| `bl-cli issue comment update <issueIdOrKey> <commentId>` | コメント本文の更新 |

共通オプションは`--profile`、`--json`、`--no-input`。profile省略時はTUIと同じく`default`。`--no-input`の有無で非対話動作は変わらない。主な入力は次のオプションで指定する。

| 入力 | オプション |
| --- | --- |
| プロジェクト | `--project/-p` |
| 状態・担当者・マイルストーン | `--status/-s`、`--assignee/-a`、`--milestone/-m` |
| 完了理由 | `--resolution` |
| 作成・更新時の種別・優先度 | `--issue-type`、`--priority` |
| 開始日・期限日 | `--start-date`、`--due-date` |
| タイトル・本文 | `--title/-t`、`--body/-b`、`--body-file` |
| 取得件数・課題一覧の開始位置 | `--limit`、`--offset` |

コメントの追加・更新にも`--body`または`--body-file`を使う。コメント追加には状態・担当者・完了理由の変更も指定でき、本文なしの変更だけでも送信できる。既存コメントの更新は本文だけを対象とする。コメント一覧のIDによるページ指定と、更新時の明示解除用flagは実装計画で定める。

名前で指定する項目にはID直接指定の手段も設ける。担当者の既定値を明示解除できるようにする。適用先・排他・反復指定などの厳密なflag契約は後続の実装計画で定める。コマンド階層を省略するaliasや利用者定義aliasは追加しない。

## 対象API

対象は次の19 API。`GET /api/v2/space`はTUIのWorkspace用で、非対話CLIでは不要な通信を発生させない。

| API | 用途 |
| --- | --- |
| `GET /api/v2/space` | 正式なspace名 |
| `GET /api/v2/projects` | プロジェクト一覧 |
| `GET /api/v2/projects/:projectIdOrKey` | project解決・本文記法確認 |
| `GET /api/v2/projects/:projectIdOrKey/issueTypes` | 種別候補・名前解決 |
| `GET /api/v2/projects/:projectIdOrKey/statuses` | 状態候補・名前解決 |
| `GET /api/v2/projects/:projectIdOrKey/users` | 担当者候補・名前解決 |
| `GET /api/v2/projects/:projectIdOrKey/versions` | マイルストーン候補・名前解決 |
| `GET /api/v2/users/myself` | 認証ユーザー・self解決 |
| `GET /api/v2/priorities` | 優先度候補・名前解決 |
| `GET /api/v2/resolutions` | 完了理由候補・名前解決 |
| `GET /api/v2/issues` | 課題検索・ページ取得 |
| `GET /api/v2/issues/count` | 同じ検索条件の全体件数 |
| `GET /api/v2/issues/:issueIdOrKey` | 課題詳細・プレビュー |
| `POST /api/v2/issues` | 課題作成 |
| `PATCH /api/v2/issues/:issueIdOrKey` | 課題更新・コメント本文と課題項目の同時送信 |
| `GET /api/v2/issues/:issueIdOrKey/comments` | コメント一覧・ページ取得 |
| `GET /api/v2/issues/:issueIdOrKey/comments/:commentId` | コメント詳細 |
| `POST /api/v2/issues/:issueIdOrKey/comments` | コメント追加 |
| `PATCH /api/v2/issues/:issueIdOrKey/comments/:commentId` | コメント本文の更新 |

課題・コメントの削除、親子設定、カテゴリー・バージョン・予定／実績時間・カスタム属性の設定、添付ファイルの送信、通知先の明示指定は対象外。Wiki・Git・添付ファイル等の参照表示のためにAPIを増やさない。

## profileと設定

### ユーザー共通設定

保存先は`$XDG_CONFIG_HOME/backlog/config.json`。XDG_CONFIG_HOMEが未設定・空・相対パスなら`~/.config/backlog/config.json`とする。macOSでもこの配置を使う。

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

API key本体は指定環境変数からのみ取得し、設定へ保存しない。指定profileやAPI key環境変数が欠落・空ならエラー。他profileや別キーへfallbackしない。現在のprofileを保存する永続状態は持たない。

### 作業ディレクトリ設定

cwd直下の`.backlog/settings.json`だけを読む。親を探索せず、profileを変更しない。

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

- 明示オプションを対応する設定値より優先する。projectはキーまたはIDを文字列で指定する。
- createの既定値を検索・課題更新・コメント操作に流用しない。課題更新・コメントの対象projectは指定課題から判断し、ローカルのprojectで置き換えない。一覧のstatusはOR条件で、未設定・空配列は絞込みなし。
- 設定の不在と、読取失敗・壊れたJSON・型不正・未知のキー・不正値を区別する。設定用CLI、wizard、自動生成、補修、API候補の永続cacheは設けない。

### 設定編集

TUIのWorkspaceの`*`から共通configとcwd settingsの編集対象を選ぶ。j/k・上下で循環、Enterで設定済みeditorを開く。対象や親が不在なら生成せず場所を案内する。

保存内容は再起動時に検証・反映する。実行中のprofile・space・editor・既定値を差し替えない。editorが非0終了しても保存済みの設定を巻き戻さない。設定破損時の復旧モードは設けない。

## 検索・名前解決・ページ送り

名前は完全一致で解決し、候補なし・曖昧ならエラー。ID直接指定は原則として正整数を要求するが、完了理由はID 0も有効とし、未設定として扱わない。不要な候補APIは呼ばない。`self`は選択profileのAPI keyに対応する認証ユーザーで、操作している人を推測しない。TUI上の認証ユーザーの表記は「自分」とする。

検索条件はproject・keyword・状態・担当者・マイルストーン。項目内はOR、項目間はAND。通常検索ではローカルのproject・検索statusを適用する。`mine`は担当者をselfに固定した検索で、別の検索処理や設定は持たない。

`--status all`は設定の状態条件を解除する特別値。`--project all`は全参加プロジェクト横断検索で、空白のみでないkeywordまたはselfを必須にする。横断時はローカルの検索既定値を一切適用せず、状態・マイルストーン・self以外の担当者指定は拒否する。

```sh
bl-cli issue list "ログイン" -p MYAPP -s 完了
bl-cli issue list "ログイン" -p all
bl-cli issue mine -p all
bl-cli issue list -p MYAPP -a self -m "v1.0"
```

- 課題一覧は既定20件、limitは1〜100、offsetは0以上。更新日時の降順（`sort=updated`、`order=desc`）で固定する。
- 新しい検索では、同じ条件の全体件数と一覧を取得する。全体件数はページ移動のたびに取り直さず、取得失敗を0件に置き換えない。
- 実取得件数0ならtotalに関係なく終了。非空なら、現在offsetと実取得件数の合計がtotal未満のときだけ次がある。
- CLIは指定の1ページだけ返す。TUIのp/nは訪問履歴を使い、訪問済みページの内容・選択・スクロールをメモリに保持する。再訪でlist/countを呼ばない。
- TUIの一覧Rは現在の検索条件で全体件数と先頭ページを再取得する。成功後だけ表示を交換して履歴を捨て、失敗では更新前の表示・履歴を保つ。
- count/listは同じsnapshotではない。途中の更新によるtotalとの差、重複、取りこぼしは保証しない。全件自動取得・定期更新・候補や履歴の永続化はしない。

## TUIの画面・操作

### 配置とフォーカス

上部Workspace、左上Project List、左下を広く取るIssue List、右上Issue Detail、右下Comment。Commentには選択課題の最新コメント（変更履歴を含む）を初期表示し、1件ずつ切り替えて閲覧する。Workspaceにはspace名と`*`の設定入口を置き、検索・作成ボタンは置かない。フォーカス枠と現在使えるキーを表示する。

100列×24行以上は全パネル、40列×12行以上はフォーカス中パネルの切替表示、それ未満はサイズ案内と安全な終了・中断だけとする。ダイアログはフォーカス項目を見える範囲に収め、極小サイズで見えない確認の承認・書込み送信・入力変更を受け付けない。

全パネル表示の右側は従来の80／20分割を基本とし、Comment外枠を最低6行にして残りをIssue Detailへ割り当てる。100×24ではDetail 14行／Comment 6行とし、Comment内の投稿者・日時・空行を固定して本文を最低1行表示する。TUIのAPI作成・更新日時はJST（UTC+09:00）の`yyyy/MM/dd HH:mm:ss`で表示する。開始日・期限日等の日付専用値、本文内の日付文字列、CLI固定出力・JSONのAPI日時は変更しない。日時を解釈できない場合は安全な原値と変換不可を示し、架空の時刻を補完しない。具体化・検証は[Issue #18再計画節](../plans/backlog-cli-initial.md#issue-18-再計画comment表示tui日時詳細のinline差分)に従う。

| 場所 | キーと動作 |
| --- | --- |
| 左右領域間（メイン画面） | Shift+Lで右、Shift+Hで左の最後にfocusしたパネルへ復帰。同じ側へのキーではfocusを変えない |
| 左2パネル | h/lのどちらでもProjectとIssueを切替。Tab=l、Shift+Tab=h |
| Project List | 上下/j/kで候補移動、Enterで確定して一覧更新。focusはProjectに維持。Ctrl+Aで横断検索 |
| Issue List | 上下/j/kで選択とプレビュー、EnterでDetail、p/nでページ移動、/で検索、cで作成、Ctrl+Oでブラウザ |
| 右2パネル | h/lのどちらでもDetailとCommentを切替。Tab=l、Shift+Tab=h。EscでIssue Listへ戻る |
| Issue Detail | 上下/j/k、PgUp/PgDn、b/Space、g/Home、G/Endで本文スクロール。eで課題編集フォームを開く |
| Comment | j/kで本文スクロール。nで古い側、pで新しい側、gで最新、Gで最古へ移動。cで追加、eで表示中の自分のコメント本文を編集 |
| メイン画面 | qで正常終了0、Ctrl+Cで中断130、Rで現在パネル更新、*で設定編集 |
| ダイアログ | Tab/Shift+Tabで項目移動。候補選択中のEscは候補だけ閉じる |
| 検索 | 初期focusはキーワード。Tab順はキーワード→状態→担当者→マイルストーン、Shift+Tabは逆。キーワード欄の外だけCtrl+K/S/A/Gで各項目へ移動し、Ctrl+Uでキーワードを含む全条件クリア。候補popupを閉じた状態のEnterで適用。検索用Ctrl+Fは廃止 |
| 作成 | Ctrl+T: タイトル、Ctrl+Y: 種別、Ctrl+P: 優先度、Ctrl+A: 担当者、Ctrl+B: 本文編集操作へfocus |

左右の最終focusは起動中だけ記憶し、初期値は左Project／右Detailとする。h/l・Tab/Shift+Tab・Enter/Esc・検索成功による実際のメイン画面focus移動も記憶へ反映し、resize・project切替・通常／横断検索だけではリセットしない。dialog内の項目focusは記憶対象外とし、取消・適用失敗で背面focusを変更しない。Shift+H/Lはfocus移動だけで選択・検索条件・本文scroll・通信を変更せず、取得中でも追加GET・取消を行わない。既存Enter/Escの移動先とEscの読取り取消は維持する。40x12以上の通常画面だけで有効にし、dialog・候補popup・適用中・極小時は背面focusを変更しない。エラー通知中は通知を消して移動し、同じ側へのキーでも通知だけは消す。大文字H/LのNONE／SHIFT Pressを対象とし、Ctrl/Alt等の併用・Repeat／Releaseは新操作として受け付けない。詳細は[実装計画の左右領域focus再計画節](../plans/backlog-cli-initial.md#issue-17-再計画shifthlによる左右領域のfocus復帰)に従う。

通常文字のq/h/l/j/k/H/L等を文字入力中に横取りしない。検索のキーワード欄ではCtrl+A/E/B/Fの移動、Ctrl+K/U/W/Hの削除、Ctrl+Yの貼戻し等、既存単行入力部品の編集操作を優先し、検索用Ctrlショートカットを実行しない。キーワード欄内のCtrl+Uはキーワードだけを削除し、欄外のCtrl+Uは全条件をクリアする。欄外のCtrl+Fは何もしない。Ctrl+Cによる中断を最優先し、一つのキーで意図しない複数の操作を起こさない。AltやCtrl+Enterの別名操作は設けない。editor利用中のキーはeditorに委ねる。

初期space/projects取得の失敗を架空の名前や空一覧に置き換えない。設定projectがなければ利用者が明示確定する。別projectの確定で検索・履歴を切り替え、同じprojectの再確定では現在の条件を保つ。

### プレビュー・検索・更新

課題選択に合わせて自動プレビューする。連続移動中の通信は間引き、別の課題や検索へ移った後に遅れて届いた応答は表示へ反映しない。取得中・失敗時は対象キーを表示し、新しい選択に別課題の古い本文を見せない。DetailへのEnterだけで追加GETしない。resizeでは本文を読みやすく折り返し直し、スクロール位置を表示範囲に収める。

検索ダイアログは「キーワード／状態／担当者／マイルストーン」の4条件を一枚に配置する。通常時は上段に状態と担当者、次段にマイルストーン、その下に枠付きキーワード欄を置く。幅=min(72,端末幅−2)、高さ=min(18,端末高−2)で中央配置し、40x12では38x10とする。狭幅ではfocus項目と必要操作を優先し、全項目へ移動可能にする。初回は設定値、その後は適用済み条件を初期値とし、クリア済み条件へ既定値を再挿入しない。

候補項目へ移ると、元の項目位置を動かさない枠付きpopupを重ねる。popupは対象項目名・選択行・操作案内を親ダイアログ内に収める。上下/j/kで候補移動、Spaceで選択/解除し、選択行が表示範囲を外れる時だけ最小限scrollする。候補は一行ずつ表示し、長い名前は左右キーで全文を確認する。閉じた項目の長い選択済み値も左右で全文確認でき、PgUp/PgDnや別の全文確認画面を必須にしない。Enter/Escは候補popupだけを閉じ、取得失敗中のEnterは再取得だけを行う。popupを閉じた項目またはキーワード欄の修飾なしEnterで検索する。適用・開閉・再試行にRepeat/Releaseを使わない。

候補は同じダイアログ内で取得成功済みなら空配列を含め再利用し、未取得時と失敗後の明示再試行時に取得する。全条件クリアでも成功候補を保持し、ダイアログを閉じたら破棄する。取得中の重複要求・古い応答の保存を防ぎ、永続cacheやアプリ全体cacheは追加しない。編集中は検索APIを呼ばず、count/list両方の適用成功後だけ条件・total・先頭ページ・履歴を交換してIssueへfocusする。失敗・取消では元の表示を保つ。適用済み条件は一覧上部、全内容はダイアログで確認できる。詳細なキー優先・候補寿命・popup寸法・検証は[実装計画のIssue #17再計画節](../plans/backlog-cli-initial.md#issue-17-再計画検索dialogの編集優先popup候補再利用)に従う。

横断検索はProject ListのCtrl+Aから専用ダイアログを開く。Tabはキーワードと担当者だけを循環し、状態・マイルストーンへの直接focusは無効とする。結果はIssue List/Detailへ表示する。横断中であることと、通常検索への復帰／作成先確定にはh/lまたはTabでProjectへ移り、具体projectをEnterで確定することを画面で案内する。cはこの案内だけを行い、nは次ページのままとする。具体project確定後もcount/list両方の成功まで横断結果を保ち、失敗・取消で通常復帰済みとして表示しない。成功後のfocusはProjectに維持する。

RはProjectならprojects再取得、Issueならcountと先頭ページ、Detailなら同課題とproject記法、Commentなら選択課題の最新側のコメントページを再取得する。成功後だけ交換し、Detailでは本文先頭へ、Commentでは最新のコメントまたは変更履歴とその本文先頭へ戻す。focusを保ち、右2パネルの再取得でIssue Listを変更しない。取得失敗時は元の表示・選択・スクロールを保つ。失敗、取消後、画面閉鎖後の古い応答で現在の内容・busy状態を壊さない。

Ctrl+Oは現在選択中の課題キーと設定originから構築したHTTPS URLを既定ブラウザへ渡す。APIの任意URLやsecret入りURLを開かず、失敗時は選択を保つ。認証・ログイン・ブラウザ起動後の監視は行わない。

### 日本語入力と安全表示

タイトルとkeywordはTUI内の単行入力とし、日本語IMEの未確定文字・変換候補が入力位置に追従することを求める。日本語・結合文字・emoji・長文でも、入力位置と表示がずれないようにする。水平scroll・resize・フォーカス移動で入力値を失わない。

検索ではIME変換の確定Enterだけで検索せず、確定後の通常Enterで適用する。Crossterm 0.29.0のイベントにはIME composition情報がなく、通常Enterと同じ形で届く確定キーをアプリだけで確実に識別できない。端末側の確定キー処理に依存する最小方式を採り、独自の時間・文字種による推測は追加しない。製品の実端末受入でこの動作を必須確認し、失敗なら未完了として操作方式の再検討へ戻る。Planningのraw tty Probe成功を製品の受入へ代用せず、全端末/IMEでの完全一致や特定端末の必須化は追加しない。

編集値と表示用コピーを分離し、表示変換で送信値を書き換えない。表示範囲にかかわらず入力値全体の安全性を確認し、既知API keyや安全変換が必要な制御文字を含む場合は欄全体を伏字にする。貼付け時の単行入力向けの正規化と、表示だけの変換を区別する。OS/IMEがアプリ受信前に描く未確定文字自体の非表示は保証しない。

## 課題の項目・作成・更新

### 課題の項目

課題の詳細ではタイトル・本文・種別・優先度・担当者・状態・完了理由に加え、開始日・期限日・マイルストーン・作成日時・更新日時を表示する。JSONでもAPIの値を保持する。

| 項目 | 作成 | 更新 |
| --- | --- | --- |
| project | 必須 | 変更しない |
| 種別・優先度・タイトル | 必須 | 指定した項目を変更 |
| 本文・担当者・開始日・期限日・マイルストーン | 任意 | 指定した項目を変更・明示解除（空本文を含む） |
| 状態 | APIの初期状態を使用 | 指定した状態へ変更 |
| 完了理由 | APIの初期値を使用 | 指定した値へ変更・明示解除 |
| 作成日時`created`・更新日時`updated` | Backlogが記録 | 読取り専用 |

タイトルは空白のみ不可。開始日・期限日は実在する日付を`YYYY-MM-DD`で指定し、作成日時・更新日時とは区別する。マイルストーンは複数指定を扱い、対象projectの候補から名前またはIDで解決する。開始日・期限日・マイルストーンの新しい設定既定値は設けない。

### 作成

種別・優先度は独自に既定選択せず、明示値または設定から決める。担当者の既定値を明示解除できるようにする。

```sh
bl-cli issue create -p MYAPP --issue-type タスク --priority 中 -t "修正" -b "修正内容"
printf '修正内容\n' | bl-cli issue create -p MYAPP --issue-type タスク --priority 中 -t "修正" --body-file -
```

TUIは確定済みの具体projectへ一枚のフォームを開く。単一候補はEnter/Spaceで開き、上下/j/kで移動、Enter/Spaceで決定する。Ctrl+Bは本文編集操作へのfocusだけでeditorを起動しない。

フォーム移動・resize・editor往復で入力を保つ。「確認へ」ではPOSTせず、profile/space/projectと全入力を安全表示した後の独立した明示操作で1回だけ送る。初期値と現在値に差がある場合だけ、取消時に破棄確認を出す。送信中の再送・編集・破棄を無効にする。中断時を含む結果の判定・通知は「JSON・通信・書込み結果」に従う。

作成結果を正常に取得できた場合は課題キーと、検証済みorigin/keyから構築したURLを安全表示する。TUIでは作成成功後、現在の検索条件でcountと先頭ページを自動再取得する。再取得成功後だけ一覧を交換し、訪問履歴を捨て、選択・スクロールを先頭へ戻す。失敗時は元の一覧・履歴・選択・スクロールを保ち、「作成済み・一覧更新失敗」と通知する。一覧の再取得失敗で作成済みの判定を取り消したり、POSTを再送したりしない。結果表示を閉じるとIssue Listへ戻る。検索条件は変えず、作成した課題が条件に合わなければ一覧へ直接挿入しない。

### 更新

更新対象はタイトル・本文・種別・優先度・担当者・状態・完了理由・開始日・期限日・マイルストーン。指定・編集した項目だけをPATCHし、未指定の項目は変更しない。未指定と明示解除を区別し、変更のない送信は行わない。projectの移動や作成日時・更新日時の設定は扱わない。

課題・コメントの編集開始時に最新情報を取得し、変更した項目だけ送る。同じ項目への他者の変更との競合を完全に検出・防止することは保証せず、初版に競合検出・自動マージは追加しない。

CLIは変更内容を明示入力する。TUIは取得した対象課題の現値を初期値にし、変更した項目と解除内容を確認してから1回だけ送る。作成と同じく、入力保持・破棄確認・送信中の再送禁止を適用する。正常に取得できた成功応答をDetailへ反映する。Issue Listの検索条件・履歴は保ち、一覧とCommentの変更履歴の再取得はRに任せる。初版ではタイトル変更もIssue Listへ即時反映せず、次の一覧再取得までは旧表示となる暫定仕様とする。

## 課題コメント

対象課題を明示して、コメントの一覧・個別閲覧・追加・本文更新を行う。追加時は本文に加え、課題の状態・担当者・完了理由を変更できる。これらはコメント固有の属性ではなく、同じ操作で変更する課題項目として扱う。コメント本文にも課題本文と同じ記法判定・安全表示・editor連携を適用する。既存コメントの編集は認証ユーザー自身が登録した本文のみを対象とし、過去の変更履歴は書き換えない。APIの権限判定を回避せず、コメント削除は提供しない。

コメント一覧は既定20件、limitは1〜100、通常取得は新しいIDからの降順とする。件数範囲はアプリ側で検証し、API側の拒否に依存しない。APIの`minId`・`maxId`でページ取得し、両境界の指定ID自体は結果に含まれない。課題一覧のoffsetやtotalを流用せず、CLIは指定の1ページを返す。コメント件数APIや全件自動取得は追加しない。

TUIはIssue Detail下の同じCommentパネルで、コメントと課題変更履歴を1件ずつ閲覧し、変更履歴も移動対象に含める。通常の降順でnは古い側、pは新しい側、gは最新、Gは最古へ移動する。最古への移動は昇順取得で最古側を取得し、途中の全件取得を必要としない。Detailとのfocus切替と本文スクロールはキー表に従う。

APIのコメント一覧には`content: null`で`changeLog`を持つ課題変更履歴も含まれる。TUIでは「コメント」「変更履歴」の種別文字ラベルを必須とせず、本文と変更項目名付き履歴の構造で区別する。Comment枠のCOMMENT／CHANGE HISTORYと表示ID、一般見出しBODY／CHANGES、末尾のID／TYPE／AUTHOR／CREATED／UPDATEDは表示しない。Commentというパネル名・focus印・必要な取得状態／通知は維持し、枠titleの区切りの`|`は使わない。状態／通知がある場合だけ空白で区切り、本文や履歴の値に含まれる`|`は削除しない。枠内を左揃えの投稿者1行・作成日時1行・空行1行・本文の順にし、先頭3行を固定して本文／履歴だけをscrollする。投稿者を太字＋色で目立たせ、NO_COLORでは太字のみとする。長い名前は表示列／grapheme境界で末尾省略し、省略時だけscroll領域で安全な全文を確認できるようにする。resizeで省略要否を再判定する。n/p等で対象が変わっても日時位置を固定する。内部IDは取得・選択・編集対象識別に保持する。

本文なし履歴も変更項目と前後の値を表示する。ただし、課題本文の`description`変更は「詳細」として、両値が文字列なら安全化した表示用原文の行＋grapheme単位inline差分を表示する。削除は文字を読み取れる高コントラストの前景＋赤系背景＋取り消し線、追加は緑系背景とし、共通部分も省略せず折返し／scrollで確認できるようにする。NO_COLOR時だけ削除を`[-...-]`、追加を`{+...+}`で識別する。空への変更・空からの追加も明示する。既知項目・description・未知fieldを問わず、変更前／変更後の値そのものがJSON nullなら、TUIの表示用値だけを`未設定`にする。object／array内部のnull、文字列`"null"`、空文字、0、falseは置き換えず、nullと空文字を同一視しない。他の非文字列は型を保持した安全な前後値表示にする。API原値・CLI固定表示・JSONは変更しない。配色・null表示・枠titleの具体化と実端末での可読性受入は[Issue #18追加再計画節](../plans/backlog-cli-initial.md#issue-18-再計画削除差分の視認性未設定表示comment枠)に従う。

**TUIの限定例外**として、descriptionの旧／新API文字列のUnicode scalar数（空白・改行込み）の合計が10,000超なら、「詳細」と「変更内容が10,000文字を超えるため、差分を表示できません。」という警告だけを表示し、その前後本文をscroll先にも表示しない。ちょうど10,000は差分対象。この基準はアプリ独自の表示方針であり、Backlog Webと同じ算定方法やAPIの文字数制限・超過フラグが確認できたことを意味しない。通常コメント本文・他の変更項目・APIデータ・CLI固定出力・JSONは例外の対象外。細粒度比較を近似する場合も10,000以下の内容は落とさない。具体的な比較・安全性・検証は[Issue #18再計画節](../plans/backlog-cli-initial.md#issue-18-再計画comment表示tui日時詳細のinline差分)に従う。

1件に本文と履歴の両方があれば空行で区切って同じ1件内で表示し、別パネル・別タブには分けない。eは自分のコメント本文の編集に使い、本文なし履歴は閲覧専用とする。履歴自体は編集しない。履歴field名は更新APIのパラメーター名と同一とは限らない。コメント一覧APIの応答では期限日が`limitDate`、担当者が`assigner`だった。期限日の更新APIパラメーターは`dueDate`のままとする。既知fieldは表示名へ置き換え、未知fieldは識別子と安全なJSON前後値を落とさない。JSONではAPIの本文・履歴を保持する。

コメント追加は「本文がある、または状態・担当者・完了理由に1つ以上の変更がある」場合に送信できる。本文も変更もなければ送信しない。TUIのcは本文・状態・担当者・完了理由の追加フォームを開く。課題項目は最新値を初期表示し、変更した項目だけ送る。担当者・完了理由の明示解除を扱い、完了理由は候補またはIDで指定する。既定値の適用や、状態に応じた完了理由の独自の自動選択・解除は行わない。

本文だけの追加はコメント追加APIを使う。課題項目の変更を伴う場合は課題更新APIへ変更項目と、本文があれば`comment`を1リクエストで送る。本文の省略・空文字時は`comment`を送らず、課題変更だけを送る。課題更新とコメント追加を2つの書込みに分割せず、権限不足等でも本文だけの追加へfallbackしない。課題変更を伴う操作には課題更新の権限が必要となる。

TUIではコメント追加の成功後、送信方式を問わず最新のコメントページを自動再取得し、Commentを最新のコメントまたは変更履歴とその本文先頭へ更新する。課題objectを正常に取得できた場合はDetailにも反映する。コメントの再取得は表示更新のためであり、書込みの成功判定には使わない。再取得失敗時は書込み済みの確度を保ち、「登録済み・コメント表示の更新に失敗」と通知して元のComment表示・選択・スクロールを残す。投稿は再送せず、表示はRで更新できる。Issue Listはこの再取得の対象に含めない。

既存コメントの更新は本文の明示入力を必須とする。空文字への更新はAPIで拒否されるため、本文解除の手段として提供しない。TUIはeditorで本文を編集し、送信前に対象課題・更新時のコメントID・本文・課題項目の変更内容を安全表示する。確認だけでは送信せず、独立した明示操作で1回だけ送る。取消・editor失敗で送信せず、送信中の再送・編集・破棄を無効にする。書込みの結果不明時に再送へ誘導しない。

TUIで既存コメントの本文更新に成功した場合は、正常に取得できた応答で同じコメントの表示を更新する。表示対象を最新コメントへ移したり、一覧を自動再取得したりしない。関連情報の再取得はRに任せる。

## 本文入力とeditor

CLIの課題作成・更新、コメント追加・更新は`--body`または`--body-file`を受け付け、両者は空文字指定も含め排他。本文はUTF-8で検証し、前後空白・改行をトリミングしない。本文省略だけでeditorを起動しない。課題更新で本文が未指定なら既存本文を変更しない。

editorは共通configの文字列配列を使い、未指定なら`["vim"]`。空配列・空の実行ファイル・実行不能な値は設定エラー。VISUAL/EDITORは読まない。shellを介さず、設定済み引数と編集対象pathを分離して渡す。待機が必要なeditorには利用者が`--wait`等を指定する。

本文編集は伏字化していない現在本文の原文を一時ファイルで外部editorへ渡す。一時データは利用者本人だけが読み書きできるように保護し、正常・失敗・処理可能な中断で削除する。editor正常終了・読取・UTF-8検証成功時だけ本文を交換し、不在・非0・読取失敗・不正UTF-8では元の本文と他の値を保つ。editor自身による原文の表示や、editorが独自に作成するswap・backup等の保存・削除は管理しない。SIGKILL後の一時データ消去は保証しない。

## 本文変換・表示の安全性

本文表示は課題・コメントの内容を端末で読み取れることを目的とし、Backlogの記法解釈・見た目との完全一致や、すべての記法への対応は要求しない。projectの`textFormattingRule`でBacklog記法とMarkdownを区別する。未知の記法値や未対応・解釈困難な記法は、安全化した原文を表示して内容を確認できるようにする。JSON取得では本文描画や記法確認を不要とする。

Markdownの標準的な構造解析にはpulldown-cmarkを利用し、Backlog記法・独自拡張は共通の本文変換処理で主要なものを扱う。原文と表示用コピーを分離し、code・escape内を誤変換せず、解釈できないことを理由に本文を黙って捨てない。通常改行は表示でも保持する。TUIは内部でスクロール表示、CLIは固定出力とする。 上記の内容保持の一般則に対し、TUIの課題本文変更履歴の前後文字数合計が10,000超の場合だけは、「課題コメント」で明示した警告のみの表示を優先する。通常本文・他の履歴・CLI・JSONへこの例外を広げない。

| 対象 | 表示方針 |
| --- | --- |
| 見出し・list・強調・引用・code・checklist・通常表 | 主要な記法を読みやすく表示。チェック操作で課題を更新しない |
| 複雑表・色・折りたたみ | 専用の再現は必須にせず、本文を保持して簡略化するか安全な原文を表示 |
| 課題link | 認識できた課題キーは検証済みorigin/keyからURLを構築し、キーと可視URLを表示 |
| Wiki・Git・SVN・添付参照 | 参照名・revision・添付ID等を可視テキストで表示。未知URLを推測しない |
| 画像 | fetch・描画せず、代替テキスト・既知URL・添付IDを表示 |
| 目次・未知macro | 専用の解釈・生成は必須にせず、安全な原文表示を許容 |
| HTML | タグを除いて本文を保持するか、安全な原文を表示。HTMLイベントを丸ごと捨てて本文を欠落させない |

候補・課題名・本文・APIエラーなど、外部由来の文字列はすべて安全表示の対象にする。危険な制御文字や端末命令を無害化し、HTML entity等の解釈後にも危険な文字を残さない。日本語・結合文字・emojiと本文の改行・TABは保ち、表示用変換で原文を変更しない。

装飾はアプリが生成するものに限り、HTMLや外部由来ANSIを端末命令として実行せず、外部データによるcursor操作・clipboard操作・title変更を許可しない。NO_COLOR指定時は両バイナリの色装飾を無効にし、非TTYのCLIは装飾なしで出力する。URLは可視テキストとし、既知API keyは表示前に伏字化する。

## JSON・通信・書込み結果

CLIの`--json`成功結果は改行終端の1つのJSON文書をstdoutへ出す。失敗はstderrへJSONを出し非0で終了する。案内・ANSIを混ぜず、本文等の文字列値・未知field・大整数を保つ。JSONの空白・field順・escape表記の一致は要求しない。ただし既知API keyはJSONでもredactする。

- issue view/create/updateはAPI応答objectへ`url`を追加する。
- コメントのview/updateはAPI応答objectを返し、一覧は`{"items":[]}`とする。コメントIDは各objectから取得できる。
- コメントaddは`{"resource":"comment","data":{}}`または`{"resource":"issue","data":{}}`の形で返す。本文だけの追加では`resource`を`comment`、課題変更を伴う追加（本文なしの変更を含む）では`issue`とし、`data`に元のAPI応答objectを保持する。CLIはコメントIDを推測したり補完のためだけに追加GETしたりせず、表示用の再取得はTUIだけが行う。
- issue list/mineは`{"items":[],"pagination":{"offset":0,"limit":20,"count":0,"total":0}}`の形。`hasMore`は付けない。
- その他の一覧は`{"items":[]}`。厳密なエラー形式と終了コードは実装計画に記す。

通信はHTTPS originを検証し、API keyを`Backlog-API-Key` headerで送る。queryに載せず、redirect・自動retryを無効にする。1 requestは本文読取を含め30秒でtimeoutする。POST・PATCHはform形式で送り、失敗時も自動再送しない。

処理の成否と書込み結果の確度は別に扱う。課題作成・本文だけのコメント追加は`not_created`・`unknown`・`created`を区別し、201受信後は本文読取・decode・表示に失敗してもcreatedを保つ。課題更新・課題変更を伴うコメント追加・既存コメントの本文更新は未更新・結果不明・更新済みを区別し、200受信後は後続処理が失敗しても更新済みを保つ。送信された可能性があって成功・拒否を確認できなければ結果不明とする。送信中の中断時も、終了時点で確認できた確度を維持する。

CLIの書込みエラーJSONには操作と結果の確度を含め、固定出力・TUIでも未実施・結果不明・実施済みを区別して通知する。成功応答後の失敗を単なる書込み失敗として通知しない。出力先の切断や端末切断など、通知自体ができない場合の到達は保証しない。成功確認だけの追加GETは不要。実施済み・結果不明から同じ操作の再送へ誘導しない。

認証・権限・通信・不正応答を空結果や別設定へのfallbackで隠さない。URL・header・raw responseのdebug dumpは追加しない。

## 品質・配布

Rust標準のtest、rustfmt、Clippyを基本とし、依存の既知脆弱性はcargo-auditで確認する。toolchainと依存を固定し、ローカルとCIで再現可能な開発環境にする。

README.mdには、導入・動作環境・設定・CLIとTUIの使い方・利用上の制約など、利用者に必要な情報を記載する。開発環境の構築・ビルド・テスト・品質確認・CI・配布物の作成など、開発に必要な情報はCONTRIBUTING.mdに記載し、利用手順と混在させない。

CIはGitHub Actionsを使用し、mainへのpush時とpull request時に品質確認を実行するworkflowを用意する。workflowの作成・設定内容の確認は実装範囲に含めるが、GitHub上でのCI実行・成功は実装完了の条件に含めない。その確認のためだけのpush・pull request作成も要求しない。ローカルで実行可能な品質確認は行い、CI未実行の場合は完了報告に残す。

品質確認では、CLIの公開契約、設定・APIの境界、本文の安全表示、書込み結果の確度、TUIの操作と状態保持を重視する。本文変換は記法の完全互換ではなく、主要記法の読みやすさ、未対応記法の内容保持、code・escapeの保護と端末への安全性を確認する。

特定の端末アプリを利用条件にしない。日本語IME・表示幅・editor復帰・resize・中断後の端末利用は実端末で確認し、使用した端末・実行環境と結果を記録する。自動試験だけで実IMEの動作確認に置き換えない。入力部品の選定時に日本語入力・表示幅・cursor位置を確認し、editor連携では復帰直後の再描画と失敗・中断経路を確認する。具体的な試験方法と開発順序は実装計画で定める。

API keyのheader認証、課題作成・更新とコメント追加・更新のform送信・成功status、課題一覧とコメント一覧のページ指定、コメント更新が認証ユーザー自身の投稿に限られることは公式資料で確認した。

2026-10-27（JST）、利用者の明示許可の下、実Backlogの検証専用課題[BLCLI-1](https://lazybl.backlog.com/view/BLCLI-1)で次を確認した。マイルストーン2件・課題1件・本文付きコメント4件（初回3件、追加検証1件）を作成し、検証データは削除せず残した。マイルストーン作成APIは検証準備だけに利用し、製品の19 APIには追加しない。

- 作成時に開始日・期限日・複数マイルストーンを指定でき、作成日時・更新日時はAPIが記録する。
- 日付は空文字で解除でき、再取得時にnullとなる。マイルストーンは指定集合への置換、空値での全解除ができる。更新で省略した日付・マイルストーンが保持されることも確認した。担当者は空文字で未割当て、課題本文は空文字で空本文にでき、追加検証後は元の担当者・本文へ復元した。
- 候補APIのID・名前の取得、種別・優先度・マイルストーンの完全一致照合、認証ユーザーのproject所属を確認した。projectのキー／ID、課題のキー／IDで同じ対象を取得できた。
- 検証課題に絞り、keyword・project・状態のOR指定・担当者・マイルストーンの条件でlistとcountの一致、offsetによる終端を確認した。project省略の検索も実施したが、現実Backlog環境では複数projectを作成できず、複数project間の検索は未検証。
- コメントの追加・個別取得・自分の本文更新、昇順／降順取得を確認した。`minId`・`maxId`は境界IDを含まず、当時の6件を2件ずつ両方向へ走査して重複なし・空ページでの終端を確認した。
- 本文・状態・担当者・完了理由（ID 0）を課題更新APIへ1回で送信し、200と課題objectが返り、コメントIDは含まれないことを確認した。コメントの再取得では本文と3項目の変更履歴が同じ1件に記録されていた。その後、本文なしの更新で元の状態・担当者・完了理由へ復元でき、担当者と完了理由の空文字による解除も確認した。
- コメント一覧には本文がnullの課題変更履歴も含まれる。同APIの`changeLog`に項目名・変更前の値・変更後の値があり、期限日変更の`field`は`limitDate`だった。空文字へのコメント更新は400で拒否され元の本文を保持した一方、一覧の`count=0`は200を返した。
- 認証headerなしは401・API code 11、存在しない課題は404・code 6で、`errors`配列の各要素に`code`・`message`・`moreInfo`があることを確認した。通常応答のレート制限headerも確認したが、429は意図的に発生させていない。

完了理由の候補APIを実取得し、ID 0が「対応済み」であることを確認した。同時送信の検証は「未対応・担当者なし・完了理由なし」から「完了・自分担当・対応済み」と、その復元を対象にしており、状態と完了理由の全組合せを網羅してはいない。同名候補の曖昧さ、複数projectをまたぐ検索、他者コメントの更新拒否、権限制限ユーザー・レート制限・通信障害時の挙動は未検証である。複数project対応は機能範囲に残し、自動試験で確認する。複数projectの実サービス検証は現環境では行えない制約として引き継ぎ、追加projectの用意は要求しない。今回の確認はAPIの挙動の検証であり、製品のCLI・TUIが実装・検証済みという意味ではない。

設計時の小規模試験で確認した範囲は次のとおり。これらの版は製品の採用版を確定するものではない。

- pulldown-cmark 0.13.5：基本的なMarkdown構造、改行・HTMLイベント・原文位置情報の取得、Backlog独自記法との差を確認した。本文変換処理全体や複合記法の内容保持・安全表示は未検証。
- Crossterm 0.29.0とPTY：raw mode切替、vim往復、editor中のresize後のサイズ取得、端末設定の復元、UTF-8本文の保持を確認した。実IME・表示幅・TUI再描画・失敗・中断経路は未検証。

配布は`bl`・`bl-cli`・README.md・設定例を含む`backlog-cli_darwin_arm64.tar.gz`とSHA-256。Rust toolchain・利用者設定・API key・開発用fixtureを含めない。実行に必要なnative libraryの有無は配布物で確認する。Release公開・署名・notarization・Homebrew formulaは初版の作業に含めない。

## 対象外・既知の限界

Intel Mac・Linux・Windowsの動作保証、plugin、外部pager、profile/spaceの実行中切替、設定自動生成、永続下書き・復旧、書込み状態の永続管理、全件自動取得、Taskflow連携の実装は行わない。

端末ごとの文字幅・IME・キー表現には差があり、全端末での完全一致は保証しない。必須カスタム属性のあるprojectでは初版の項目だけでは作成できない場合がある。実Backlogへの書込みは、接続先・内容を確認した別の明示許可がある場合だけ行う。

## 後続の実装計画で具体化する事項

- 主要技術の具体版と互換性、単行入力部品の選定。
- CLIのflagの適用先・排他条件、コメント一覧のID指定、明示解除、JSONエラーの厳密な構造と終了コード。
- 確認済みの日付・マイルストーン解除とコメントID境界の送信形式・試験への落とし込み、ページ終端と空コメント入力、状態・担当者・完了理由と本文の同時送信の検証。
- 通信・取消・画面更新の実装構成、日本語入力とeditor連携の具体的な実現方法。
- 開発単位と順序、自動試験・実端末受入の方法、CI・配布の手順。

これらは本設計の機能・責務・安全要件を満たすための具体化であり、機能追加や責務の拡大を前提にしない。

## 主要参照

- [Ratatui](https://ratatui.rs/)・[Crossterm](https://docs.rs/crossterm/)
- [Yazi](https://github.com/sxyazi/yazi)：実用TUIの参考。使用ライブラリだけで本製品のIME・editor動作が保証されるとは扱わない。
- [Reqwest](https://docs.rs/reqwest/)・[serde_json](https://docs.rs/serde_json/)・[Clap](https://docs.rs/clap/)・[pulldown-cmark](https://docs.rs/pulldown-cmark/)
- [Backlog認証](https://developer.nulab.com/ja/docs/backlog/auth/)・[スペース情報](https://developer.nulab.com/ja/docs/backlog/api/2/get-space/)・[エラー応答](https://developer.nulab.com/ja/docs/backlog/error-response/)・[レート制限](https://developer.nulab.com/ja/docs/backlog/rate-limit/)
- [課題追加](https://developer.nulab.com/ja/docs/backlog/api/2/add-issue/)・[課題更新](https://developer.nulab.com/ja/docs/backlog/api/2/update-issue/)・[課題取得](https://developer.nulab.com/ja/docs/backlog/api/2/get-issue/)・[課題一覧](https://developer.nulab.com/ja/docs/backlog/api/2/get-issue-list/)・[課題数](https://developer.nulab.com/ja/docs/backlog/api/2/count-issue/)
- [コメント一覧](https://developer.nulab.com/ja/docs/backlog/api/2/get-comment-list/)・[コメント取得](https://developer.nulab.com/ja/docs/backlog/api/2/get-comment/)・[コメント追加](https://developer.nulab.com/ja/docs/backlog/api/2/add-comment/)・[コメント更新](https://developer.nulab.com/ja/docs/backlog/api/2/update-comment/)
- [プロジェクト一覧](https://developer.nulab.com/ja/docs/backlog/api/2/get-project-list/)・[プロジェクト情報](https://developer.nulab.com/ja/docs/backlog/api/2/get-project/)・[種別](https://developer.nulab.com/ja/docs/backlog/api/2/get-issue-type-list/)・[状態](https://developer.nulab.com/ja/docs/backlog/api/2/get-status-list-of-project/)・[優先度](https://developer.nulab.com/ja/docs/backlog/api/2/get-priority-list/)・[完了理由](https://developer.nulab.com/ja/docs/backlog/api/2/get-resolution-list/)
- [認証ユーザー](https://developer.nulab.com/ja/docs/backlog/api/2/get-own-user/)・[projectユーザー](https://developer.nulab.com/ja/docs/backlog/api/2/get-project-user-list/)・[マイルストーン](https://developer.nulab.com/ja/docs/backlog/api/2/get-version-milestone-list/)
- [Backlog記法](https://help-center.backlog.com/テキスト整形ルール（Backlog記法）-6a1d4d7f3abb3ada78c5658b)・[Markdown記法](https://help-center.backlog.com/テキスト整形のルール（Markdown記法）-6a1d4d7f3abb3ada78c5658a)
- [XDG Base Directory Specification](https://specifications.freedesktop.org/basedir/latest/)
