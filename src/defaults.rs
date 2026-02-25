use once_cell::sync::Lazy;
use serde::Deserialize;

#[derive(Deserialize, Debug, Clone)]
pub struct ProviderDefaults {
    pub name:          String,
    pub base_url:      String,
    pub default_model: String,
    /// Fallback model list for providers that have no /models endpoint.
    #[serde(default)]
    pub models:        Vec<String>,
    /// Provider-specific API version header (e.g. Anthropic).
    pub api_version:   Option<String>,
    /// Provider-specific max_tokens value.
    pub max_tokens:    Option<u32>,
}

#[derive(Deserialize)]
struct ProvidersFile {
    provider: Vec<ProviderDefaults>,
}

static PROVIDERS: Lazy<Vec<ProviderDefaults>> = Lazy::new(|| {
    toml::from_str::<ProvidersFile>(include_str!("../providers.toml"))
        .expect("built-in providers.toml is invalid")
        .provider
});

/// All known provider defaults.
pub fn all() -> &'static [ProviderDefaults] {
    &PROVIDERS
}

/// Defaults for a specific provider type, e.g. "openai".
pub fn find(name: &str) -> Option<&'static ProviderDefaults> {
    PROVIDERS.iter().find(|p| p.name == name)
}
