# Issue tracker

## Tracker
GitHub Issues

## Repository
minorunakamura/backlog-cli
https://github.com/minorunakamura/backlog-cli

## 作成・取得
- 公開gh CLIを使い、常に --repo minorunakamura/backlog-cli を指定する。
- 作成: gh issue create --repo minorunakamura/backlog-cli --title <title> --body <body> --label ready-for-agent
- 取得: gh issue view <number-or-url> --repo minorunakamura/backlog-cli --json number,url,title,body,comments,labels,state
- ready-for-agentを使用する。ラベル不足時は、その1つだけを別途承認後に準備する。

## 依存関係
blockerを先に作成し、本文のBlocked byへ実Issueリンクを記す。
利用可能なnative blocking relationshipも併用する。
機能非対応の場合だけ本文リンクで代替し、認証・権限・通信失敗では停止する。
元Issueの本文・状態は変更しない。
