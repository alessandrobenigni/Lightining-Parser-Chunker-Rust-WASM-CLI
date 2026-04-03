use crate::chunking::{estimate_tokens, fast_estimate_tokens};
use crate::model::{Block, Chunk, ElementType};

/// Build a chunk from accumulated blocks, applying overlap from the previous chunk.
fn build_chunk(
    id: usize,
    blocks: &[Block],
    overlap_text: Option<String>,
) -> Chunk {
    let text: String = blocks.iter().map(|b| b.text.as_str()).collect::<Vec<_>>().join("\n");
    let full_text = match &overlap_text {
        Some(prefix) if !prefix.is_empty() => format!("{}\n{}", prefix, text),
        _ => text,
    };
    // Use accurate BPE only for the final chunk metadata
    let token_count = estimate_tokens(&full_text);
    let page_start = blocks.iter().map(|b| b.page).min().unwrap_or(0);
    let page_end = blocks.iter().map(|b| b.page).max().unwrap_or(0);
    let confidence = Chunk::min_confidence(blocks);

    Chunk {
        id: format!("chunk-{}", id),
        text: full_text,
        token_count,
        source_blocks: blocks.to_vec(),
        page_start,
        page_end,
        overlap_prefix: overlap_text,
        confidence,
    }
}

/// Extract overlap text from the end of a chunk's source blocks.
/// Uses fast estimation since exact token count is not critical here.
fn extract_overlap_text(blocks: &[Block], overlap_tokens: usize) -> String {
    if overlap_tokens == 0 || blocks.is_empty() {
        return String::new();
    }

    // Walk backwards through blocks, collecting text until we have enough tokens
    let mut collected = Vec::new();
    let mut tokens_so_far = 0;

    for block in blocks.iter().rev() {
        let block_tokens = fast_estimate_tokens(&block.text);
        collected.push(block.text.as_str());
        tokens_so_far += block_tokens;
        if tokens_so_far >= overlap_tokens {
            break;
        }
    }

    collected.reverse();
    collected.join("\n")
}

/// Element-aware chunking: accumulates blocks until adding the next would exceed
/// max_tokens. Tables and CodeBlocks are never split — if one exceeds max_tokens
/// it becomes its own chunk.
pub fn chunk(
    blocks: &[Block],
    max_tokens: usize,
    overlap: usize,
) -> Result<Vec<Chunk>, crate::Error> {
    if blocks.is_empty() {
        return Ok(Vec::new());
    }

    let mut chunks = Vec::new();
    let mut current_blocks: Vec<Block> = Vec::new();
    let mut current_tokens: usize = 0;
    let mut overlap_text: Option<String> = None;

    for block in blocks {
        // Use fast heuristic for accumulation decisions (exact count in build_chunk)
        let block_tokens = fast_estimate_tokens(&block.text);
        let is_unsplittable = matches!(
            block.element_type,
            ElementType::Table | ElementType::CodeBlock
        );

        // If this unsplittable block exceeds max_tokens on its own, flush current
        // and emit it as a standalone chunk.
        if is_unsplittable && block_tokens > max_tokens {
            // Flush accumulated blocks first
            if !current_blocks.is_empty() {
                let chunk = build_chunk(chunks.len(), &current_blocks, overlap_text.take());
                overlap_text = Some(extract_overlap_text(&current_blocks, overlap));
                chunks.push(chunk);
                current_blocks.clear();
                current_tokens = 0;
            }

            // Emit the oversized block as its own chunk
            let chunk = build_chunk(chunks.len(), std::slice::from_ref(block), overlap_text.take());
            overlap_text = Some(extract_overlap_text(std::slice::from_ref(block), overlap));
            chunks.push(chunk);
            continue;
        }

        // Would adding this block exceed the limit?
        if current_tokens + block_tokens > max_tokens && !current_blocks.is_empty() {
            // Flush current chunk
            let chunk = build_chunk(chunks.len(), &current_blocks, overlap_text.take());
            overlap_text = Some(extract_overlap_text(&current_blocks, overlap));
            chunks.push(chunk);
            current_blocks.clear();
            current_tokens = 0;
        }

        current_blocks.push(block.clone());
        current_tokens += block_tokens;
    }

    // Flush remaining blocks
    if !current_blocks.is_empty() {
        let chunk = build_chunk(chunks.len(), &current_blocks, overlap_text.take());
        chunks.push(chunk);
    }

    Ok(chunks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Block;

    fn make_block(text: &str, element_type: ElementType) -> Block {
        let mut b = Block::new(element_type, text);
        b.page = 1;
        b
    }

    #[test]
    fn test_empty_blocks() {
        let result = chunk(&[], 100, 10).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_single_block_fits() {
        let blocks = vec![make_block("Hello world", ElementType::NarrativeText)];
        let result = chunk(&blocks, 100, 0).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "chunk-0");
        assert!(result[0].text.contains("Hello world"));
    }

    #[test]
    fn test_blocks_split_at_max_tokens() {
        // With real BPE, repeated single chars compress well.
        // Use realistic text that generates enough tokens to force a split.
        let blocks = vec![
            make_block("The quick brown fox jumps over the lazy dog and runs across the field", ElementType::NarrativeText),
            make_block("Another paragraph with completely different words about technology and science", ElementType::NarrativeText),
        ];
        // max_tokens=10 should force a split since each block is > 10 tokens
        let result = chunk(&blocks, 10, 0).unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_oversized_table_becomes_own_chunk() {
        let blocks = vec![
            make_block("short", ElementType::NarrativeText),
            make_block(&"x".repeat(500), ElementType::Table),
            make_block("after", ElementType::NarrativeText),
        ];
        let result = chunk(&blocks, 20, 0).unwrap();
        assert!(result.len() >= 3);
        // The table chunk should contain only the table text
        assert!(result[1].text.contains(&"x".repeat(100)));
    }

    #[test]
    fn test_overlap_applied() {
        let blocks = vec![
            make_block("The quick brown fox jumps over the lazy dog and runs across the field", ElementType::NarrativeText),
            make_block("Another paragraph with completely different words about technology and science", ElementType::NarrativeText),
        ];
        let result = chunk(&blocks, 10, 5).unwrap();
        assert_eq!(result.len(), 2);
        // Second chunk should have overlap prefix
        assert!(result[1].overlap_prefix.is_some());
    }
}
