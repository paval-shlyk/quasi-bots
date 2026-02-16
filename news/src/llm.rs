//todo: add healthcheck that gemini model is still available

pub struct GeminiConfig {
    pub api_key: String,
    pub model: gemini_rust::Model,

    pub summarize_instruction: String,
    ///Controls the randomness of the output.
    ///Note: The default value varies by model, see the Model.temperature attribute of the Model returned from the getModel function.
    ///Values can range from [0.0, 2.0].
    pub summarize_temperature: f32,
}

#[derive(Clone)]
pub struct GeminiApi {
    api: gemini_rust::Gemini,
    config: std::sync::Arc<GeminiConfig>,
}

impl GeminiApi {
    pub async fn connect(config: GeminiConfig) -> anyhow::Result<Self> {
        let api = gemini_rust::GeminiBuilder::new(&config.api_key)
            .with_model(config.model.clone())
            .build()?;

        Ok(Self {
            api,
            config: std::sync::Arc::new(config),
        })
    }

    /// Send text to LLM with prompt to summarize given text is   
    pub async fn summarize(&self, text: &str) -> anyhow::Result<String> {
        let text = format!("Text to process: {text}");

        let resp = self
            .api
            .generate_content()
            .with_temperature(self.config.summarize_temperature)
            .with_system_instruction(&self.config.summarize_instruction)
            .with_message(gemini_rust::Message::user(text))
            .execute()
            .await?;

        let parts = resp
            .candidates
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("No candidates available"))?
            .content
            .parts
            .unwrap_or_default();

        if parts.is_empty() {
            return Err(anyhow::anyhow!("No parts available"));
        }

        match &parts[0] {
            gemini_rust::Part::Text { text, .. } => Ok(text.clone()),
            part => Err(anyhow::anyhow!("Not supported part format: {part:?}")),
        }
    }
}
