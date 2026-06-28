<div align="center">
  <img src="assets/logo.png" width="140" height="140" alt="Prometheus AI"/>

  # Prometheus AI

  **Assistant IA Local-First avec Routage Intelligent**

  [![Rust](https://img.shields.io/badge/Rust-1.93+-orange?logo=rust)](https://www.rust-lang.org)
  [![License](https://img.shields.io/badge/License-MIT-blue)](LICENSE)
  [![CI](https://img.shields.io/github/actions/workflow/status/5201314wzq1023-alt/prometheus-ai/ci.yml?branch=main&logo=github-actions&label=CI)](https://github.com/5201314wzq1023-alt/prometheus-ai/actions/workflows/ci.yml)
  [![Tests](https://img.shields.io/badge/Tests-2731%20passing-success)](core/tests)
  [![Platform](https://img.shields.io/badge/Platform-iOS%20%7C%20macOS%20%7C%20Android%20%7C%20Windows%20%7C%20Linux-green)]()

  **🌐 Langue:**
  [🇨🇳 简体中文](README.md) &nbsp;|&nbsp;
  [🇺🇸 English](README.en.md) &nbsp;|&nbsp;
  [🇯🇵 日本語](README.ja.md) &nbsp;|&nbsp;
  🇫🇷 Français &nbsp;|&nbsp;
  [🇷🇺 Русский](README.ru.md)

</div>

---

## Présentation

Prometheus AI est un assistant intelligent qui privilégie l'inférence locale tout en intégrant de manière transparente des modèles cloud lorsque nécessaire. Développé en Rust pour la performance et la sécurité, il propose un routage adaptatif, des systèmes de mémoire multicouches et une optimisation économique.

**Philosophie fondamentale** : Exécuter plus de 80 % des conversations en local (coût zéro), acheminer les tâches complexes vers des API cloud uniquement si nécessaire, et donner aux utilisateurs un contrôle total sur leurs données.

---

## État actuel (mars 2026)

### Phases 0-6 + P2 + P3 terminées ✓

**Moteur principal** (P0-P6, 229 tests) :
- Intégration de l'inférence locale Ollama (qwen2.5:1.5b + all-minilm)
- Système de routage intelligent (classification d'intention + sélection de modèle)
- Systèmes de mémoire (mémoire de travail + mémoire épisodique + base de connaissances)
- Daemon Mind (modules de prédiction, de croyance et de réflexion)
- Cortex économique (suivi des coûts + optimisation du ROI)
- Tests d'intégration de bout en bout

**Couche de production** (P2, 77 tests supplémentaires, 306 au total) :
- Outil CLI : REPL interactif, gestion de sessions, commandes mémoire
- API HTTP : points de terminaison RESTful (chat, mémoire, statistiques), interface Swagger à `/api/docs`
- Authentification : double mode Clé API + JWT, trois niveaux de permissions
- Performance : cache vectoriel, mode WAL, limites de concurrence et de corps de requête
- Supervision : métriques Prometheus (`/metrics`), journalisation JSON structurée, vérifications de santé approfondies
- Plugins : Calculatrice (analyseur d'expressions) + Météo (simulée), PluginManager
- FFI : liaisons UniFFI avec API du système de mémoire pour Swift/Kotlin
- Déploiement : build Docker multi-étapes, docker-compose, pipelines CI/CD

**Fonctionnalités avancées** (P3, 83 tests supplémentaires, 389 au total) :
- Runtime de plugins WASM : exécution en bac à sable basée sur wasmi, WasmPluginExecutor, contrôlé par permissions
- Abstraction voix E/S : traits STT/TTS/Wakeword, moteurs simulés, VoicePipeline
- Conformité RGPD : gestion du consentement, export de données (portabilité), droit à l'effacement, anonymisation des données personnelles
- API de cycle de vie FFI : pause/reprise/arrêt, lecture/écriture des paramètres, opérations de confidentialité pour mobile
- Scripts de build : compilation croisée Android NDK (aarch64/armv7/x86_64), bureau (Linux/macOS/Windows)

---

## Fonctionnalités clés

### Routage intelligent

Sélection adaptative du modèle selon la complexité de la tâche :

```
Entrée utilisateur → Classification d'intention (local, <1ms)
  ├─ Casual/Simple → Modèle local 3B/7B (gratuit)
  ├─ Tâches moyennes  → Local 14B ou modèle cloud économique
  └─ Tâches complexes → API cloud premium (à la demande)
```

**Implémentation** : `IntentClassifier` avec classification pilotée par LLM (repli sur règles), `Router` avec plusieurs stratégies (Balanced/CostOptimized/QualityFirst), `CapabilityMatrix` dynamique pour la sélection de modèle.

### Systèmes de mémoire

Architecture mémoire multicouches pour la gestion du contexte :

- **Mémoire de travail** : contexte à court terme avec courbe d'oubli d'Ebbinghaus
- **Mémoire épisodique** : historique des conversations avec recherche vectorielle (sqlite-vec)
- **Base de connaissances** : stockage de documents avec récupération hybride (vecteur + BM25 + RRF)
- **Service d'embedding** : abstraction modulaire (Ollama/Mock), vecteurs de 384 dimensions

**Implémentation** : `WorkingMemory`, `EpisodicMemory`, `KnowledgeBase` avec `EmbeddingService` injecté.

### Daemon Mind

Modules cognitifs pour une intelligence proactive :

- **Module de prédiction** : cycle PBRIC (Predict → Belief → Reflect → Intent → Curiosity)
- **Module de croyance** : inférence bayésienne pour la modélisation des préférences utilisateur
- **Module de réflexion** : évaluation de la qualité des conversations pilotée par LLM (repli sur règles)

**Implémentation** : `PredictModule`, `BeliefModule`, `ReflectionModule` avec fournisseur LLM optionnel.

### Cortex économique

Suivi des coûts et optimisation du ROI :

- Suivi des coûts en temps réel avec lissage EMA
- Statistiques par modèle (nombre d'appels, coût moyen, latence moyenne, satisfaction)
- Calcul du ROI : `satisfaction / (cost + latency_penalty)`
- Gestion du budget avec limites journalières/mensuelles configurables

**Implémentation** : `EconomicCortex` avec suivi `CostRecord` et agrégation `ModelStats`.

---

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                   Couche Application                     │
│  CLI · HTTP API · iOS/macOS · Android · Windows/Linux   │
└────────────────────────┬────────────────────────────────┘
                         │
┌────────────────────────▼────────────────────────────────┐
│                  Moteur Rust Principal                   │
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
│              Services Cloud (Optionnels)                 │
│  Anthropic · OpenAI · Google · Qwen · Doubao            │
└─────────────────────────────────────────────────────────┘
```

---

## Stack technologique

| Composant | Technologie | Justification |
|-----------|-------------|---------------|
| Moteur principal | Rust, Tokio | Haute performance, sécurité mémoire, runtime asynchrone |
| Inférence locale | Ollama (llama.cpp) | Quantification GGUF, accélération Metal/CUDA |
| Recherche vectorielle | sqlite-vec | Base de données vectorielle embarquée |
| Embedding | all-minilm (384 dim) | Léger, multilingue |
| Stockage | SQLite | Embarqué, ACID, multiplateforme |
| Tests | cargo test, httpmock | 389 tests, 0 échec |
| CI/CD | GitHub Actions | Tests, clippy, fmt, build Docker, audit de sécurité |

---

## Structure du projet

```
prometheus-ai/
├── core/                 # Moteur principal Rust
│   ├── src/
│   │   ├── router/       # Classification d'intention, routage, matrice de capacités
│   │   ├── memory/       # Mémoire de travail, épisodique, base de connaissances
│   │   ├── mind/         # Daemon Mind (prédiction, croyance, réflexion, économie)
│   │   ├── providers/    # Fournisseurs LLM (Ollama, OpenAI, Anthropic)
│   │   ├── plugins/      # Système de plugins (gestionnaire, registre, bac à sable, runtime WASM)
│   │   ├── voice/        # E/S vocales (traits STT/TTS/Wakeword, mock, pipeline)
│   │   └── privacy/      # RGPD (consentement, export, suppression, anonymisation)
│   └── tests/            # Tests unitaires, tests d'intégration, tests E2E
├── ffi/                  # Liaisons UniFFI (Rust → Swift/Kotlin)
├── cli/                  # Outil CLI (REPL interactif, gestion de sessions)
├── server/               # Service API HTTP (axum, OpenAPI, auth, métriques)
├── apps/                 # Applications natives (planifiées)
│   ├── macos/            # macOS (SwiftUI)
│   ├── ios/              # iOS (SwiftUI)
│   ├── android/          # Android (Jetpack Compose)
│   └── desktop/          # Windows/Linux (Tauri)
├── plugins/              # Écosystème de plugins
│   ├── sdk/              # SDK de développement de plugins
│   ├── calculator/       # Plugin calculatrice intégré
│   └── weather/          # Plugin météo intégré
├── docs/                 # Documentation
│   ├── plans/            # Plans de développement (P1, P2, ...)
│   ├── reports/          # Rapports de complétion
│   └── engineering/      # Documentation technique
└── assets/               # Logo, icônes, ressources
```

---

## Démarrage rapide

### Prérequis

- Rust 1.93+ ([installer](https://rustup.rs))
- Ollama ([installer](https://ollama.com))

### Installation

```bash
# Cloner le dépôt
git clone https://github.com/5201314wzq1023-alt/prometheus-ai.git
cd prometheus-ai

# Installer les modèles Ollama
ollama pull qwen2.5:1.5b
ollama pull all-minilm

# Lancer les tests
cargo test --workspace

# Lancer le CLI (mode interactif)
cargo run --bin prometheus-cli

# Lancer le serveur API HTTP
cargo run --bin prometheus-server
# Docs API : http://localhost:3000/api/docs
# Santé :    http://localhost:3000/api/v1/health
# Métriques: http://localhost:3000/metrics
```

### Déploiement Docker

```bash
# Construire et lancer avec Docker Compose
docker compose up -d

# Ou construire manuellement
docker build -t prometheus-ai .
docker run -p 3000:3000 -v prometheus-data:/home/prometheus/data prometheus-ai
```

### Exécution des tests

```bash
# Tous les tests (hors tests d'intégration Ollama)
cargo test --workspace

# Tests d'intégration Ollama (nécessite Ollama en cours d'exécution)
cargo test --workspace -- --ignored

# Suite de tests spécifique
cargo test --test e2e_integration_test

# Avec couverture
cargo tarpaulin --workspace --out Html
```

### Qualité du code

```bash
# Analyse statique
cargo clippy --workspace --all-targets -- -D warnings

# Formatage
cargo fmt --all -- --check

# Audit de sécurité
cargo audit
```

---

## Feuille de route

| Phase | Durée | Statut | Livrables clés |
|-------|-------|--------|----------------|
| **P0-P6** | 3 semaines | ✅ Terminé | Moteur principal, intégration Ollama, routage, mémoire, Daemon Mind, économie, tests E2E |
| **P2** | 2 semaines | ✅ Terminé | CLI, API HTTP, auth, performance, supervision, plugins, FFI, Docker, CI/CD |
| **P3** | 2 semaines | ✅ Terminé | Plugins WASM, abstraction voix E/S, conformité RGPD, FFI Android/bureau |
| **P4** | À définir | ⏳ Futur | Intégration IoT, agent écran, synchronisation multi-appareils, personnalisation avancée |

**Focus actuel** : P4 - Intégration IoT et personnalisation avancée.

---

## Métriques de performance

Benchmarks actuels (référence P0-P6) :

| Métrique | Cible | Actuel |
|----------|-------|--------|
| Classification d'intention | < 1ms | ✓ Règles + repli LLM |
| Recherche vectorielle (1K docs) | < 50ms | ✓ 8,8ms (en cache) |
| Recherche base de connaissances | < 50ms | ✓ 2,4ms |
| Mémoire de travail | < 1ms | ✓ 67µs |
| Débit concurrent | > 15 QPS | ✓ Limite 64 connexions simultanées |
| Utilisation mémoire (inactif) | < 200Mo | ✓ Vérifié |
| Couverture des tests | > 80% | ✓ 389 tests réussis |

---

## Points de terminaison API HTTP

URL de base : `http://localhost:3000`

| Méthode | Chemin | Description |
|---------|--------|-------------|
| GET | `/api/v1/health` | Vérification de santé basique |
| GET | `/api/v1/health/deep` | Vérification de santé approfondie (état des sous-systèmes) |
| GET | `/api/v1/info` | Informations du service (nom, version) |
| POST | `/api/v1/sessions` | Créer une session |
| GET | `/api/v1/sessions` | Lister toutes les sessions |
| POST | `/api/v1/sessions/{id}/chat` | Envoyer un message |
| GET | `/api/v1/sessions/{id}/messages` | Obtenir l'historique des messages |
| POST | `/api/v1/memory/search` | Rechercher dans la mémoire épisodique |
| POST | `/api/v1/memory/knowledge` | Ajouter une connaissance |
| GET | `/api/v1/memory/knowledge` | Rechercher dans la base de connaissances |
| POST | `/api/v1/memory/working` | Ajouter en mémoire de travail |
| GET | `/api/v1/memory/working` | Obtenir la mémoire de travail |
| GET | `/api/v1/stats/sessions` | Statistiques des sessions |
| GET | `/api/v1/stats/overview` | Vue d'ensemble du système |
| POST | `/api/v1/privacy/consent` | Enregistrer le consentement utilisateur |
| GET | `/api/v1/privacy/export` | Exporter les données utilisateur (portabilité RGPD) |
| DELETE | `/api/v1/privacy/data` | Supprimer les données utilisateur (droit à l'effacement RGPD) |
| POST | `/api/v1/privacy/anonymize` | Anonymisation des données personnelles dans le texte |
| GET | `/metrics` | Métriques Prometheus |
| GET | `/api/docs` | Interface Swagger |

## Variables d'environnement

| Variable | Défaut | Description |
|----------|--------|-------------|
| `PROMETHEUS_HOST` | `0.0.0.0` | Adresse d'écoute |
| `PROMETHEUS_PORT` | `3000` | Port d'écoute |
| `PROMETHEUS_DATA_DIR` | `/tmp/prometheus-server` | Répertoire de stockage des données |
| `PROMETHEUS_LOG_FORMAT` | `text` | Format des journaux (`text` / `json`) |
| `PROMETHEUS_AUTH_ENABLED` | `false` | Activer l'authentification |
| `PROMETHEUS_API_KEYS` | - | Liste de clés API (séparées par des virgules) |
| `PROMETHEUS_JWT_SECRET` | - | Secret de signature JWT |
| `PROMETHEUS_PLUGIN_DIR` | `./plugins` | Répertoire des plugins WASM |
| `PROMETHEUS_CONSENT_DB` | `./consent.db` | Chemin de la base de données des consentements RGPD |
| `RUST_LOG` | `info` | Niveau de journalisation |

---

## Documentation

### Plans de développement
- [Plan d'exécution P1](docs/P1_EXECUTION_PLAN.md) - Feuille de route d'intégration Ollama
- [Plan de développement P2](docs/plans/20260307-P2-PLAN.md) - Couche production et application
- [Plan de développement P3](docs/plans/20260309-P3-PLAN.md) - Plugins WASM, voix E/S, RGPD, FFI

### Rapports de complétion
- [Rapport de complétion P0-P5](docs/reports/2026-03-06-P1-P5-completion.md)
- [Rapport de tests E2E P6](docs/reports/2026-03-06-P6-completion.md)

### Documents de conception
- [Conception produit](docs/product-design.md) - Vision produit complète (20 chapitres)
- [Conception algorithmique](docs/ALGORITHM_DESIGN.md) - Algorithmes et structures de données principaux
- [Vue d'ensemble de l'architecture](docs/architecture.md) - Architecture du système
- [Déploiement de modèles locaux](docs/LOCAL_MODEL_DEPLOYMENT.md) - Guide de configuration Ollama

### Référence API
- [Documentation API](docs/api-reference.md) - Référence de l'API principale
- [Guide de contribution](docs/contributing.md) - Comment contribuer

---

## Contribuer

Les contributions sont les bienvenues ! Veuillez consulter [CONTRIBUTING.md](docs/contributing.md) pour les directives.

### Flux de développement

1. Forker le dépôt
2. Créer une branche de fonctionnalité (`git checkout -b feature/amazing-feature`)
3. Apporter vos modifications avec des tests
4. Lancer les vérifications de qualité (`cargo test && cargo clippy && cargo fmt`)
5. Valider avec les commits conventionnels (`feat: add amazing feature`)
6. Pousser et créer une Pull Request

### Standards de code

- Tout le code doit passer `cargo clippy` sans aucun avertissement
- Tout le code doit être formaté avec `cargo fmt`
- Toute nouvelle fonctionnalité doit inclure des tests (objectif : couverture > 80 %)
- Toutes les API publiques doivent avoir une documentation rustdoc

---

## Licence

Ce projet est sous licence MIT — voir le fichier [LICENSE](LICENSE) pour plus de détails.

---

## Remerciements

- **Ollama** - Moteur d'inférence LLM local
- **llama.cpp** - Runtime LLM haute performance
- **sqlite-vec** - Recherche vectorielle embarquée
- **Rust Community** - Écosystème et outillage remarquables

---

<div align="center">

**Prometheus AI** — Apporter le feu de l'IA à tous 🔥

[Documentation](docs/) · [Signaler un bug](https://github.com/5201314wzq1023-alt/prometheus-ai/issues) · [Demander une fonctionnalité](https://github.com/5201314wzq1023-alt/prometheus-ai/issues)

</div>
