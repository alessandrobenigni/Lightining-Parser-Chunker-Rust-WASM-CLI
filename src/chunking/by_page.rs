use crate::chunking::by_structure;
use crate::chunking::{estimate_tokens, fast_estimate_tokens};
use crate::model::{Block, Chunk};

/// Page boundary chunking: groups blocks by page number.
/// No chunk spans multiple pages. Within a page, if text exceeds max_tokens,
/// subdivides using by_structure logic.
pub fn chunk(
    blocks: &[Block],
    max_tokens: usize,
    overlap: usize,
) -> Result<Vec<Chunk>, crate::Error> {
    if blocks.is_empty() {
        return Ok(Vec::new());
    }

    // Group blocks by page number (preserving order)
    let mut pages: Vec<(u32, Vec<Block>)> = Vec::new();
    for block in blocks {
        match pages.last_mut() {
            Some((page, page_blocks)) if *page == block.page => {
                page_blocks.push(block.clone());
            }
            _ => {
                pages.push((block.page, vec![block.clone()]));
            }
        }
    }

    let mut all_chunks: Vec<Chunk> = Vec::new();

    for (page_num, page_blocks) in &pages {
        let page_text: String = page_blocks
            .iter()
            .map(|b| b.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        // Use fast estimate for the fits-in-one-chunk decision
        let page_tokens_fast = fast_estimate_tokens(&page_text);

        if page_tokens_fast <= max_tokens {
            // Entire page fits in one chunk — compute accurate count for metadata
            let page_tokens = estimate_tokens(&page_text);
            let confidence = Chunk::min_confidence(page_blocks);
            let chunk = Chunk {
                id: format!("chunk-{}", all_chunks.len()),
                text: page_text,
                token_count: page_tokens,
                source_blocks: page_blocks.clone(),
                page_start: *page_num,
                page_end: *page_num,
                overlap_prefix: None,
                confidence,
            };
            all_chunks.push(chunk);
        } else {
            // Page too large — subdivide using by_structure (no overlap across pages)
            let mut sub_chunks = by_structure::chunk(page_blocks, max_tokens, overlap)?;
            for sub_chunk in &mut sub_chunks {
                sub_chunk.id = format!("chunk-{}", all_chunks.len());
                // Ensure page boundaries are respected
                sub_chunk.page_start = *page_num;
                sub_chunk.page_end = *page_num;
                all_chunks.push(sub_chunk.clone());
            }
        }
    }

    Ok(all_chunks)
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
    fn test_single_page() {
        let blocks = vec![
            make_block("Hello", 1),
            make_block("World", 1),
        ];
        let result = chunk(&blocks, 100, 0).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].page_start, 1);
        assert_eq!(result[0].page_end, 1);
    }

    #[test]
    fn test_multiple_pages() {
        let blocks = vec![
            make_block("Page 1 text", 1),
            make_block("Page 2 text", 2),
            make_block("Page 3 text", 3),
        ];
        let result = chunk(&blocks, 100, 0).unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].page_start, 1);
        assert_eq!(result[1].page_start, 2);
        assert_eq!(result[2].page_start, 3);
    }

    #[test]
    fn test_no_cross_page_chunks() {
        let blocks = vec![
            make_block("Page 1 text", 1),
            make_block("Page 2 text", 2),
        ];
        let result = chunk(&blocks, 1000, 0).unwrap();
        for c in &result {
            assert_eq!(c.page_start, c.page_end, "Chunk should not span pages");
        }
    }

    #[test]
    fn test_large_page_subdivided() {
        let blocks = vec![
            make_block(&"a".repeat(200), 1),
            make_block(&"b".repeat(200), 1),
        ];
        let result = chunk(&blocks, 60, 0).unwrap();
        assert!(result.len() > 1);
        for c in &result {
            assert_eq!(c.page_start, 1);
            assert_eq!(c.page_end, 1);
        }
    }
}
