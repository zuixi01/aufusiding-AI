//! 导航 + 应用控制 + 文件管理测试

#[cfg(test)]
mod unit {
    use crate::plugins::capabilities::app_control::AppControlCapability;
    use crate::plugins::capabilities::app_control_platform::MockPlatformController;
    use crate::plugins::capabilities::file_indexer::FileIndexer;
    use crate::plugins::capabilities::navigation::NavigationCapability;
    use crate::plugins::capabilities::navigation_provider::*;

    // ─── Navigation ────────────────────────────────────

    #[tokio::test]
    async fn navigation_plan_route() {
        let nav = NavigationCapability::new(Box::new(MockNavigationProvider));
        let from = Location {
            lat: 39.9,
            lng: 116.4,
            name: Some("起点".into()),
        };
        let to = Location {
            lat: 40.0,
            lng: 116.5,
            name: Some("终点".into()),
        };

        let route = nav
            .plan_route(&from, &to, TravelMode::Driving)
            .await
            .unwrap();
        assert!(!route.steps.is_empty());
        assert!(route.distance_m > 0.0);
    }

    #[tokio::test]
    async fn navigation_geocode() {
        let nav = NavigationCapability::new(Box::new(MockNavigationProvider));
        let loc = nav.geocode("北京天安门").await.unwrap();
        assert!(loc.name.is_some());
    }

    #[tokio::test]
    async fn navigation_eta() {
        let nav = NavigationCapability::new(Box::new(MockNavigationProvider));
        let from = Location {
            lat: 39.9,
            lng: 116.4,
            name: None,
        };
        let to = Location {
            lat: 40.0,
            lng: 116.5,
            name: None,
        };
        let route = nav
            .plan_route(&from, &to, TravelMode::Driving)
            .await
            .unwrap();
        let eta = nav.eta(&route);
        assert!(eta.as_secs() > 0);
    }

    #[tokio::test]
    async fn navigation_voice_instructions() {
        let nav = NavigationCapability::new(Box::new(MockNavigationProvider));
        let from = Location {
            lat: 39.9,
            lng: 116.4,
            name: None,
        };
        let to = Location {
            lat: 40.0,
            lng: 116.5,
            name: None,
        };
        let route = nav
            .plan_route(&from, &to, TravelMode::Driving)
            .await
            .unwrap();
        let instructions = nav.voice_instructions(&route);
        assert!(instructions.len() >= 3); // 开头 + 步骤 + 到达
        assert!(instructions[0].contains("公里"));
        assert!(instructions.last().unwrap().contains("到达"));
    }

    #[test]
    fn travel_mode_serde() {
        let mode = TravelMode::Transit;
        let json = serde_json::to_string(&mode).unwrap();
        let back: TravelMode = serde_json::from_str(&json).unwrap();
        assert_eq!(mode, back);
    }

    #[test]
    fn location_serde() {
        let loc = Location {
            lat: 39.9,
            lng: 116.4,
            name: Some("北京".into()),
        };
        let json = serde_json::to_string(&loc).unwrap();
        let back: Location = serde_json::from_str(&json).unwrap();
        assert_eq!(back.lat, 39.9);
    }

    // ─── AppControl ────────────────────────────────────

    #[tokio::test]
    async fn app_control_launch_and_list() {
        let mock = MockPlatformController::new();
        let ctrl = AppControlCapability::new(Box::new(mock));
        assert_eq!(ctrl.platform_name(), "mock");

        ctrl.launch_app("微信").await.unwrap();
        let apps = ctrl.list_running_apps().await.unwrap();
        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].name, "微信");
    }

    #[tokio::test]
    async fn app_control_switch_and_close() {
        let mock = MockPlatformController::new();
        let ctrl = AppControlCapability::new(Box::new(mock));

        ctrl.launch_app("微信").await.unwrap();
        ctrl.launch_app("浏览器").await.unwrap();

        ctrl.switch_to_app("微信").await.unwrap();
        let apps = ctrl.list_running_apps().await.unwrap();
        let wechat = apps.iter().find(|a| a.name == "微信").unwrap();
        assert!(wechat.is_foreground);

        ctrl.close_app("浏览器").await.unwrap();
        let apps = ctrl.list_running_apps().await.unwrap();
        assert_eq!(apps.len(), 1);
    }

    #[tokio::test]
    async fn app_control_type_text() {
        let mock = MockPlatformController::new();
        let ctrl = AppControlCapability::new(Box::new(mock));
        ctrl.type_text("你好").await.unwrap();
    }

    #[tokio::test]
    async fn app_control_launch_duplicate_fails() {
        let mock = MockPlatformController::new();
        let ctrl = AppControlCapability::new(Box::new(mock));
        ctrl.launch_app("微信").await.unwrap();
        assert!(ctrl.launch_app("微信").await.is_err());
    }

    #[tokio::test]
    async fn app_control_close_nonexistent_fails() {
        let mock = MockPlatformController::new();
        let ctrl = AppControlCapability::new(Box::new(mock));
        assert!(ctrl.close_app("不存在").await.is_err());
    }

    // ─── FileIndexer ───────────────────────────────────

    #[test]
    fn file_indexer_index_and_search() {
        let mut indexer = FileIndexer::new();
        // 索引当前 crate 的 src 目录
        let count = indexer
            .index_directory(std::path::Path::new("src"))
            .unwrap();
        assert!(count > 0);
        assert!(indexer.entry_count() > 0);

        // 搜索 mod.rs
        let results = indexer.search("mod");
        assert!(!results.is_empty());
    }

    #[test]
    fn file_indexer_by_extension() {
        let mut indexer = FileIndexer::new();
        indexer
            .index_directory(std::path::Path::new("src"))
            .unwrap();
        let rs_files = indexer.by_extension("rs");
        assert!(!rs_files.is_empty());
    }

    #[test]
    fn file_indexer_recent_files() {
        let mut indexer = FileIndexer::new();
        indexer
            .index_directory(std::path::Path::new("src"))
            .unwrap();
        let recent = indexer.recent_files(5);
        assert!(recent.len() <= 5);
    }

    #[test]
    fn file_indexer_nonexistent_path_fails() {
        let mut indexer = FileIndexer::new();
        assert!(indexer
            .index_directory(std::path::Path::new("/nonexistent/path"))
            .is_err());
    }

    #[test]
    fn file_indexer_clear() {
        let mut indexer = FileIndexer::new();
        indexer
            .index_directory(std::path::Path::new("src"))
            .unwrap();
        assert!(indexer.entry_count() > 0);
        indexer.clear();
        assert_eq!(indexer.entry_count(), 0);
    }
}
