# CtxE and System One Research

Conducted: 2026-09-22 UTC

## Findings

CtxE's public documentation describes a local, syntax-aware index with a
two-phase publish pipeline, hybrid semantic plus indexed-text retrieval, typed
symbol relationships, a goal-driven multi-round Ask flow, source snapshots that
report evidence validity, and explicit degradation/truncation diagnostics. Its
MCP integration is stdio-first and exposes separate status, tree, retrieval,
definition, usage, graph, impact, and record tools.

The current repository already has strong local foundations: Tree-sitter
parsing, incremental indexing, a persisted call graph, vector shards with lazy
warming, optional LLM/agentic reranking, freshness gates, and a streamable
HTTP MCP service. The main gaps are exact lexical recall, machine-readable
confidence/evidence metadata, a structured Ask entry point, and stdio MCP
transport.

TypeSafe's article presents System One models as fast decision functions: typed
outputs selected from a fixed schema, probabilities/confidence on every answer,
parallel sampling, and a preference for composing many small decisions inside
ordinary software workflows. The article itself did not include the API
contract, so the initial implementation used a typed local decision layer;
follow-up documentation verification now supports the optional API client
described below.

## Implementation choice

The change keeps vector search as the semantic backbone, adds bounded lexical
candidate lookup against the already-indexed chunk store, and fuses both scores
before graph expansion and reranking. Query results gain structured evidence
and decision metadata. `ask-context` returns that data as JSON, while the
existing text tool remains unchanged for clients that need concise snippets.

## Follow-up API verification

The current TypeSafe documentation now publishes a stable HTTP contract at
`POST https://api.typesafe.ai/v1/systemone` with `Bearer` authentication,
`jev-latest`, and the `noul`, `choice`, and `score` question primitives. The
follow-up implementation uses `TYPESAFE_API_KEY` and keeps the integration
optional: a missing key or non-success response falls back to the local typed
decision without copying credentials or provider response bodies into logs.

## References

- CtxE overview and docs: `https://ctxe.tlelabs.com/`
- CtxE machine-readable index: `https://ctxe.tlelabs.com/llms.txt`
- CtxE retrieval/evidence docs: `https://ctxe.tlelabs.com/docs/how-it-works`
- CtxE MCP contract: `https://ctxe.tlelabs.com/docs/mcp-integration`
- CtxE tools: `https://ctxe.tlelabs.com/docs/tools-reference`
- TypeSafe article: `https://typesafe.ai/blog/introducing-system-one-models-and-jev`
