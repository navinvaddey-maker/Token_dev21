# Project rules (always apply)

## Architecture
- Target layering: domain (pure), app (use cases), infra (sqlx repositories), api (Axum). Dependencies point inward only.
- domain and app must not import sqlx, axum, or tokio I/O types.
- All DB access goes through repository traits defined in app.

## Code
- No unsafe, unwrap, panic, todo, or dbg!. Use typed errors (thiserror in libraries, anyhow only in main).
- Handlers stay thin: parse input, call a use case, map the error. No SQL in handlers.
- Every public function has a doc comment.

## Database (SQLite now, PostgreSQL target)
- Use sqlx compile-time checked queries. Run `cargo sqlx prepare` after any query change and commit `.sqlx/`.
- Write portable SQL. Isolate any SQLite-only feature behind a repository method.
- List endpoints use keyset pagination, never OFFSET.
- Every new query path states which index serves it.
- Migrations are forward-only and small.

## Security
- No secrets in code or config files. Load them from the environment.
- Validate all input at the API boundary. Use parameterised queries only.
- Set timeouts on handlers, DB statements, and outbound calls.

## Dependencies
- Do not add a dependency without a one-line justification. It must pass `cargo deny check`.

## Process
- State the plan before editing.
- Before every commit, run the /review workflow. Do not commit with Critical or High findings.
