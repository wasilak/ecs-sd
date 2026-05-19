---
title: Project Structure
created: 2026-05-19
codebase: ecs-sd
---

# Project Structure

## Directory Layout

```
ecs-sd/
├── .git/                    # Git repository
├── .gitignore              # Git ignore rules
├── .serena/                # Serena project configuration
│   ├── .gitignore
│   ├── project.local.yml
│   └── project.yml
├── .planning/              # GSD planning documents (created)
│   └── codebase/           # Codebase mapping documents
├── Cargo.lock              # Dependency lockfile
├── Cargo.toml              # Package manifest
├── src/                    # Source code
│   └── main.rs             # Main application entry
└── target/                 # Build artifacts (ignored)
    └── debug/              # Debug build outputs
```

## Key Locations

### Source Code
| Path | Description |
|------|-------------|
| `src/main.rs` | Single-file application with all logic (100 lines) |

### Configuration
| Path | Description |
|------|-------------|
| `Cargo.toml` | Rust package definition and dependencies |
| `Cargo.lock` | Locked dependency versions |
| `.gitignore` | Excludes `/target` and `.serena` |

### Project Metadata
| Path | Description |
|------|-------------|
| `.serena/project.yml` | Serena project configuration |
| `.serena/project.local.yml` | Local Serena settings |

## File Organization

### Single-File Architecture
This project uses a **single-file approach** with all code in `src/main.rs`:

```
src/main.rs
├── imports (aws-sdk, tokio)
├── main() - async entry point
├── show_clusters() - cluster retrieval
└── list_tasks_in_cluster() - task enumeration
```

## Naming Conventions

### Rust Files
- `main.rs` - Standard Rust entry point
- Uses `snake_case` for function names
- Uses `PascalCase` for types (when applicable)

### Functions
- `main` - Entry point (standard)
- `show_clusters` - Verb + noun pattern
- `list_tasks_in_cluster` - Verb + noun + context pattern

## Build Outputs
- **Target directory:** `target/`
- **Debug builds:** `target/debug/`
- **Build cache:** Excluded from git via `.gitignore`

## Current Scope
This is a **minimal, focused project**:
- 1 source file (`src/main.rs`)
- 3 functions
- ~100 lines of code
- No modules or subdirectories
- No test files
- No documentation beyond inline comments

---

*Document generated: 2026-05-19*
