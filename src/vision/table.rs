/// OTSL (Optimized Table Structure Language) decoder for TableFormer.
///
/// OTSL tokens:
///   C  = regular cell
///   L  = left-looking (merge with left neighbor -> colspan)
///   U  = up-looking (merge with cell above -> rowspan)
///   X  = cross (merge both left and up)
///   NL = new line (end of row)
use serde::{Deserialize, Serialize};

/// OTSL token types emitted by the TableFormer model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OtslToken {
    /// Regular cell
    C,
    /// Left-merge (extend colspan of left neighbor)
    L,
    /// Up-merge (extend rowspan of cell above)
    U,
    /// Cross-merge (both left and up)
    X,
    /// New line (end of row)
    NL,
}

/// A cell in the resolved grid with span information.
#[derive(Debug, Clone)]
struct GridCell {
    token: OtslToken,
    /// Number of columns this cell spans (1 = no merge).
    colspan: usize,
    /// Number of rows this cell spans (1 = no merge).
    rowspan: usize,
    /// If true, this cell is "consumed" by a merge and should not emit HTML.
    consumed: bool,
    /// Optional text content for the cell.
    content: String,
}

impl GridCell {
    fn new(token: OtslToken) -> Self {
        Self {
            token,
            colspan: 1,
            rowspan: 1,
            consumed: false,
            content: String::new(),
        }
    }
}

/// Structured table output from the recognition pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableStructure {
    /// OTSL token sequence predicted by the model.
    pub tokens: Vec<OtslToken>,
    /// Number of rows detected.
    pub num_rows: usize,
    /// Number of columns detected.
    pub num_cols: usize,
    /// Rendered HTML table string.
    pub html: String,
}

/// Table structure recognition engine using TableFormer ONNX model.
///
/// Takes a cropped table image and produces structured table output
/// (OTSL tokens -> HTML with correct colspan/rowspan).
pub struct TableRecognizer {
    // Will hold: ort::Session when model is loaded
    _private: (),
}

impl TableRecognizer {
    /// Create a new TableRecognizer. Will require an ONNX session in the future.
    pub fn new() -> Self {
        Self { _private: () }
    }

    /// Recognize table structure from a cropped table image.
    ///
    /// Returns OTSL token sequence and decoded HTML structure.
    pub fn recognize_structure(
        &self,
        _image: &[u8],
    ) -> Result<TableStructure, crate::Error> {
        // TODO: When ONNX model is available:
        // 1. Preprocess table image
        // 2. Run TableFormer inference -> OTSL token sequence
        // 3. Decode tokens via decode_otsl()
        // 4. Return TableStructure with HTML
        Err(crate::Error::NotImplemented(
            "Table recognition model not loaded",
        ))
    }

    /// Decode an OTSL token sequence into an HTML table.
    ///
    /// This is the core OTSL decoder, validated in spike-003. It handles:
    /// - Simple tables (no merges)
    /// - Horizontal merges (colspan via L tokens)
    /// - Vertical merges (rowspan via U tokens)
    /// - Cross merges (colspan + rowspan via X tokens)
    pub fn decode_otsl(
        tokens: &[OtslToken],
        cell_contents: &[&str],
    ) -> Result<String, crate::Error> {
        let mut grid = build_grid(tokens);
        resolve_merges(&mut grid);
        assign_content(&mut grid, cell_contents);
        Ok(grid_to_html(&grid))
    }

    /// Decode an OTSL token sequence and return structured table data
    /// suitable for the Block model.
    pub fn decode_otsl_to_table_data(
        tokens: &[OtslToken],
        cell_contents: &[&str],
    ) -> Result<crate::model::TableData, crate::Error> {
        let mut grid = build_grid(tokens);
        resolve_merges(&mut grid);
        assign_content(&mut grid, cell_contents);

        let mut rows = Vec::new();
        for row in &grid {
            let mut row_cells = Vec::new();
            for cell in row {
                if !cell.consumed {
                    row_cells.push(cell.content.clone());
                }
            }
            if !row_cells.is_empty() {
                rows.push(row_cells);
            }
        }

        let headers = if !rows.is_empty() {
            Some(rows[0].clone())
        } else {
            None
        };

        Ok(crate::model::TableData { rows, headers })
    }
}

