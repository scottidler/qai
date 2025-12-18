//! History & Learning System for qai
//!
//! Tracks queries, results, selections, and corrections to personalize
//! command suggestions over time.

#![allow(dead_code)] // APIs used in tests and will be used by shell integration

use chrono::{DateTime, Utc};
use eyre::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use uuid::Uuid;

/// A single query interaction record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryRecord {
    /// Unique identifier
    pub id: Uuid,

    /// When this query was made
    pub timestamp: DateTime<Utc>,

    /// The natural language query
    pub query: String,

    /// Commands returned by the AI
    pub results: Vec<String>,

    /// Which command the user selected (index into results)
    pub selected_index: Option<usize>,

    /// If user edited the command before executing
    pub edited_command: Option<String>,

    /// Whether the command was actually executed
    pub executed: bool,

    /// Working directory when query was made
    pub cwd: Option<PathBuf>,

    /// Model used for this query
    pub model: String,
}

impl QueryRecord {
    /// Create a new query record
    pub fn new(query: String, results: Vec<String>, model: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            query,
            results,
            selected_index: None,
            edited_command: None,
            executed: false,
            cwd: std::env::current_dir().ok(),
            model,
        }
    }

    /// Mark a selection
    pub fn select(&mut self, index: usize) {
        self.selected_index = Some(index);
    }

    /// Mark as edited
    pub fn edit(&mut self, edited: String) {
        self.edited_command = Some(edited);
    }

    /// Mark as executed
    pub fn execute(&mut self) {
        self.executed = true;
    }

    /// Get the final command (edited or selected)
    pub fn final_command(&self) -> Option<&str> {
        if let Some(edited) = &self.edited_command {
            Some(edited)
        } else if let Some(idx) = self.selected_index {
            self.results.get(idx).map(|s| s.as_str())
        } else {
            None
        }
    }
}

/// Command selection statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandSelection {
    pub command: String,
    pub selection_count: u32,
    pub last_selected: DateTime<Utc>,
}

/// Aggregated statistics for a query pattern
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryPattern {
    /// Normalized query (lowercase, trimmed)
    pub normalized_query: String,

    /// Number of times this pattern was queried
    pub query_count: u32,

    /// Most frequently selected command
    pub preferred_command: Option<String>,

    /// All commands ever selected for this pattern
    pub command_history: Vec<CommandSelection>,

    /// Last time this pattern was used
    pub last_used: DateTime<Utc>,
}

impl QueryPattern {
    /// Create a new pattern from a query
    pub fn new(query: &str) -> Self {
        Self {
            normalized_query: normalize_query(query),
            query_count: 1,
            preferred_command: None,
            command_history: Vec::new(),
            last_used: Utc::now(),
        }
    }

    /// Record a command selection for this pattern
    pub fn record_selection(&mut self, command: &str) {
        self.last_used = Utc::now();
        self.query_count += 1;

        // Find or create command selection entry
        if let Some(selection) = self.command_history.iter_mut().find(|s| s.command == command) {
            selection.selection_count += 1;
            selection.last_selected = Utc::now();
        } else {
            self.command_history.push(CommandSelection {
                command: command.to_string(),
                selection_count: 1,
                last_selected: Utc::now(),
            });
        }

        // Update preferred command (most selected)
        self.preferred_command = self
            .command_history
            .iter()
            .max_by_key(|s| s.selection_count)
            .map(|s| s.command.clone());
    }
}

/// Normalize a query for pattern matching
pub fn normalize_query(query: &str) -> String {
    query.trim().to_lowercase()
}

/// History store using flat files (JSON Lines format)
#[derive(Debug)]
pub struct HistoryStore {
    /// Directory where history files are stored
    data_dir: PathBuf,

    /// In-memory cache of patterns
    patterns: HashMap<String, QueryPattern>,

    /// Whether patterns cache is dirty
    patterns_dirty: bool,
}

impl HistoryStore {
    /// Create a new history store
    pub fn new() -> Result<Self> {
        let data_dir = Self::default_data_dir();
        Self::with_data_dir(data_dir)
    }

    /// Create with custom data directory
    pub fn with_data_dir(data_dir: PathBuf) -> Result<Self> {
        fs::create_dir_all(&data_dir).context("Failed to create history data directory")?;

        let mut store = Self {
            data_dir,
            patterns: HashMap::new(),
            patterns_dirty: false,
        };

        // Load patterns from disk
        store.load_patterns()?;

        Ok(store)
    }

