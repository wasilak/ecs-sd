---
title: Concerns & Technical Debt
created: 2026-05-19
codebase: ecs-sd
---

# Concerns & Technical Debt

## Security Issues

### 🔴 HIGH: Environment Variable Exposure
**Location:** `src/main.rs:89-95`

The application prints **all environment variable values** to stdout:

```rust
for env_var in container_def.environment() {
    println!(
        "        {}: {}",
        env_var.name().unwrap(),
        env_var.value().unwrap()  // <-- Exposes secrets!
    );
}
```

**Risk:** Container environment variables often contain:
- Database passwords
- API keys
- Private tokens
- Service credentials

**Impact:** Running this tool could expose sensitive credentials in logs or terminal history.

**Recommendation:**
- Filter or mask sensitive variables
- Add `--show-secrets` flag (opt-in)
- Document security implications
- Redact values by default: `println!("        {}: [REDACTED]", env_var.name().unwrap());`

### 🟡 MEDIUM: Hardcoded AWS Account Information
**Location:** `src/main.rs:16-17`

```rust
"arn:aws:ecs:eu-west-1:723255075185:cluster/service-platform-default".to_string(),
```

**Risk:** AWS account ID and region are hardcoded.

**Impact:**
- Limits reusability
- Exposes internal infrastructure details
- Cannot be used across environments without code changes

**Recommendation:**
- Accept cluster ARNs via CLI arguments
- Support configuration file
- Use environment variables for defaults

## Error Handling Issues

### 🟡 MEDIUM: Unwrap Usage in Production Code
**Location:** `src/main.rs:56, 64, 79, 82-83, 86-94`

Multiple `.unwrap()` calls that could panic:
```rust
.send()
.await
.unwrap();  // Will panic on any error
```

**Risk:** Application crashes on:
- Network failures
- AWS API errors
- Permission denials
- Invalid cluster/task ARNs

**Recommendation:**
- Replace with proper error handling (`?` or `match`)
- Add context to errors
- Implement retry logic for transient failures

## Code Quality

### 🟡 MEDIUM: No Input Validation
- No validation of cluster ARNs
- No validation of AWS responses
- No sanitization of output

### 🟢 LOW: No Logging
- Uses `println!` for all output
- No structured logging
- No log levels (debug, info, error)

**Recommendation:**
- Add `tracing` or `log` crate
- Use structured logging for machine parsing
- Add verbosity flags

### 🟢 LOW: No Tests
See `TESTING.md` - complete absence of test coverage.

## Architecture Concerns

### 🟡 MEDIUM: Hardcoded Configuration
Multiple hardcoded values:
- Cluster list (lines 15-18)
- Default region (`us-east-1`)
- No configuration file support
- No CLI arguments

**Impact:** Requires code changes for different environments.

### 🟢 LOW: Single Responsibility Violation
`list_tasks_in_cluster()` does too much:
- Iterates clusters
- Lists tasks
- Describes tasks
- Describes task definitions
- Prints output

**Recommendation:** Consider splitting into smaller functions.

### 🟢 LOW: No Parallelism
Despite using Tokio with full features, all operations are sequential.

**Opportunity:** Could process multiple clusters or tasks in parallel for better performance.

## Maintenance Concerns

### 🟢 LOW: No Documentation
- No README.md
- No inline documentation (doc comments)
- No usage examples
- No contribution guidelines

### 🟢 LOW: Dependency Version Locking
Uses exact version pinning in `Cargo.lock` but loose constraints in `Cargo.toml`.

## Recommendations Summary

### Immediate (Before Production Use)
1. **Fix security issue** - Don't print environment variable values
2. **Add error handling** - Remove unwrap calls
3. **Add configuration** - CLI args or config file

### Short Term
4. Add unit and integration tests
5. Add logging framework
6. Input validation

### Long Term
7. Add README and documentation
8. Set up CI/CD
9. Consider parallel processing
10. Extract hardcoded values to configuration

---

*Document generated: 2026-05-19*
