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
候補であり、まだ依存に追加していません。設定/API/TUIの機能は後続Work Itemの範囲です。

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
GitHub上で未実行ならその旨を記録し、確認のためだけのpushやPR作成は行いません。

## テストと今回の範囲

承認済み [Plan](docs/plans/backlog-cli-initial.md) と
[基本設計](docs/designs/backlog-cli.md) を参照してください。
公開境界を一つの失敗テスト→最小実装の縦sliceで検証します。
初期基盤の `tests/cli_help_version.rs` は、実CLIプロセスのargv/stdin/stdout/stderr/exitを
境界に、環境変数を消し、空のHOME/XDG_CONFIG_HOME/cwdと非TTYでhelp/versionを確認します。
内部private関数の名前や実装詳細はテストに固定しません。

現段階ではビルド・品質確認・help/versionのみを対象とします。実Backlogへの書込み、
実IME/TUI受入、配布tar.gz/SHA-256作成や公開は行いません。
