//! 交互式 REPL 模块
//!
//! 基于 rustyline 提供命令补全、历史记录、彩色输出。

mod completer;
mod prompt;

pub use completer::PrometheusCompleter;
pub use prompt::PromptRenderer;

use crate::config::CliConfig;
use anyhow::Result;
use colored::Colorize;
use prometheus_core::orchestrator::{ChatBackend, ChatOrchestrator};
use prometheus_core::session::history::Role;
use prometheus_core::session::Session;
use rustyline::error::ReadlineError;
use rustyline::{history::DefaultHistory, Config, Editor};
use std::path::PathBuf;

/// REPL 控制命令
#[derive(Debug, PartialEq, Eq)]
pub enum ReplCommand {
    Quit,
    Clear,
    Help,
    SwitchModel(String),
    NewSession(Option<String>),
    ListSessions,
    SwitchSession(String),
    ShowHistory(Option<usize>),
    ShowConfig,
    SetConfig(String, String),
    ShowStats,
    ExportSession(Option<String>),
}

/// 解析 REPL 命令
pub fn parse_command(input: &str) -> Option<ReplCommand> {
    let trimmed = input.trim();
    if !trimmed.starts_with('/') {
        return None;
    }

    let parts: Vec<&str> = trimmed.splitn(3, ' ').collect();
    let cmd = parts[0].to_lowercase();
    match cmd.as_str() {
        "/quit" | "/exit" | "/q" => Some(ReplCommand::Quit),
        "/clear" | "/cls" => Some(ReplCommand::Clear),
        "/help" | "/h" | "/?" => Some(ReplCommand::Help),
        "/switch" | "/model" => {
            let model = parts.get(1).map(|s| s.trim()).unwrap_or("");
            if model.is_empty() {
                Some(ReplCommand::Help)
            } else {
                Some(ReplCommand::SwitchModel(model.to_string()))
            }
        }
        "/new" => {
            let title = parts.get(1).map(|s| s.to_string());
            Some(ReplCommand::NewSession(title))
        }
        "/sessions" | "/ls" => Some(ReplCommand::ListSessions),
        "/use" => {
            let id = parts.get(1).map(|s| s.trim()).unwrap_or("");
            if id.is_empty() {
                Some(ReplCommand::Help)
            } else {
                Some(ReplCommand::SwitchSession(id.to_string()))
            }
        }
        "/history" => {
            let n = parts.get(1).and_then(|s| s.parse().ok());
            Some(ReplCommand::ShowHistory(n))
        }
        "/config" => {
            if let Some(key) = parts.get(1) {
                if let Some(value) = parts.get(2) {
                    Some(ReplCommand::SetConfig(key.to_string(), value.to_string()))
                } else {
                    Some(ReplCommand::ShowConfig)
                }
            } else {
                Some(ReplCommand::ShowConfig)
            }
        }
        "/stats" => Some(ReplCommand::ShowStats),
        "/export" => {
            let path = parts.get(1).map(|s| s.to_string());
            Some(ReplCommand::ExportSession(path))
        }
        _ => Some(ReplCommand::Help),
    }
}

/// REPL 主循环
pub struct Repl<B: ChatBackend> {
    orchestrator: ChatOrchestrator<B>,
    config: CliConfig,
    current_session: Session,
    history_path: PathBuf,
}

impl<B: ChatBackend> Repl<B> {
    pub fn new(orchestrator: ChatOrchestrator<B>, config: CliConfig, session: Session) -> Self {
        let history_path = config.data_dir.join("repl_history.txt");
        Self {
            orchestrator,
            config,
            current_session: session,
            history_path,
        }
    }

    /// 启动 REPL 主循环
    pub async fn run(&mut self) -> Result<()> {
        let rl_config = Config::builder()
            .history_ignore_space(true)
            .max_history_size(self.config.history_size)?
            .build();

        let completer = PrometheusCompleter::new();
        let mut rl: Editor<PrometheusCompleter, DefaultHistory> = Editor::with_config(rl_config)?;
        rl.set_helper(Some(completer));

        if self.history_path.exists() {
            let _ = rl.load_history(&self.history_path);
        }

        self.print_welcome();

        loop {
            let prompt = PromptRenderer::render(
                &self.current_session.title,
                self.orchestrator.model_override(),
            );

            match rl.readline(&prompt) {
                Ok(line) => {
                    let input = line.trim();
                    if input.is_empty() {
                        continue;
                    }
                    let _ = rl.add_history_entry(input);

                    if let Some(cmd) = parse_command(input) {
                        match self.handle_command(cmd).await {
                            Ok(should_quit) => {
                                if should_quit {
                                    break;
                                }
                            }
                            Err(e) => {
                                eprintln!("{} {e}", "错误:".red().bold());
                            }
                        }
                    } else {
                        self.handle_chat(input).await;
                    }
                }
                Err(ReadlineError::Interrupted | ReadlineError::Eof) => break,
                Err(e) => {
                    eprintln!("{} {e}", "读取错误:".red().bold());
                    break;
                }
            }
        }

        if let Some(parent) = self.history_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let _ = rl.save_history(&self.history_path);
        println!("{}", "再见！".cyan());
        Ok(())
    }

