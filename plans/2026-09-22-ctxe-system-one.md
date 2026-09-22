# CtxE and System One Integration

Status: completed

## Outcome

Improve `vibervn-context-engine` with the strongest compatible ideas found in
CtxE and TypeSafe's System One announcement: hybrid semantic and lexical
retrieval, a goal-driven structured Ask surface, source-evidence validity, and
typed local confidence decisions that clients can consume without parsing prose.

## Constraints

- Preserve the existing HTTP UI, streamable HTTP MCP endpoint, indexing model,
  and existing `codebase-retrieval` response contract.
- Keep all retrieval bounded and repository-scoped.
- Do not pretend to call Jev or reproduce proprietary calibration; expose an
  explicit local heuristic decision with its uncertainty and signals.
- Keep optional LLM/network work outside database write transactions and degrade
  to local retrieval when optional enrichment is unavailable.

## Non-goals

- Replacing the existing embedding or LLM providers.
- Adding a hosted TypeSafe dependency without a documented public API.
- Rebuilding the entire CtxE records database or daemon/service manager.

## Acceptance criteria

1. Queries combine vector candidates with bounded lexical candidates and keep
   exact symbol/path matches useful when embeddings miss them.
2. Query JSON includes a typed decision (`action`, confidence, uncertainty,
   evidence coverage, and reasons) and evidence records that distinguish
   unchanged, changed, partial, missing, and unavailable source content.
3. MCP exposes a default `ask-context` tool returning structured JSON while
   retaining the existing text retrieval tools.
4. The binary supports a stdio MCP mode suitable for Codex, Claude Code,
   Cursor, and other subprocess-launched clients.
5. Focused unit/integration checks cover lexical scoring, decision outcomes,
   evidence classification, config migration, and MCP tool wiring; the normal
   Rust checks pass.

## Sources

- CtxE public contract: `https://ctxe.tlelabs.com/llms.txt` and its current docs
  pages for hybrid retrieval, Ask, graph tools, evidence validity, and stdio MCP.
- TypeSafe AI, “Introducing System One Models & Jev”, published 2026-09-15:
  `https://typesafe.ai/blog/introducing-system-one-models-and-jev`.

## Verification

- `cargo test --lib`: 665 passed, 0 failed, 6 ignored.
- The `ask-context` schema and native MCP `structuredContent` tests are included in that run.
- `cargo test --test integration --test router_integration --test mcp_session_restore`: 33 passed.
- `cargo check --all-targets` and `cargo run -- --help`: passed.
- A temp-home MCP stdio initialize handshake returned a valid JSON-RPC response.
- `cargo clippy --all-targets -- -D warnings` still reports five pre-existing findings in `src/embedding/cache.rs`, `src/store/ops.rs`, `src/router/plan.rs`, and `src/server.rs`; no new findings remain in the changed code.
