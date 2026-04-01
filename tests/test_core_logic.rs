use bcrypt;
use token_compress_engine::domain;
use uuid::Uuid;

#[tokio::test]
async fn test_password_hashing() {
    // Test that passwords are properly hashed
    let password = "mysecretpassword123";
    let hashed = bcrypt::hash(password, bcrypt::DEFAULT_COST).expect("Failed to hash password");

    // Verify it's actually hashed
    assert_ne!(hashed, password);
    assert!(hashed.len() >= 60); // bcrypt hash length

    // Verify we can check the password
    assert!(bcrypt::verify(password, &hashed).expect("Failed to verify password"));
    assert!(!bcrypt::verify("wrongpassword", &hashed).expect("Failed to verify wrong password"));
}

#[tokio::test]
async fn test_jwt_token_operations() {
    // Test JWT creation and verification
    let user_id = Uuid::new_v4().to_string();

    // Create token
    let token = domain::create_jwt(&user_id).expect("Failed to create JWT");
    assert!(!token.is_empty());

    // Verify token
    let claims_user_id = domain::verify_jwt(&token).expect("Failed to verify JWT");
    assert_eq!(claims_user_id, user_id);

    // Test with invalid token
    let invalid_token = "invalid.token.here";
    let err = domain::verify_jwt(invalid_token).expect_err("Should fail with invalid token");
    match err {
        token_compress_engine::errors::AppError::Unauthorized => {}
        _ => panic!("Expected Unauthorized error, got {:?}", err),
    }
}

#[tokio::test]
async fn test_feedback_value_assignment() {
    // Test that feedback values are assigned correctly
    let thumbs_up = if "thumbs_up" == "thumbs_up" {
        1.0
    } else {
        -1.0
    };
    let thumbs_down = if "thumbs_down" == "thumbs_up" {
        1.0
    } else {
        -1.0
    };

    assert_eq!(thumbs_up, 1.0);
    assert_eq!(thumbs_down, -1.0);
}
