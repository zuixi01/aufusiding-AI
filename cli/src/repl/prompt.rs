//! REPL 提示符渲染器

use colored::Colorize;

pub struct PromptRenderer;

impl PromptRenderer {
    /// 渲染 REPL 提示符
    ///
    /// 格式: [会话名] (模型) ❯
    pub fn render(session_title: &str, model_override: Option<&str>) -> String {
        let session = session_title.cyan();
        if let Some(model) = model_override {
            format!("[{session}] ({}) ❯ ", model.yellow())
        } else {
            format!("[{session}] ❯ ")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_without_model() {
        let prompt = PromptRenderer::render("默认会话", None);
        assert!(prompt.contains("默认会话"));
        assert!(prompt.contains("❯"));
    }

    #[test]
    fn prompt_with_model() {
        let prompt = PromptRenderer::render("测试", Some("gpt-4"));
        assert!(prompt.contains("测试"));
        assert!(prompt.contains("gpt-4"));
    }
}
