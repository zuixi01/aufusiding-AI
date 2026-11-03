# aufusiding-AI

**Rust によるローカルファースト AI アシスタント · Early Alpha**

[简体中文](README.md) · [English](README.en.md) · 日本語 · [Français](README.fr.md)

[![CI](https://img.shields.io/github/actions/workflow/status/zuixi01/aufusiding-AI/ci.yml?branch=main&label=CI)](https://github.com/zuixi01/aufusiding-AI/actions/workflows/ci.yml)
[![MIT License](https://img.shields.io/badge/license-MIT-blue)](LICENSE)

aufusiding-AI は、ローカルでの対話、セッション保存、モデルルーティングを実験するプロジェクトです。crate、実行ファイル、設定ディレクトリ、アプリ内では **Prometheus AI** という名前を引き続き使用しています。CLI の実行ファイル名は `prometheus` です。

現在は開発者向けの初期 Alpha です。まず CLI + Ollama を試してください。HTTP、デスクトップ、モバイル、音声、IoT の統合状況はそれぞれ異なり、模擬データを使う部分や追加実装が必要な部分があります。API とデータ形式は変更される可能性があります。本番運用、法令への適合、性能やコスト削減、全プラットフォームの動作は保証していません。

## クイックスタート

Git、rustup（リポジトリで固定した Rust **1.99.1**）、ネイティブ C/C++ ビルドツール、[Ollama](https://ollama.com/) が必要です。CLI に Node.js、Bun、Docker は不要です。固定バージョンは現在の開発基準であり、検証済みの最低 Rust バージョンではありません。

Ollama アプリを起動するか、別のターミナルで `ollama serve` を実行してから、次を実行します。

```bash
git clone https://github.com/zuixi01/aufusiding-AI.git
cd aufusiding-AI
ollama pull qwen2.5:1.5b
ollama list
cargo run --locked -p prometheus-cli -- --help
cargo run --locked -p prometheus-cli -- ask "こんにちは。一文で自己紹介してください。" --model qwen2.5:1.5b
cargo run --locked -p prometheus-cli -- chat
```

REPL では `/switch qwen2.5:1.5b` でインストール済みモデルを選び、`/help` でコマンドを表示、`/quit` で終了します。Ollama の既定 URL は `http://localhost:11434` です。`PROMETHEUS_PROVIDER_OLLAMA_URL` で変更できます。モデルの重みは同梱していません。

モデルを使わずに CLI を試す場合は、模擬モードを明示的に選択します。

```bash
cargo run --locked -p prometheus-cli -- --mock ask "こんにちは"
```

模擬モードの応答は推論結果ではありません。`model list` は固定リスト、`model test` は未完成のコマンドです。実際のインストール状態は `ollama list` で確認してください。

## 設定と貢献

```bash
cargo run --locked -p prometheus-cli -- config list
cargo run --locked -p prometheus-cli -- config set default_model qwen2.5:1.5b
cargo fmt --all -- --check
cargo check --locked -p prometheus-cli
cargo test --locked -p prometheus-cli
```

設定ファイルとデータディレクトリは `config list` で確認できます。保存された設定の一部は、まだ推論処理に接続されていません。会話と REPL 履歴はローカルに保存されるため、ログやエクスポートを共有する前に内容を確認してください。デスクトップとモバイルは実験段階で、各プラットフォームのツールチェーンと追加の動作確認が必要です。

- [設定・HTTP・トラブルシューティング](docs/getting-started.md)（中国語）
- [機能の実装状況と制限](docs/project-status.md)（中国語）
- [アーキテクチャ](docs/architecture.md) · [API リファレンス](docs/api-reference.md)
- [貢献ガイド](CONTRIBUTING.md) · [行動規範](CODE_OF_CONDUCT.md)
- [セキュリティポリシー](SECURITY.md) · [Issue の報告](https://github.com/zuixi01/aufusiding-AI/issues/new/choose)

コードは [MIT License](LICENSE) で公開されています。外部モデル、サービス、データ、依存関係には、それぞれのライセンスと利用条件が適用されます。