    /// Get the default data directory
    pub fn default_data_dir() -> PathBuf {
        dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("qai")
            .join("history")
    }

    /// Path to history.jsonl file
    fn history_path(&self) -> PathBuf {
        self.data_dir.join("history.jsonl")
    }

    /// Path to patterns.json file
    fn patterns_path(&self) -> PathBuf {
        self.data_dir.join("patterns.json")
    }

    /// Record a query and its results
    pub fn record_query(&mut self, record: &QueryRecord) -> Result<()> {
        // Append to history.jsonl
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.history_path())
            .context("Failed to open history file")?;

        let json = serde_json::to_string(record).context("Failed to serialize query record")?;
        writeln!(file, "{}", json).context("Failed to write to history file")?;

        Ok(())
    }

    /// Record that a command was selected for a query
    pub fn record_selection(&mut self, query: &str, command: &str) -> Result<()> {
        let normalized = normalize_query(query);

        // Update or create pattern
        let pattern = self
            .patterns
            .entry(normalized.clone())
            .or_insert_with(|| QueryPattern::new(query));

        pattern.record_selection(command);
        self.patterns_dirty = true;

        // Persist patterns
        self.save_patterns()?;

        Ok(())
    }

    /// Get pattern for a query if it exists
    pub fn get_pattern(&self, query: &str) -> Option<&QueryPattern> {
        let normalized = normalize_query(query);
        self.patterns.get(&normalized)
    }

    /// Re-rank AI results based on user history
    pub fn personalize_results(&self, query: &str, ai_results: Vec<String>) -> Vec<String> {
        let normalized = normalize_query(query);

        if let Some(pattern) = self.patterns.get(&normalized) {
            // Score each result based on history
            let mut scored: Vec<(String, f32)> = ai_results
                .into_iter()
                .map(|cmd| {
                    let score = self.score_command(&cmd, pattern);
                    (cmd, score)
                })
                .collect();

            // Sort by score descending
            scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

            scored.into_iter().map(|(cmd, _)| cmd).collect()
        } else {
            // No history, return as-is
            ai_results
        }
    }

    /// Score a command based on pattern history
    fn score_command(&self, cmd: &str, pattern: &QueryPattern) -> f32 {
        let mut score = 0.0;

        // Exact match with preferred command gets big boost
        if pattern.preferred_command.as_deref() == Some(cmd) {
            score += 10.0;
        }

        // Previously selected commands get boost based on selection count
        for selection in &pattern.command_history {
            if selection.command == cmd {
                // Log scale to avoid huge scores for frequently used commands
                score += (selection.selection_count as f32 + 1.0).ln();
            }
        }

        score
    }

    /// Load patterns from disk
    fn load_patterns(&mut self) -> Result<()> {
        let path = self.patterns_path();
        if !path.exists() {
            return Ok(());
        }

        let content = fs::read_to_string(&path).context("Failed to read patterns file")?;

        self.patterns = serde_json::from_str(&content).unwrap_or_default();

        Ok(())
    }

    /// Save patterns to disk
    fn save_patterns(&mut self) -> Result<()> {
        if !self.patterns_dirty {
            return Ok(());
        }

        let path = self.patterns_path();
        let content = serde_json::to_string_pretty(&self.patterns).context("Failed to serialize patterns")?;

        fs::write(&path, content).context("Failed to write patterns file")?;

        self.patterns_dirty = false;
        Ok(())
    }

    /// Get recent queries (for history command)
    pub fn get_recent_queries(&self, limit: usize) -> Result<Vec<QueryRecord>> {
        let path = self.history_path();
        if !path.exists() {
            return Ok(Vec::new());
        }

        let file = File::open(&path).context("Failed to open history file")?;
        let reader = BufReader::new(file);

        // Read all lines, keep last `limit` entries
        let mut records: Vec<QueryRecord> = Vec::new();

        for line in reader.lines() {
            let line = line.context("Failed to read history line")?;
            if line.trim().is_empty() {
                continue;
            }

            if let Ok(record) = serde_json::from_str(&line) {
                records.push(record);
            }
        }

        // Return last N records
        let start = records.len().saturating_sub(limit);
        Ok(records.into_iter().skip(start).collect())
    }

    /// Get all patterns sorted by usage
    pub fn get_patterns_by_usage(&self) -> Vec<&QueryPattern> {
        let mut patterns: Vec<&QueryPattern> = self.patterns.values().collect();
        patterns.sort_by(|a, b| b.query_count.cmp(&a.query_count));
        patterns
    }

    /// Clear all history
    pub fn clear(&mut self) -> Result<()> {
        // Remove files
        let _ = fs::remove_file(self.history_path());
        let _ = fs::remove_file(self.patterns_path());

        // Clear in-memory cache
        self.patterns.clear();
        self.patterns_dirty = false;

        Ok(())
    }

    /// Get history statistics
    pub fn stats(&self) -> Result<HistoryStats> {
        let history_path = self.history_path();
        let query_count = if history_path.exists() {
            let file = File::open(&history_path)?;
            BufReader::new(file).lines().count()
        } else {
            0
        };

        Ok(HistoryStats {
            total_queries: query_count,
            unique_patterns: self.patterns.len(),
            patterns_with_preference: self.patterns.values().filter(|p| p.preferred_command.is_some()).count(),
        })
    }
}

