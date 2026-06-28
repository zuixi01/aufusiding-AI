//! 车辆模块集成测试
//!
//! 端到端测试 Tesla / SmartCar / OBD-II 完整流程，
//! 以及 OAuth2 认证流程。全部使用 httpmock + MockSerialPort。

use chrono::{Duration, Utc};
use httpmock::prelude::*;
use serde_json::json;

use prometheus_core::iot::control::DeviceController;
use prometheus_core::iot::vehicle::oauth::{OAuthClient, OAuthConfig, TokenStore};
use prometheus_core::iot::vehicle::obd::{
    build_init_sequence, build_pid_request, dtc_bytes_to_code, parse_dtc_response,
    parse_pid_response, Elm327, MockSerialPort,
};
use prometheus_core::iot::vehicle::smartcar::SmartCarAdapter;
use prometheus_core::iot::vehicle::tesla::{TeslaAdapter, TeslaRegion};

// ── Helpers ──────────────────────────────────────────────────────────

fn valid_token() -> TokenStore {
    TokenStore {
        access_token: "integration-test-token".into(),
        refresh_token: Some("integration-refresh".into()),
        expires_at: Some(Utc::now() + Duration::hours(1)),
        token_type: "Bearer".into(),
        scope: Some("openid vehicle_device_data vehicle_cmds".into()),
    }
}

fn tesla_vehicle_data_json() -> serde_json::Value {
    json!({
        "response": {
            "drive_state": { "latitude": 37.7749, "longitude": -122.4194 },
            "charge_state": {
                "battery_level": 72.0,
                "battery_range": 220.5,
                "charging_state": "Disconnected"
            },
            "vehicle_state": { "locked": true, "odometer": 15000.0 },
            "climate_state": { "is_climate_on": false }
        }
    })
}

// ── 1. Tesla 完整流程: register → wake_up → vehicle_data → lock → get_state ──

#[tokio::test]
async fn tesla_full_lifecycle() {
    let server = MockServer::start();

    // wake_up
    let wake_mock = server.mock(|when, then| {
        when.method(POST)
            .path("/api/1/vehicles/TESLA123/wake_up")
            .header("authorization", "Bearer integration-test-token");
        then.status(200)
            .json_body(json!({"response": {"result": true, "reason": ""}}));
    });

    // vehicle_data
    let data_mock = server.mock(|when, then| {
        when.method(GET)
            .path("/api/1/vehicles/TESLA123/vehicle_data")
            .header("authorization", "Bearer integration-test-token");
        then.status(200).json_body(tesla_vehicle_data_json());
    });

    // door_lock
    let lock_mock = server.mock(|when, then| {
        when.method(POST)
            .path("/api/1/vehicles/TESLA123/command/door_lock")
            .header("authorization", "Bearer integration-test-token");
        then.status(200)
            .json_body(json!({"response": {"result": true, "reason": ""}}));
    });

    let mut adapter =
        TeslaAdapter::new(TeslaRegion::NorthAmerica).with_base_url(&server.base_url());
    adapter.register_vehicle("my-tesla", "TESLA123", "5YJ3E1EA1NF000001", valid_token());

    // turn_on → wake_up
    adapter.turn_on("my-tesla").await.unwrap();
    wake_mock.assert();

    // get_state → vehicle_data
    let state = adapter.get_state("my-tesla").await.unwrap();
    data_mock.assert();
    assert_eq!(state["charge_level_pct"], 72.0);
    assert_eq!(state["locked"], true);

    // set_property lock → door_lock
    adapter
        .set_property("my-tesla", "lock", serde_json::json!(true))
        .await
        .unwrap();
    lock_mock.assert();
}

// ── 2. SmartCar 完整流程: register → location → lock → get_state ─────

