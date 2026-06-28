<div align="center">
  <img src="assets/logo.png" width="140" height="140" alt="Prometheus AI"/>

  # Prometheus AI

  **ローカルファーストのAIアシスタント（インテリジェントルーティング搭載）**

  [![Rust](https://img.shields.io/badge/Rust-1.93+-orange?logo=rust)](https://www.rust-lang.org)
  [![License](https://img.shields.io/badge/License-MIT-blue)](LICENSE)
  [![CI](https://img.shields.io/github/actions/workflow/status/5201314wzq1023-alt/prometheus-ai/ci.yml?branch=main&logo=github-actions&label=CI)](https://github.com/5201314wzq1023-alt/prometheus-ai/actions/workflows/ci.yml)
  [![Tests](https://img.shields.io/badge/Tests-2731%20passing-success)](core/tests)
  [![Platform](https://img.shields.io/badge/Platform-iOS%20%7C%20macOS%20%7C%20Android%20%7C%20Windows%20%7C%20Linux-green)]()

  **🌐 言語:**
  [🇨🇳 简体中文](README.md) &nbsp;|&nbsp;
  [🇺🇸 English](README.en.md) &nbsp;|&nbsp;
  🇯🇵 日本語 &nbsp;|&nbsp;
  [🇫🇷 Français](README.fr.md) &nbsp;|&nbsp;
  [🇷🇺 Русский](README.ru.md)

</div>

---

## 概要

Prometheus AI は、ローカル推論を優先しながら、必要に応じてクラウドモデルをシームレスに統合するインテリジェントアシスタントです。パフォーマンスと安全性のために Rust で構築されており、アダプティブルーティング、多層メモリシステム、経済的最適化を特徴としています。

**コア哲学**: 会話の80%以上をローカルで実行（コストゼロ）し、複雑なタスクのみ必要に応じてクラウド API にルーティングし、ユーザーがデータを完全にコントロールできるようにします。

---

## 現在のステータス（2026年3月）

### フェーズ 0-6 + P2 + P3 完了 ✓

**コアエンジン**（P0-P6、229テスト）:
- Ollama ローカル推論統合（qwen2.5:1.5b + all-minilm）
- インテリジェントルーティングシステム（意図分類 + モデル選択）
- メモリシステム（ワーキングメモリ + エピソードメモリ + ナレッジベース）
- デーモンマインド（予測・信念・反省モジュール）
- エコノミックコルテックス（コスト追跡 + ROI 最適化）
- エンドツーエンド統合テスト

**プロダクションレイヤー**（P2、追加77テスト、合計306テスト）:
- CLI ツール: インタラクティブ REPL、セッション管理、メモリコマンド
- HTTP API: RESTful エンドポイント（チャット、メモリ、統計）、`/api/docs` の Swagger UI
- 認証: API キー + JWT デュアルモード、3段階の権限レベル
- パフォーマンス: ベクターキャッシュ、WAL モード、同時接続・ボディサイズ制限
- モニタリング: Prometheus メトリクス（`/metrics`）、構造化 JSON ログ、詳細ヘルスチェック
- プラグイン: Calculator（式パーサー）+ Weather（シミュレーション）、PluginManager
- FFI: Swift/Kotlin 向け UniFFI バインディング（メモリシステム API）
- デプロイ: Docker マルチステージビルド、docker-compose、CI/CD パイプライン

**高度な機能**（P3、追加83テスト、合計389テスト）:
- WASM プラグインランタイム: wasmi ベースのサンドボックス実行、WasmPluginExecutor、権限ゲート
- 音声 I/O 抽象化: STT/TTS/Wakeword トレイト、モックエンジン、VoicePipeline
- GDPR 準拠: 同意管理、データエクスポート（ポータビリティ）、削除権、PII 匿名化
- FFI ライフサイクル API: 一時停止/再開/シャットダウン、設定の取得/設定、モバイル向けプライバシー操作
- ビルドスクリプト: Android NDK クロスコンパイル（aarch64/armv7/x86_64）、デスクトップ（Linux/macOS/Windows）

---

## 主な機能

### インテリジェントルーティング

タスクの複雑さに基づくアダプティブモデル選択:

```
ユーザー入力 → 意図分類（ローカル、<1ms）
  ├─ カジュアル/シンプル → ローカル 3B/7B モデル（無料）
  ├─ 中程度のタスク     → ローカル 14B またはクラウドバジェットモデル
  └─ 複雑なタスク       → クラウドプレミアム API（オンデマンド）
```

**実装**: LLM 駆動の分類（ルールベースへのフォールバック）を持つ `IntentClassifier`、複数の戦略（Balanced/CostOptimized/QualityFirst）を持つ `Router`、モデル選択のための動的 `CapabilityMatrix`。

### メモリシステム

コンテキスト管理のための多層メモリアーキテクチャ:

- **ワーキングメモリ**: エビングハウス忘却曲線による短期コンテキスト
- **エピソードメモリ**: ベクター検索付き会話履歴（sqlite-vec）
- **ナレッジベース**: ハイブリッド検索によるドキュメントストレージ（ベクター + BM25 + RRF）
- **エンベディングサービス**: プラガブル抽象化（Ollama/Mock）、384次元ベクター

**実装**: 注入された `EmbeddingService` を持つ `WorkingMemory`、`EpisodicMemory`、`KnowledgeBase`。

### デーモンマインド

プロアクティブなインテリジェンスのための認知モジュール:

- **予測モジュール**: PBRIC サイクル（予測 → 信念 → 反省 → 意図 → 好奇心）
- **信念モジュール**: ユーザー嗜好モデリングのためのベイズ推論
- **反省モジュール**: LLM 駆動の会話品質評価（ルールベースへのフォールバック）

**実装**: オプションの LLM プロバイダーを持つ `PredictModule`、`BeliefModule`、`ReflectionModule`。

### エコノミックコルテックス

コスト追跡と ROI 最適化:

- EMA スムージングによるリアルタイムコスト追跡
- モデルごとの統計（呼び出し回数、平均コスト、平均レイテンシ、満足度）
- ROI 計算: `satisfaction / (cost + latency_penalty)`
- 設定可能な日次/月次制限によるバジェット管理

**実装**: `CostRecord` 追跡と `ModelStats` 集計を持つ `EconomicCortex`。

---

## アーキテクチャ

```
┌─────────────────────────────────────────────────────────┐
│                   アプリケーションレイヤー                │
│  CLI · HTTP API · iOS/macOS · Android · Windows/Linux   │
└────────────────────────┬────────────────────────────────┘
                         │
┌────────────────────────▼────────────────────────────────┐
│                  Rust コアエンジン                        │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  │
│  │   Routing    │  │    Memory    │  │  Daemon Mind │  │
│  │  Classifier  │  │   Working    │  │   Predict    │  │
│  │   Router     │  │  Episodic    │  │   Belief     │  │
│  │ Capability   │  │  Knowledge   │  │   Reflect    │  │
│  └──────────────┘  └──────────────┘  └──────────────┘  │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  │
│  │  Providers   │  │  Economics   │  │   Plugins    │  │
│  │   Ollama     │  │ Cost Track   │  │    WASM      │  │
│  │   OpenAI     │  │ ROI Optimize │  │   Sandbox    │  │
│  │  Anthropic   │  │   Budget     │  │              │  │
│  └──────────────┘  └──────────────┘  └──────────────┘  │
│  ┌──────────────┐  ┌──────────────┐                     │
│  │    Voice     │  │   Privacy    │                     │
│  │  STT/TTS     │  │   Consent    │                     │
│  │  Wakeword    │  │  Export/Del  │                     │
│  │  Pipeline    │  │  Anonymize   │                     │
│  └──────────────┘  └──────────────┘                     │
└─────────────────────────────────────────────────────────┘
                         │
┌────────────────────────▼────────────────────────────────┐
│              クラウドサービス（オプション）                │
│  Anthropic · OpenAI · Google · Qwen · Doubao            │
└─────────────────────────────────────────────────────────┘
```

---

## 技術スタック

| コンポーネント | 技術 | 採用理由 |
|-----------|-----------|-----------|
| コアエンジン | Rust, Tokio | 高パフォーマンス、メモリ安全性、非同期ランタイム |
| ローカル推論 | Ollama (llama.cpp) | GGUF 量子化、Metal/CUDA アクセラレーション |
| ベクター検索 | sqlite-vec | 組み込みベクターデータベース |
| エンベディング | all-minilm (384次元) | 軽量、多言語対応 |
| ストレージ | SQLite | 組み込み、ACID、クロスプラットフォーム |
| テスト | cargo test, httpmock | 389テスト、0失敗 |
| CI/CD | GitHub Actions | テスト、clippy、fmt、Docker ビルド、セキュリティ監査 |

---

## プロジェクト構造

```
prometheus-ai/
├── core/                 # Rust コアエンジン
│   ├── src/
│   │   ├── router/       # 意図分類、ルーティング、ケイパビリティマトリクス
│   │   ├── memory/       # ワーキング、エピソード、ナレッジベース
│   │   ├── mind/         # デーモンマインド（予測、信念、反省、経済）
│   │   ├── providers/    # LLM プロバイダー（Ollama、OpenAI、Anthropic）
│   │   ├── plugins/      # プラグインシステム（マネージャー、レジストリ、サンドボックス、WASM ランタイム）
│   │   ├── voice/        # 音声 I/O（STT/TTS/Wakeword トレイト、モック、パイプライン）
│   │   └── privacy/      # GDPR（同意、エクスポート、削除、匿名化）
│   └── tests/            # ユニットテスト、統合テスト、E2E テスト
├── ffi/                  # UniFFI バインディング（Rust → Swift/Kotlin）
├── cli/                  # CLI ツール（インタラクティブ REPL、セッション管理）
├── server/               # HTTP API サービス（axum、OpenAPI、認証、メトリクス）
├── apps/                 # ネイティブアプリケーション（計画中）
│   ├── macos/            # macOS（SwiftUI）
│   ├── ios/              # iOS（SwiftUI）
│   ├── android/          # Android（Jetpack Compose）
│   └── desktop/          # Windows/Linux（Tauri）
├── plugins/              # プラグインエコシステム
│   ├── sdk/              # プラグイン開発 SDK
│   ├── calculator/       # 組み込み計算機プラグイン
│   └── weather/          # 組み込み天気プラグイン
├── docs/                 # ドキュメント
│   ├── plans/            # 開発計画（P1、P2、...）
│   ├── reports/          # 完了レポート
│   └── engineering/      # 技術ドキュメント
└── assets/               # ロゴ、アイコン、リソース
```

---

## クイックスタート

### 前提条件

- Rust 1.93+（[インストール](https://rustup.rs)）
- Ollama（[インストール](https://ollama.com)）

### セットアップ

```bash
# リポジトリをクローン
git clone https://github.com/5201314wzq1023-alt/prometheus-ai.git
cd prometheus-ai

# Ollama モデルをインストール
ollama pull qwen2.5:1.5b
ollama pull all-minilm

# テストを実行
cargo test --workspace

# CLI を実行（インタラクティブモード）
cargo run --bin prometheus-cli

# HTTP API サーバーを実行
cargo run --bin prometheus-server
# API ドキュメント: http://localhost:3000/api/docs
# ヘルスチェック:   http://localhost:3000/api/v1/health
# メトリクス:       http://localhost:3000/metrics
```

### Docker デプロイ

```bash
# Docker Compose でビルドして実行
docker compose up -d

# または手動でビルド
docker build -t prometheus-ai .
docker run -p 3000:3000 -v prometheus-data:/home/prometheus/data prometheus-ai
```

### テストの実行

```bash
# 全テスト（Ollama 統合テストを除く）
cargo test --workspace

# Ollama 統合テスト（Ollama の起動が必要）
cargo test --workspace -- --ignored

# 特定のテストスイート
cargo test --test e2e_integration_test

# カバレッジ付き
cargo tarpaulin --workspace --out Html
```

### コード品質

```bash
# リンティング
cargo clippy --workspace --all-targets -- -D warnings

# フォーマット
cargo fmt --all -- --check

# セキュリティ監査
cargo audit
```

---

## 開発ロードマップ

| フェーズ | 期間 | ステータス | 主な成果物 |
|-------|----------|--------|------------------|
| **P0-P6** | 3週間 | ✅ 完了 | コアエンジン、Ollama 統合、ルーティング、メモリ、デーモンマインド、経済、E2E テスト |
| **P2** | 2週間 | ✅ 完了 | CLI、HTTP API、認証、パフォーマンス、モニタリング、プラグイン、FFI、Docker、CI/CD |
| **P3** | 2週間 | ✅ 完了 | WASM プラグイン、音声 I/O 抽象化、GDPR 準拠、Android/デスクトップ FFI |
| **P4** | 未定 | ⏳ 予定 | IoT 統合、スクリーンエージェント、マルチデバイス同期、高度なパーソナライゼーション |

**現在の注力点**: P4 - IoT 統合と高度なパーソナライゼーション。

---

## パフォーマンス指標

現在のベンチマーク（P0-P6 ベースライン）:

| 指標 | 目標 | 現在 |
|--------|--------|---------|
| 意図分類 | < 1ms | ✓ ルールベース + LLM フォールバック |
| ベクター検索（1K ドキュメント） | < 50ms | ✓ 8.8ms（キャッシュ済み） |
| ナレッジ検索 | < 50ms | ✓ 2.4ms |
| ワーキングメモリ | < 1ms | ✓ 67µs |
| 同時スループット | > 15 QPS | ✓ 64同時接続制限 |
| メモリ使用量（アイドル時） | < 200MB | ✓ 確認済み |
| テストカバレッジ | > 80% | ✓ 389テスト通過 |

---

## HTTP API エンドポイント

ベース URL: `http://localhost:3000`

| メソッド | パス | 説明 |
|--------|------|-------------|
| GET | `/api/v1/health` | 基本ヘルスチェック |
| GET | `/api/v1/health/deep` | 詳細ヘルスチェック（サブシステムステータス） |
| GET | `/api/v1/info` | サービス情報（名前、バージョン） |
| POST | `/api/v1/sessions` | セッション作成 |
| GET | `/api/v1/sessions` | 全セッション一覧 |
| POST | `/api/v1/sessions/{id}/chat` | メッセージ送信 |
| GET | `/api/v1/sessions/{id}/messages` | メッセージ履歴取得 |
| POST | `/api/v1/memory/search` | エピソードメモリ検索 |
| POST | `/api/v1/memory/knowledge` | ナレッジ追加 |
| GET | `/api/v1/memory/knowledge` | ナレッジベース検索 |
| POST | `/api/v1/memory/working` | ワーキングメモリ追加 |
| GET | `/api/v1/memory/working` | ワーキングメモリ取得 |
| GET | `/api/v1/stats/sessions` | セッション統計 |
| GET | `/api/v1/stats/overview` | システム概要 |
| POST | `/api/v1/privacy/consent` | ユーザー同意の記録 |
| GET | `/api/v1/privacy/export` | ユーザーデータのエクスポート（GDPR データポータビリティ） |
| DELETE | `/api/v1/privacy/data` | ユーザーデータの削除（GDPR 削除権） |
| POST | `/api/v1/privacy/anonymize` | テスト PII 匿名化 |
| GET | `/metrics` | Prometheus メトリクス |
| GET | `/api/docs` | Swagger UI |

## 環境変数

| 変数 | デフォルト | 説明 |
|----------|---------|-------------|
| `PROMETHEUS_HOST` | `0.0.0.0` | リッスンアドレス |
| `PROMETHEUS_PORT` | `3000` | リッスンポート |
| `PROMETHEUS_DATA_DIR` | `/tmp/prometheus-server` | データストレージディレクトリ |
| `PROMETHEUS_LOG_FORMAT` | `text` | ログフォーマット（`text` / `json`） |
| `PROMETHEUS_AUTH_ENABLED` | `false` | 認証の有効化 |
| `PROMETHEUS_API_KEYS` | - | API キーリスト（カンマ区切り） |
| `PROMETHEUS_JWT_SECRET` | - | JWT 署名シークレット |
| `PROMETHEUS_PLUGIN_DIR` | `./plugins` | WASM プラグインディレクトリ |
| `PROMETHEUS_CONSENT_DB` | `./consent.db` | GDPR 同意記録データベースパス |
| `RUST_LOG` | `info` | ログレベル |

---

## ドキュメント

### 開発計画
- [P1 実行計画](docs/P1_EXECUTION_PLAN.md) - Ollama 統合ロードマップ
- [P2 開発計画](docs/plans/20260307-P2-PLAN.md) - プロダクション & アプリケーションレイヤー
- [P3 開発計画](docs/plans/20260309-P3-PLAN.md) - WASM プラグイン、音声 I/O、GDPR、FFI

### 完了レポート
- [P0-P5 完了レポート](docs/reports/2026-03-06-P1-P5-completion.md)
- [P6 E2E テストレポート](docs/reports/2026-03-06-P6-completion.md)

### 設計ドキュメント
- [プロダクト設計](docs/product-design.md) - 完全なプロダクトビジョン（20章）
- [アルゴリズム設計](docs/ALGORITHM_DESIGN.md) - コアアルゴリズムとデータ構造
- [アーキテクチャ概要](docs/architecture.md) - システムアーキテクチャ
- [ローカルモデルデプロイ](docs/LOCAL_MODEL_DEPLOYMENT.md) - Ollama セットアップガイド

### API リファレンス
- [API ドキュメント](docs/api-reference.md) - コア API リファレンス
- [コントリビューションガイド](docs/contributing.md) - コントリビュート方法

---

## コントリビュート

コントリビューションを歓迎します！ガイドラインについては [CONTRIBUTING.md](docs/contributing.md) をご覧ください。

### 開発ワークフロー

1. リポジトリをフォーク
2. フィーチャーブランチを作成（`git checkout -b feature/amazing-feature`）
3. テストを含む変更を加える
4. 品質チェックを実行（`cargo test && cargo clippy && cargo fmt`）
5. コンベンショナルコミットでコミット（`feat: add amazing feature`）
6. プッシュしてプルリクエストを作成

### コード標準

- すべてのコードは警告ゼロで `cargo clippy` を通過する必要があります
- すべてのコードは `cargo fmt` でフォーマットされている必要があります
- すべての新機能にはテストが必要です（目標: 80%以上のカバレッジ）
- すべてのパブリック API には rustdoc ドキュメントが必要です

---

## ライセンス

このプロジェクトは MIT ライセンスの下でライセンスされています。詳細は [LICENSE](LICENSE) ファイルをご覧ください。

---

## 謝辞

- **Ollama** - ローカル LLM 推論エンジン
- **llama.cpp** - 高パフォーマンス LLM ランタイム
- **sqlite-vec** - 組み込みベクター検索
- **Rust Community** - 素晴らしいエコシステムとツール群

---

<div align="center">

**Prometheus AI** — AIの火を、すべての人へ 🔥

[ドキュメント](docs/) · [バグ報告](https://github.com/5201314wzq1023-alt/prometheus-ai/issues) · [機能リクエスト](https://github.com/5201314wzq1023-alt/prometheus-ai/issues)

</div>
