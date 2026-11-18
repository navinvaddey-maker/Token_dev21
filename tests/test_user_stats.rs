use sqlx::sqlite::SqlitePoolOptions;
use token_compress_engine::data::Repository;

#[tokio::test]
async fn test_get_user_stats_empty_and_aggregated() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("connect in-memory sqlite");

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("run migrations");

    // Create two users to verify tenant isolation.
    Repository::create_user(&pool, "u1", "alice", "hash1", "alice@example.com", "user")
        .await
        .expect("create u1");
    Repository::create_user(&pool, "u2", "bob", "hash2", "bob@example.com", "user")
        .await
        .expect("create u2");

    // 1. Empty user stats should return zeroes and empty breakdown arrays.
    let empty = Repository::get_user_stats(&pool, "u1")
        .await
        .expect("empty stats");
    assert_eq!(empty["total_compressions"], 0);
    assert_eq!(empty["total_tokens_saved"], 0);
    assert_eq!(empty["total_original_tokens"], 0);
    assert_eq!(empty["total_final_tokens"], 0);
    assert_eq!(empty["best_tokens_saved"], 0);
    assert_eq!(empty["mode_breakdown"].as_array().map(Vec::len), Some(0));
    assert_eq!(empty["domain_breakdown"].as_array().map(Vec::len), Some(0));
    assert_eq!(empty["daily_series"].as_array().map(Vec::len), Some(0));
    assert_eq!(empty["top_runs"].as_array().map(Vec::len), Some(0));

    // 2. Insert 4 runs for u1 and 1 run for u2.
    let inserts = [
        (
            "h1",
            "u1",
            "Software bug report prompt...",
            100,
            60,
            40,
            "software",
            "balanced",
        ),
        (
            "h2",
            "u1",
            "Legal contract review prompt...",
            200,
            110,
            90,
            "legal",
            "aggressive",
        ),
        (
            "h3",
            "u1",
            "Real estate mortgage prompt...",
            150,
            120,
            30,
            "real-estate",
            "gentle",
        ),
        (
            "h4",
            "u1",
            "Another software prompt...",
            120,
            60,
            60,
            "software",
            "balanced",
        ),
        (
            "h5",
            "u2",
            "Bob private prompt...",
            500,
            100,
            400,
            "finance",
            "aggressive",
        ),
    ];

    for (id, uid, orig, t_orig, t_fin, t_saved, uc, mode) in inserts {
        sqlx::query(
            "INSERT INTO token_history
                (id, user_id, original_prompt, optimized_prompt, tokens_saved, token_original, token_final, use_case, mode)
             VALUES ($1, $2, $3, 'optimized', $4, $5, $6, $7, $8)",
        )
        .bind(id)
        .bind(uid)
        .bind(orig)
        .bind(t_saved)
        .bind(t_orig)
        .bind(t_fin)
        .bind(uc)
        .bind(mode)
        .execute(&pool)
        .await
        .expect("insert history row");
    }

    let stats = Repository::get_user_stats(&pool, "u1")
        .await
        .expect("populated stats");

    assert_eq!(stats["total_compressions"], 4);
    assert_eq!(stats["total_tokens_saved"], 220);
    assert_eq!(stats["total_original_tokens"], 570);
    assert_eq!(stats["total_final_tokens"], 350);
    assert_eq!(stats["best_tokens_saved"], 90);

    let modes = stats["mode_breakdown"].as_array().expect("mode array");
    assert_eq!(modes.len(), 3);
    assert_eq!(modes[0]["mode"], "balanced");
    assert_eq!(modes[0]["count"], 2);
    assert_eq!(modes[0]["tokens_saved"], 100);

    let domains = stats["domain_breakdown"].as_array().expect("domain array");
    assert_eq!(domains.len(), 3);
    assert_eq!(domains[0]["domain"], "software");
    assert_eq!(domains[0]["tokens_saved"], 100);

    let daily = stats["daily_series"].as_array().expect("daily array");
    assert_eq!(daily.len(), 1);
    assert_eq!(daily[0]["runs"], 4);
    assert_eq!(daily[0]["tokens_saved"], 220);

    let top = stats["top_runs"].as_array().expect("top array");
    assert_eq!(top.len(), 3);
    assert_eq!(top[0]["id"], "h2");
    assert_eq!(top[0]["tokens_saved"], 90);
}
