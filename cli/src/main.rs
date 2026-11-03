//! Prometheus CLI — 本地优先的智能助手
//!
//! 支持交互式 REPL 和子命令两种模式。

mod commands;
mod config;
mod repl;

use clap::{Parser, Subcommand};
use commands::{chat::ChatArgs, config::ConfigArgs, model::ModelArgs, session::SessionArgs};
use config::CliConfig;
use prometheus_core::orchestrator::{ChatOrchestrator, ProviderChatBackend};
use prometheus_core::providers::mock::MockProvider;
use prometheus_core::providers::ollama::OllamaProvider;
use prometheus_core::providers::registry::ProviderRegistry;
use prometheus_core::router::fallback::FallbackRouter;
use prometheus_core::router::runtime::RoutingRuntime;
use prometheus_core::router::strategy::Router;
use prometheus_core::router::{IntentClassifier, RoutingStrategy};
use prometheus_core::session::{HistoryStore, SessionManager};
use repl::Repl;
use std::sync::Arc;
use std::time::Duration;

#[derive(Parser)]
#[command(name = "prometheus")]
#[command(about = "Prometheus AI — 本地优先的智能助手", long_about = None)]
#[command(version)]
struct Cli {
    /// 使用显式的离线演示后端（不会执行真实模型推理）
    #[arg(long, global = true)]
    mock: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// 启动交互式 REPL（默认）
    Chat,
    /// 发送单条消息（非交互模式）
    Ask(ChatArgs),
    /// 会话管理
    Session(SessionArgs),
    /// 模型管理
    Model(ModelArgs),
    /// 配置管理
    Config(ConfigArgs),
    /// 测试路由器
    TestRouter {
        /// 输入文本
        input: String,
    },
    /// 查询记忆
    QueryMemory {
        /// 查询文本
        query: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let config = CliConfig::load()?;

    match cli.command {
        None | Some(Commands::Chat) => {
            run_repl(config, cli.mock).await?;
        }
        Some(Commands::Ask(args)) => {
            run_ask(config, args, cli.mock).await?;
        }
        Some(Commands::Session(args)) => {
            run_session(config, args).await?;
        }
        Some(Commands::Model(args)) => {
            run_model(args).await?;
        }
        Some(Commands::Config(args)) => {
            run_config(config, args)?;
        }
        Some(Commands::TestRouter { input }) => {
            run_test_router(&input).await?;
        }
        Some(Commands::QueryMemory { query }) => {
            run_query_memory(&config, &query).await?;
        }
    }

    Ok(())
}

// ─── 构建 orchestrator ───

async fn build_orchestrator(
    config: &CliConfig,
    mock: bool,
) -> anyhow::Result<ChatOrchestrator<ProviderChatBackend>> {
    let data_dir = &config.data_dir;
    std::fs::create_dir_all(data_dir)?;

    let history = HistoryStore::new(data_dir.join("history.db"));
    let session_db = data_dir.join("sessions.db");

    let mut registry = ProviderRegistry::new();

    let provider_names = if mock {
        eprintln!("演示模式：使用 MockProvider，回复不代表真实模型推理。");
        registry.register(Arc::new(MockProvider::new("mock")));
        vec!["mock".to_string()]
    } else {
        let url = std::env::var("PROMETHEUS_PROVIDER_OLLAMA_URL")
            .unwrap_or_else(|_| "http://localhost:11434".to_string());
        registry.register(Arc::new(connect_ollama(&url).await?));
        vec!["ollama".to_string()]
    };

    let fallback = FallbackRouter::with_config(provider_names, 2, Duration::from_secs(30));
    let runtime = RoutingRuntime::new(registry, fallback);
    let backend = ProviderChatBackend::new(runtime);

    let mut orchestrator = ChatOrchestrator::new_with_persistence(session_db, history, backend)?;
    orchestrator.set_model_override(Some(config.default_model.clone()));
    Ok(orchestrator)
}

async fn connect_ollama(url: &str) -> anyhow::Result<OllamaProvider> {
    let response = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()?
        .get(format!("{}/api/tags", url.trim_end_matches('/')))
        .send()
        .await
        .map_err(|e| {
            anyhow::anyhow!(
                "无法连接 Ollama ({url}): {e}。请启动 Ollama 并拉取模型；离线演示请使用 --mock。"
            )
        })?;
    response
        .error_for_status()
        .map_err(|e| anyhow::anyhow!("Ollama 服务不可用 ({url}): {e}。离线演示请使用 --mock。"))?;
    Ok(OllamaProvider::new().with_base_url(url))
}

// ─── REPL 交互模式 ───

async fn run_repl(config: CliConfig, mock: bool) -> anyhow::Result<()> {
    let mut orchestrator = build_orchestrator(&config, mock).await?;
    let session = if let Some(existing) = orchestrator.sessions().first() {
        existing.clone()
    } else {
        orchestrator.create_session(Some("默认会话")).await?
    };

    let mut repl = Repl::new(orchestrator, config, session);
    repl.run().await
}

// ─── 单条消息模式 ───

async fn run_ask(config: CliConfig, args: ChatArgs, mock: bool) -> anyhow::Result<()> {
    let mut orchestrator = build_orchestrator(&config, mock).await?;

    let session = if let Some(ref sid) = args.session {
        let sessions = orchestrator.sessions();
        sessions
            .iter()
            .find(|s| s.id.to_string().starts_with(sid))
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("未找到会话: {sid}"))?
    } else {
        orchestrator.create_session(None).await?
    };

