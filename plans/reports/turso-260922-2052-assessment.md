# Turso Integration Assessment

Conducted: 2026-09-22 UTC

## Decision

Use Turso as an optional experimental storage and benchmark backend, not as an
immediate replacement for SurrealDB.

The current engine already has large-repository behavior that would be risky to
discard: per-repository SurrealDB handles, persisted vector shards, mmap warmup,
quantized resident vectors, LRU eviction, mutation fences, and extensive
incremental/crash-recovery tests. The Turso path should prove equivalent
behavior and performance before it becomes the default.

## Evidence

Turso's current public documentation provides a code-indexing schema that maps
well to this repository: indexed files with content hashes, chunks with names,
signatures, line ranges and embeddings, FTS indexes over identifiers, and vector
distance queries. It also supports `BEGIN CONCURRENT` with MVCC for independent
writes.

The important limits are equally clear:

- FTS is powered by Tantivy and requires the experimental `index_method` feature.
- Vector similarity queries are exact linear scans; approximate vector indexing
  remains on the roadmap.
- FTS and vector syntax/API may change while those features are experimental.
- Graph edges can be represented in relational tables, but traversal, cleanup,
  and incremental blast-radius logic remain application-owned.

Sources:

- Turso code indexing guide: `https://docs.turso.tech/guides/code-indexing`
- Turso vector search: `https://docs.turso.tech/guides/vector-search`
- Turso FTS reference: `https://docs.turso.tech/sql-reference/functions/fts`
- Turso Rust reference: `https://docs.turso.tech/sdk/rust/reference`
- Turso concurrent writes: `https://docs.turso.tech/tursodb/concurrent-writes`
- Turso repository: `https://github.com/tursodatabase/turso`

## Recommended prototype

1. Add an opt-in `turso-experimental` feature and a separate local database
   file; do not alter the existing SurrealDB path.
2. Import one indexed repository into `codebases`, `indexed_files`, `chunks`,
   `symbols`, and `edges`, using `file_hash`/`chunk_key` for idempotent upserts.
3. Create weighted FTS over `name`, `signature`, and a bounded snippet field;
   store vectors as `vector8` or `vector32` and compare exact-search latency to
   the current resident shard.
4. Run the same query corpus through SurrealDB and Turso and record indexing
   time, query p50/p95, recall@k, memory, FTS freshness, and crash recovery.
5. Promote only if Turso matches the current correctness suite and wins on a
   measured workload; otherwise retain it as an export/portable-index option.

No database migration has been applied. If the prototype becomes a migration,
the repository must first create a durable backup and define a rollback path.
