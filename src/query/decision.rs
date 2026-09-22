//! Typed retrieval decisions and source-evidence validity.
//!
//! This is the local equivalent of a System One decision boundary: the engine
//! emits a fixed schema with explicit uncertainty instead of forcing clients to
//! infer confidence from prose. The scores are intentionally labelled as local
//! heuristics; they are useful routing signals, not claims about Jev's or an
//! external model's calibration.

use std::collections::HashMap;
use std::path::Path;

use chrono::Utc;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::query::engine::CodeResult;
use crate::query::merger::MergeChunk;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceStatus {
    Unchanged,
    Changed,
    Partial,
    Missing,
    Unavailable,
}

#[derive(Debug, Clone, Serialize)]
pub struct EvidenceRecord {
    pub file: String,
    pub line_start: u32,
    pub line_end: u32,
    pub status: EvidenceStatus,
    pub indexed_sha256: Option<String>,
    pub current_sha256: Option<String>,
    pub reason: String,
    pub captured_at: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DecisionAction {
    Answer,
    NeedMoreEvidence,
    RetryIndex,
    NoRelevantEvidence,
}

#[derive(Debug, Clone, Serialize)]
pub struct RetrievalSignals {
    pub result_count: usize,
    pub top_score: f32,
    pub evidence_coverage: f32,
    pub graph_pending: bool,
    pub warming: bool,
    pub rerank_fallback: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RetrievalDecision {
    pub action: DecisionAction,
    /// A bounded local routing score. It is not an externally calibrated
    /// probability and is labelled as such by `confidence_kind`.
    pub confidence: f32,
    pub uncertainty: f32,
    pub confidence_kind: &'static str,
    pub signals: RetrievalSignals,
    pub reasons: Vec<String>,
}

/// Build evidence records by comparing the indexed chunk body with the current
/// source lines. Reads are cached per `(file, line range)` so a result set with
/// adjacent snippets does not repeatedly hit the filesystem.
pub fn collect_evidence(results: &[CodeResult], chunks: &[MergeChunk]) -> Vec<EvidenceRecord> {
    let mut current_cache: HashMap<(String, u32, u32), Option<String>> = HashMap::new();
    let captured_at = Utc::now().to_rfc3339();

    results
        .iter()
        .map(|result| {
            let source_key = (result.file.clone(), result.line_start, result.line_end);
            let current = current_cache
                .entry(source_key)
                .or_insert_with(|| read_numbered_source(result))
                .clone();
            let indexed_chunk = chunks.iter().find(|chunk| {
                chunk.file == result.file
                    && chunk.line_start <= result.line_start
                    && chunk.line_end >= result.line_end
            });
            let indexed = indexed_chunk.map(|chunk| normalize_indexed(&chunk.content));

            let (status, current_sha256, reason) = match current.as_deref() {
                None if !Path::new(&result.file).exists() => (
                    EvidenceStatus::Missing,
                    None,
                    "source file is no longer present on disk".to_owned(),
                ),
                None => (
                    EvidenceStatus::Unavailable,
                    None,
                    "source lines could not be read from disk".to_owned(),
                ),
                Some(current) => {
                    let current_hash = sha256(current);
                    let partial = indexed_chunk.is_some_and(|chunk| {
                        chunk.line_start != result.line_start || chunk.line_end != result.line_end
                    });
                    if partial {
                        (
                            EvidenceStatus::Partial,
                            Some(current_hash),
                            "the result contains a bounded slice of a larger indexed chunk"
                                .to_owned(),
                        )
                    } else if indexed.as_deref() == Some(current) {
                        (
                            EvidenceStatus::Unchanged,
                            Some(current_hash),
                            "source content matches the indexed snapshot".to_owned(),
                        )
                    } else {
                        (
                            EvidenceStatus::Changed,
                            Some(current_hash),
                            "source content differs from the indexed snapshot".to_owned(),
                        )
                    }
                }
            };

            EvidenceRecord {
                file: result.file.clone(),
                line_start: result.line_start,
                line_end: result.line_end,
                status,
                indexed_sha256: indexed.as_deref().map(sha256),
                current_sha256,
                reason,
                captured_at: captured_at.clone(),
            }
        })
        .collect()
}

/// Emit a typed routing decision from deterministic retrieval signals.
pub fn decide(
    results: &[CodeResult],
    evidence: &[EvidenceRecord],
    warming: bool,
    graph_pending: bool,
    rerank_fallback: bool,
) -> RetrievalDecision {
    let top_score = results
        .iter()
        .map(|result| result.score)
        .fold(0.0_f32, f32::max)
        .clamp(0.0, 1.0);
    let unchanged = evidence
        .iter()
        .filter(|record| matches!(record.status, EvidenceStatus::Unchanged))
        .count();
    let coverage = if evidence.is_empty() {
        if results.is_empty() { 0.0 } else { 0.5 }
    } else {
        unchanged as f32 / evidence.len() as f32
    };

    let (mut action, mut confidence, mut reasons) = if results.is_empty() && warming {
        (
            DecisionAction::RetryIndex,
            0.95,
            vec!["the vector shard is still warming or publishing".to_owned()],
        )
    } else if results.is_empty() {
        (
            DecisionAction::NoRelevantEvidence,
            0.65,
            vec!["no candidate survived the retrieval and content fences".to_owned()],
        )
    } else {
        (
            DecisionAction::Answer,
            (top_score * 0.65 + coverage * 0.35).clamp(0.0, 1.0),
            vec!["retrieval returned content-verified candidates".to_owned()],
        )
    };

    if evidence.iter().any(|record| {
        matches!(
            record.status,
            EvidenceStatus::Changed | EvidenceStatus::Missing | EvidenceStatus::Unavailable
        )
    }) {
        action = DecisionAction::NeedMoreEvidence;
        confidence *= 0.55;
        reasons.push("at least one cited source is stale, missing, or unreadable".to_owned());
    } else if evidence
        .iter()
        .any(|record| matches!(record.status, EvidenceStatus::Partial))
    {
        confidence *= 0.85;
        reasons.push("some evidence is a bounded slice of a larger chunk".to_owned());
    }
    if graph_pending && !results.is_empty() {
        action = DecisionAction::NeedMoreEvidence;
        confidence *= 0.8;
        reasons.push("call-graph resolution is still pending".to_owned());
    }
    if rerank_fallback && !results.is_empty() {
        confidence *= 0.9;
        reasons.push("optional reranking was unavailable; local ranking was used".to_owned());
    }

    confidence = confidence.clamp(0.0, 1.0);
    RetrievalDecision {
        action,
        confidence,
        uncertainty: 1.0 - confidence,
        confidence_kind: "local_heuristic",
        signals: RetrievalSignals {
            result_count: results.len(),
            top_score,
            evidence_coverage: coverage,
            graph_pending,
            warming,
            rerank_fallback,
        },
        reasons,
    }
}

fn read_numbered_source(result: &CodeResult) -> Option<String> {
    crate::query::engine::read_lines_from_fs(&result.file, result.line_start, result.line_end)
        .ok()
        .map(|text| normalize_numbered(&text))
}

fn normalize_indexed(content: &str) -> String {
    content.replace("\r\n", "\n").trim_end().to_owned()
}

fn normalize_numbered(content: &str) -> String {
    content
        .lines()
        .map(|line| {
            line.split_once(": ")
                .and_then(|(number, text)| number.parse::<u32>().ok().map(|_| text))
                .unwrap_or(line)
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim_end()
        .to_owned()
}

fn sha256(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    format!("{digest:x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(score: f32) -> CodeResult {
        CodeResult {
            file: "/tmp/example.rs".to_owned(),
            line_start: 1,
            line_end: 1,
            score,
            content: "1: fn main() {}".to_owned(),
            symbol: Some("main".to_owned()),
            callers: None,
            caller_files: None,
            caller_names: vec![],
            callee_names: vec![],
            callees: None,
        }
    }

    #[test]
    fn empty_warming_result_requests_retry() {
        let decision = decide(&[], &[], true, false, false);
        assert_eq!(decision.action, DecisionAction::RetryIndex);
        assert!(decision.confidence > 0.9);
    }

    #[test]
    fn stale_evidence_requires_more_evidence() {
        let evidence = vec![EvidenceRecord {
            file: "x.rs".to_owned(),
            line_start: 1,
            line_end: 2,
            status: EvidenceStatus::Changed,
            indexed_sha256: Some("a".to_owned()),
            current_sha256: Some("b".to_owned()),
            reason: "changed".to_owned(),
            captured_at: "now".to_owned(),
        }];
        let decision = decide(&[result(0.9)], &evidence, false, false, false);
        assert_eq!(decision.action, DecisionAction::NeedMoreEvidence);
        assert!(decision.uncertainty > 0.4);
    }
}