impl Default for TableRecognizer {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse a string of space-separated OTSL tokens into a token sequence.
pub fn parse_otsl_string(input: &str) -> Result<Vec<OtslToken>, crate::Error> {
    let mut tokens = Vec::new();
    for part in input.split_whitespace() {
        match part {
            "C" => tokens.push(OtslToken::C),
            "L" => tokens.push(OtslToken::L),
            "U" => tokens.push(OtslToken::U),
            "X" => tokens.push(OtslToken::X),
            "NL" => tokens.push(OtslToken::NL),
            other => {
                return Err(crate::Error::Parse(format!(
                    "Unknown OTSL token: '{}'",
                    other
                )))
            }
        }
    }
    Ok(tokens)
}

/// Build a 2D grid from a flat token sequence.
/// Returns Vec<Vec<GridCell>> where outer vec = rows, inner vec = columns.
fn build_grid(tokens: &[OtslToken]) -> Vec<Vec<GridCell>> {
    let mut grid: Vec<Vec<GridCell>> = Vec::new();
    let mut current_row: Vec<GridCell> = Vec::new();

    for &token in tokens {
        match token {
            OtslToken::NL => {
                if !current_row.is_empty() {
                    grid.push(current_row);
                    current_row = Vec::new();
                }
            }
            _ => {
                current_row.push(GridCell::new(token));
            }
        }
    }
    // Handle case where last row doesn't end with NL
    if !current_row.is_empty() {
        grid.push(current_row);
    }

    grid
}

/// Resolve merge directives in the grid.
///
/// Algorithm:
/// - L cells: find the nearest non-L cell to the left in the same row,
///   increment its colspan, mark this cell as consumed.
/// - U cells: find the "origin" cell above (walking up through U/X cells),
///   increment its rowspan, mark this cell as consumed.
/// - X cells: merge both left (colspan of left origin) and up (rowspan of above origin),
///   mark as consumed.
fn resolve_merges(grid: &mut [Vec<GridCell>]) {
    let num_rows = grid.len();
    if num_rows == 0 {
        return;
    }

    // Process left-merges first (L and X extend colspan)
    for row_idx in 0..num_rows {
        let num_cols = grid[row_idx].len();
        for col_idx in 0..num_cols {
            let token = grid[row_idx][col_idx].token;
            if token == OtslToken::L || token == OtslToken::X {
                // Find the origin cell to the left (first non-L, non-X cell scanning left)
                if let Some(origin_col) = find_left_origin(grid, row_idx, col_idx) {
                    grid[row_idx][origin_col].colspan += 1;
                    grid[row_idx][col_idx].consumed = true;
                }
            }
        }
    }

    // Process up-merges (only U cells extend rowspan).
    // X cells are already consumed by the colspan pass. They represent the
    // horizontal continuation of the U cell in the same row, so they do NOT
    // add an extra rowspan. Only U cells (the leftmost vertical-merge token
    // in a row) trigger a rowspan increment on the origin cell above.
    for row_idx in 0..num_rows {
        let num_cols = grid[row_idx].len();
        for col_idx in 0..num_cols {
            let token = grid[row_idx][col_idx].token;
            if token == OtslToken::U {
                // Find the origin cell above (first non-U, non-X cell scanning up)
                if let Some(origin_row) = find_up_origin(grid, row_idx, col_idx) {
                    // The rowspan belongs to the cell that "owns" the span at origin_row.
                    // If that cell is part of a colspan group, find the left-origin.
                    let target_col =
                        find_left_origin(grid, origin_row, col_idx).unwrap_or(col_idx);
                    grid[origin_row][target_col].rowspan += 1;
                }
                grid[row_idx][col_idx].consumed = true;
            } else if token == OtslToken::X {
                // X is already consumed by the colspan pass, but ensure it's marked
                grid[row_idx][col_idx].consumed = true;
            }
        }
    }
}

/// Find the leftmost origin cell in the same row by scanning left from col_idx.
/// Returns the column index of the first C or U cell (not L or X).
fn find_left_origin(grid: &[Vec<GridCell>], row: usize, col: usize) -> Option<usize> {
    if col == 0 {
        return None;
    }
    for c in (0..col).rev() {
        let t = grid[row][c].token;
        if t == OtslToken::C || t == OtslToken::U {
            return Some(c);
        }
    }
    None
}

/// Find the topmost origin cell in the same column by scanning up from row_idx.
/// Returns the row index of the first C or L cell (not U or X).
fn find_up_origin(grid: &[Vec<GridCell>], row: usize, col: usize) -> Option<usize> {
    if row == 0 {
        return None;
    }
    for r in (0..row).rev() {
        if col < grid[r].len() {
            let t = grid[r][col].token;
            if t == OtslToken::C || t == OtslToken::L {
                return Some(r);
            }
        }
    }
    None
}

/// Assign cell content to the grid. Content is provided as a flat list
/// matching the order of non-consumed cells in reading order.
fn assign_content(grid: &mut [Vec<GridCell>], contents: &[&str]) {
    let mut content_idx = 0;
    for row in grid.iter_mut() {
        for cell in row.iter_mut() {
            if !cell.consumed && content_idx < contents.len() {
                cell.content = contents[content_idx].to_string();
                content_idx += 1;
            }
        }
    }
}

/// Convert the resolved grid to an HTML table string.
fn grid_to_html(grid: &[Vec<GridCell>]) -> String {
    let mut html = String::from("<table>\n");

    for row in grid {
        html.push_str("  <tr>\n");
        for cell in row {
            if cell.consumed {
                continue;
            }
            let mut attrs = String::new();
            if cell.colspan > 1 {
                attrs.push_str(&format!(" colspan=\"{}\"", cell.colspan));
            }
            if cell.rowspan > 1 {
                attrs.push_str(&format!(" rowspan=\"{}\"", cell.rowspan));
            }
            html.push_str(&format!("    <td{}>{}</td>\n", attrs, cell.content));
        }
        html.push_str("  </tr>\n");
    }

    html.push_str("</table>");
    html
}

#[cfg(test)]
mod tests {
    use super::*;

