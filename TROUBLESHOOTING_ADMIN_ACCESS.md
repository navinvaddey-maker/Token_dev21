# Troubleshooting Guide: Admin Access Issue for User "Navin"

## Problem Statement
Navin has been assigned the admin role in the Token Compress Engine system but is unable to access or view the admin screen/dashboard.

## Possible Causes
This guide investigates the following potential causes:
1. Role-Based Access Control (RBAC) misconfigurations
2. Missing or incomplete permission assignments
3. Cached session or token issues
4. UI rendering errors
5. Feature flag settings
6. Backend authorization middleware failures

## Step-by-Step Troubleshooting Guide

### Step 1: Verify User Role Assignment in Database
**Check if Navin's user record has the correct `business_type` set to "Admin".**

**How to check:**
```sql
SELECT id, username, business_type, email 
FROM users 
WHERE username = 'navin';
```

**Expected result:**
- `business_type` should be `"Admin"`

**If incorrect:**
- Update the user's business_type:
```sql
UPDATE users 
SET business_type = 'Admin', 
    updated_at = CURRENT_TIMESTAMP 
WHERE username = 'navin';
```

### Step 2: Validate JWT Token and Claims
**Check if the JWT token being used contains the correct user ID and if the token is valid.**

**How to check:**
1. Obtain Navin's current JWT token (from browser dev tools or API client)
2. Decode the token (using jwt.io or similar) to inspect the `sub` (subject) claim
3. Verify the `sub` claim matches Navin's user ID from the database

**Common issues:**
- Token expired (default expiry is 7 days)
- Token malformed or tampered with
- Wrong user ID in token

**Solution:**
- Have Navin log out and log back in to get a fresh token
- Ensure the login endpoint is correctly setting the JWT with the user ID

### Step 3: Test Authorization Middleware Directly
**Test the `is_admin` function in the domain layer to verify it returns true for Navin's user ID.**

**How to check (via SQL):**
```sql
-- Replace 'navin-user-id-here' with the actual user ID from the database
SELECT business_type = 'Admin' AS is_admin 
FROM users 
WHERE id = 'navin-user-id-here';
```

**Expected result:** `is_admin` should be `true`

**If false:**
- Double-check the `business_type` value (case-sensitive: must be exactly "Admin")
- Check for whitespace or hidden characters

### Step 4: Verify Admin Middleware Execution
**Check if the admin middleware is correctly blocking or allowing requests.**

**How to check:**
1. Enable debug/trace logging for the middleware
2. Make an admin API request (e.g., GET `/api/admin/stats`) with Navin's token
3. Check server logs for:
   - `admin_middleware` execution
   - Result of `domain::is_admin` call
   - Any authorization errors

**Look for in logs:**
- `tracing::info!("Admin user 'navin' already exists.")` during startup (if user was seeded)
- `tracing::info!("Authorized admin access")` or similar in middleware
- `Err(AppError::Unauthorized)` indicating failed authorization

### Step 5: Test Admin Endpoints Directly
**Test if admin endpoints return 401/403 or 200 when accessed with Navin's token.**

**How to check:**
```bash
# Replace <token> with Navin's current JWT
curl -H "Authorization: Bearer <token>" \
  http://localhost:8081/api/admin/stats
```

**Expected response:**
- HTTP 200 OK with JSON data

**If getting 401 Unauthorized:**
- Token missing, expired, or invalid
- Auth middleware failing to extract/verify token

**If getting 403 Forbidden:**
- Token valid but admin middleware denying access (user not recognized as admin)

### Step 6: Check for Cached Sessions or Tokens
**Verify if there are any token caching mechanisms or stale sessions.**

**Possible sources:**
- Browser localStorage/sessionStorage holding old token
- API client caching tokens
- Reverse proxy or CDN caching responses

**Solution:**
- Clear browser storage for the application domain
- Use incognito/private browsing window to test
- Ensure API client is not caching authorization headers

### Step 7: Inspect UI/Routing Logic
**If the issue is specifically with the admin screen/dashboard not rendering (but API works):**

**How to check:**
1. Verify if admin API endpoints are returning data correctly (Step 5)
2. Check browser console for JavaScript errors
3. Verify frontend routing logic for admin routes
4. Check if conditional rendering based on user role is working correctly

**Common UI issues:**
- Frontend role check failing (parsing user role incorrectly)
- Admin route not protected properly in frontend router
- Dashboard component failing to render due to missing data

### Step 8: Verify No Feature Flags Blocking Access
**Check if any feature flags are disabling admin functionality.**

**How to check:**
- Look for feature flag configuration in environment variables or config files
- Check if any middleware or route conditionally disables admin features
- Verify no recent deployments toggled admin features off

### Step 9: Review Recent Changes and Deployments
**Check if any recent changes could have affected admin access.**

**How to check:**
- Review git history for changes to:
  - Authentication/authorization middleware
  - User model or business_type handling
  - Admin route definitions
  - Database schema related to users
- Check deployment logs for any migration errors

### Step 10: Test with Alternative Admin Account
**Determine if the issue is specific to Navin's account or affects all admins.**

**How to check:**
1. Create a temporary admin user via the registration endpoint (if business_type can be set)
   - Note: The current registration endpoint defaults to "Developer", so direct DB insert may be needed
2. Test admin access with the new user's credentials

**If new admin works:**
- Issue is specific to Navin's user record or session
- Focus on Navin's specific data and token history

**If new admin also fails:**
- System-wide admin access issue
- Focus on middleware, role checking logic, or database connectivity

## Recommended Solutions Based on Diagnosis

### If Role Assignment is Wrong:
```sql
-- Correct the business_type
UPDATE users 
SET business_type = 'Admin', 
    updated_at = CURRENT_TIMESTAMP 
WHERE username = 'navin';

-- Verify
SELECT business_type FROM users WHERE username = 'navin';
```

### If Token Issues:
- Instruct Navin to log out and log back in
- Consider reducing JWT expiry for faster token rotation during testing
- Ensure token is being sent correctly in Authorization header

### If Middleware/Authorization Bug:
- Check the `is_admin` function in `src/domain.rs`:
  ```rust
  pub async fn is_admin(pool: &DbPool, user_id: &str) -> Result<bool, AppError> {
      let user = Repository::find_user_by_id(pool, user_id).await
          .map_err(AppError::Database)?
          .ok_or(AppError::Unauthorized)?;
      Ok(user.business_type == "Admin") // Case-sensitive!
  }
  ```
- Ensure no typos in "Admin" string comparison
- Verify database connection is working in middleware

### If UI-Specific Issue:
- Fix frontend role checking logic
- Ensure admin routes are properly protected in frontend router
- Check API endpoint calls from dashboard component

### If Caching Issue:
- Clear all relevant caches (browser, CDN, application-level if any)
- Add cache-busting parameters to API calls during development
- Review any caching middleware in the Axum stack

## Prevention Measures
1. **Add automated tests** for admin authorization
2. **Implement health check endpoint** that includes permission verification
3. **Add audit logging** for admin access attempts (success and failure)
4. **Create role management API** for safely updating user roles
5. **Monitor admin access metrics** in the new monitoring endpoints

## Conclusion
By following this troubleshooting guide systematically, you should be able to identify and resolve the root cause of Navin's admin access issue. Start with database verification (Step 1) and progress through the steps in order, as later steps often depend on earlier ones being correct.

If the issue persists after completing all steps, consider:
- Checking for database connection issues in the middleware
- Verifying the deployed code matches the source
- Looking for environmental differences between dev/staging/prod