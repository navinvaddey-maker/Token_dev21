---
trigger: glob
globs: "static/**/*.{js,html,css},tests/verify_frontend_ux.js"
---

# Frontend Studio Standards

Adhere to these standards when developing or modifying frontend assets, templates, and UI tests:

1. **`Oryx*` Helper Prefix**: New JS helper classes must use the `Oryx*` prefix (e.g. `OryxDomUtils`).
2. **Strict CSP & Sanitization**: Strict CSP `script-src 'self' https://cdn.jsdelivr.net` (zero `'unsafe-inline'` in `script-src`). Zero inline `on*=` attributes in HTML or JS template strings — always use `addEventListener` / event delegation.
3. **Session & Error Handling**: `HttpOnly` `tce_session` cookie auth + rethrowing API errors when signed in (never silently falling back to `MockAPI` for authenticated users).
4. **Score Scale**: `0–10` score scale across live API and `MockAPI`.
5. **Test Artifact Integrity**: Keep `static/prompt_optimizer.js` & `static/dev_engine.js` intact for `tests/verify_frontend_ux.js`.
