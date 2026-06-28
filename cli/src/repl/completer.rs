//! REPL 命令补全器

use rustyline::completion::{Completer, Pair};
use rustyline::highlight::Highlighter;
use rustyline::hint::{Hinter, HistoryHinter};
use rustyline::validate::Validator;
use rustyline::{Context, Helper};

const COMMANDS: &[(&str, &str)] = &[
    ("/quit", "退出 REPL"),
    ("/exit", "退出 REPL"),
    ("/clear", "清屏"),
    ("/help", "显示帮助"),
    ("/switch", "切换模型"),
    ("/new", "新建会话"),
    ("/sessions", "列出会话"),
    ("/use", "切换会话"),
    ("/history", "查看历史"),
    ("/config", "查看/修改配置"),
    ("/stats", "使用统计"),
    ("/export", "导出会话"),
    ("/memory", "查询记忆"),
];

pub struct PrometheusCompleter {
    hinter: HistoryHinter,
}

impl PrometheusCompleter {
    pub fn new() -> Self {
        Self {
            hinter: HistoryHinter::new(),
        }
    }
}

impl Completer for PrometheusCompleter {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        if !line.starts_with('/') {
            return Ok((0, vec![]));
        }

        let prefix = &line[..pos];
        let matches: Vec<Pair> = COMMANDS
            .iter()
            .filter(|(cmd, _)| cmd.starts_with(prefix))
            .map(|(cmd, desc)| Pair {
                display: format!("{cmd}  {desc}"),
                replacement: format!("{cmd} "),
            })
            .collect();

        Ok((0, matches))
    }
}

impl Hinter for PrometheusCompleter {
    type Hint = String;

    fn hint(&self, line: &str, pos: usize, ctx: &Context<'_>) -> Option<String> {
        self.hinter.hint(line, pos, ctx)
    }
}

impl Highlighter for PrometheusCompleter {}
impl Validator for PrometheusCompleter {}
impl Helper for PrometheusCompleter {}
