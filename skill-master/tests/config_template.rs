#[tokio::test]
async fn serialize_deserialize_config() {
    let raw_config = tokio::fs::read_to_string("config.deb.toml")
        .await
        .expect("Failed to read config.example.toml");

    let config: skill_master::Config = toml::from_str(&raw_config)
        .expect("Failed to parse config.example.toml");

    let _raw_config =
        toml::to_string(&config).expect("Failed to serialize config");
}