    fn print_welcome(&self) {
        println!();
        println!("{}", "  Prometheus AI".cyan().bold());
        println!("{}", "  本地优先的智能助手".dimmed());
        println!();
        println!(
            "  会话: {}  模型: {}",
            self.current_session.title.green(),
            self.config.default_model.yellow()
        );
        println!("  输入 {} 查看可用命令", "/help".cyan());
        println!();
    }

    fn print_help(&self) {
        println!();
        println!("{}", "可用命令:".cyan().bold());
        println!("  {}          退出", "/quit, /q".yellow());
        println!("  {}       清屏", "/clear, /cls".yellow());
        println!("  {}    切换模型", "/switch <model>".yellow());
        println!("  {}   新建会话", "/new [title]".yellow());
        println!("  {}      列出会话", "/sessions".yellow());
        println!("  {}      切换会话", "/use <id>".yellow());
        println!("  {} 查看历史", "/history [n]".yellow());
        println!("  {}        查看/修改配置", "/config".yellow());
        println!("  {}         使用统计", "/stats".yellow());
        println!("  {}  导出会话", "/export [path]".yellow());
        println!();
    }

    /// 处理 REPL 命令，返回 true 表示退出
    async fn handle_command(&mut self, cmd: ReplCommand) -> Result<bool> {
        match cmd {
            ReplCommand::Quit => return Ok(true),
            ReplCommand::Clear => {
                print!("\x1B[2J\x1B[1;1H");
                self.print_welcome();
            }
            ReplCommand::Help => self.print_help(),
            ReplCommand::SwitchModel(model) => {
                self.orchestrator.set_model_override(Some(model.clone()));
                println!("{} {}", "已切换模型:".green(), model.yellow());
            }
            ReplCommand::NewSession(title) => {
                let title_ref = title.as_deref();
                let session = self.orchestrator.create_session(title_ref).await?;
                println!(
                    "{} {} ({})",
                    "新会话:".green(),
                    session.title.cyan(),
                    session.id.to_string()[..8].dimmed()
                );
                self.current_session = session;
            }
            ReplCommand::ListSessions => {
                self.cmd_list_sessions();
            }
            ReplCommand::SwitchSession(id_prefix) => {
                self.cmd_switch_session(&id_prefix)?;
            }
            ReplCommand::ShowHistory(n) => {
                self.cmd_show_history(n.unwrap_or(20)).await?;
            }
            ReplCommand::ShowConfig => {
                self.cmd_show_config();
            }
            ReplCommand::SetConfig(key, value) => {
                self.config.set(&key, &value)?;
                println!("{} {} = {}", "已更新:".green(), key.cyan(), value.yellow());
            }
            ReplCommand::ShowStats => {
                self.cmd_show_stats()?;
            }
            ReplCommand::ExportSession(path) => {
                self.cmd_export_session(path).await?;
            }
        }
        Ok(false)
    }

    fn cmd_list_sessions(&self) {
        let sessions = self.orchestrator.sessions();
        if sessions.is_empty() {
            println!("{}", "暂无会话".dimmed());
            return;
        }
        println!();
        for s in sessions {
            let marker = if s.id == self.current_session.id {
                "▸"
            } else {
                " "
            };
            let state = format!("{:?}", s.state);
            println!(
                "  {} {} {} [{}] {}",
                marker.green(),
                s.id.to_string()[..8].cyan(),
                s.title.white(),
                state.dimmed(),
                s.last_active.format("%m-%d %H:%M").to_string().dimmed()
            );
        }
        println!();
    }

    fn cmd_switch_session(&mut self, id_prefix: &str) -> Result<()> {
        let sessions = self.orchestrator.sessions();
        let matched: Vec<&Session> = sessions
            .iter()
            .filter(|s| s.id.to_string().starts_with(id_prefix))
            .collect();

        match matched.len() {
            0 => anyhow::bail!("未找到匹配的会话: {id_prefix}"),
            1 => {
                self.current_session = matched[0].clone();
                println!(
                    "{} {}",
                    "已切换到:".green(),
                    self.current_session.title.cyan()
                );
            }
            _ => {
                println!("{}", "匹配到多个会话，请提供更长的 ID 前缀:".yellow());
                for s in matched {
                    println!("  {} {}", s.id.to_string()[..12].cyan(), s.title);
                }
            }
        }
        Ok(())
    }

