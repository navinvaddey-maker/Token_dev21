-- Migration 0016: Add role and tenant_id for Auth, RBAC, and Multi-Tenancy
-- ARCH: §0 Precedence, §7 sqlx Rules, §8 Vector DB Rules, §2 Component Map
-- Forward-safe note: SQLite ALTER TABLE ADD COLUMN with constant default executes in O(1)
-- time without rewriting table contents or acquiring blocking table locks.
-- Rollback note / risk: Columns added with constant defaults in SQLite cannot be dropped via
-- DROP COLUMN in SQLite < 3.35.0 (requires table recreation). Postgres supports DROP COLUMN.
-- Adding tenant_id and role with backwards-compatible defaults ensures zero disruption to existing queries.

-- 1. Users table updates
ALTER TABLE users ADD COLUMN role TEXT NOT NULL DEFAULT 'user';
ALTER TABLE users ADD COLUMN tenant_id TEXT NOT NULL DEFAULT 'default';

-- Migrate existing admin users
UPDATE users SET role = 'admin' WHERE LOWER(business_type) = 'admin';

-- 2. Multi-tenancy tenant_id on core relational tables
ALTER TABLE token_history ADD COLUMN tenant_id TEXT NOT NULL DEFAULT 'default';
ALTER TABLE rag_documents ADD COLUMN tenant_id TEXT NOT NULL DEFAULT 'default';
ALTER TABLE feedback_signals ADD COLUMN tenant_id TEXT NOT NULL DEFAULT 'default';
ALTER TABLE user_prompts ADD COLUMN tenant_id TEXT NOT NULL DEFAULT 'default';
ALTER TABLE learned_context ADD COLUMN tenant_id TEXT NOT NULL DEFAULT 'default';
ALTER TABLE user_domain_profile ADD COLUMN tenant_id TEXT NOT NULL DEFAULT 'default';
ALTER TABLE rag_retrieval_logs ADD COLUMN tenant_id TEXT NOT NULL DEFAULT 'default';

-- 3. Composite indexes for high-scale multi-tenant lookups
CREATE INDEX IF NOT EXISTS idx_users_tenant ON users(tenant_id, id);
CREATE INDEX IF NOT EXISTS idx_token_history_tenant_user ON token_history(tenant_id, user_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_rag_documents_tenant_user ON rag_documents(tenant_id, user_id);
CREATE INDEX IF NOT EXISTS idx_feedback_signals_tenant_user ON feedback_signals(tenant_id, user_id);