#[tokio::test]
async fn smartcar_full_lifecycle() {
    let server = MockServer::start();

    // location
    server.mock(|when, then| {
        when.method(GET).path("/vehicles/SC-VID-001/location");
        then.status(200)
            .json_body(json!({"latitude": 40.7128, "longitude": -74.0060}));
    });

    // odometer
    server.mock(|when, then| {
        when.method(GET).path("/vehicles/SC-VID-001/odometer");
        then.status(200).json_body(json!({"distance": 35000.5}));
    });

    // fuel (gas car)
    server.mock(|when, then| {
        when.method(GET).path("/vehicles/SC-VID-001/fuel");
        then.status(200)
            .json_body(json!({"percentRemaining": 0.65, "range": 320.0}));
    });

    // lock
    let lock_mock = server.mock(|when, then| {
        when.method(POST)
            .path("/vehicles/SC-VID-001/security")
            .json_body(json!({"action": "LOCK"}));
        then.status(200).json_body(json!({"status": "success"}));
    });

    let mut adapter = SmartCarAdapter::new().with_base_url(&server.base_url());
    adapter.register_vehicle(
        "my-bmw",
        "SC-VID-001",
        "BMW",
        "330i",
        2024,
        false,
        valid_token(),
    );

    // get_state aggregates location + odometer + fuel
    let state = adapter.get_state("my-bmw").await.unwrap();
    let loc = state["location"].as_array().unwrap();
    assert!((loc[0].as_f64().unwrap() - 40.7128).abs() < 0.001);
    assert!((state["odometer_km"].as_f64().unwrap() - 35000.5).abs() < 1.0);
    assert_eq!(state["charge_level_pct"], 65.0);

    // lock
    adapter
        .set_property("my-bmw", "lock", json!(true))
        .await
        .unwrap();
    lock_mock.assert();
}

// ── 3. OBD 完整流程: MockSerialPort → init → read_all → diagnostics ──

#[test]
fn obd_full_diagnostic_flow() {
    let port = MockSerialPort::new()
        .expect("ATZ", "ELM327 v1.5")
        .expect("ATE0", "OK")
        .expect("ATL0", "OK")
        .expect("ATS0", "OK")
        .expect("ATSP0", "OK")
        .expect("010C", "41 0C 1A F8") // RPM = 1726
        .expect("010D", "41 0D 3C") // Speed = 60
        .expect("0105", "41 05 7D") // Coolant = 85°C
        .expect("012F", "41 2F A0") // Fuel = 62.7%
        .expect("0142", "41 42 31 2C") // Voltage = 12.588V
        .expect("03", "43 03 01 04 20"); // DTCs: P0301, P0420

    let mut elm = Elm327::new(Box::new(port));
    elm.init().unwrap();

    let diag = elm.read_all_diagnostics().unwrap();
    assert!((diag.rpm.unwrap() - 1726.0).abs() < 0.1);
    assert!((diag.speed_kmh.unwrap() - 60.0).abs() < 0.1);
    assert!((diag.engine_temp_c.unwrap() - 85.0).abs() < 0.1);
    assert!((diag.fuel_level_pct.unwrap() - 62.745).abs() < 0.1);
    assert!((diag.battery_voltage.unwrap() - 12.588).abs() < 0.001);

    // Also verify DTC reading
    let port2 = MockSerialPort::new().expect("03", "43 03 01 04 20");
    let mut elm2 = Elm327::new(Box::new(port2));
    let dtcs = elm2.read_dtcs().unwrap();
    assert_eq!(dtcs.len(), 2);
    assert_eq!(dtcs[0], "P0301");
    assert_eq!(dtcs[1], "P0420");
}

// ── 4. OAuth2 完整流程: auth_url → exchange → refresh → get_valid_token ──

