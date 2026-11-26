//! User persistence, tenant-scoped lookups, and atomic admin-preservation deletes.
//!
//! // ARCH: ARCHITECTURE.md §2 Component Map, §7 sqlx Rules — Multi-tenant user store & RBAC.

use db::DbPool;

use super::Repository;
use crate::models::user::User;

/// Outcome of a tenant-scoped user deletion guarded by last-admin protection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeleteUserOutcome {
    /// Target user existed in the tenant and was deleted.
    Deleted,
    /// Target user was not found in the tenant.
    NotFound,
    /// Target user is the last remaining admin in the tenant and cannot be deleted.
    LastAdminProtected,
}

impl Repository {
    /// Checks whether the backing database pool responds to a lightweight `SELECT 1` probe.
    ///
    /// # Panics
    /// Never.
    pub async fn verify_database_connectivity(pool: &DbPool) -> bool {
        sqlx::query("SELECT 1").execute(pool).await.is_ok()
    }

    /// Creates a user defaulting to `'user'` role and `'default'` tenant.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] on query execution failure.
    ///
    /// # Panics
    /// Never.
    pub async fn create_user(
        pool: &DbPool,
        id: &str,
        username: &str,
        password_hash: &str,
        email: &str,
        business_type: &str,
    ) -> Result<User, sqlx::Error> {
        Self::create_user_scoped(
            pool,
            id,
            username,
            password_hash,
            email,
            business_type,
            "user",
            "default",
        )
        .await
    }

    /// Creates a user with explicit `role` and `tenant_id` scoping.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] on query execution failure.
    ///
    /// # Panics
    /// Never.
    #[allow(clippy::too_many_arguments)] // Preserves existing multi-column user creation API.
    pub async fn create_user_scoped(
        pool: &DbPool,
        id: &str,
        username: &str,
        password_hash: &str,
        email: &str,
        business_type: &str,
        role: &str,
        tenant_id: &str,
    ) -> Result<User, sqlx::Error> {
        sqlx::query_as::<_, User>(
            "INSERT INTO users (id, username, password_hash, email, business_type, role, tenant_id)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             RETURNING id, username, password_hash, email, business_type, role, tenant_id, license, created_at, updated_at",
        )
        .bind(id)
        .bind(username)
        .bind(password_hash)
        .bind(email)
        .bind(business_type)
        .bind(role)
        .bind(tenant_id)
        .fetch_one(pool)
        .await
    }

