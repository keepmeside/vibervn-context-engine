# Turso Experimental Backend Prototype

Status: planned

## Outcome

Benchmark a Turso-backed code index against the existing SurrealDB/vector-shard
path without changing the production default or deleting existing data.

## Boundaries

- Additive feature flag and separate database file only.
- No automatic migration, destructive schema change, or default backend switch.
- Reuse the existing parser/chunker/symbol/call-edge output.
- Keep TypeSafe and MCP behavior independent of the storage choice.

## Acceptance criteria

1. A fixture repository can be imported idempotently into Turso tables for files,
   chunks, symbols, and edges.
2. Turso FTS and exact vector queries return a common retrieval record shape.
3. A benchmark compares both backends on recall@k, p50/p95 latency, indexing
   time, memory, and recovery after an interrupted update.
4. Existing SurrealDB tests remain unchanged and green.
5. Promotion remains a measured decision; the prototype never silently becomes
   the default backend.
