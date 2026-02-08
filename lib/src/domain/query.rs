use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QueryReference {
    pub node_id: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QueryAnswer {
    pub query: String,
    pub response: String,
    pub references: Vec<QueryReference>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct QuerySession {
    pub entries: Vec<QueryAnswer>,
}
