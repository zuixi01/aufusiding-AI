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
use prometheus_core::session::history::{Message, Role};
use prometheus_core::session::{HistoryStore, SessionManager};
use repl::Repl;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

#[derive(Parser)]
#[command(name = "prometheus")]
#[command(about = "Prometheus AI — 本地优先的智能助手", long_about = None)]
#[command(version)]
struct Cli {
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
    let config = CliConfig::load().unwrap_or_default();

    match cli.command {
        None | Some(Commands::Chat) => {
            run_repl(config).await?;
        }
        Some(Commands::Ask(args)) => {
            run_ask(config, args).await?;
        }
        Some(Commands::Session(args)) => {
            run_session(config, args)?;
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
            run_query_memory(&query).await?;
        }
    }

    Ok(())
}

// ─── 构建 orchestrator ───

fn build_orchestrator(config: &CliConfig) -> anyhow::Result<ChatOrchestrator<ProviderChatBackend>> {
    let data_dir = &config.data_dir;
    std::fs::create_dir_all(data_dir)?;

    let history = HistoryStore::new(data_dir.join("history.db"));
    let session_db = data_dir.join("sessions.db");

    let mut registry = ProviderRegistry::new();

    // 优先检测 Ollama 可用性
    let ollama_available = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .ok()
        .and_then(|c| c.get("http://localhost:11434/api/tags").send().ok())
        .map(|r| r.status().is_success())
        .unwrap_or(false);

    let provider_names = if ollama_available {
        registry.register(Arc::new(OllamaProvider::new()));
        vec!["ollama".to_string()]
    } else {
        registry.register(Arc::new(MockProvider::new("qwen")));
        registry.register(Arc::new(MockProvider::new("openai")));
        registry.register(Arc::new(MockProvider::new("anthropic")));
        vec![
            "qwen".to_string(),
            "openai".to_string(),
            "anthropic".to_string(),
        ]
    };

    let fallback = FallbackRouter::with_config(provider_names, 2, Duration::from_secs(30));
    let runtime = RoutingRuntime::new(registry, fallback);
    let backend = ProviderChatBackend::new(runtime);

    ChatOrchestrator::new_with_persistence(session_db, history, backend)
}

// ─── REPL 交互模式 ───

async fn run_repl(config: CliConfig) -> anyhow::Result<()> {
    let mut orchestrator = build_orchestrator(&config)?;
    let session = if let Some(existing) = orchestrator.sessions().first() {
        existing.clone()
    } else {
        orchestrator.create_session(Some("默认会话")).await?
    };

    let mut repl = Repl::new(orchestrator, config, session);
    repl.run().await
}

// ─── 单条消息模式 ───

async fn run_ask(config: CliConfig, args: ChatArgs) -> anyhow::Result<()> {
    let mut orchestrator = build_orchestrator(&config)?;

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

fn run_session(config: CliConfig, args: SessionArgs) -> anyhow::Result<()> {
    use commands::session::SessionCommand;

    let data_dir = &config.data_dir;
    std::fs::create_dir_all(data_dir)?;
    let session_db = data_dir.join("sessions.db");
    let manager = SessionManager::with_db_path(session_db.clone())?;

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
            let mut orch = build_orchestrator(&config)?;
            let s = tokio::runtime::Handle::current()
                .block_on(orch.create_session(title.as_deref()))?;
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
                    tokio::runtime::Handle::current().block_on(mgr.archive_session(sid))?;
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
                    let messages = tokio::runtime::Handle::current()
                        .block_on(history.get_session_messages(s.id))?;
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
            println!("  qwen2.5-3b-local   (本地 Ollama)");
            println!("  qwen2.5-7b-local   (本地 Ollama)");
            println!("  local-llama        (本地 GGUF)");
            println!("  openai-gpt-4o-mini (云端)");
            println!("  anthropic-claude   (云端)");
        }
        ModelCommand::Info { name } => {
            println!("模型: {name}");
            println!("(详细信息功能开发中)");
        }
        ModelCommand::Default { name } => {
            let mut config = CliConfig::load().unwrap_or_default();
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

async fn run_query_memory(query: &str) -> anyhow::Result<()> {
    let history = seed_history().await?;
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

async fn seed_history() -> anyhow::Result<HistoryStore> {
    let history = HistoryStore::new(PathBuf::from("/tmp/prometheus-memory.db"));
    let session_id = Uuid::new_v4();

    for content in [
        "今天晚上八点提醒我复盘",
        "我想要一个简洁风格的回复",
        "明天帮我整理开会纪要",
    ] {
        history
            .append(&Message {
                id: Uuid::new_v4(),
                session_id,
                role: Role::User,
                content: content.to_string(),
                timestamp: chrono::Utc::now(),
            })
            .await?;
    }

    Ok(history)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_command_is_none() {
        // 无子命令时默认进入 REPL
        let cli = Cli::try_parse_from(["prometheus"]);
        assert!(cli.is_ok());
        assert!(cli.unwrap().command.is_none());
    }
}
