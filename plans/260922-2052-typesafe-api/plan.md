# TypeSafe API Integration

Status: completed

## Outcome

Use the public TypeSafe System One HTTP API from the structured `ask-context`
workflow when `TYPESAFE_API_KEY` is configured, while preserving local retrieval
and degradation when the key or service is unavailable.

## Scope

- Add a small Rust client for `https://api.typesafe.ai/v1/systemone`.
- Ask bounded `noul`, `choice`, and `score` questions over retrieved evidence.
- Return the typed TypeSafe answers, model, and usage in `ask-context` JSON and
  MCP `structuredContent` without exposing credentials.
- Keep all TypeSafe calls outside database transactions and cap request state.

## Acceptance criteria

1. `TYPESAFE_API_KEY` enables a real authenticated request using `jev-latest`.
2. Missing keys and non-success responses degrade to local Ask output with a
   warning and no secret values in logs or responses.
3. Valid TypeSafe responses preserve typed answers and usage metadata.
4. Unit tests cover request shape, response decoding, error redaction, and the
   no-key path; existing library/integration checks remain green.

## Verification

- `cargo test --lib`: 668 passed, 0 failed, 6 ignored, including the TypeSafe
  request/response tests.
- `cargo test --test integration --test router_integration --test mcp_session_restore`:
  33 passed.
- `cargo check --all-targets`: passed.
- TypeSafe failures are logged only as status-level errors and return local Ask
  output; response bodies and credentials are never copied into errors.