    if let Some(ref model) = args.model {
        orchestrator.set_model_override(Some(model.clone()));
    }

    let turn = orchestrator.send_message(session.id, &args.message).await?;

    match args.format.as_str() {
        "json" => {
            let out = serde_json::json!({
                "reply": turn.reply,
                "route": turn.route_reason,
                "session_id": session.id.to_string(),
            });
            println!("{}", serde_json::to_string_pretty(&out)?);
        }
        _ => {
            println!("{}", turn.reply);
        }
    }
    Ok(())
}

// ─── 会话管理 ───

async fn run_session(config: CliConfig, args: SessionArgs) -> anyhow::Result<()> {
    use commands::session::SessionCommand;

    let data_dir = &config.data_dir;
    std::fs::create_dir_all(data_dir)?;
    let session_db = data_dir.join("sessions.db");
    let mut manager = SessionManager::with_db_path(session_db.clone())?;

    match args.command {
        SessionCommand::List => {
            for s in manager.sessions() {
                println!(
                    "  {} {} [{:?}] {}",
                    &s.id.to_string()[..8],
                    s.title,
                    s.state,
                    s.last_active.format("%Y-%m-%d %H:%M")
                );
            }
        }
        SessionCommand::Show { id } => {
            let sessions = manager.sessions();
            let found = sessions.iter().find(|s| s.id.to_string().starts_with(&id));
            match found {
                Some(s) => {
                    println!("ID:      {}", s.id);
                    println!("标题:    {}", s.title);
                    println!("状态:    {:?}", s.state);
                    println!("创建:    {}", s.created_at);
                    println!("最后活跃: {}", s.last_active);
                }
                None => println!("未找到会话: {id}"),
            }
        }
        SessionCommand::New { title } => {
            let s = manager.create_session(title.as_deref()).await?;
            println!("已创建会话: {} ({})", s.title, &s.id.to_string()[..8]);
        }
        SessionCommand::Delete { id } => {
            let sessions = manager.sessions();
            let found = sessions.iter().find(|s| s.id.to_string().starts_with(&id));
            match found {
                Some(s) => {
                    let sid = s.id;
                    let title = s.title.clone();
                    let mut mgr = SessionManager::with_db_path(session_db.clone())?;
                    mgr.archive_session(sid).await?;
                    let history = HistoryStore::new(data_dir.join("history.db"));
                    history.delete_by_session(sid)?;
                    println!("已归档并清理会话: {} ({})", title, &sid.to_string()[..8]);
                }
                None => println!("未找到会话: {id}"),
            }
        }
        SessionCommand::Export { id, output } => {
            let sessions = manager.sessions();
            let found = sessions.iter().find(|s| s.id.to_string().starts_with(&id));
            match found {
                Some(s) => {
                    let history = HistoryStore::new(data_dir.join("history.db"));
                    let messages = history.get_session_messages(s.id).await?;
                    let msg_json: Vec<serde_json::Value> = messages
                        .iter()
                        .map(|m| {
                            serde_json::json!({
                                "role": format!("{:?}", m.role),
                                "content": m.content,
                                "timestamp": m.timestamp.to_rfc3339(),
                            })
                        })
                        .collect();
                    let out_path = output
                        .unwrap_or_else(|| format!("session-{}.json", &s.id.to_string()[..8]));
                    let export = serde_json::json!({
                        "session_id": s.id.to_string(),
                        "title": s.title,
                        "state": format!("{:?}", s.state),
                        "created_at": s.created_at.to_rfc3339(),
                        "message_count": messages.len(),
                        "messages": msg_json,
                    });
                    std::fs::write(&out_path, serde_json::to_string_pretty(&export)?)?;
                    println!("已导出到: {} ({} 条消息)", out_path, messages.len());
                }
                None => println!("未找到会话: {id}"),
            }
        }
    }
    Ok(())
}

