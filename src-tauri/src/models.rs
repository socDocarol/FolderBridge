use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanOptions {
    pub source: String,
    pub destination: Option<String>,
    #[serde(default)]
    pub compare_both_ways: bool,
    #[serde(default = "yes")]
    pub verify_contents: bool,
    #[serde(default)]
    pub excluded_extensions: Vec<String>,
    #[serde(default)]
    pub collect_owners: bool,
    #[serde(default)]
    pub rules: FileRules,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRules {
    #[serde(default)]
    pub excluded_paths: Vec<String>,
    #[serde(default)]
    pub min_size: Option<u64>,
    #[serde(default)]
    pub max_size: Option<u64>,
    #[serde(default)]
    pub review_above: Option<u64>,
    #[serde(default)]
    pub review_access_above: Option<u64>,
    #[serde(default)]
    pub review_extensions: Vec<String>,
}
fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub running: bool,
    pub kind: String,
    pub phase: String,
    pub current_path: String,
    pub processed: u64,
    pub total: u64,
    pub bytes: u64,
    pub errors: u64,
    pub scan_id: Option<i64>,
    pub operation_id: Option<i64>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scan {
    pub id: i64,
    pub source: String,
    pub destination: Option<String>,
    pub started_at: i64,
    pub state: String,
    pub verified: bool,
    pub files: u64,
    pub errors: u64,
    pub options: Option<ScanOptions>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: i64,
    pub scan_id: i64,
    pub relative_path: String,
    pub source_relative: Option<String>,
    pub destination_relative: Option<String>,
    pub extension: String,
    pub status: String,
    pub source_size: Option<u64>,
    pub destination_size: Option<u64>,
    pub source_modified: Option<i64>,
    pub destination_modified: Option<i64>,
    pub source_hash: Option<String>,
    pub destination_hash: Option<String>,
    pub owner: Option<String>,
    pub issue: Option<String>,
    pub rule_review: bool,
    pub rule_reason: String,
    pub migration_state: String,
    pub migration_reason: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryFilter {
    pub scan_id: i64,
    #[serde(default)]
    pub source_only: bool,
    #[serde(default)]
    pub search: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub migration_state: String,
    #[serde(default)]
    pub extension: String,
    #[serde(default)]
    pub min_size: u64,
    #[serde(default)]
    pub offset: u64,
    #[serde(default)]
    pub limit: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryPage {
    pub entries: Vec<Entry>,
    pub total: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupTotal {
    pub label: String,
    pub count: u64,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Analysis {
    pub statuses: Vec<GroupTotal>,
    pub source_statuses: Vec<GroupTotal>,
    pub migration_states: Vec<GroupTotal>,
    pub extensions: Vec<GroupTotal>,
    pub folders: Vec<GroupTotal>,
    pub source_bytes: u64,
    pub destination_bytes: u64,
    pub duplicate_bytes: u64,
    pub size_statistics: Vec<SizeStatistics>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SizeStatistics {
    pub extension: String,
    pub count: u64,
    pub mean: f64,
    pub median: f64,
    pub lower_quartile: f64,
    pub upper_quartile: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationRequest {
    pub scan_id: i64,
    pub entry_ids: Vec<i64>,
    pub action: String,
    #[serde(default)]
    pub approved_entry_ids: Vec<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub action: String,
    pub eligible: u64,
    pub skipped: u64,
    pub bytes: u64,
    pub description: String,
    pub review_entry_ids: Vec<i64>,
    pub eligible_entry_ids: Vec<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Operation {
    pub id: i64,
    pub scan_id: i64,
    pub action: String,
    pub started_at: i64,
    pub state: String,
    pub completed: u64,
    pub skipped: u64,
    pub errors: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationItem {
    pub id: i64,
    pub operation_id: i64,
    pub relative_path: String,
    pub original_path: String,
    pub target_path: String,
    pub state: String,
    pub message: String,
    pub content_hash: Option<String>,
    pub cleanup: Option<CleanupRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupRecord {
    pub method: String,
    pub state: String,
    pub holding_path: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupRequest {
    pub operation_id: i64,
    pub item_ids: Vec<i64>,
    pub handling: String,
    pub confirmation: String,
    #[serde(default)]
    pub sharepoint_confirmed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupPreviewItem {
    pub item: OperationItem,
    pub eligible: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupPreview {
    pub original_root: String,
    pub target_root: String,
    pub is_network: bool,
    pub items: Vec<CleanupPreviewItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupSummary {
    pub completed: u64,
    pub skipped: u64,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedPair {
    pub id: i64,
    pub name: String,
    pub options: ScanOptions,
}