    async fn cmd_show_history(&self, n: usize) -> Result<()> {
        let messages = self
            .orchestrator
            .session_messages(self.current_session.id)
            .await?;
        let start = messages.len().saturating_sub(n);
        println!();
        for msg in &messages[start..] {
            let (role_str, color_fn): (&str, fn(&str) -> colored::ColoredString) = match msg.role {
                Role::User => ("你", |s| s.green()),
                Role::Assistant => ("AI", |s| s.cyan()),
                Role::System => ("系统", |s| s.yellow()),
                Role::Tool => ("工具", |s| s.magenta()),
            };
            let time = msg.timestamp.format("%H:%M:%S");
            println!(
                "  {} [{}] {}",
                color_fn(role_str),
                time.to_string().dimmed(),
                msg.content
            );
        }
        println!();
        Ok(())
    }

    fn cmd_show_config(&self) {
        println!();
        println!("{}", "当前配置:".cyan().bold());
        for key in CliConfig::list_keys() {
            if let Ok(val) = self.config.get(key) {
                println!("  {} = {}", key.yellow(), val.white());
            }
        }
        println!();
        println!("  修改: {} ", "/config <key> <value>".dimmed());
        println!();
    }

    fn cmd_show_stats(&self) -> Result<()> {
        let usage = self
            .orchestrator
            .session_token_usage(self.current_session.id)?;
        println!();
        println!("{}", "会话统计:".cyan().bold());
        println!("  会话: {}", self.current_session.title.white());
        println!(
            "  ID:   {}",
            self.current_session.id.to_string()[..8].dimmed()
        );
        if let Some(u) = usage {
            println!("  Token 用量:");
            println!("    输入:  {}", u.prompt_tokens.to_string().yellow());
            println!("    输出:  {}", u.completion_tokens.to_string().yellow());
            println!("    总计:  {}", u.total_tokens.to_string().green().bold());
        } else {
            println!("  {}", "暂无 token 使用记录".dimmed());
        }
        println!();
        Ok(())
    }

    async fn cmd_export_session(&self, path: Option<String>) -> Result<()> {
        let messages = self
            .orchestrator
            .session_messages(self.current_session.id)
            .await?;
        let export = serde_json::json!({
            "session_id": self.current_session.id.to_string(),
            "title": self.current_session.title,
            "exported_at": chrono::Utc::now().to_rfc3339(),
            "message_count": messages.len(),
            "messages": messages.iter().map(|m| {
                serde_json::json!({
                    "role": format!("{:?}", m.role),
                    "content": m.content,
                    "timestamp": m.timestamp.to_rfc3339(),
                })
            }).collect::<Vec<_>>(),
        });

        let out_path = path.unwrap_or_else(|| {
            format!(
                "prometheus-session-{}.json",
                &self.current_session.id.to_string()[..8]
            )
        });
        std::fs::write(&out_path, serde_json::to_string_pretty(&export)?)?;
        println!("{} {}", "已导出到:".green(), out_path.cyan());
        Ok(())
    }

    /// 处理普通聊天消息
    async fn handle_chat(&mut self, input: &str) {
        let spinner = indicatif::ProgressBar::new_spinner();
        spinner.set_message("思考中...");
        spinner.enable_steady_tick(std::time::Duration::from_millis(80));

        match self
            .orchestrator
            .send_message(self.current_session.id, input)
            .await
        {
            Ok(turn) => {
                spinner.finish_and_clear();
                if self.config.show_route_info {
                    println!("  {} {}", "→".dimmed(), turn.route_reason.dimmed());
                }
                println!();
                println!("{}", turn.reply);
                println!();
            }
            Err(e) => {
                spinner.finish_and_clear();
                eprintln!("{} {e}", "请求失败:".red().bold());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_quit_commands() {
        assert_eq!(parse_command("/quit"), Some(ReplCommand::Quit));
        assert_eq!(parse_command("/exit"), Some(ReplCommand::Quit));
        assert_eq!(parse_command("/q"), Some(ReplCommand::Quit));
    }

    #[test]
    fn parse_switch_model() {
        assert_eq!(
            parse_command("/switch gpt-4"),
            Some(ReplCommand::SwitchModel("gpt-4".to_string()))
        );
    }

    #[test]
    fn parse_config_set() {
        assert_eq!(
            parse_command("/config max_tokens 4096"),
            Some(ReplCommand::SetConfig(
                "max_tokens".to_string(),
                "4096".to_string()
            ))
        );
    }

    #[test]
    fn non_command_returns_none() {
        assert_eq!(parse_command("hello world"), None);
        assert_eq!(parse_command(""), None);
    }

    #[test]
    fn unknown_command_returns_help() {
        assert_eq!(parse_command("/unknown"), Some(ReplCommand::Help));
    }
}
