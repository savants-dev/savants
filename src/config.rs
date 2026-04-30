use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct State {
    pub cloud_token: Option<String>,
    pub cloud_org: Option<String>,
}

impl State {
    pub fn load() -> Self {
        let path = dirs::home_dir()
            .unwrap_or_default()
            .join(".savants")
            .join("state.json");
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> Result<(), String> {
        let dir = dirs::home_dir().unwrap_or_default().join(".savants");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(dir.join("state.json"), json).map_err(|e| e.to_string())
    }

    pub fn is_cloud_authenticated(&self) -> bool {
        self.cloud_token.is_some()
    }
}