impl Default for HistoryStore {
    fn default() -> Self {
        Self::new().unwrap_or_else(|_| Self {
            data_dir: PathBuf::from("."),
            patterns: HashMap::new(),
            patterns_dirty: false,
        })
    }
}

/// Statistics about history
#[derive(Debug)]
pub struct HistoryStats {
    pub total_queries: usize,
    pub unique_patterns: usize,
    pub patterns_with_preference: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_store() -> (HistoryStore, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let store = HistoryStore::with_data_dir(temp_dir.path().to_path_buf()).unwrap();
        (store, temp_dir)
    }

    #[test]
    fn test_query_record_new() {
        let record = QueryRecord::new(
            "list files".to_string(),
            vec!["ls -la".to_string(), "ls".to_string()],
            "gpt-4o-mini".to_string(),
        );

        assert!(!record.id.is_nil());
        assert_eq!(record.query, "list files");
        assert_eq!(record.results.len(), 2);
        assert_eq!(record.model, "gpt-4o-mini");
        assert!(record.selected_index.is_none());
        assert!(record.edited_command.is_none());
        assert!(!record.executed);
    }

    #[test]
    fn test_query_record_select() {
        let mut record = QueryRecord::new(
            "test".to_string(),
            vec!["cmd1".to_string(), "cmd2".to_string()],
            "model".to_string(),
        );

        record.select(1);
        assert_eq!(record.selected_index, Some(1));
        assert_eq!(record.final_command(), Some("cmd2"));
    }

    #[test]
    fn test_query_record_edit() {
        let mut record = QueryRecord::new("test".to_string(), vec!["cmd1".to_string()], "model".to_string());

        record.select(0);
        record.edit("cmd1 --modified".to_string());
        assert_eq!(record.final_command(), Some("cmd1 --modified"));
    }

    #[test]
    fn test_query_record_execute() {
        let mut record = QueryRecord::new("test".to_string(), vec![], "model".to_string());
        assert!(!record.executed);
        record.execute();
        assert!(record.executed);
    }

    #[test]
    fn test_query_record_final_command_none() {
        let record = QueryRecord::new("test".to_string(), vec!["cmd".to_string()], "model".to_string());
        assert!(record.final_command().is_none());
    }

    #[test]
    fn test_normalize_query() {
        assert_eq!(normalize_query("  List Files  "), "list files");
        assert_eq!(normalize_query("UPPERCASE"), "uppercase");
        assert_eq!(normalize_query("  spaces  "), "spaces");
    }

    #[test]
    fn test_query_pattern_new() {
        let pattern = QueryPattern::new("List Files");
        assert_eq!(pattern.normalized_query, "list files");
        assert_eq!(pattern.query_count, 1);
        assert!(pattern.preferred_command.is_none());
        assert!(pattern.command_history.is_empty());
    }