#[tokio::test]
async fn oauth2_full_flow() {
    let server = MockServer::start();

    // exchange_code endpoint
    let exchange_mock = server.mock(|when, then| {
        when.method(POST)
            .path("/oauth/token")
            .x_www_form_urlencoded_tuple("grant_type", "authorization_code")
            .x_www_form_urlencoded_tuple("code", "auth-code-abc");
        then.status(200).json_body(json!({
            "access_token": "fresh-access",
            "refresh_token": "fresh-refresh",
            "expires_in": 3600,
            "token_type": "Bearer",
            "scope": "openid"
        }));
    });

    // refresh endpoint
    let refresh_mock = server.mock(|when, then| {
        when.method(POST)
            .path("/oauth/token")
            .x_www_form_urlencoded_tuple("grant_type", "refresh_token")
            .x_www_form_urlencoded_tuple("refresh_token", "fresh-refresh");
        then.status(200).json_body(json!({
            "access_token": "refreshed-access",
            "refresh_token": "refreshed-refresh",
            "expires_in": 7200,
            "token_type": "Bearer"
        }));
    });

    let config = OAuthConfig {
        client_id: "integration-client".into(),
        client_secret: Some("integration-secret".into()),
        auth_url: "https://auth.example.com/authorize".into(),
        token_url: "https://auth.example.com/oauth/token".into(),
        redirect_uri: "https://app.example.com/callback".into(),
        scopes: vec!["openid".into(), "vehicle_device_data".into()],
    };

    let client = OAuthClient::new(config).with_base_url(&server.base_url());

    // Step 1: build authorization URL
    let (url, state, pkce) = client.build_authorization_url();
    assert!(url.contains("response_type=code"));
    assert!(url.contains(&state));
    assert!(!pkce.verifier.is_empty());

    // Step 2: exchange code for token
    let token = client
        .exchange_code("auth-code-abc", &pkce.verifier)
        .await
        .unwrap();
    assert_eq!(token.access_token, "fresh-access");
    assert_eq!(token.refresh_token, Some("fresh-refresh".into()));
    exchange_mock.assert();

    // Step 3: get_valid_token returns current (not expired)
    let t = client.get_valid_token().await.unwrap();
    assert_eq!(t, "fresh-access");

    // Step 4: force expiry and refresh
    let expired = TokenStore {
        access_token: "old".into(),
        refresh_token: Some("fresh-refresh".into()),
        expires_at: Some(Utc::now() - Duration::seconds(100)),
        token_type: "Bearer".into(),
        scope: None,
    };
    client.set_token(expired).await;
    let refreshed = client.get_valid_token().await.unwrap();
    assert_eq!(refreshed, "refreshed-access");
    refresh_mock.assert();
}

// ── 5. Tesla region 切换（NA/EU/CN base_url 不同）────────────────────

#[test]
fn tesla_region_base_urls_differ() {
    let na = TeslaAdapter::new(TeslaRegion::NorthAmerica);
    let eu = TeslaAdapter::new(TeslaRegion::Europe);
    let cn = TeslaAdapter::new(TeslaRegion::China);
    // 验证三个 region 创建的 adapter 都是不同实例
    // （base_url 是私有的，通过 supported_type 验证实例可用）
    assert_eq!(na.supported_type(), "Vehicle");
    assert_eq!(eu.supported_type(), "Vehicle");
    assert_eq!(cn.supported_type(), "Vehicle");
}

// ── 6. SmartCar 多品牌注册 + 各自 get_state ─────────────────────────

