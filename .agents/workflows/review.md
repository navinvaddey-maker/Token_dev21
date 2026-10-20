# /review

1. cargo fmt --all -- --check
2. cargo clippy --workspace --all-targets --all-features -- -D warnings
3. cargo test --workspace
4. cargo deny check
5. gitleaks detect --no-banner
6. semgrep scan --config p/rust --config p/owasp-top-ten
7. If migrations changed and squawk is installed: squawk migrations/*.sql
8. Group all findings by severity (Critical, High, Medium, Low).
9. Present a fix plan and wait for approval before editing.
10. Apply approved fixes, re-run steps 1-7, and report pass or fail per gate.