    /// Finds a user by `id`, selecting `role` and `tenant_id`.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] on query execution failure.
    ///
    /// # Panics
    /// Never.
    pub async fn find_user_by_id(pool: &DbPool, id: &str) -> Result<Option<User>, sqlx::Error> {
        sqlx::query_as::<_, User>(
            "SELECT id, username, password_hash, email, business_type, role, tenant_id, license, created_at, updated_at
             FROM users WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(pool)
        .await
    }

    /// Finds a user by `username`, selecting `role` and `tenant_id`.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] on query execution failure.
    ///
    /// # Panics
    /// Never.
    pub async fn find_user_by_username(
        pool: &DbPool,
        username: &str,
    ) -> Result<Option<User>, sqlx::Error> {
        sqlx::query_as::<_, User>(
            "SELECT id, username, password_hash, email, business_type, role, tenant_id, license, created_at, updated_at
             FROM users WHERE username = $1",
        )
        .bind(username)
        .fetch_optional(pool)
        .await
    }

    /// Lists all users paginated, selecting `role` and `tenant_id`.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] on query execution failure.
    ///
    /// # Panics
    /// Never.
    pub async fn list_all_users(
        pool: &DbPool,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<User>, sqlx::Error> {
        sqlx::query_as::<_, User>(
            "SELECT id, username, password_hash, email, business_type, role, tenant_id, license, created_at, updated_at
             FROM users ORDER BY created_at DESC LIMIT $1 OFFSET $2",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
    }

    /// Lists users scoped to a specific `tenant_id`.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] on query execution failure.
    ///
    /// # Panics
    /// Never.
    pub async fn list_users_by_tenant(
        pool: &DbPool,
        tenant_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<User>, sqlx::Error> {
        sqlx::query_as::<_, User>(
            "SELECT id, username, password_hash, email, business_type, role, tenant_id, license, created_at, updated_at
             FROM users WHERE tenant_id = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3",
        )
        .bind(tenant_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
    }

    /// Counts active administrative users based on `LOWER(role) = 'admin'`.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] on query execution failure.
    ///
    /// # Panics
    /// Never.
    pub async fn count_admins(pool: &DbPool) -> Result<i64, sqlx::Error> {
        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM users WHERE LOWER(role) = 'admin'")
                .fetch_one(pool)
                .await?;
        Ok(count.0)
    }

    /// Counts active administrative users within a specific `tenant_id`.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] on query execution failure.
    ///
    /// # Panics
    /// Never.
    pub async fn count_admins_scoped(pool: &DbPool, tenant_id: &str) -> Result<i64, sqlx::Error> {
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM users WHERE LOWER(role) = 'admin' AND tenant_id = $1",
        )
        .bind(tenant_id)
        .fetch_one(pool)
        .await?;
        Ok(count.0)
    }

    /// Deletes a user by `id`.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] on query execution failure.
    ///
    /// # Panics
    /// Never.
    pub async fn delete_user(pool: &DbPool, id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }

    /// Deletes a user by `id` scoped to a specific `tenant_id`.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] on query execution failure.
    ///
    /// # Panics
    /// Never.
    pub async fn delete_user_scoped(
        pool: &DbPool,
        tenant_id: &str,
        id: &str,
    ) -> Result<bool, sqlx::Error> {
        let res = sqlx::query("DELETE FROM users WHERE id = $1 AND tenant_id = $2")
            .bind(id)
            .bind(tenant_id)
            .execute(pool)
            .await?;
        Ok(res.rows_affected() > 0)
    }

    /// Atomically deletes a user within `tenant_id` while refusing to remove the last remaining
    /// admin in that tenant, using a single guarded `DELETE` write statement so SQLite's write lock
    /// serializes concurrent admin deletions without TOCTOU races.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] on any SQL execution failure.
    ///
    /// # Panics
    /// Never.
    pub async fn delete_user_preserving_last_admin_scoped(
        pool: &DbPool,
        tenant_id: &str,
        id: &str,
    ) -> Result<DeleteUserOutcome, sqlx::Error> {
        // ARCH: §7 sqlx Rules — single atomic guarded DELETE acquires SQLite write lock up-front,
        // eliminating DEFERRED transaction TOCTOU races when two admins are deleted concurrently.
        let rows = sqlx::query(
            "DELETE FROM users
             WHERE id = $1
               AND tenant_id = $2
               AND (
                 LOWER(role) != 'admin'
                 OR (SELECT COUNT(*) FROM users WHERE LOWER(role) = 'admin' AND tenant_id = $2) > 1
               )",
        )
        .bind(id)
        .bind(tenant_id)
        .execute(pool)
        .await?;

        if rows.rows_affected() > 0 {
            return Ok(DeleteUserOutcome::Deleted);
        }

        let existing_role: Option<(String,)> =
            sqlx::query_as("SELECT role FROM users WHERE id = $1 AND tenant_id = $2")
                .bind(id)
                .bind(tenant_id)
                .fetch_optional(pool)
                .await?;

        match existing_role {
            Some((role,)) if role.eq_ignore_ascii_case("admin") => {
                Ok(DeleteUserOutcome::LastAdminProtected)
            }
            _ => Ok(DeleteUserOutcome::NotFound),
        }
    }

    /// Updates a user's `business_type`.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] on query execution failure.
    ///
    /// # Panics
    /// Never.
    pub async fn update_user_business_type(
        pool: &DbPool,
        id: &str,
        business_type: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE users SET business_type = $1, updated_at = CURRENT_TIMESTAMP WHERE id = $2",
        )
        .bind(business_type)
        .bind(id)
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Updates a user's `business_type` scoped to a specific `tenant_id`.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] on query execution failure.
    ///
    /// # Panics
    /// Never.
    pub async fn update_user_business_type_scoped(
        pool: &DbPool,
        tenant_id: &str,
        id: &str,
        business_type: &str,
    ) -> Result<u64, sqlx::Error> {
        let res = sqlx::query(
            "UPDATE users SET business_type = $1, updated_at = CURRENT_TIMESTAMP WHERE id = $2 AND tenant_id = $3",
        )
        .bind(business_type)
        .bind(id)
        .bind(tenant_id)
        .execute(pool)
        .await?;
        Ok(res.rows_affected())
    }

    /// Updates a user's password hash.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] on query execution failure.
    ///
    /// # Panics
    /// Never.
    pub async fn update_user_password(
        pool: &DbPool,
        id: &str,
        password_hash: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE users SET password_hash = $1, updated_at = CURRENT_TIMESTAMP WHERE id = $2",
        )
        .bind(password_hash)
        .bind(id)
        .execute(pool)
        .await?;
        Ok(())
    }
}