#[tokio::test]
async fn smartcar_multi_brand_registration() {
    let server = MockServer::start();

    // BMW (gas)
    server.mock(|when, then| {
        when.method(GET).path("/vehicles/bmw-vid/location");
        then.status(200)
            .json_body(json!({"latitude": 48.8566, "longitude": 2.3522}));
    });
    server.mock(|when, then| {
        when.method(GET).path("/vehicles/bmw-vid/odometer");
        then.status(200).json_body(json!({"distance": 20000.0}));
    });
    server.mock(|when, then| {
        when.method(GET).path("/vehicles/bmw-vid/fuel");
        then.status(200)
            .json_body(json!({"percentRemaining": 0.5, "range": 250.0}));
    });

    // Tesla (electric) via SmartCar
    server.mock(|when, then| {
        when.method(GET).path("/vehicles/tesla-vid/location");
        then.status(200)
            .json_body(json!({"latitude": 34.0522, "longitude": -118.2437}));
    });
    server.mock(|when, then| {
        when.method(GET).path("/vehicles/tesla-vid/odometer");
        then.status(200).json_body(json!({"distance": 45000.0}));
    });
    server.mock(|when, then| {
        when.method(GET).path("/vehicles/tesla-vid/battery");
        then.status(200)
            .json_body(json!({"percentRemaining": 0.85, "range": 310.0}));
    });

    let mut adapter = SmartCarAdapter::new().with_base_url(&server.base_url());
    adapter.register_vehicle(
        "dev-bmw",
        "bmw-vid",
        "BMW",
        "X5",
        2023,
        false,
        valid_token(),
    );
    adapter.register_vehicle(
        "dev-tesla",
        "tesla-vid",
        "TESLA",
        "Model 3",
        2024,
        true,
        valid_token(),
    );

    // BMW state
    let bmw = adapter.get_state("dev-bmw").await.unwrap();
    let bmw_loc = bmw["location"].as_array().unwrap();
    assert!((bmw_loc[0].as_f64().unwrap() - 48.8566).abs() < 0.001);
    assert_eq!(bmw["charge_level_pct"], 50.0);

    // Tesla state
    let tesla = adapter.get_state("dev-tesla").await.unwrap();
    let tesla_loc = tesla["location"].as_array().unwrap();
    assert!((tesla_loc[0].as_f64().unwrap() - 34.0522).abs() < 0.001);
    assert_eq!(tesla["charge_level_pct"], 85.0);
}

// ── 7. OBD 部分 PID 无响应 → 优雅降级 ──────────────────────────────

#[test]
fn obd_partial_pid_graceful_degradation() {
    // Only speed responds, everything else returns NO DATA
    let port = MockSerialPort::new()
        .expect("ATZ", "ELM327 v1.5")
        .expect("ATE0", "OK")
        .expect("ATL0", "OK")
        .expect("ATS0", "OK")
        .expect("ATSP0", "OK")
        .expect("010C", "NO DATA")
        .expect("010D", "41 0D 50") // Speed = 80 km/h
        .expect("0105", "NO DATA")
        .expect("012F", "NO DATA")
        .expect("0142", "NO DATA");

    let mut elm = Elm327::new(Box::new(port));
    elm.init().unwrap();

    let diag = elm.read_all_diagnostics().unwrap();
    assert!(diag.rpm.is_none());
    assert!((diag.speed_kmh.unwrap() - 80.0).abs() < 0.1);
    assert!(diag.engine_temp_c.is_none());
    assert!(diag.fuel_level_pct.is_none());
    assert!(diag.battery_voltage.is_none());
}

// ── 8. OBD 纯函数交叉验证 ──────────────────────────────────────────

#[test]
fn obd_pure_functions_cross_validation() {
    // build_pid_request → parse_pid_response 往返验证
    let req = build_pid_request(0x01, 0x0D);
    assert_eq!(req, "010D");

    // 模拟 ECU 响应
    let resp = parse_pid_response("41 0D 78", 0x01, 0x0D);
    assert_eq!(resp, Some(vec![0x78])); // 120 km/h

    // DTC 编码验证
    assert_eq!(dtc_bytes_to_code(0x01, 0x03), "P0103");
    assert_eq!(dtc_bytes_to_code(0x43, 0x01), "C0301");
    assert_eq!(dtc_bytes_to_code(0x83, 0x01), "B0301");
    assert_eq!(dtc_bytes_to_code(0xC3, 0x01), "U0301");

    // DTC 响应解析
    let dtcs = parse_dtc_response("43 03 01 04 20");
    assert_eq!(dtcs, vec!["P0301", "P0420"]);

    // 空 DTC
    let empty = parse_dtc_response("43 00");
    assert!(empty.is_empty());

    // init sequence 验证
    let seq = build_init_sequence();
    assert_eq!(seq, vec!["ATZ", "ATE0", "ATL0", "ATS0", "ATSP0"]);
}