    fn otsl_to_html(otsl: &str, contents: &[&str]) -> Result<String, crate::Error> {
        let tokens = parse_otsl_string(otsl)?;
        TableRecognizer::decode_otsl(&tokens, contents)
    }

    #[test]
    fn test_simple_2x3_table() {
        let html = otsl_to_html("C C C NL C C C NL", &["A", "B", "C", "D", "E", "F"]).unwrap();
        assert!(html.contains("<td>A</td>"));
        assert!(html.contains("<td>F</td>"));
        assert!(!html.contains("colspan"));
        assert!(!html.contains("rowspan"));
    }

    #[test]
    fn test_horizontal_merge_colspan() {
        let html = otsl_to_html("C L C NL C C C NL", &["Merged", "Right", "D", "E", "F"]).unwrap();
        assert!(html.contains("<td colspan=\"2\">Merged</td>"));
        assert!(html.contains("<td>Right</td>"));
    }

    #[test]
    fn test_vertical_merge_rowspan() {
        let html = otsl_to_html("C C NL U C NL", &["Tall", "B", "D"]).unwrap();
        assert!(html.contains("<td rowspan=\"2\">Tall</td>"));
    }

    #[test]
    fn test_cross_merge_2x2_block() {
        let html = otsl_to_html(
            "C L C NL U X C NL C C C NL",
            &["Big", "R1C3", "R2C3", "R3C1", "R3C2", "R3C3"],
        )
        .unwrap();
        assert!(html.contains("<td colspan=\"2\" rowspan=\"2\">Big</td>"));
    }

    #[test]
    fn test_full_width_header_colspan_3() {
        let html = otsl_to_html("C L L NL C C C NL", &["Header", "A", "B", "C"]).unwrap();
        assert!(html.contains("<td colspan=\"3\">Header</td>"));
    }

    #[test]
    fn test_triple_rowspan() {
        let html = otsl_to_html("C C NL U C NL U C NL", &["Tall3", "B1", "B2", "B3"]).unwrap();
        assert!(html.contains("<td rowspan=\"3\">Tall3</td>"));
    }

    #[test]
    fn test_unknown_token_error() {
        let result = parse_otsl_string("C Z C NL");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Unknown OTSL token"));
    }

    #[test]
    fn test_large_merged_block() {
        let html = otsl_to_html(
            "C L L NL U X X NL C C C NL",
            &["BigBlock", "R3C1", "R3C2", "R3C3"],
        )
        .unwrap();
        assert!(html.contains("<td colspan=\"3\" rowspan=\"2\">BigBlock</td>"));
    }

    #[test]
    fn test_recognize_structure_not_implemented() {
        let recognizer = TableRecognizer::new();
        let result = recognizer.recognize_structure(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_decode_otsl_to_table_data() {
        let tokens = parse_otsl_string("C C NL C C NL").unwrap();
        let data = TableRecognizer::decode_otsl_to_table_data(&tokens, &["A", "B", "C", "D"]).unwrap();
        assert_eq!(data.rows.len(), 2);
        assert_eq!(data.rows[0], vec!["A", "B"]);
        assert_eq!(data.rows[1], vec!["C", "D"]);
        assert_eq!(data.headers, Some(vec!["A".to_string(), "B".to_string()]));
    }
}