// ─── 模型管理 ───

async fn run_model(args: ModelArgs) -> anyhow::Result<()> {
    use commands::model::ModelCommand;

    match args.command {
        ModelCommand::List => {
            println!("可用模型:");
            println!("  qwen2.5:1.5b       (默认 Ollama 模型，需先 ollama pull qwen2.5:1.5b)");
            println!("  其他已安装 Ollama tag 可通过 ask --model <tag> 指定");
        }
        ModelCommand::Info { name } => {
            println!("模型: {name}");
            println!("(详细信息功能开发中)");
        }
        ModelCommand::Default { name } => {
            let mut config = CliConfig::load()?;
            config.set("default_model", &name)?;
            println!("默认模型已设为: {name}");
        }
        ModelCommand::Test { name } => {
            println!("测试模型连通性: {name} ...");
            // TODO: 实际 ping 测试
            println!("(连通性测试功能开发中)");
        }
    }
    Ok(())
}

// ─── 配置管理 ───

fn run_config(config: CliConfig, args: ConfigArgs) -> anyhow::Result<()> {
    use commands::config::ConfigCommand;

    let mut config = config;
    match args.command {
        ConfigCommand::List => {
            println!("当前配置:");
            for key in CliConfig::list_keys() {
                if let Ok(val) = config.get(key) {
                    println!("  {key} = {val}");
                }
            }
            println!("\n配置文件: {}", config::config_file_path().display());
        }
        ConfigCommand::Get { key } => match config.get(&key) {
            Ok(val) => println!("{val}"),
            Err(e) => eprintln!("错误: {e}"),
        },
        ConfigCommand::Set { key, value } => {
            config.set(&key, &value)?;
            println!("已设置 {key} = {value}");
        }
        ConfigCommand::Reset => {
            let config = CliConfig::default();
            config.save()?;
            println!("配置已重置为默认值");
        }
    }
    Ok(())
}

// ─── 路由测试 ───

async fn run_test_router(input: &str) -> anyhow::Result<()> {
    let classifier = IntentClassifier::new();
    let mut router = Router::new(RoutingStrategy::Balanced);
    let classification = classifier.classify(input).await?;
    let decision = router.select_model(&classification, 1000);
    println!("intent:     {:?}", classification.primary);
    println!("confidence: {:.2}", classification.confidence);
    println!("complexity: {}", classification.complexity);
    println!("model:      {:?}", decision.model);
    println!("quality:    {:.2}", decision.quality_score);
    println!("cost:       ${:.4}", decision.estimated_cost);
    Ok(())
}

// ─── 记忆查询 ───

