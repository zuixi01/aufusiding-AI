//! 浏览器能力测试

#[cfg(test)]
mod unit {
    use crate::plugins::capabilities::browser::BrowserCapability;
    use crate::plugins::capabilities::browser_actions::{BrowserAction, BrowserResult};
    use crate::plugins::capabilities::browser_pool::BrowserPool;
    use crate::plugins::capability_bridge::CapabilityBridge;
    use crate::plugins::capability_registry::CapabilityRegistry;

    // ─── BrowserPool ───────────────────────────────────

    #[test]
    fn pool_acquire_release() {
        let pool = BrowserPool::new(3);
        let id1 = pool.acquire().unwrap();
        let id2 = pool.acquire().unwrap();
        assert_ne!(id1, id2);
        assert_eq!(pool.active_count(), 2);
        assert_eq!(pool.total_count(), 2);

        pool.release(id1);
        assert_eq!(pool.active_count(), 1);

        // 复用释放的实例
        let id3 = pool.acquire().unwrap();
        assert_eq!(id3, id1);
    }

    #[test]
    fn pool_max_instances() {
        let pool = BrowserPool::new(2);
        pool.acquire().unwrap();
        pool.acquire().unwrap();
        assert!(pool.acquire().is_err());
    }

    // ─── BrowserAction ─────────────────────────────────

    #[test]
    fn browser_action_serde() {
        let action = BrowserAction::Navigate {
            url: "https://example.com".into(),
        };
        let json = serde_json::to_string(&action).unwrap();
        let back: BrowserAction = serde_json::from_str(&json).unwrap();
        match back {
            BrowserAction::Navigate { url } => assert_eq!(url, "https://example.com"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn browser_result_ok() {
        let r = BrowserResult::ok("data".into());
        assert!(r.success);
        assert_eq!(r.data, Some("data".into()));
    }

    #[test]
    fn browser_result_err() {
        let r = BrowserResult::err("fail".into());
        assert!(!r.success);
        assert_eq!(r.error, Some("fail".into()));
    }

    // ─── BrowserCapability ─────────────────────────────

    #[tokio::test]
    async fn browser_domain_whitelist() {
        let browser = BrowserCapability::new(2).with_allowed_domains(vec!["example.com".into()]);

        // 无远程服务时，白名单域名也应返回错误（需要配置远程浏览器）
        let result = browser.screenshot("https://example.com/page").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("远程浏览器"));

        // 非白名单域名返回不同的错误
        let result = browser.screenshot("https://evil.com/page").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("白名单"));
    }

    #[tokio::test]
    async fn browser_execute_actions() {
        let browser = BrowserCapability::new(2);
        let actions = vec![
            BrowserAction::Navigate {
                url: "https://example.com".into(),
            },
            BrowserAction::ExtractText {
                selector: "h1".into(),
            },
        ];
        let result = browser.execute_actions(&actions).await.unwrap();
        assert!(result.success);
    }

    #[tokio::test]
    async fn browser_research() {
        let browser = BrowserCapability::new(2);
        let result = browser
            .research("test", &["site1.com", "site2.com"])
            .await
            .unwrap();
        assert!(result.contains("site1.com"));
    }

    #[tokio::test]
    async fn browser_extract_text() {
        let browser = BrowserCapability::new(2);
        let text = browser
            .extract_text("https://example.com", "h1")
            .await
            .unwrap();
        assert!(!text.is_empty());
    }

    // ─── CapabilityRegistry ────────────────────────────

    #[test]
    fn registry_register_and_list() {
        let mut reg = CapabilityRegistry::new();
        reg.register("browser", "浏览器自动化", "1.0").unwrap();
        reg.register("navigation", "导航", "1.0").unwrap();
        assert_eq!(reg.count(), 2);
        assert!(reg.is_enabled("browser"));
    }

    #[test]
    fn registry_duplicate_fails() {
        let mut reg = CapabilityRegistry::new();
        reg.register("browser", "浏览器", "1.0").unwrap();
        assert!(reg.register("browser", "重复", "1.0").is_err());
    }

    #[test]
    fn registry_enable_disable() {
        let mut reg = CapabilityRegistry::new();
        reg.register("browser", "浏览器", "1.0").unwrap();
        reg.disable("browser").unwrap();
        assert!(!reg.is_enabled("browser"));
        assert_eq!(reg.list_enabled().len(), 0);

        reg.enable("browser").unwrap();
        assert!(reg.is_enabled("browser"));
    }

    #[test]
    fn registry_unknown_capability() {
        let mut reg = CapabilityRegistry::new();
        assert!(reg.enable("nonexistent").is_err());
        assert!(!reg.is_enabled("nonexistent"));
    }

    // ─── CapabilityBridge ──────────────────────────────

    #[test]
    fn bridge_builds_tools_from_registry() {
        let mut reg = CapabilityRegistry::new();
        reg.register("browser", "浏览器", "1.0").unwrap();
        reg.register("navigation", "导航", "1.0").unwrap();
        reg.register("app_control", "应用控制", "1.0").unwrap();
        reg.register("file_manager", "文件管理", "1.0").unwrap();

        let mut bridge = CapabilityBridge::new();
        bridge.build_from_registry(&reg);

        assert!(bridge.tool_count() > 0);
        assert!(!bridge.tools_for("browser").is_empty());
        assert!(!bridge.tools_for("navigation").is_empty());
    }

    #[test]
    fn bridge_excludes_disabled_capabilities() {
        let mut reg = CapabilityRegistry::new();
        reg.register("browser", "浏览器", "1.0").unwrap();
        reg.register("navigation", "导航", "1.0").unwrap();
        reg.disable("browser").unwrap();

        let mut bridge = CapabilityBridge::new();
        bridge.build_from_registry(&reg);

        assert!(bridge.tools_for("browser").is_empty());
        assert!(!bridge.tools_for("navigation").is_empty());
    }
}
