pub mod by_page;
pub mod by_structure;
pub mod by_title;
pub mod fixed_size;

use crate::cli::ChunkStrategy;
use crate::model::{Block, Chunk};

/// Count tokens using the cl100k_base BPE tokenizer (same as OpenAI's tokenizer).
/// Returns accurate token counts matching OpenAI's tiktoken cl100k_base encoding.
/// The underlying tokenizer is a `&'static Tokenizer` so no per-call allocation.
pub fn estimate_tokens(text: &str) -> usize {
    if text.is_empty() {
        return 0;
    }
    bpe_openai::cl100k_base().count(text)
}

/// Fast token estimation using byte-length heuristic.
/// Much faster than BPE but approximate. Suitable for chunking boundary decisions
/// where exact counts are not critical. Uses a ratio of ~5 bytes per token which
/// is slightly conservative (underestimates) to avoid over-splitting compared
/// to real BPE (cl100k_base averages ~4-5 chars per token for English).
#[inline]
pub fn fast_estimate_tokens(text: &str) -> usize {
    text.len().div_ceil(5)
}

/// Dispatch chunking based on the chosen strategy.
pub fn chunk_blocks(
    blocks: &[Block],
    strategy: &ChunkStrategy,
    max_tokens: usize,
    overlap: usize,
) -> Result<Vec<Chunk>, crate::Error> {
    match strategy {
        ChunkStrategy::ByStructure => by_structure::chunk(blocks, max_tokens, overlap),
        ChunkStrategy::ByTitle => by_title::chunk(blocks, max_tokens, overlap),
        ChunkStrategy::ByPage => by_page::chunk(blocks, max_tokens, overlap),
        ChunkStrategy::Fixed => fixed_size::chunk(blocks, max_tokens, overlap),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimate_tokens_empty() {
        assert_eq!(estimate_tokens(""), 0);
    }

    #[test]
    fn test_estimate_tokens_short() {
        // "hello" is 1 token in cl100k_base
        let count = estimate_tokens("hello");
        assert_eq!(count, 1);
    }

    #[test]
    fn test_estimate_tokens_sentence() {
        // "Hello, world!" is typically 4 tokens in cl100k_base
        let count = estimate_tokens("Hello, world!");
        assert!(count > 0 && count < 10, "Expected small token count, got {}", count);
    }

    #[test]
    fn test_estimate_tokens_longer() {
        // Repeated 'a' characters - BPE will encode efficiently
        let text = "a".repeat(100);
        let count = estimate_tokens(&text);
        // Real BPE should produce some tokens (not necessarily 25 like the heuristic)
        assert!(count > 0, "Expected nonzero token count");
    }

    #[test]
    fn test_estimate_tokens_matches_known_values() {
        // cl100k_base tokenization: "tiktoken is great!" => 6 tokens
        // (tik, token, is, great, !)  -- bpe-openai splits "tiktoken" into 2 tokens
        let count = estimate_tokens("tiktoken is great!");
        assert_eq!(count, 6, "Expected 6 tokens for 'tiktoken is great!', got {}", count);
    }
}
