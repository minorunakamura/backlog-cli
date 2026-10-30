# backlog-cli

Apple Silicon Mac向けのBacklog TUI `bl` / 非対話CLI `bl-cli`（Rust）。
現在は `bl-cli project list` / `issue-type list` / `priority list` / `resolution list` と
`issue list` / `issue mine` / `issue view` / `issue create` / `issue update` / `issue comment list` / `issue comment view` / `issue comment add` / `issue comment update` を利用できます。
`bl` はプロジェクト選択、通常/横断検索ダイアログ、課題一覧のページ巡回、選択追従プレビュー/詳細・コメント閲覧と再取得、ブラウザ起動、端末復元を利用できます。
設定editor連携と課題作成・更新フォームを利用できます。コメント書込みのTUIフォームは未実装です。
課題作成 (#20) の実端末・実Backlog受入結果は[手動受入記録](docs/testing/manual-acceptance.md#issue-20-tui課題作成)を参照してください。
両binaryの `--help` / `-h` と `--version` / `-V` は設定・API接続・TTYなしで利用できます。

```sh
cargo run --locked --bin bl -- --help
cargo run --locked --bin bl-cli -- project list --json
```

## TUIプロジェクト・課題一覧・詳細・コメント

```sh
bl                  # default profile、stdin/stdoutの両方がTTY必須
bl --profile work
NO_COLOR=1 bl        # 色なしでも二重枠・太字・>でfocusを表示
```

起動時に正式なspace名と参加プロジェクトを取得します。取得中もfocus移動・終了・中断ができ、
失敗はstderrに明示して非0で終了します。初期space/projects取得が完了するまで、Esc/Rや候補の移動・確定は受け付けません。
空projectsの成功と取得失敗を区別します。
設定project（キー/ID）が候補にあれば初期対象として課題を取得します。設定なし/候補に不在なら
明示選択を案内し、先頭候補を勝手に確定しません。Workspaceの `*` で設定の編集対象を選べます（compact表示でも利用可）。

- Project List: j/k・上下は候補移動だけ（通信なし）。Enterで確定し、focusはProjectに維持します。
  別projectは設定の検索statusを適用して新しい一覧へ切替。同projectの再確定は現在の条件/履歴を保持します。
- Issue List: j/k・上下で選択、p/nでページ移動。既定20件・更新日時降順で、実取得数から次offsetを求めます。
  訪問済みページの一覧内容/選択/scrollはメモリ保持し、再訪では一覧を再取得しません。全件自動取得はしません。
  選択に約150ms遅延でプレビューが追従します。連続移動は間引き、選択変更時点で旧課題の本文を隠します。
  取得中/失敗時は対象キーを表示し、別課題の本文は表示しません。Enterだけでは追加GETしません。
- Issue Detail: KEY・タイトル・本文・種別・優先度・担当者・状態・完了理由・開始日・期限日・
  マイルストーン・作成/更新日時を一つの折返し文書として表示します。上下/j/k、PgUp/PgDn、b/Space、
  g/Home（先頭）、G/End（末尾）で移動し、resizeで折返し直してscrollを表示範囲へ収めます。
  Rは同課題とそのproject記法を再取得し、両方の成功後だけ文書先頭へ交換します。失敗時は内容/scroll/focusを保持し、一覧は変更しません。
- Issue ListのCtrl+O: 設定HTTPS originと選択キーだけからURLを構築してmacOSの`open`へ渡します。
  shell・APIの任意URLは使用せず、既知API key入りURLは開きません。起動失敗時も選択を保持します。
- R: Projectではprojectsだけを再取得。確定projectが候補から消えても課題一覧や条件は変更しません。
  Issueでは現在の解決済み条件でcountと先頭listを取得し、両方の成功後だけ履歴を置換します。
- 再取得・ブラウザ起動の失敗は画面中央の枠付き通知に安全なメッセージを表示し、以前の内容/選択/scroll/履歴を保持します。
  通常画面の通知中の修飾キーなしEnter・Esc・Spaceは通知だけを閉じ、通常の画面遷移/取消/scrollは実行しません。
  その他の通常入力（j/k/h/l・R・Ctrl+O等）は通知を閉じてから本来の動作を実行し、Rが再び失敗すれば新しい通知を表示します。
  通常dispatchに進むShift付きキーや未割当の文字も通知を閉じますが、無視するmodifier・キーRelease・resize・時間経過では閉じません。
  qは終了、Ctrl+Cは中断130を優先します。
  赤/太字で表示し、NO_COLORでは色なしの枠/太字で識別します。
  通知がない時のEscは実行中の読取りを取消して表示を保持します。
  取消/切替前の古い応答は新しい表示・通知やbusyを変更しません。通信中もfocus移動・終了・中断ができます。

- 100列×24行以上: Workspace / Project List / Issue List / Issue Detail / Comment。
- 40列×12行以上: focus中パネルのみ。h/l・Tab/Shift+Tabは左2パネル内または右2パネル内を切替。
- 通常メイン画面の大文字 **L（Shift+L）で右、H（Shift+H）で左**の最後にfocusしたパネルへ戻ります。
  初期値は左Project／右Detail。同じ側へのキーはfocusを変えません。小文字h/l・Tab/Shift+Tabは左右それぞれの内部切替です。
  記憶は起動中だけで、内部切替・Enter/Esc・検索適用成功も反映し、resize・project切替・通常/横断検索ではresetしません。
  新キーは空一覧・通信中もfocusだけを変え、選択・条件・履歴・本文scrollを保ち、追加GETや読取り取消をしません。
  dialog/popup/適用中/40x12未満では背面focusを変えず、キーワード中のH/Lは文字です。
  大文字H/LのNONEまたはSHIFTのPressだけが対象で、Ctrl/Alt等の併用・Repeat/Releaseはfocus・記憶・通知を変えません。
  端末がPressとして送る反復やCaps Lock由来の大文字は区別しません。案内の `H:Left L:Right` が領域間、`h/l Tab/S-Tab` が内部切替です。
- 通知がない時はIssue ListのEnterで、右の記憶に関係なくDetailへfocusを移します。右側のEscは読取りを取消しIssue Listへ戻ります。
  有効なH/Lは通知を消して移動します。同じ側へのno-opでも通知だけは消しますが、拒否されたH/Lでは消しません。
- それ未満: サイズ案内とq/Ctrl+Cだけ。見えない画面の操作は受け付けません。
- 通常画面ではqで終了0（極小サイズでも可）。検索dialogはEscで候補を閉じる／取消、Ctrl+Cで中断します。
  Ctrl+C（キー/SIGINT）は130、実行エラーは1。終了時にraw mode・表示属性・alternate screen・cursorを可能な限り復元し、
  復元失敗も通知します。SIGKILL/端末切断後は保証しません。

### 設定のeditor編集 (#19)

通常画面の **`*`** で共通config／cwd settingsの2対象を表示します。
**j/k・上下**で循環、**Enter**で開き、**Esc**で取り消します。
対象は起動時と同じXDG/HOMEルールの共通 `backlog/config.json` と、cwd直下の `.backlog/settings.json` です。
対象fileや親ディレクトリがない時は作らず、場所を通知します。既存の設定だけを開きます。

設定の `editor` argvに対象pathを独立した引数として追加し、shellなしで実行します。
editor中はTUIの入力pollを止め、raw mode・alternate screen・cursor等を復元します。
復帰時はサイズを再取得し、追加キーなしでclear・全体再描画します。focus・選択・検索・ページ・scrollは保持します。
起動失敗やeditorだけの非0／シグナル終了は画面へ通知して復帰します。
**保存した設定は非0終了でも巻き戻しません。検証・反映は再起動後だけ**で、実行中のprofile・接続先・editor・既定値は変えません。
親 `bl` がSIGINTを受けた時は子を停止して回収し、通常端末へ復元を試みて130で終了します。
editorが必要とする待機引数（例: `--wait`）は利用者が設定してください。

共有libraryの本文編集は原文を本人のみ読書き可能な一時fileへ渡し、正常終了・読取り・UTF-8検証成功時だけ交換します。
失敗／処理可能な中断では元本文を保持して一時fileを削除します。課題作成の本文編集にもこの処理を使います。
editor自身の表示やswap／backupは管理せず、SIGKILL・端末切断後の復元／一時file削除は保証しません。
実端末目視の受入状況は[手動受入記録](docs/testing/manual-acceptance.md#issue-19-editor連携)を参照してください。

### TUI課題作成 (#20)

Issue Listの **c** で、明示確定済みの具体projectへ一枚のフォームを開きます。
Project Listのハイライトだけでは作成先を変えません。直前の検索が失敗していても、Enterで確定した作成先をフォームに表示します。
横断結果の表示中はフォームを開かず、Projectへ移って具体projectをEnterで確定する手順を案内します。**nは次ページのまま**です。

- Tab / Shift+Tab: タイトル→種別→優先度→担当者→開始日→期限日→マイルストーン→本文編集→確認へ→取消。
  Ctrl+T/Y/P/A/Bは、それぞれタイトル／種別／優先度／担当者／本文編集操作へのfocusです。
  ただしタイトル／開始日／期限日の入力中は単行入力の標準編集を優先し、Ctrl+A/B/Yは先頭移動／左移動／貼戻しです。
  空欄や端で編集がno-opでもfocusは移りません。Ctrl+Y/A/Bのfocus移動には先にTab等で入力欄外へ出てください。
  Ctrl+T/Pなど編集キーでないshortcut、Tab/Shift+Tab、Enter/Esc、候補popupは変わりません。
  **Ctrl+Bだけではeditorを起動しません**。本文編集操作のEnter/Spaceで設定editorを開きます。
- 単一候補: Enter/Spaceで開き、上下/j/kで移動、Enter/Spaceで決定。担当者は未設定も選べます。
  マイルストーン: Spaceで複数選択/解除、Enter/Escで候補だけを閉じます。長い候補名は左右で確認できます。
  候補欄のDelete/Backspaceでクリア、本文編集操作のDelete/Backspaceで本文をクリアします。
- 種別・優先度は設定既定値または明示選択が必須で、先頭候補を勝手に選びません。担当者既定値も解除できます。
  日付は実在するYYYY-MM-DD。タイトルは空白のみ不可です。本文とタイトルの前後空白は送信時に保持します。
  単行貼付けのCRLF/CR/LF/TABはspaceへ正規化します。入力欄全体にsecret/制御文字があれば欄全体を伏字にします。
- focus・resize・editor往復で値を保持します。editorの正常終了＋読取り＋UTF-8成功時だけ本文を交換し、失敗時は元本文を保持します。
  Esc／取消は現在値と初期値に差がある時だけ破棄確認を出します（Enter/Spaceで破棄、Escで編集へ戻る）。
- 「確認へ」はPOSTしません。profile・origin・space・projectと全入力／解除を安全表示します。
  j/k・PgUp/PgDn・Space・Home/End（g/G）で長い値・本文の末尾まで確認し、**独立したsで一度だけ送信**します。
  確認中のEnterは送信せず、Escは編集へ戻ります。送信中は編集・再送・破棄・qを受け付けません。
  40x12未満では隠れた編集・承認・送信はできず、送信中はqも無効です。Ctrl+Cは常に中断130です。
- 中断・通信/応答失敗はnot_created／unknown／createdを区別し、201 headers後のbody失敗でも作成済みを保ちます。
  unknown／createdでは再送を勧めません。自動retry・永続下書き・結果照会用GETはありません。
- 正常な作成応答には課題キーと構築URLを表示し、現在の検索条件のcountと先頭listを自動再取得します。
  両方の成功後だけ一覧/履歴を交換します。失敗時は元の一覧/ページ/選択/scrollを保ち、作成済み・一覧更新失敗を区別します。
  新課題を一覧へ直接挿入せず、結果のEnter/EscでIssue Listへ戻ります。

ローカルHTTPS/PTYの再現手順は[CONTRIBUTING](CONTRIBUTING.md#tui課題作成-20-releaseの直接ローカル検証)、
実施済み・未実施の境界は[手動受入記録](docs/testing/manual-acceptance.md#issue-20-tui課題作成)を参照してください。

### TUI課題更新 (#21)

**Detailのe** で対象を固定し、最新の課題とそのprojectの候補を取得します（create既定値は使いません）。
タイトル／本文／種別／優先度／担当者／状態／完了理由／開始日／期限日／複数マイルストーンを現値で初期表示します。
作成フォームと同じTab・候補popup・Ctrl+T/Y/P/A/B・本文editor・resize・破棄確認を使い、本文編集の次に状態→完了理由→確認へ進みます。
タイトル／開始日／期限日の入力中は作成と同じ標準編集優先で、Ctrl+A/B/Yのfocus移動は入力欄外で使います。
担当者・完了理由・日付・マイルストーン・本文は明示解除でき、完了理由ID **0は解除とは別の有効な値**です。
種別・優先度・状態は解除せず候補から変更します。日付はYYYY-MM-DDで編集します。

- 「確認へ」で共通Serviceが入力検証・差分を確定します。変更した項目と解除だけを安全表示し、未編集・同値・同じmilestone集合は送信しません。
  確認中のEnterでは送信せず、**独立したsで一度だけPATCH**します。Escは編集へ戻り、差がある取消だけ破棄確認を出します。
- editor失敗時は元の本文と他の入力を保持します。送信中は編集・再送・破棄・qを無効にし、極小時は隠れた編集・確認・送信をしません。
  Ctrl+Cは中断130です。結果はnot_updated／unknown／updatedを区別し、200 headers後はbody／decode／表示失敗でもupdatedを下げません。
  unknown／updatedから再送を勧めず、自動retryや成功確認GETはありません。
- 対象keyと一致する正常な成功objectをDetailへ反映できた場合だけ、同じ課題の最新Commentを一度取得します（20件・ID降順・境界なし）。
  必要な既存記法GETと全件の表示準備に成功した後だけ、最新のコメント／本文なし変更履歴1件・本文先頭へ交換します。空ページも正常成功です。
  更新履歴IDの探索や成功確認GETはしません。200後の応答処理失敗、未更新／unknown／未送信では自動取得しません。
- 取得中は「更新済み・コメント表示更新中」の結果画面を維持します。Enter/Esc・再編集・再送・背面focus変更・極小時のqは無効で、
  結果scroll／resizeとCtrl+C／SIGINTによる中断130は有効です。中断時も端末を復元し、updated／再送不要を通知します。
  完了後は更新完了／失敗を区別し、Enter/EscでDetailへ戻ります。失敗は更新済みDetailと旧Commentのページ・選択／scrollを保持し、
  updated／旧表示保持・再送不要とCommentのRによる回復を案内します。手動Rも成功時だけ交換します。
- **Issue Listのタイトル・全行・条件・total・ページ履歴・選択・scrollは自動変更しません**。一覧Rに任せます。
  編集開始前の一覧／Detail／Comment応答は失効し、新しい表示や通知を上書きしません。
  同じ項目への他者の変更の競合検出・自動マージはしません。

ローカルrelease製品の直接再現は[CONTRIBUTING](CONTRIBUTING.md#tui課題更新-21-releaseの直接ローカル検証)、
実端末・実Backlogの必須受入と未実施記録は[手動受入記録](docs/testing/manual-acceptance.md#issue-21-tui課題更新)を参照してください。

### TUIコメント閲覧 (#18)

Commentは選択課題の最新コメント／本文なし変更履歴を自動取得します（選択追従はDetailと同じ約150ms）。
右側のh/l・Tab/Shift+TabでDetailとCommentを切替え、Escで読取りを取消してIssue Listへ戻ります。
focus切替だけでは追加GETや取消を行いません。課題切替では旧本文を直ちに消し、旧応答を反映しません。

- **n**: 1件古い側、**p**: 1件新しい側。ページ内は通信なし、境界で20件だけ取得します。
  古い側は`order=desc`+exclusive `maxId`、新しい隣接側は`order=asc`+exclusive `minId`です。
- **g**: 最新側、**G**: 最古側のページを直接取得して端の1件を表示します。
  Gは昇順1ページだけで、途中の全件走査はしません。offset・total・件数API・全件自動取得は使いません。
- **j/k**: 折返し本文のscroll。g/Gは本文の先頭／末尾ではなくコメントの端です。
  本文・履歴をscrollし、投稿者／作成日時／空行の先頭3行は固定します。resize時も折返しとscroll上限を補正します。
- **R**: 最新側ページを再取得し、全取得・安全表示の成功後だけ最新の1件と本文先頭へ交換します。
  失敗時は元のコメント選択・本文・scrollを保持して通知します。Issue Listは変更しません。

枠titleは通常`Comment`、focus時は`> Comment`です。取得中の`Loading... Esc: cancel`や終端・取消通知は必要な時だけ空白で続け、`|`を付けません（本文・履歴内の`|`は保持）。
枠内は左揃えの投稿者（太字＋色、NO_COLORでは太字のみ）→作成日時→空行→本文です。
長い投稿者は表示列／grapheme境界で`…`へ省略し、省略時だけscroll領域で全文を確認できます。
resizeで省略要否を再判定します。100×24ではDetail 14行／Comment 6行で本文を最低1行表示します。
TUIのComment createdとDetail CREATED／UPDATEDは固定JSTの`yyyy/MM/dd HH:mm:ss`です。
不正な日時は安全な原値と「変換不可」を表示し、固定行に収まらない場合は原値をscroll領域に置きます。
開始日・期限日・本文内の日付・CLI固定出力・JSONの日時は変更しません。
TUIでは種別・ID・BODY／CHANGES見出し・末尾metadataを付けません（API本文そのものは削りません）。
本文と履歴が同居する応答は空行で区切り、同じ1件に両方表示します。期限日`limitDate`・担当者`assigner`等は表示名へ変換し、
未知fieldと任意JSONの前後値も安全表示します。通常本文はCLI／Detailと共通の記法変換を使います。

`description`の文字列前後値は「詳細」として、安全化した表示用原文を行＋graphemeでinline比較します。
削除は白文字（#FFFFFF）＋暗赤背景（#800000）＋取り消し線、追加は従来の黒文字＋緑背景、NO_COLOR時だけ`[-削除-]`／`{+追加+}`で表示します。
空への変更／空から追加・同値も明示し、共通部分や空白・TAB・改行を省略しません。TUI本文のTABは`⇥`で可視化し、
色あり差分の変更された改行は改行自体に加えて`↵`を削除／追加の装飾で表示します。非文字列は型を保持した前後値表示です。
既知／詳細／未知fieldの変更前後値自体がJSON nullなら、TUIだけ引用符なしの`未設定`にします。
入れ子のnull・文字列`"null"`・空文字・0・falseは置き換えません。例: `START DATE: 2026-11-01 -> 未設定`、
`詳細: 未設定 -> ""`、`詳細: "null" -> 未設定`。他fieldの従来の文字列表示方法・API原値・CLI固定表示・JSONは変更しません。
**限定例外**: 元API文字列のUnicode scalar数（改行・空白込み）の前後合計が10,000超なら、
「詳細」と「変更内容が10,000文字を超えるため、差分を表示できません。」だけを表示し、scroll先にも前後本文を含めません。
ちょうど10,000は比較対象です。これはTUI独自の表示基準で、Backlog WebやAPIの文字数制限・超過フラグとは同一ではありません。
通常コメント・他の履歴・CLI／JSON・APIデータには適用しません。50ms目安の近似比較でも対象全文は保持し、秘匿を装飾より優先します。
本文なし履歴だけのページでは記法確認のためのissue/project GETを追加しません。
`c`／`e`による追加・編集は今回未実装です。最新buildでの#18受入結果と未確認事項は[手動受入記録](docs/testing/manual-acceptance.md)を参照してください。

### TUI検索ダイアログ (#17)

Issue Listの `/` でkeyword・状態・担当者・マイルストーンの4条件を一枚で編集します。
初回は設定の検索status、その後は適用済み条件を表示します。Projectの同project再確定でも条件を保持し、
別projectへの切替ではそのprojectで通常検索を開始します。

| キー | 検索中の操作 |
| --- | --- |
| キーワード欄の編集キー | Ctrl+A/E/B/Fで移動、Ctrl+K/U/W/Hで削除、Ctrl+Yで貼戻し等、単行入力の標準編集を優先 |
| Tab / Shift+Tab | キーワード→状態→担当者→マイルストーンの順/逆順。候補項目へ移るとpopupを開く |
| キーワード欄外のCtrl+K / S / A / G | キーワード / 状態 / 担当者 / マイルストーンへfocus |
| キーワード欄外のCtrl+U | キーワードを含む全条件をクリア。focusと成功候補を保持しpopupは閉じる。欄内のCtrl+Uはキーワードだけを削除 |
| 修飾なしEnter（Pressのみ） | キーワード/閉じた候補項目では適用、popup内は閉じるだけ、候補失敗中は再取得だけ |
| 上下 / j/k、Space | popup内は候補移動・複数選択/解除。閉じた項目では再表示だけ。キーワード中のj/k等は文字 |
| 左右 | popupの選択行や閉じた項目の長い選択済み値をgrapheme単位で水平scrollし、複数値の全文を確認 |
| Esc | 候補だけを閉じる。候補が閉じていれば未適用draftを破棄して取消 |
| Ctrl+C | 常に中断130。キーワード中のq/h/l/H/L/j/k/R/c等は通常の文字 |

検索用Ctrl+Fは廃止しました（キーワード欄内は右移動、欄外は何もしません）。Alt/Ctrl+Enterの別名はなく、
Repeat/ReleaseのEnterで適用・候補閉鎖・再試行はしません。端末がPressとして送る反復の完全識別は保証しません。
中央dialogは最大72x18（100x24時）、40x12では38x10です。状態/担当者を上段、マイルストーンを次段、
枠付きキーワードを下に表示します。選択値は各ラベルの右に一行で表示し、未選択の値欄は空欄です。
候補は親内に最大60x10のpopupとして重ねます。
候補は一行ずつで、選択が表示範囲を越える時だけ最小scrollします。
成功候補は空配列も含め同一dialog内で再利用し、Tab周回・再表示・全クリアでGETを繰り返しません。
未取得/失敗後の明示再試行だけ取得し、取得中の同項目は重複しません。dialogを閉じると候補を破棄します。

編集中は候補取得だけでcount/listを呼びません。適用時のcountと先頭listの両方が成功した後だけ、
条件・total・一覧・履歴を交換してIssueへfocusします。適用中はEsc取消とCtrl+Cのみ受け付けます。
失敗時はdraft/値を残して通知し、以前の条件/一覧/履歴/選択/scrollを保持します。
検索中の通知は次の入力で消え、その入力による編集/候補操作/取消を実行します。
一覧上部の `K:` はkeywordの短縮表示、`S:`/`A:`/`G:` は状態/担当者/マイルストーンの選択数（0は絞込みなし）です。
全値・候補の名前とIDはダイアログで確認できます。Rも適用済み条件を使い、クリア後に設定statusを戻しません。

Project Listの **Ctrl+A** は専用の横断検索を開きます。編集できるのはkeywordと担当者「自分」（内部値self）だけで、
空白のみでないkeywordまたはselfが必要です。状態・マイルストーン・他の担当者やローカル既定値は適用しません。
結果は同じIssue List/Detailに表示され、横断中の `/` は現在の横断条件を再編集します。
h/lまたはTabでProjectへ移り、具体projectをEnterで確定すると通常検索へ戻り作成先が決まります。
横断一覧とcはこの手順を案内するだけで作成しません。nは次ページのままです。通常復帰もcount/list両方の成功まで旧結果を保持します。

keywordは共有単行入力を使い、Unicode編集/水平scroll/実cursorを維持します。
bracketed pasteのCRLF/CR/LF/TABだけをspaceへ正規化します。既知API keyまたは制御文字を含めば、
画面外も含む全値を検査して欄全体を固定表示 `[REDACTED]` にします。送信原文を表示用copyで書き換えません。
40x12未満では入力/適用せず、q/Ctrl+Cだけです。OS/IME受信前の未確定文字の秘匿は保証しません。
検索の長い値は左右で確認します。IME確定Enterは端末側の処理に依存し、アプリはcompositionを推測しません。
製品で「変換確定では検索せず、次のEnterで検索」を実端末で必須確認し、失敗した環境では未完了として停止します。
実Backlog/実IME受入の完了状況は手動受入記録を参照してください。

本文スクロール開始位置の上限は65,535折返し行で、これを超える本文末尾へは到達できない場合があります。
本文原文を切り詰め/変更する制限ではありません。固定Ratatui版のunstable行数APIを利用しているため、依存更新時に再確認します。

実端末での確認手順・未実施記録は [手動受入](docs/testing/manual-acceptance.md) を参照してください。

## 設定

共通設定は `$XDG_CONFIG_HOME/backlog/config.json`。XDG_CONFIG_HOMEが未設定・空・相対pathなら
`~/.config/backlog/config.json` を使います。必要なfileは利用者が用意してください。
自動生成や壊れた設定の補修はしません。

```json
{
  "editor": ["vim"],
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

API key本体は `apiKeyEnv` で指定した環境変数にだけ設定します。fileへ保存しません。
指定profileや環境変数が不在・空ならエラーで、他profileや別keyへfallbackしません。
`spaceUrl` はHTTPS originだけを受け付け、userinfo・query・fragment・root以外のpathは拒否します。

editorは文字列配列で、未指定なら `["vim"]`。空配列・空program・NULや実行不能なprogramは
設定エラーです。PATH未設定時の探索先は `/usr/bin:/bin`。VISUAL/EDITORは読みません。
非対話の `bl-cli` はeditorを起動せず、argvとprogramのfile/実行権限だけを検証します。TUIの設定編集は上記のeditorを使います。

cwd直下の `.backlog/settings.json` は省略可能です。親ディレクトリを探索せず、profileも変えません。
`project` は `issue-type list`・通常の `issue list/mine`・`issue create` の既定値です。
`issue.list.status` は通常検索の状態OR条件です。createの既定値は検索へ流用しません。
`project list` / `priority list` / `resolution list` の取得対象には適用しません。

```json
{
  "project": "MYAPP",
  "issue": {
    "create": {"issueType": "タスク", "priority": "中", "assignee": "self"},
    "list": {"status": ["未対応", "処理中"]}
  }
}
```

設定は未知キー・型不正・壊れたJSON・読取失敗・不正値を拒否します。既定値の文字列は
空白のみを含め空を拒否します（保持値はtrimしません）。statusの未設定・空配列は有効です。
候補の存在や所属確認のためのAPIは呼びません。

## 一覧コマンド

```sh
bl-cli project list
bl-cli --profile work --json project list --no-input
bl-cli project list --profile work --json
bl-cli issue-type list -p MYAPP
bl-cli issue-type list --project 123 --json
bl-cli priority list --json
bl-cli resolution list
```

`--profile`（省略時 `default`）・`--json`・`--no-input` はsubcommandの前後で指定できます。
TTYでも非対話で、明示 `--body-file -` 以外のstdin・editor・選択UI・確認待ちは使いません。`--no-input` は動作を変えません。

`issue-type list` のprojectは明示 `--project/-p`、次にcwd設定 `project` を使い、
どちらもなければ入力エラーです。projectキー/数値IDはURLの単一path部品として扱います。
空文字・`.`・`..` はpathを安全に構築できないため入力エラーです。余分なproject取得や所属確認はしません。
`priority list` / `resolution list` には入力flagを設けず、`--project` 等の適用外flagを拒否します。

固定表示はprojectが `ID` / `KEY` / `NAME`、他の3一覧が `ID` / `NAME` のTAB区切りで、
空一覧でも見出しを出します。完了理由のID 0は有効な値として表示します。
JSON成功はstdoutへ改行終端の1文書 `{"items":[...]}`。未知fieldと大整数も保持します。
失敗を空一覧へ置き換えず、JSONモードの失敗はstderrへ次の形で出します（stdoutは空）。

```json
{"error":{"kind":"network","message":"HTTPS request failed or timed out","httpStatus":null,"apiErrors":[]}}
```

終了コードは成功/help/versionが0、引数・project不足・名前解決エラーが2、設定/API/通信/出力エラーが1、
処理可能なCtrl+C中断が130です。help/version自体はJSONではなく通常のテキストです。
外部文字列の危険な端末制御を無害化し、日本語・結合文字・emoji・LF/TABは保持します。
選択profileのAPI keyは値・未知key・配列・数値も伏字化し、key衝突は出力エラーとします。
非TTY/NO_COLOR時を含め、現CLIは色装飾を出しません。

読取りの通信は操作に必要なGETだけです。検索ではcountとlist、必要なproject解決・候補・self取得を行い、
space取得・所属確認・成功確認の追加GETはしません。
API keyは `Backlog-API-Key` headerに送ります。
redirect・自動retryは禁止、body読取を含め1 requestは30秒でtimeoutします。
raw responseやURL/headerのdebug dumpは出しません。

## 課題検索

```sh
bl-cli issue list "ログイン" -p MYAPP -s 完了
bl-cli issue list -p MYAPP -s 未対応 -s 処理中 -a self -m v1.0
bl-cli issue list -p 123 --status-id 1 --status-id 2 --assignee-id 456 --milestone-id 789 --json
bl-cli issue list -p MYAPP --status all --limit 20 --offset 20
bl-cli issue list "ログイン" -p all --json
bl-cli issue mine -p all --profile work --json
```

通常検索は明示 `--project/-p` をcwdのprojectより優先し、どちらもなければ入力エラーです。
projectキーはproject APIでIDへ解決し、数値IDは直接 `projectId[]` へ送ります。
位置引数keyword、`--status/-s`、`--assignee/-a`、`--milestone/-m` は項目間AND、
反復した同じ項目内はORです。同一値は重複送信しません。名前は完全一致で、不在・複数一致は入力エラー。
名前flagと対応する `--status-id` / `--assignee-id` / `--milestone-id` は排他です。
IDは任意精度の正整数で、直接IDでは候補APIを呼びません。数値の名前も名前として照合します。

明示statusはcwdの検索statusを置き換え、`--status all` は既定の状態条件を解除します。
`all` と他の状態指定は混在できません。`--assignee self` は選択profileの認証ユーザーです。
`issue mine` は同じ検索の担当者self固定版で、担当者flagは一切受け付けません。

`--project all` は全参加project横断です。空白のみでないkeywordまたはselfが必須で、
ローカル検索既定値を一切適用しません。明示status（`all`も含む）・milestone・self以外の担当者は
ID flagも含め拒否します。project一覧や所属確認のための追加通信はしません。

limitは既定20・1〜100、offsetは既定0・0以上。`sort=updated` / `order=desc` 固定で、
同じ検索条件のcountと指定した1ページのlistが両方成功した場合だけ出力します。全件自動取得はしません。
固定表示は `ID` / `KEY` / `TITLE` / `STATUS` のTAB区切り。空一覧も見出しだけを出す成功です。
JSONは元の課題配列と次のページ情報だけを返します（`hasMore`は付けません）。

```json
{"items":[],"pagination":{"offset":0,"limit":20,"count":0,"total":43}}
```

countは実取得数、totalはcount APIの値で、両API間の更新による不一致を補正しません。
count/listいずれの失敗も空一覧にせず、成功stdoutを出しません。

## 課題詳細

```sh
bl-cli issue view MYAPP-123
bl-cli --profile work issue view 123 --json --no-input
```

対象は正整数のIDまたは課題キー（英数字/underscoreのprojectキー + `-` + 正整数）です。
任意精度のIDを扱い、URLは設定済みHTTPS originと応答の課題キーから構築します。
`--project` は受け付けず、cwdのproject・検索条件・create既定値も対象を上書きしません。

固定表示は課題キー・タイトル・種別・優先度・担当者・状態・完了理由・開始日・期限日・
複数マイルストーン・created/updated・URLと本文です。未設定の担当者/完了理由/日付は `-`、
null本文は空本文として表示します。対象課題のprojectIdでprojectを1回取得し、
`textFormattingRule` を確認します。Markdownは見出し・強調の記号を外し、箇条書きを `•`、
checklistを `☑` / `☐`、通常表をTAB区切りで表示します。番号list・引用・code blockは
読み取り用の記号を保ち、inline codeは内容をそのまま表示します。通常改行・空行・TAB、
code/escape内の記法は保ちます。link・画像・参照は名前/代替textとURL・題名を可視textで表示し、
画像や参照先の取得はしません。

Backlog記法では `*` / `**` / `***` の見出し、`-` / `--` / `---` の箇条書き、
`+` の番号list、checklist、`''強調''` / `'''斜体'''` / `%%取消%%`、`>` / `{quote}` の引用、
通常表（末尾 `h` / cell先頭 `~` の見出しを含む）と `&br;` を表示用に変換します。
`{code}` / `{code:java}` 等のcode blockは区切りも内容も保持し、backslash escapeも誤変換しません。
認識できる `BLG-104` / `[[BLG-87]]` はキーと設定済みoriginの可視URLを表示します。
`[[URL]]` / `[[名前>URL]]` / `[[名前:URL]]` のHTTP(S) linkも可視textです。
Wiki/Git/SVN・添付/画像参照・目次は、名前・revision・ID等を原文のまま保持し、URLを推測しません。
HTMLのタグ/内容・未知macro/拡張は落とさず、複雑表や未閉鎖構文など難しい構造と未知の記法値は
安全な原文fallbackです（未知macro/HTMLを含む行は行全体を保持）。完全な記法互換は保証しません。
表示用copyだけを変換し、entity等の解釈後にも危険な制御文字は可視escape、既知API keyは伏字にします。
TTY/非TTY・NO_COLORの有無によらず、本文も常に色装飾なしです。

JSONは元の応答objectへ構築した `url` を追加し、未知field・大整数・通常の文字列値を保持します
（secret/危険制御文字の安全化は共通契約どおり）。JSONでは課題GETだけで、記法用project GETはしません。
必要fieldの欠落/型不正・不正な応答キーはinvalid_response（HTTP 200、exit 1）、
不正な入力ID/キーはinput（exit 2）です。取得/検証/表示構築がすべて成功してからstdoutへ書きます。
出力失敗やredaction key衝突はoutput（exit 1）として扱います。

## 課題作成

```sh
bl-cli issue create -p MYAPP --issue-type タスク --priority 中 -t "修正" -b "修正内容"
printf '修正内容\n' | bl-cli issue create -p MYAPP --issue-type タスク --priority 中 -t "修正" --body-file - --json
bl-cli issue create -p 123 --issue-type-id 7 --priority-id 3 -t "修正" --clear-assignee \
  --start-date 2024-02-29 --due-date 2024-03-01 --milestone-id 8 --milestone-id 9
```

project・タイトル・種別・優先度が必須です。projectはcwd設定、種別・優先度・担当者は
`issue.create` の既定値を使え、明示flagを優先します。タイトルは空白のみ不可（送信値はtrimしません）。
`--assignee/-a self` は選択profileの認証ユーザー、`--clear-assignee` は担当者既定値を適用せず、
assigneeId自体を送信しません。担当者の値指定と解除は排他です。状態・完了理由はAPIの初期値を使います。

名前flagと `--issue-type-id` / `--priority-id` / `--assignee-id` / `--milestone-id` は項目ごとに排他です。
名前は完全一致で解決し、不在・複数一致は入力エラー。IDは任意精度の正整数で、候補取得はしません。
projectキーだけはproject GETでIDへ解決します。`--project all` は作成では使えません。
`--milestone/-m` または `--milestone-id` は反復可能で、同じIDは一度だけ送ります。
他の単一flagの反復はエラー。開始日・期限日は実在する `YYYY-MM-DD` を検証し、前後関係の制限は加えません。

`--body/-b` と `--body-file` は空文字指定も含め排他です。UTF-8の本文を空白・改行・記法・制御文字も
変換せず送信します。空ファイル・空stdinは空本文、未指定ならdescriptionを送信しません。
`--body-file -` のときだけstdinを読みます。本文省略でeditorを起動せず、TTYでも確認待ちはありません。

POST `/api/v2/issues` をformで一回だけ送信し、milestoneは反復 `milestoneId[]` にします。
成功確認・表示用の追加GET、自動retry、redirect追従、結果の永続保存はしません。
成功はHTTP 201のobject・正整数id・安全なissueKeyを検証し、固定表示は `CREATED` / `URL`、
JSONは元objectへ検証済みorigin/keyからの `url` を追加します。未知field・巨大整数は保持し、表示時だけ安全化します。

識別できる作成エラーは、引数・設定・本文エラーも含め `write` が必須です（読取りエラーでは省略）。
送信前または具体的なBacklog errorsを伴う信頼できる4xx拒否は `not_created`、送信開始後の切断・timeout・
不正応答・5xx等は `unknown`、201観測後は本文読取・decode・必要field検証・出力失敗でも `created` を維持します。
中断時も通信taskを停止・回収して観測済み確度を報告します。固定エラーにも同じ確度を表示します。
終了コードは確度と独立で、createdでも後続エラーは1、中断は130です。実施済み・結果不明から再送を促しません。

```json
{"error":{"kind":"network","message":"Response body failed or timed out","httpStatus":201,"apiErrors":[],"write":{"operation":"issue.create","outcome":"created"}}}
```

## 課題の部分更新

```sh
bl-cli issue update MYAPP-123 -t "修正" -b "修正内容" --status 完了 --resolution-id 0
bl-cli issue update 123 --assignee self --milestone-id 8 --milestone-id 9 --json
printf '' | bl-cli issue update MYAPP-123 --body-file - --clear-due-date --clear-milestones
```

対象は正整数IDまたは課題キーです。最新の課題をGETし、明示入力と比較して異なる項目だけを
単一PATCH `/api/v2/issues/:issueIdOrKey` へform送信します。未指定項目は保持し、全く変更がなければ
input（exit 2、PATCHなし）です。projectは最新課題のprojectIdで判断し、cwdのproject・create既定値は
適用しません。`--project`・`--created`・`--updated` やコメント送信flagは受け付けません。
競合検出・自動mergeはなく、GET後に同じ項目が他者に変更される可能性は残ります。

更新項目は `--title/-t`・`--body/-b` / `--body-file`・`--issue-type`・`--priority`・
`--assignee/-a`・`--status/-s`・`--resolution`・`--start-date`・`--due-date`・`--milestone/-m` です。
名前と対応する `--issue-type-id` / `--priority-id` / `--assignee-id` / `--status-id` /
`--resolution-id` / `--milestone-id` は項目ごとに排他。名前は対象project（優先度・完了理由はspace）の
候補で完全一致解決し、不在・複数一致は入力エラーです。`--assignee self` は選択profileの認証ユーザー。
直接IDは候補GETなしで、任意精度の正整数を使います。resolution ID 0は値であり解除ではありません。

`--clear-assignee`・`--clear-resolution`・`--clear-start-date`・`--clear-due-date`・`--clear-milestones`
が明示解除で、対応する値指定と排他です。担当者・完了理由・日付は空値、milestoneは空の `milestoneId[]` を
送ります。milestoneは反復可能で指定集合へ置換し、重複IDは一度だけ送信、順番だけの変更は送信しません。
他の単一flagの反復は拒否します。タイトル・実在日付の検証は作成と同じで、日付の前後関係は制限しません。
応答の日付に時刻が付いていても、変更判定はカレンダー日付で比較します。

本文はUTF-8の原文（空白・改行・制御文字を含む）をtrim・変換せず送信します。
本文省略なら保持、明示空文字・空file・空stdinなら解除で、null本文と空本文は同値です。
`--body` / `--body-file` は空指定も含め排他、`--body-file -` だけがstdinを読みます。
TTYでもeditorや確認を起動せず、成功確認GET・retry・redirect追従・永続的な結果管理はしません。

HTTP 200の応答object・正整数id・安全なissueKeyを検証し、固定表示は `UPDATED` / `URL`、
JSONは元の応答objectへ検証済みorigin/keyからの `url` を追加します。未知field・大整数は保持し、表示時だけ安全化します。
識別できる更新エラーは引数・設定・本文・事前GET/名前解決を含め `write.operation: "issue.update"` を持ちます。
送信前または信頼できる具体的な4xx拒否は `not_updated`、送信開始後の成否不明は `unknown`、
200観測後は本文読取・decode・必須field検証・出力失敗・中断でも `updated` を保持します。
終了コードは確度と独立で、通常の後続エラーは1、処理可能なCtrl+C中断は130です。結果不明・更新済みから再送は促しません。

```json
{"error":{"kind":"network","message":"Response body failed or timed out","httpStatus":200,"apiErrors":[],"write":{"operation":"issue.update","outcome":"updated"}}}
```

## コメントの読取り

```sh
bl-cli issue comment list MYAPP-123
bl-cli issue comment list MYAPP-123 --limit 20 --max-id 456 --json
bl-cli issue comment list MYAPP-123 --min-id 100 --max-id 456 --json
bl-cli --profile work issue comment view MYAPP-123 234 --no-input
```

課題は `issue view` と同じ正整数IDまたは課題キー、コメントIDと `--min-id` / `--max-id` は
任意精度の正整数です。両境界指定時はmin < maxが必要です。limitは既定20・1〜100。
`count` / `order=desc` と指定した `minId` / `maxId` で降順の1ページだけ取得し、境界ID自体を除外します。
offset・total・コメント件数API・全件自動取得は使いません。`--project` は受け付けず、cwdのprojectで対象を置き換えません。

固定表示は1件ごとにID、COMMENT / CHANGE HISTORYの文字ラベル、投稿者、created/updated、本文と変更履歴を出します。
`content:null` の履歴も落とさず、本文と履歴の両方があれば同じ1件として両方表示します。
履歴の `limitDate` は DUE DATE、`assigner` は ASSIGNEEなどの表示名にし、未知fieldは識別子のまま、
変更前後の値も安全に表示します。空ページは `No comments` を出す成功で、API失敗と区別します。
固定表示に文字列本文がある場合だけ対象課題のprojectIdとproject記法を取得し、本文は課題詳細と同じMarkdown/Backlog変換・
安全な原文fallbackを使います。全件 `content:null` の履歴はコメントGETだけで表示します。
文字列本文が混在する場合は記法取得の失敗を隠しません。画像・参照先は取得しません。

JSONは一覧が `{"items":[...]}`、個別詳細は元のAPI objectです（url追加なし）。
本文・変更履歴・未知field・大整数は共通のsecret/制御文字保護以外は保持します。
JSONではコメントGETだけで、記法確認用のissue/project GETは行いません。
入力エラーはexit 2、取得・応答検証・表示/出力エラーはexit 1で、全構築成功後だけstdoutへ出します。

## コメント追加・課題項目の変更

```sh
bl-cli issue comment add MYAPP-123 -b "確認しました"
printf '対応内容\n' | bl-cli issue comment add MYAPP-123 --body-file - -s 完了 --resolution-id 0 --json
bl-cli issue comment add MYAPP-123 --assignee self --clear-resolution
```

`--body/-b` または `--body-file`、`--status/-s` / `--status-id`、`--assignee/-a` / `--assignee-id`、
`--resolution` / `--resolution-id`、`--clear-assignee` / `--clear-resolution` だけを受け付けます。
名前/ID/解除は項目ごとに排他、すべて単一指定です。IDは正整数、完了理由だけ0も有効です。
名前は完全一致（不在・複数一致は入力エラー）、`--assignee self` は選択profileの認証ユーザーです。
課題項目を指定した場合は対象課題の最新値と比較し、実変更だけ送ります。
名前解決には対象課題のprojectを使い、cwdのprojectやcreate既定値は適用しません。
状態変更に応じた完了理由の自動設定・解除も行いません。

本文はUTF-8原文の空白・改行・記法・制御文字をtrim/変換せず送ります。本文入力の両flagは空文字も含め排他、
空文字・空ファイル・空stdinは本文なしです。`--body-file -` のときだけstdinを読み、本文省略でeditorや確認待ちを起動しません。
本文も課題項目の実変更もなければinput（exit 2）として書き込みません。

- 課題項目の実変更なし（同値指定も含む）: POST `/api/v2/issues/:issueIdOrKey/comments` に `content=<本文>`。
  本文だけの場合は事前GETも行いません。成功は201と正整数コメントIDを検証し、固定表示は `CREATED` とID、
  JSONは `{"resource":"comment","data":<元のコメントobject>}` です。
- 実変更あり: PATCH `/api/v2/issues/:issueIdOrKey` に変更項目と、非空本文があれば `comment=<本文>`。
  解除値は空文字で送信します。成功は200と正整数課題ID・有効なissueKeyを検証し、固定表示は `UPDATED` と課題キー、
  JSONは `{"resource":"issue","data":<元の課題object>}` です。課題objectにurlを追加・上書きしません。

未知field・大整数・通常文字列は保持し、出力時に共通safetyを適用します。
書込みは一回だけで、二重書込み、権限不足時の本文POSTへのfallback、コメントID補完/成功確認の追加GET、retryはしません。
エラーは引数・設定・本文・事前GET・名前解決を含め `write.operation: "comment.add"` を持ちます。
方式未確定の送信前エラーは `not_created`、POST選択後は `not_created/unknown/created`、
PATCH選択後は `not_updated/unknown/updated` です。具体的な4xx拒否・不明応答の判定は課題作成/更新と同じです。
201/200観測後は本文読取・decode・必須field検証・出力失敗・中断でも `created/updated` を保持します。
終了コードは確度と独立し、成功0、入力エラー2、実行エラー1、中断130です。

## 既存コメントの本文更新

```sh
bl-cli issue comment update MYAPP-123 234 -b "修正したコメント"
printf '修正内容\n' | bl-cli issue comment update MYAPP-123 234 --body-file - --json
```

対象課題とコメントIDを指定し、最新コメントと認証ユーザーをGETして、自分が登録した既存本文だけを更新します。
他者のコメントと `content:null` の変更履歴は入力エラーで書き込みません。所有者確認に使う投稿者IDも正整数として検証します。
APIの権限拒否はそのまま報告し、回避や別方式へのfallbackはしません。

`--body/-b` または `--body-file` の明示入力が必須で、空文字指定も含め排他・単一指定です。
UTF-8原文の空白・改行はtrimせず、空白/改行だけの本文も保持します。空文字・空file・空stdin・同値更新は
input（exit 2、PATCHなし）です。明示 `--body-file -` だけがstdinを読み、editorや確認待ちは使いません。
課題項目・解除flag・project指定は受け付けず、cwdのprojectやcreate既定値は適用しません。

PATCH `/api/v2/issues/:issueIdOrKey/comments/:commentId` へ `content` だけをformで一回送ります。
成功はHTTP 200とコメントの必要fieldを検証し、固定表示は `UPDATED` とコメントIDだけ、JSONは元の応答objectです。
urlやresource wrapperは追加せず、未知field・巨大整数・通常文字列は保持し、出力時に共通safetyを適用します。
成功確認/表示用の追加GET・retry・redirect追従・結果の永続保存はしません。
最新取得後の競合検出・自動mergeはなく、同じ本文が他者に変更される可能性は残ります。

引数・設定・本文・事前GETも含め、識別できるエラーは `write.operation: "comment.update"` を持ちます。
送信前または信頼できる具体的な4xx拒否は `not_updated`、送信開始後の成否不明は `unknown`、
200観測後は本文読取・decode・必要field検証・出力失敗・中断でも `updated` を保持します。
終了コードは確度と独立で、成功0、入力エラー2、実行エラー1、中断130です。結果不明・更新済みから再送は促しません。

## 共通libraryの候補取得・名前解決

CLI/TUIから共通に `service::Service::new(&Config)` を使えます（生成だけでは通信しません）。
`candidates(Candidate)` は元のAPI配列、`myself()` は認証ユーザーの元objectを返します。
`Candidate` は `IssueType` / `Status` / `Assignee` / `Milestone`（各 `project: &str`）と、
space全体の `Priority` / `Resolution` です。milestoneはversions APIを使います。

`resolve(Candidate, Selection)` は `serde_json::Number` のIDを返し、巨大なIDを縮小しません。
`Selection::Name("123")` は数値の名前として完全一致し、`Selection::Id("123")` と区別します。
候補不在・同名の複数一致はinputエラー（exit 2）。IDは正整数、Resolutionだけ0も許可し、
0を未指定/解除と扱いません。直接IDでは候補API・project取得・所属確認を呼びません。
`Assignee` の `Selection::Name("self")` だけは選択profileのmyselfを使い、projectユーザー一覧は取得しません。
候補やmyselfは呼出し時だけ取得し、永続/メモリcacheは持ちません。API失敗を空候補やinputエラーに置き換えません。
返却データは表示用に変更せず、表示側で共通 `safety::json/text` に選択profileのkeyを渡して安全化します。
`search(&Settings, IssueSearch)` は通常・横断・mineの共通入口で、CLIと同じ入力検証・既定値・
名前解決・重複除去・count/listを適用し、上記の課題検索JSONと同じ構造の `Value` を返します。
`IssueSearch::default()` は未指定project/keyword/各条件、mine=false、limit=20、offset="0"。
条件は `Vec<Selection>`（名前とIDは項目ごとに排他）、statusの空Vecは設定を使い、
`Selection::Name("all")` は設定の状態条件を解除します。mine=trueはassignee入力と排他です。
offsetは任意精度を保つ10進文字列で受け取り、返却値は数値です。返却データは安全表示前の原文です。

`issue_view(&str)` は課題詳細の必要fieldを検証し、元objectへ `url` を追加した `Value` を返します。
返却値はsecret/制御文字を含む原文のままで、表示用コピーとは分離します。
`project_text_formatting_rule(&Number)` は正整数project IDの記法を取得し、文字列で返します。
課題取得と分離しているため、表示側が必要な場合だけ呼びます。
`comment_list(&str, limit: u8, min_id: Option<&str>, max_id: Option<&str>)` は上記の1ページの
`{"items":[...]}`、`comment_view(issue_id_or_key: &str, comment_id: &str)` は元objectを返します。
入力と表示に必要な応答fieldを検証し、未知fieldや本文・履歴は表示前の原文で保持します。
`comment_page(issue_id_or_key, limit, min_id, max_id, CommentOrder::Asc/Desc)` は同じ検証と返却形式で順序を選べます。
TUIの隣接取得／最古取得に使い、CLI用`comment_list`の降順固定契約は変えません。
`issue_text_formatting_rule(&str)` は対象課題のprojectIdだけを取得・検証し、既存のproject記法取得へ渡します。
コメントJSON取得と独立しており、cwdのprojectや課題詳細の他fieldを要求しません。
`issue_create(&Settings, IssueCreate)` はCLIと共通の作成入口です。`IssueCreate` は `project: Option<&str>`、
`title: &str`、`body: Option<&str>`、`issue_type/priority/assignee: Option<Selection>`、
`milestone: Vec<Selection>`、`clear_assignee: bool`、`start_date/due_date: Option<&str>` を受け取ります。
`Default` は全項目未指定（titleは空文字）で、必須値は呼出し時に検証します。
既定値・入力検証・名前解決・重複除去・単一POST・確度判定を共通で行い、元objectとurlの `Value` を返します。
返却値と送信本文はsecret/制御文字を含む原文のままで、表示の安全化とは分離しています。
`issue_update(issue_id_or_key: &str, IssueUpdate)` は最新取得・変更判定・名前解決・単一PATCH・確度判定の共通入口です。
`IssueUpdate` は `title/body/start_date/due_date: Option<&str>`、
`issue_type/priority/status/assignee/resolution: Option<Selection>`、`milestone: Vec<Selection>` と
`clear_assignee/clear_resolution/clear_start_date/clear_due_date/clear_milestones: bool` を受け取ります。
`Default` は全項目未指定です。設定引数は持たず、名前解決に使うprojectは対象課題から取得します。
比較に使うID・本文・日付などを検証し、使用しない表示名や未指定項目は要求しません。
返却値は作成同様に原文objectと構築urlの `Value` です。
`comment_add(issue_id_or_key: &str, CommentAdd)` は上記の追加操作の共通入口です。
`CommentAdd` は `body: Option<&str>`、`status/assignee/resolution: Option<Selection>` と
`clear_assignee/clear_resolution: bool` を受け取り、`Default` は全項目未指定です。
設定引数は持たず、必要な最新値取得・名前解決・差分比較と単一POST/PATCH・確度判定を行います。
返却 `Value` は `resource/data` の形で、dataは安全化前の元object（url追加なし）です。
`comment_update(issue_id_or_key: &str, comment_id: &str, body: &str)` は既存本文更新の共通入口です。
必須本文・対象IDの検証、最新コメント/認証ユーザー取得、投稿者IDの検証・所有者確認、同値/履歴の拒否と単一PATCHを行います。
設定引数は持たず、返却 `Value` は安全化前の元コメントobject（url追加なし）です。
失敗時は `Error::write` の `WriteResult { operation, outcome: WriteOutcome }` で確度を取得できます。
`WriteOutcome` は `NotCreated/Unknown/Created/NotUpdated/Updated`、従来の `CreateOutcome` 名は同じenumの再exportです。
作成エラーのJSON語彙・`with_create_outcome` の動作は維持しています。

共通 `body::display(raw, text_formatting_rule, secrets)` が本文表示入口です。
Markdownはpulldown-cmark、Backlog記法は主要構造の変換と原文fallbackを使い、最後に共通safetyを適用します。
`body::display_with_origin(raw, text_formatting_rule, &origin, secrets)` は課題link用のHTTPS originも受け取ります。
CLIは検証済み `Config::origin` を渡します。origin不正時や従来の `display` では課題参照を原文のまま保持します。
原文/API JSONは変更しません。未知の記法値は安全な原文fallbackです。

開発環境・ローカルHTTPS試験・品質確認は [CONTRIBUTING.md](CONTRIBUTING.md)、
実装方針は [承認済みPlan](docs/plans/backlog-cli-initial.md) を参照してください。
