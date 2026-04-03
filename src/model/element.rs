use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ElementType {
    Title,
    Header,
    NarrativeText,
    ListItem,
    Table,
    Image,
    PageBreak,
    Footer,
    Caption,
    Formula,
    CodeBlock,
    Address,
    EmailBody,
    EmailHeader,
    Unknown,
}
