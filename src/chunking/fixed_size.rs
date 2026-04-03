use crate::chunking::{estimate_tokens, fast_estimate_tokens};
use crate::model::{Block, Chunk};


/// Token-count chunking: concatenates all block text and splits at estimated
/// token boundaries with configurable overlap. Ignores element boundaries entirely.
pub fn chunk(
    blocks: &[Block],
    max_tokens: usize,
    overlap: usize,
) -> Result<Vec<Chunk>, crate::Error> {
    if blocks.is_empty() {
        return Ok(Vec::new());
    }

    // Concatenate all block text
    let full_text: String = blocks.iter().map(|b| b.text.as_str()).collect::<Vec<_>>().join("\n");

    if full_text.is_empty() {
        return Ok(Vec::new());
    }

    // Use fast estimation for the fits-in-one-chunk decision
    let total_tokens_fast = fast_estimate_tokens(&full_text);
    if total_tokens_fast <= max_tokens {
        let total_tokens = estimate_tokens(&full_text);
        let page_start = blocks.iter().map(|b| b.page).min().unwrap_or(0);
        let page_end = blocks.iter().map(|b| b.page).max().unwrap_or(0);
        let confidence = Chunk::min_confidence(blocks);
        return Ok(vec![Chunk {
            id: "chunk-0".to_string(),
            text: full_text,
            token_count: total_tokens,
            source_blocks: blocks.to_vec(),
            page_start,
            page_end,
            overlap_prefix: None,
            confidence,
        }]);
    }

    // Split by estimated character boundaries (1 token ~ 4 chars)
    let max_chars = max_tokens * 4;
    let overlap_chars = overlap * 4;

    let chars: Vec<char> = full_text.chars().collect();
    let total_chars = chars.len();

    let mut chunks = Vec::new();
    let mut start = 0;

    // Pre-compute page ranges and confidence from blocks
    let page_start = blocks.iter().map(|b| b.page).min().unwrap_or(0);
    let page_end = blocks.iter().map(|b| b.page).max().unwrap_or(0);
    let confidence = Chunk::min_confidence(blocks);

    while start < total_chars {
        let end = (start + max_chars).min(total_chars);
        let chunk_text: String = chars[start..end].iter().collect();

        let overlap_prefix = if start > 0 && overlap_chars > 0 {
            let overlap_start = start.saturating_sub(overlap_chars);
            Some(chars[overlap_start..start].iter().collect::<String>())
        } else {
            None
        };

        let display_text = match &overlap_prefix {
            Some(prefix) if !prefix.is_empty() => format!("{}{}", prefix, chunk_text),
            _ => chunk_text,
        };

        let token_count = estimate_tokens(&display_text);

        chunks.push(Chunk {
            id: format!("chunk-{}", chunks.len()),
            text: display_text,
            token_count,
            source_blocks: blocks.to_vec(), // All blocks contribute to fixed-size chunks
            page_start,
            page_end,
            overlap_prefix,
            confidence,
        });

        // Advance by (max_chars - overlap_chars) to leave room for overlap on next chunk
        let step = if max_chars > overlap_chars {
            max_chars - overlap_chars
        } else {
            max_chars
        };
        start += step;
    }

    Ok(chunks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Block, ElementType};

    fn make_block(text: &str, page: u32) -> Block {
        let mut b = Block::new(ElementType::NarrativeText, text);
        b.page = page;
        b
    }

    #[test]
    fn test_empty_blocks() {
        let result = chunk(&[], 100, 0).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_fits_in_one_chunk() {
        let blocks = vec![make_block("Hello world", 1)];
        let result = chunk(&blocks, 100, 0).unwrap();
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_splits_into_multiple_chunks() {
        // 200 chars => 50 tokens. max_tokens=20 => should split.
        let blocks = vec![make_block(&"a".repeat(200), 1)];
        let result = chunk(&blocks, 20, 0).unwrap();
        assert!(result.len() > 1);
    }

    #[test]
    fn test_overlap_present() {
        let blocks = vec![make_block(&"a".repeat(200), 1)];
        let result = chunk(&blocks, 20, 5).unwrap();
        assert!(result.len() > 1);
        // Second chunk should have overlap
        assert!(result[1].overlap_prefix.is_some());
    }

    #[test]
    fn test_ignores_element_boundaries() {
        let blocks = vec![
            make_block("aaaa", 1),
            make_block("bbbb", 1),
            make_block("cccc", 1),
        ];
        // All text is joined: "aaaa\nbbbb\ncccc" = 14 chars => ~4 tokens
        let result = chunk(&blocks, 100, 0).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].text.contains("aaaa"));
        assert!(result[0].text.contains("bbbb"));
        assert!(result[0].text.contains("cccc"));
    }
}
