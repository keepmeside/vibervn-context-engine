//! Bounded lexical retrieval used alongside semantic vector search.
//!
//! The vector index is still the primary semantic signal. This module adds a
//! small, deterministic text signal so exact symbol names, paths, and error
//! strings remain discoverable when an embedding is a poor match. Results are
//! capped before they enter the normal graph/merge/rerank pipeline.

use anyhow::{Context, Result};
use serde::Deserialize;
use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::vector::{ChunkId, SearchResult};

const MIN_TERM_CHARS: usize = 2;

#[derive(Debug, Clone)]
pub struct LexicalCandidate {
    pub result: SearchResult,
    pub lexical_score: f32,
}

#[derive(Debug, Deserialize)]
struct LexicalRow {
    file: String,
    line_start: i64,
    line_end: i64,
    content: String,
    symbol_ref: Option<String>,
}

/// Split a query into stable, case-insensitive terms suitable for bounded
/// substring lookup. Duplicate terms are removed while preserving order.
pub fn lexical_terms(query: &str) -> Vec<String> {
    let mut terms = Vec::new();
    for term in query
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .map(str::trim)
        .filter(|term| term.chars().count() >= MIN_TERM_CHARS)
        .map(str::to_lowercase)
    {
        if !terms.iter().any(|existing| existing == &term) {
            terms.push(term);
        }
    }
    terms
}

/// Score one row using transparent, bounded signals. The value is always in
/// `[0, 1]`; it is a ranking feature, not a calibrated probability.
pub fn lexical_score(
    query: &str,
    terms: &[String],
    content: &str,
    file: &str,
    symbol: Option<&str>,
) -> f32 {
    if terms.is_empty() {
        return 0.0;
    }

    let content_lower = content.to_lowercase();
    let file_lower = file.to_lowercase();
    let query_lower = query.trim().to_lowercase();
    let matched = terms
        .iter()
        .filter(|term| content_lower.contains(term.as_str()))
        .count();
    let term_coverage = matched as f32 / terms.len() as f32;
    let phrase_match =
        if query_lower.len() >= MIN_TERM_CHARS && content_lower.contains(&query_lower) {
            1.0
        } else {
            0.0
        };
    let path_match = if terms.iter().any(|term| file_lower.contains(term)) {
        1.0
    } else {
        0.0
    };
    let symbol_match = symbol
        .map(|name| {
            let name = name.to_lowercase();
            if terms
                .iter()
                .any(|term| name == *term || name.contains(term))
            {
                1.0
            } else {
                0.0
            }
        })
        .unwrap_or(0.0);

    (term_coverage * 0.55 + phrase_match * 0.2 + symbol_match * 0.2 + path_match * 0.05)
        .clamp(0.0, 1.0)
}

/// Find text matches in one repository database. The query uses only bound
/// values; the dynamic part consists solely of generated parameter names.
pub async fn search_db(
    db: &Surreal<Db>,
    query: &str,
    limit: usize,
) -> Result<Vec<LexicalCandidate>> {
    let terms = lexical_terms(query);
    if terms.is_empty() || limit == 0 {
        return Ok(Vec::new());
    }

    let predicates = terms
        .iter()
        .enumerate()
        .map(|(index, _)| {
            format!(
                "(string::lowercase(content) CONTAINS $term{index} OR \
                 string::lowercase(file) CONTAINS $term{index})"
            )
        })
        .collect::<Vec<_>>();
    let statement = format!(
        "SELECT file, line_start, line_end, content, symbol_ref FROM chunk WHERE {} LIMIT $limit",
        predicates.join(" OR ")
    );

    let mut query_builder = db.query(statement).bind(("limit", limit.min(500) as i64));
    for (index, term) in terms.iter().enumerate() {
        query_builder = query_builder.bind((format!("term{index}"), term.clone()));
    }
    let rows: Vec<LexicalRow> = query_builder
        .await
        .context("hybrid lexical chunk search")?
        .take(0)
        .context("decode hybrid lexical chunk search")?;

    let mut candidates = rows
        .into_iter()
        .map(|row| {
            let symbol = row
                .symbol_ref
                .as_deref()
                .and_then(|value| value.strip_prefix("symbol:⟨"))
                .and_then(|value| value.strip_suffix('⟩'))
                .and_then(|value| value.rsplit("::").next());
            let score = lexical_score(query, &terms, &row.content, &row.file, symbol);
            LexicalCandidate {
                result: SearchResult {
                    chunk_id: ChunkId {
                        file: row.file,
                        line_start: row.line_start.max(1) as u32,
                        line_end: row.line_end.max(row.line_start).max(1) as u32,
                    },
                    score,
                },
                lexical_score: score,
            }
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|a, b| {
        b.lexical_score
            .partial_cmp(&a.lexical_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.result.chunk_id.file.cmp(&b.result.chunk_id.file))
            .then_with(|| {
                a.result
                    .chunk_id
                    .line_start
                    .cmp(&b.result.chunk_id.line_start)
            })
    });
    candidates.truncate(limit.min(500));
    Ok(candidates)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terms_are_stable_and_deduplicated() {
        assert_eq!(
            lexical_terms("Auth auth: UserService.create()"),
            vec!["auth", "userservice", "create"]
        );
    }

    #[test]
    fn exact_phrase_and_symbol_match_rank_above_path_only() {
        let terms = lexical_terms("UserService create");
        let exact = lexical_score(
            "UserService create",
            &terms,
            "fn UserService create() {}",
            "/repo/src/service.rs",
            Some("create"),
        );
        let path_only = lexical_score(
            "UserService create",
            &terms,
            "fn unrelated() {}",
            "/repo/UserService/create.rs",
            None,
        );
        assert!(exact > path_only);
        assert!((0.0..=1.0).contains(&exact));
    }

    #[tokio::test]
    async fn search_db_returns_bounded_text_matches() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let db = crate::store::open_db(dir.path(), "/repo", 0)
            .await
            .expect("open db");
        db.query(
            "CREATE chunk SET file = $file, line_start = 1, line_end = 2, \
             content = $content, symbol_ref = $symbol",
        )
        .bind(("file", "/repo/src/auth.rs".to_owned()))
        .bind(("content", "fn authenticate_user() {}".to_owned()))
        .bind((
            "symbol",
            "symbol:⟨/repo/src/auth.rs::authenticate_user⟩".to_owned(),
        ))
        .await
        .expect("insert chunk");

        let matches = search_db(&db, "authenticate_user", 5)
            .await
            .expect("lexical search");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].result.chunk_id.file, "/repo/src/auth.rs");
        assert!(matches[0].lexical_score > 0.5);
    }
}
