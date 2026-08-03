#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ProviderConfig {
    pub base_url: String,
    pub api_key: String,
    pub api_secret: String,
}
