use bcrypt::verify;

#[test]
fn test_crack() {
    let hash = "$2b$12$Qvu.Qlcv1KcVPFdj61XRPOCZ4TwHl1QxWzW386qF2IZNl/v4BQ3Xe";
    let passwords = ["password", "admin", "navin", "navin123", "password123", "admin123", "password@123", "mysecretpassword123", "admin_password", "Admin123!", "Admin@123", "TokenCompress", "TokenCompress123"];
    for p in passwords.iter() {
        if verify(p, hash).unwrap_or(false) {
            println!("FOUND PASSWORD: {}", p);
            std::fs::write("FOUND_PASSWORD.txt", p).unwrap();
        }
    }
}
