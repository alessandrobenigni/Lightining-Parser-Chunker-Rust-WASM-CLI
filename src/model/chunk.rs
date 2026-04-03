use serde::{Deserialize, Serialize};

use super::block::Block;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    pub id: String,
    pub text: String,
    pub token_count: usize,
    pub source_blocks: Vec<Block>,
    pub page_start: u32,
    pub page_end: u32,
    pub overlap_prefix: Option<String>,
    /// Minimum confidence of all source blocks (0.0 = no extraction, 1.0 = perfect).
    pub confidence: f32,
}

impl Chunk {
    /// Compute the minimum confidence from a set of source blocks.
    /// Returns 1.0 if the slice is empty.
    pub fn min_confidence(blocks: &[Block]) -> f32 {
        blocks
            .iter()
            .map(|b| b.confidence)
            .fold(f32::INFINITY, f32::min)
            .min(1.0) // clamp INFINITY to 1.0 for empty case
    }
}
