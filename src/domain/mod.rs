//! Domain business logic: authentication, compression execution, continuation state,
//! history/feedback, and tenant-scoped administrative operations.
//!
//! // ARCH: ARCHITECTURE.md §1 Reality Check, §2 Component Map, §7 sqlx Rules — Auth and multi-tenancy.

pub mod admin;
pub mod auth;
pub mod compress;
pub mod continuation;
pub mod history;

pub use admin::{
    admin_delete_user, admin_get_compression_analytics, admin_get_error_metrics,
    admin_get_feedback_analysis, admin_get_learning_engine_stats, admin_get_stats,
    admin_get_system_performance, admin_get_user_engagement_stats, admin_list_history,
    admin_list_users, admin_update_user, is_admin,
};
pub use auth::{
    create_jwt, create_jwt_for_user, jwt_secret, login_user, register_user, validate_security_env,
    verify_jwt, verify_jwt_claims, AuthContext, Claims,
};
pub use compress::{compress_demo, compress_new, compress_new_scoped, run_aggressive_scoped};
pub use history::{
    get_dev_config, get_history_record, get_history_record_scoped, get_stats, get_stats_scoped,
    list_history, list_history_scoped, record_explicit_feedback, record_explicit_feedback_scoped,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::user::RegisterRequest;

    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn test_validate_security_env() {
        let _lock = ENV_LOCK.lock().unwrap();
        std::env::remove_var("JWT_SECRET");
        assert!(validate_security_env().is_err());

        std::env::set_var("JWT_SECRET", "short_secret");
        assert!(validate_security_env().is_err());

        std::env::set_var(
            "JWT_SECRET",
            "this_is_a_sufficiently_long_secret_key_32_bytes",
        );
        assert!(validate_security_env().is_ok());
    }

    #[test]
    fn test_jwt_claims_and_auth_context() {
        let _lock = ENV_LOCK.lock().unwrap();
        std::env::set_var(
            "JWT_SECRET",
            "this_is_a_sufficiently_long_secret_key_32_bytes",
        );

        let token =
            create_jwt_for_user("user_123", "tenant_xyz", "admin").expect("Token creation failed");

        let auth_ctx = verify_jwt_claims(&token).expect("Token verification failed");
        assert_eq!(auth_ctx.user_id, "user_123");
        assert_eq!(auth_ctx.tenant_id, "tenant_xyz");
        assert_eq!(auth_ctx.role, "admin");

        let simple_user_id = verify_jwt(&token).expect("Simple verify failed");
        assert_eq!(simple_user_id, "user_123");
    }

    #[tokio::test]
    async fn test_registration_business_type_allowlist() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory pool");

        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("migrations");

        // Reject "admin" as business_type
        let bad_admin_req = RegisterRequest {
            username: "hacker".into(),
            password: "password123".into(),
            email: "hacker@example.com".into(),
            business_type: Some("admin".into()),
        };
        assert!(register_user(&pool, bad_admin_req).await.is_err());

        // Reject unknown business_type
        let bad_type_req = RegisterRequest {
            username: "hacker2".into(),
            password: "password123".into(),
            email: "hacker2@example.com".into(),
            business_type: Some("super_user".into()),
        };
        assert!(register_user(&pool, bad_type_req).await.is_err());

        // Accept valid business_type
        let valid_req = RegisterRequest {
            username: "developer1".into(),
            password: "password123".into(),
            email: "dev@example.com".into(),
            business_type: Some("developer".into()),
        };
        let user = register_user(&pool, valid_req)
            .await
            .expect("registration failed");
        assert_eq!(user.role, "user");
        assert_eq!(user.tenant_id, "default");
        assert_eq!(user.business_type, "developer");
    }
}