    #[test]
    fn test_query_pattern_record_selection() {
        let mut pattern = QueryPattern::new("list files");

        pattern.record_selection("ls -la");
        assert_eq!(pattern.query_count, 2);
        assert_eq!(pattern.preferred_command, Some("ls -la".to_string()));
        assert_eq!(pattern.command_history.len(), 1);
        assert_eq!(pattern.command_history[0].selection_count, 1);

        pattern.record_selection("ls -la");
        assert_eq!(pattern.query_count, 3);
        assert_eq!(pattern.command_history[0].selection_count, 2);

        pattern.record_selection("ls");
        assert_eq!(pattern.command_history.len(), 2);
        assert_eq!(pattern.preferred_command, Some("ls -la".to_string())); // Still most selected
    }

    #[test]
    fn test_history_store_new() {
        let (store, _temp_dir) = create_test_store();
        assert!(store.patterns.is_empty());
    }

    #[test]
    fn test_history_store_record_query() {
        let (mut store, _temp_dir) = create_test_store();

        let record = QueryRecord::new(
            "list files".to_string(),
            vec!["ls -la".to_string()],
            "gpt-4o-mini".to_string(),
        );

        store.record_query(&record).unwrap();

        // Verify file was created
        assert!(store.history_path().exists());
    }

    #[test]
    fn test_history_store_record_selection() {
        let (mut store, _temp_dir) = create_test_store();

        store.record_selection("list files", "ls -la").unwrap();

        let pattern = store.get_pattern("list files").unwrap();
        assert_eq!(pattern.preferred_command, Some("ls -la".to_string()));
    }

    #[test]
    fn test_history_store_personalize_results() {
        let (mut store, _temp_dir) = create_test_store();

        // Record some history
        store.record_selection("list files", "ls -la").unwrap();
        store.record_selection("list files", "ls -la").unwrap();
        store.record_selection("list files", "ls").unwrap();

        // Personalize results
        let results = vec!["ls".to_string(), "ls -la".to_string(), "dir".to_string()];
        let personalized = store.personalize_results("list files", results);

        // ls -la should be first (more selections + preferred)
        assert_eq!(personalized[0], "ls -la");
    }

    #[test]
    fn test_history_store_personalize_results_no_history() {
        let (store, _temp_dir) = create_test_store();

        let results = vec!["cmd1".to_string(), "cmd2".to_string()];
        let personalized = store.personalize_results("unknown query", results.clone());

        // Should be unchanged
        assert_eq!(personalized, results);
    }

    #[test]
    fn test_history_store_get_recent_queries() {
        let (mut store, _temp_dir) = create_test_store();

        // Add some queries
        for i in 0..5 {
            let record = QueryRecord::new(format!("query {}", i), vec![], "model".to_string());
            store.record_query(&record).unwrap();
        }

        let recent = store.get_recent_queries(3).unwrap();
        assert_eq!(recent.len(), 3);
        assert_eq!(recent[0].query, "query 2");
        assert_eq!(recent[2].query, "query 4");
    }

    #[test]
    fn test_history_store_get_recent_queries_empty() {
        let (store, _temp_dir) = create_test_store();
        let recent = store.get_recent_queries(10).unwrap();
        assert!(recent.is_empty());
    }

    #[test]
    fn test_history_store_get_patterns_by_usage() {
        let (mut store, _temp_dir) = create_test_store();

        // Add patterns with different usage counts
        store.record_selection("query a", "cmd").unwrap();
        store.record_selection("query b", "cmd").unwrap();
        store.record_selection("query b", "cmd").unwrap();
        store.record_selection("query b", "cmd").unwrap();
        store.record_selection("query c", "cmd").unwrap();
        store.record_selection("query c", "cmd").unwrap();

        let patterns = store.get_patterns_by_usage();
        assert_eq!(patterns.len(), 3);
        assert_eq!(patterns[0].normalized_query, "query b"); // Most used
    }

    #[test]
    fn test_history_store_clear() {
        let (mut store, _temp_dir) = create_test_store();

        // Add some data
        let record = QueryRecord::new("test".to_string(), vec![], "model".to_string());
        store.record_query(&record).unwrap();
        store.record_selection("test", "cmd").unwrap();

        // Clear
        store.clear().unwrap();

        assert!(store.patterns.is_empty());
        assert!(!store.history_path().exists());
        assert!(!store.patterns_path().exists());
    }

