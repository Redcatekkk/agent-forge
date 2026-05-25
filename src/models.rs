use crate::scanner::AnalysisSource;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Argument {
    pub name: String,
    pub required: bool,
    pub type_hint: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntryPoint {
    pub name: String,
    pub intent: String,
    pub framework: String,
    pub method: Option<String>,
    pub route: Option<String>,
    pub file: String,
    pub args: Vec<Argument>,
    pub docstring: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalysisReport {
    pub repo_name: String,
    pub source: AnalysisSource,
    pub target_dir: String,
    pub files_scanned: u64,
    pub condensation_threshold: u64,
    pub entry_points: Vec<EntryPoint>,
    pub warnings: Vec<String>,
}
