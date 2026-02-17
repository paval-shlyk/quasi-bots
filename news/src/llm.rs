#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
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
    config: std::sync::Arc<GeminiConfig>,
}

impl GeminiApi {
    pub async fn connect(config: GeminiConfig) -> anyhow::Result<Self> {
        //fixme: it seems that's better to build single gemini_rust::Gemini
        //instance. But sending too much requests cause
        //reqwest::Client fails sometimes (i guess it mutate some internal state)
        Ok(Self {
            config: std::sync::Arc::new(config),
        })
    }

    pub async fn summarize_all(
        &self,
        texts: Vec<String>,
    ) -> anyhow::Result<()> {
        let api = gemini_rust::GeminiBuilder::new(&self.config.api_key)
            .with_model(self.config.model.clone())
            .build()?;

        let requests = texts
            .into_iter()
            // .enumerate()
            .map(|text| {
                api.generate_content()
                    .with_temperature(self.config.summarize_temperature)
                    .with_system_instruction(&self.config.summarize_instruction)
                    .with_message(gemini_rust::Message::user(text))
                    .build()
                // (id, req)
            })
            .collect::<Vec<_>>();

        let _handle = api
            .batch_generate_content()
            .with_requests(requests)
            .execute()
            .await?;

        todo!()
    }

    /// Send text to LLM with prompt to summarize given text is   
    pub async fn summarize(&self, text: &str) -> anyhow::Result<String> {
        telemetry::execution_time!("Gemini summarize");

        let text = format!("Text to process: {text}");

        let api = gemini_rust::GeminiBuilder::new(&self.config.api_key)
            .with_model(self.config.model.clone())
            .build()?;

        let resp = api
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