    #[test]
    fn test_history_store_stats() {
        let (mut store, _temp_dir) = create_test_store();

        // Add some data
        for i in 0..3 {
            let record = QueryRecord::new(format!("query {}", i), vec![], "model".to_string());
            store.record_query(&record).unwrap();
        }
        store.record_selection("query 0", "cmd").unwrap();
        store.record_selection("query 1", "cmd").unwrap();

        let stats = store.stats().unwrap();
        assert_eq!(stats.total_queries, 3);
        assert_eq!(stats.unique_patterns, 2);
        assert_eq!(stats.patterns_with_preference, 2);
    }

    #[test]
    fn test_history_store_persistence() {
        let temp_dir = TempDir::new().unwrap();
        let data_dir = temp_dir.path().to_path_buf();

        // Create store and add data
        {
            let mut store = HistoryStore::with_data_dir(data_dir.clone()).unwrap();
            store.record_selection("test query", "test command").unwrap();
        }

        // Create new store from same directory
        {
            let store = HistoryStore::with_data_dir(data_dir).unwrap();
            let pattern = store.get_pattern("test query").unwrap();
            assert_eq!(pattern.preferred_command, Some("test command".to_string()));
        }
    }

    #[test]
    fn test_history_store_default() {
        let store = HistoryStore::default();
        // Should not panic, creates with default path
        assert!(store.patterns.is_empty());
    }

    #[test]
    fn test_score_command_preferred() {
        let (mut store, _temp_dir) = create_test_store();

        store.record_selection("query", "preferred_cmd").unwrap();
        store.record_selection("query", "preferred_cmd").unwrap();
        store.record_selection("query", "other_cmd").unwrap();

        let pattern = store.get_pattern("query").unwrap();

        let preferred_score = store.score_command("preferred_cmd", pattern);
        let other_score = store.score_command("other_cmd", pattern);
        let unknown_score = store.score_command("unknown", pattern);

        assert!(preferred_score > other_score);
        assert!(other_score > unknown_score);
        assert_eq!(unknown_score, 0.0);
    }

    #[test]
    fn test_command_selection_serialization() {
        let selection = CommandSelection {
            command: "ls -la".to_string(),
            selection_count: 5,
            last_selected: Utc::now(),
        };

        let json = serde_json::to_string(&selection).unwrap();
        let deserialized: CommandSelection = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.command, selection.command);
        assert_eq!(deserialized.selection_count, selection.selection_count);
    }

    #[test]
    fn test_query_record_serialization() {
        let record = QueryRecord::new("test query".to_string(), vec!["cmd1".to_string()], "model".to_string());

        let json = serde_json::to_string(&record).unwrap();
        let deserialized: QueryRecord = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.query, record.query);
        assert_eq!(deserialized.results, record.results);
        assert_eq!(deserialized.id, record.id);
    }

    #[test]
    fn test_query_pattern_serialization() {
        let mut pattern = QueryPattern::new("test");
        pattern.record_selection("cmd1");

        let json = serde_json::to_string(&pattern).unwrap();
        let deserialized: QueryPattern = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.normalized_query, pattern.normalized_query);
        assert_eq!(deserialized.query_count, pattern.query_count);
    }

    #[test]
    fn test_history_stats_empty() {
        let (store, _temp_dir) = create_test_store();
        let stats = store.stats().unwrap();

        assert_eq!(stats.total_queries, 0);
        assert_eq!(stats.unique_patterns, 0);
        assert_eq!(stats.patterns_with_preference, 0);
    }

    #[test]
    fn test_get_pattern_case_insensitive() {
        let (mut store, _temp_dir) = create_test_store();

        store.record_selection("List Files", "ls").unwrap();

        // Should find pattern regardless of case
        assert!(store.get_pattern("list files").is_some());
        assert!(store.get_pattern("LIST FILES").is_some());
        assert!(store.get_pattern("List Files").is_some());
    }

    #[test]
    fn test_multiple_selections_same_command() {
        let (mut store, _temp_dir) = create_test_store();

        for _ in 0..10 {
            store.record_selection("query", "frequent_cmd").unwrap();
        }
        store.record_selection("query", "rare_cmd").unwrap();

        let pattern = store.get_pattern("query").unwrap();
        let frequent = pattern
            .command_history
            .iter()
            .find(|c| c.command == "frequent_cmd")
            .unwrap();
        let rare = pattern
            .command_history
            .iter()
            .find(|c| c.command == "rare_cmd")
            .unwrap();

        assert_eq!(frequent.selection_count, 10);
        assert_eq!(rare.selection_count, 1);
    }
}
