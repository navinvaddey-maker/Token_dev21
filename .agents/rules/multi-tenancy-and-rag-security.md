---
trigger: glob
globs: "src/api/**/*.rs,src/domain/**/*.rs,src/data/**/*.rs,src/rag/**/*.rs,src/scenario/**/*.rs,crates/db/**/*.rs,migrations/**/*.sql"
---

# Multi-Tenancy & RAG Security Invariants

Enforce these security and multi-tenancy rules across all database, API, and RAG components.

## Key Rules

1. **Query Scoping**: Every database fetch or modification must enforce `(tenant_id, user_id)` scoping via `*_scoped` method suffixes where appropriate.
2. **Tenant-Admin Protection**: Tenant-admin deletion must verify `auth.tenant_id` scoping and include last-admin deletion protection.
3. **Privilege Separation**: `user.role` vs `user.business_type` must be verified via authoritative DB reload in `auth_middleware`.
4. **Zero Inline SQL in Handlers or Domain**: No inline `sqlx::query` in `src/api/` or `src/domain/`. All SQL queries must reside in persistence layers (`src/data/`, `src/rag/store.rs`, `src/npae/ory/memory.rs`, `src/classifier/mod.rs`, `crates/db/`).
5. **FTS5 Safety**: FTS5 queries must enforce `AND d.status = 'ready'`, `document_ids` parity, and double-quote every alphanumeric FTS5 token (including reserved keywords like `"NOT"`, `"AND"`, `"OR"`).
6. **Cloud vs Own Data Exclusivity**: Strict isolation (`127.0.0.1:49152`, no `Origin: null`).
7. **Embedding Dimension Contract**: `EMBEDDING_DIM = 384` across all vector operations and schema definitions.
