# aufusiding-AI

**Assistant IA privilégiant l'exécution locale, écrit en Rust · Early Alpha**

[简体中文](README.md) · [English](README.en.md) · [日本語](README.ja.md) · Français

[![CI](https://img.shields.io/github/actions/workflow/status/zuixi01/aufusiding-AI/ci.yml?branch=main&label=CI)](https://github.com/zuixi01/aufusiding-AI/actions/workflows/ci.yml)
[![Licence MIT](https://img.shields.io/badge/license-MIT-blue)](LICENSE)

aufusiding-AI explore les conversations locales, la persistance des sessions et le routage des modèles. Les crates, exécutables, applications et répertoires de configuration conservent le nom **Prometheus AI**. L'exécutable CLI s'appelle `prometheus`.

Le projet est une première version alpha destinée aux développeurs. Commencez par le CLI et Ollama. Les intégrations HTTP, bureau, mobile, voix et IoT ont des niveaux d'avancement différents ; certaines utilisent des données simulées ou demandent une intégration supplémentaire. Les API et les formats de données peuvent changer. Le projet ne revendique ni aptitude à la production, ni conformité réglementaire, ni gains mesurés, ni validation de toutes les plateformes.

## Démarrage rapide

Installez Git, rustup (le dépôt fixe Rust **1.99.1**), les outils de compilation C/C++ natifs, et [Ollama](https://ollama.com/). Le CLI ne nécessite ni Node.js, ni Bun, ni Docker. Cette version est la référence de développement actuelle, pas une version minimale validée.

Démarrez l'application Ollama, ou exécutez `ollama serve` dans un terminal séparé, puis :

```bash
git clone https://github.com/zuixi01/aufusiding-AI.git
cd aufusiding-AI
ollama pull qwen2.5:1.5b
ollama list
cargo run --locked -p prometheus-cli -- --help
cargo run --locked -p prometheus-cli -- ask "Bonjour, présente-toi en une phrase." --model qwen2.5:1.5b
cargo run --locked -p prometheus-cli -- chat
```

Dans le REPL, utilisez `/switch qwen2.5:1.5b` pour sélectionner un modèle installé, `/help` pour afficher les commandes et `/quit` pour quitter. L'URL Ollama par défaut est `http://localhost:11434` ; `PROMETHEUS_PROVIDER_OLLAMA_URL` permet de la modifier. Les poids des modèles ne sont pas inclus.

Pour essayer le CLI sans modèle, choisissez explicitement le mode simulé :

```bash
cargo run --locked -p prometheus-cli -- --mock ask "Bonjour"
```

Les réponses simulées ne proviennent pas d'une inférence réelle. `model list` affiche un catalogue statique et `model test` est incomplet ; utilisez `ollama list` pour connaître les modèles installés.

## Configuration et contributions

```bash
cargo run --locked -p prometheus-cli -- config list
cargo run --locked -p prometheus-cli -- config set default_model qwen2.5:1.5b
cargo fmt --all -- --check
cargo check --locked -p prometheus-cli
cargo test --locked -p prometheus-cli
```

`config list` affiche les chemins de configuration et des données locales. Certains paramètres enregistrés ne sont pas encore reliés à l'inférence. Les conversations et l'historique du REPL sont conservés localement : vérifiez les exports et journaux avant de les partager. Les clients bureau et mobiles restent expérimentaux et nécessitent leurs outils de développement spécifiques ainsi qu'une validation supplémentaire.

- [Installation, configuration, HTTP et dépannage](docs/getting-started.md) (chinois)
- [État des fonctionnalités et limites connues](docs/project-status.md) (chinois)
- [Architecture](docs/architecture.md) · [Référence API](docs/api-reference.md)
- [Guide de contribution](CONTRIBUTING.md) · [Code de conduite](CODE_OF_CONDUCT.md)
- [Politique de sécurité](SECURITY.md) · [Signaler un problème](https://github.com/zuixi01/aufusiding-AI/issues/new/choose)

Le code est sous [licence MIT](LICENSE). Les modèles, services, données et dépendances externes conservent leurs propres licences et conditions.
