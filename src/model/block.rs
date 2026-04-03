use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::element::ElementType;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoundingBox {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableData {
    pub rows: Vec<Vec<String>>,
    pub headers: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub element_type: ElementType,
    pub text: String,
    pub bbox: Option<BoundingBox>,
    pub page: u32,
    pub confidence: f32,
    pub hierarchy: Vec<String>,
    pub metadata: HashMap<String, serde_json::Value>,
    pub table_data: Option<TableData>,
}

impl Block {
    pub fn new(element_type: ElementType, text: impl Into<String>) -> Self {
        Self {
            element_type,
            text: text.into(),
            bbox: None,
            page: 0,
            confidence: 1.0,
            hierarchy: Vec::new(),
            metadata: HashMap::new(),
            table_data: None,
        }
    }
}

impl Default for Block {
    fn default() -> Self {
        Self {
            element_type: ElementType::NarrativeText,
            text: String::new(),
            bbox: None,
            page: 1,
            confidence: 1.0,
            hierarchy: Vec::new(),
            metadata: HashMap::new(),
            table_data: None,
        }
    }
}
