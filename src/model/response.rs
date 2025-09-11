use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterfacesResponse {
    pub name: Vec<String>,
}
