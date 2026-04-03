use crate::chunking::by_structure;
use crate::chunking::{estimate_tokens, fast_estimate_tokens};
use crate::model::{Block, Chunk, ElementType};


/// Section boundary chunking: closes a chunk when a Title element is encountered.
/// Within a section, if text exceeds max_tokens, subdivides using by_structure logic.
/// Heading hierarchy is preserved in chunk metadata.
pub fn chunk(
    blocks: &[Block],
    max_tokens: usize,
    overlap: usize,
) -> Result<Vec<Chunk>, crate::Error> {
    if blocks.is_empty() {
        return Ok(Vec::new());
    }

    // Split blocks into sections at Title boundaries
    let mut sections: Vec<Vec<Block>> = Vec::new();
    let mut current_section: Vec<Block> = Vec::new();

    for block in blocks {
        let is_title = matches!(block.element_type, ElementType::Title | ElementType::Header);

        if is_title && !current_section.is_empty() {
            sections.push(std::mem::take(&mut current_section));
        }
        current_section.push(block.clone());
    }
    if !current_section.is_empty() {
        sections.push(current_section);
    }

    // Process each section
    let mut all_chunks: Vec<Chunk> = Vec::new();

    for section in &sections {
        let section_text: String = section.iter().map(|b| b.text.as_str()).collect::<Vec<_>>().join("\n");
        // Use fast estimate for the fits-in-one-chunk decision
        let section_tokens_fast = fast_estimate_tokens(&section_text);

        if section_tokens_fast <= max_tokens {
            // Compute accurate token count for the chunk metadata
            let section_tokens = estimate_tokens(&section_text);
            // Section fits in one chunk
            let page_start = section.iter().map(|b| b.page).min().unwrap_or(0);
            let page_end = section.iter().map(|b| b.page).max().unwrap_or(0);

            // Extract heading hierarchy from the section
            let hierarchy: Vec<String> = section
                .iter()
                .filter(|b| matches!(b.element_type, ElementType::Title | ElementType::Header))
                .map(|b| b.text.clone())
                .collect();

            let confidence = Chunk::min_confidence(section);
            let mut chunk = Chunk {
                id: format!("chunk-{}", all_chunks.len()),
                text: section_text,
                token_count: section_tokens,
                source_blocks: section.clone(),
                page_start,
                page_end,
                overlap_prefix: None,
                confidence,
            };

            // Store heading hierarchy in the first source block's hierarchy field
            // if available, otherwise use metadata on the chunk's source_blocks
            if !hierarchy.is_empty() {
                for sb in &mut chunk.source_blocks {
                    if matches!(sb.element_type, ElementType::Title | ElementType::Header) {
                        sb.hierarchy = hierarchy.clone();
                        break;
                    }
                }
            }

            all_chunks.push(chunk);
        } else {
            // Section too large — subdivide using by_structure
            let mut sub_chunks = by_structure::chunk(section, max_tokens, overlap)?;

            // Re-number chunks to maintain global sequential IDs
            for sub_chunk in &mut sub_chunks {
                sub_chunk.id = format!("chunk-{}", all_chunks.len());
                all_chunks.push(sub_chunk.clone());
            }
        }
    }

    Ok(all_chunks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Block;

    fn make_block(text: &str, element_type: ElementType, page: u32) -> Block {
        let mut b = Block::new(element_type, text);
        b.page = page;
        b
    }

    #[test]
    fn test_empty_blocks() {
        let result = chunk(&[], 100, 0).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_single_section_no_title() {
        let blocks = vec![
            make_block("Some text", ElementType::NarrativeText, 1),
            make_block("More text", ElementType::NarrativeText, 1),
        ];
        let result = chunk(&blocks, 100, 0).unwrap();
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_splits_at_title() {
        let blocks = vec![
            make_block("Introduction", ElementType::Title, 1),
            make_block("Intro text", ElementType::NarrativeText, 1),
            make_block("Chapter 1", ElementType::Title, 2),
            make_block("Chapter text", ElementType::NarrativeText, 2),
        ];
        let result = chunk(&blocks, 1000, 0).unwrap();
        assert_eq!(result.len(), 2);
        assert!(result[0].text.contains("Introduction"));
        assert!(result[1].text.contains("Chapter 1"));
    }

    #[test]
    fn test_large_section_subdivided() {
        let blocks = vec![
            make_block("Title", ElementType::Title, 1),
            make_block(&"a".repeat(200), ElementType::NarrativeText, 1),
            make_block(&"b".repeat(200), ElementType::NarrativeText, 1),
        ];
        // max_tokens = 60, so the section should be subdivided
        let result = chunk(&blocks, 60, 0).unwrap();
        assert!(result.len() > 1);
    }
}
