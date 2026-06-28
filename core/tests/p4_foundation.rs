use prometheus_core::iot::{
    DeviceDiscovery, GenericController, MockDeviceController, SecurePairing,
};
use prometheus_core::memory::UserProfile;
use prometheus_core::persona::{Persona, PersonaEngine};
use serde_json::json;
use uuid::Uuid;

#[tokio::test]
async fn discovered_device_can_be_paired_and_controlled() {
    // DeviceDiscovery uses built-in mock templates; scan returns default devices
    let mut discovery = DeviceDiscovery::new();
    let devices = discovery.scan(2).await.unwrap();
    assert!(!devices.is_empty());

    let device = &devices[0];

    // Pair the first discovered device
    let mut pairing = SecurePairing::new();
    let code = pairing.initiate(&device.id).await.unwrap();
    pairing.confirm(&code).await.unwrap();
    assert!(pairing.is_paired(&device.id));

    // Control via GenericController + MockDeviceController
    let mock_ctrl = MockDeviceController::new(&device.device_type);
    mock_ctrl.register_device(&device.id, &device.device_type);

    let mut controller = GenericController::new();
    controller.register_adapter(Box::new(mock_ctrl));

    controller
        .execute(&device.id, "turn_on", json!({}))
        .await
        .unwrap();
    let state = controller
        .execute(&device.id, "get_state", json!({}))
        .await
        .unwrap();
    assert_eq!(state["powered"], true);
}

#[tokio::test]
async fn profile_snapshot_and_persona_engine_work_together() {
    // Update profile from conversation — current API extracts tech/topic interests
    let mut profile = UserProfile::new("user-42");
    let messages = vec![
        "I'm learning rust and python.".to_string(),
        "Also interested in ai and web development.".to_string(),
    ];

    profile.update_from_conversation(&messages).await.unwrap();
    assert!(profile.interests.contains(&"rust".to_string()));
    assert!(profile.interests.contains(&"python".to_string()));
    assert!(profile.interests.contains(&"ai".to_string()));

    // Persona engine integration
    let mut engine = PersonaEngine::new();
    let persona = Persona {
        id: Uuid::new_v4(),
        name: "travel-guide".to_string(),
        system_prompt: "Be concise and travel-savvy.".to_string(),
        tone: "precise".to_string(),
        expertise: vec!["travel".to_string()],
    };
    let persona_id = persona.id;
    engine.load_persona(persona);
    engine.switch_persona(persona_id).unwrap();
    assert_eq!(engine.current_persona().unwrap().name, "travel-guide");
}