async fn run_query_memory(config: &CliConfig, query: &str) -> anyhow::Result<()> {
    let history = HistoryStore::new(config.data_dir.join("history.db"));
    let matches = history.search(query, 10).await?;

    if matches.is_empty() {
        println!("未找到相关消息");
    } else {
        for item in matches {
            println!("- [{}] {}", item.session_id, item.content);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use commands::session::SessionCommand;
    use prometheus_core::session::history::{Message, Role};
    use std::path::PathBuf;
    use uuid::Uuid;

    struct TestData(PathBuf);

    impl TestData {
        fn new() -> Self {
            Self(std::env::temp_dir().join(format!("prometheus-cli-test-{}", Uuid::new_v4())))
        }

        fn config(&self) -> CliConfig {
            CliConfig {
                data_dir: self.0.clone(),
                ..CliConfig::default()
            }
        }
    }

    impl Drop for TestData {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn default_command_is_none() {
        // 无子命令时默认进入 REPL
        let cli = Cli::try_parse_from(["prometheus"]);
        assert!(cli.is_ok());
        assert!(cli.unwrap().command.is_none());
    }

    #[test]
    fn demo_mode_is_explicit_and_global() {
        let cli = Cli::try_parse_from(["prometheus", "ask", "hello", "--mock"]).unwrap();
        assert!(cli.mock);
        assert!(
            !Cli::try_parse_from(["prometheus", "ask", "hello"])
                .unwrap()
                .mock
        );
    }

    #[tokio::test]
    async fn ollama_probe_is_async_and_reports_http_failure() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        for (status, success) in [("200 OK", true), ("503 Service Unavailable", false)] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let url = format!("http://{}", listener.local_addr().unwrap());
            let server = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                let mut buffer = [0_u8; 1024];
                while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
                    let read = socket.read(&mut buffer).await.unwrap();
                    assert!(read > 0);
                    request.extend_from_slice(&buffer[..read]);
                }
                assert!(request.starts_with(b"GET /api/tags "));
                let body = r#"{"models":[]}"#;
                let response = format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                socket.write_all(response.as_bytes()).await.unwrap();
            });

            let result = connect_ollama(&url).await;
            assert_eq!(result.is_ok(), success);
            if let Err(error) = result {
                assert!(error.to_string().contains("--mock"));
            }
            tokio::time::timeout(Duration::from_secs(3), server)
                .await
                .unwrap()
                .unwrap();
        }
    }

    #[tokio::test]
    async fn mock_chat_uses_configured_model_and_persists_history() {
        let data = TestData::new();
        let mut config = data.config();
        config.default_model = "custom-local-model:latest".to_string();
        let mut orchestrator = build_orchestrator(&config, true).await.unwrap();
        assert_eq!(
            orchestrator.model_override(),
            Some("custom-local-model:latest")
        );
        let session = orchestrator.create_session(Some("demo")).await.unwrap();
        let turn = orchestrator
            .send_message(session.id, "hello")
            .await
            .unwrap();
        assert!(turn.reply.starts_with("[mock:mock]"));
        let history = HistoryStore::new(config.data_dir.join("history.db"));
        assert_eq!(
            history
                .get_session_messages(session.id)
                .await
                .unwrap()
                .len(),
            2
        );
    }

    #[tokio::test]
    async fn session_commands_work_inside_async_runtime_without_provider() {
        let data = TestData::new();
        let config = data.config();
        run_session(
            config.clone(),
            SessionArgs {
                command: SessionCommand::New {
                    title: Some("saved session".to_string()),
                },
            },
        )
        .await
        .unwrap();

        let manager = SessionManager::with_db_path(config.data_dir.join("sessions.db")).unwrap();
        let session = manager.sessions()[0].clone();
        drop(manager);
        let history = HistoryStore::new(config.data_dir.join("history.db"));
        history
            .append(&Message {
                id: Uuid::new_v4(),
                session_id: session.id,
                role: Role::User,
                content: "savedterm".to_string(),
                timestamp: chrono::Utc::now(),
            })
            .await
            .unwrap();

        let output = config.data_dir.join("export.json");
        run_session(
            config.clone(),
            SessionArgs {
                command: SessionCommand::Export {
                    id: session.id.to_string(),
                    output: Some(output.to_string_lossy().to_string()),
                },
            },
        )
        .await
        .unwrap();
        let exported: serde_json::Value =
            serde_json::from_slice(&std::fs::read(output).unwrap()).unwrap();
        assert_eq!(exported["messages"][0]["content"], "savedterm");

        run_session(
            config.clone(),
            SessionArgs {
                command: SessionCommand::Delete {
                    id: session.id.to_string(),
                },
            },
        )
        .await
        .unwrap();
        assert!(history
            .get_session_messages(session.id)
            .await
            .unwrap()
            .is_empty());
        let manager = SessionManager::with_db_path(config.data_dir.join("sessions.db")).unwrap();
        assert_eq!(
            manager.sessions()[0].state,
            prometheus_core::session::SessionState::Archived
        );
    }

    #[tokio::test]
    async fn memory_query_uses_configured_database_without_demo_records() {
        let data = TestData::new();
        let config = data.config();
        run_query_memory(&config, "提醒").await.unwrap();
        assert!(config.data_dir.join("history.db").is_file());
        let history = HistoryStore::new(config.data_dir.join("history.db"));
        assert!(history.search("提醒", 10).await.unwrap().is_empty());
    }
}
