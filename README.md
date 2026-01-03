# Luny

A multi-language CLI tool that generates AI-optimized `.toon` DOSE (Distilled, Optimized, Semantic Extract) files for codebases using [Token-Oriented Object Notation](https://toonformat.dev/).

## What Problem Does This Solve?

| Problem | Solution |
|---------|----------|
| AI must read entire files to understand them | DOSE provides semantic briefing |
| Tribal knowledge lives only in people's heads | DOSE captures invariants, gotchas, flows |
| AI makes reasoning errors without context | Semantic anchoring prevents common mistakes |
| Large files exceed context limits | DOSE is small (~200-400 tokens) |

## Research Foundation

Luny's approach is grounded in peer-reviewed research on LLM context handling:

- **[Lost in the Middle](https://arxiv.org/abs/2307.03172)** (Liu et al., 2023): Models perform best when relevant info is at the **beginning or end** of context (U-shaped curve). Luny uses "U-curve ordering" to place critical info first.

- **[Focused Chain-of-Thought](https://arxiv.org/abs/2511.22176)** (Struppek et al., 2025): Structured input formatting reduces generated tokens **2-3x** while maintaining accuracy. DOSE's structured format aligns with F-CoT principles.

- **[UID Hypothesis](https://arxiv.org/abs/2510.06953)** (Gwak et al., 2025): Uniform information density correlates with **10-32% accuracy gains** in reasoning. DOSE's consistent structure promotes uniform density.

- **[RULER](https://arxiv.org/abs/2404.06654)** (Hsieh et al., 2024): Models claiming 32K+ context often fail at actual 32K usage. DOSE keeps context small and focused.

- **[Meta Knowledge for RAG](https://arxiv.org/abs/2408.09017)** (Sukumaran et al., 2024): Preparing structured metadata before retrieval **significantly outperforms traditional document chunking** (p < 0.01). DOSE files are meta knowledge summaries for code.

- **[ClassEval](https://arxiv.org/abs/2308.01861)** (Du et al., 2023): LLMs struggle with class-level code due to "limited ability of understanding long instructions." Stripping context to **minimal semantic information** improves reasoning on complex code structures.


## Installation

### From source

```bash
git clone https://github.com/jordanrinke/luny
cd luny
make install
```

## Quick Start

```bash
# Generate .toon files for your project
luny generate src/

# Validate .toon files against source
luny validate

# Strip @dose comments when reading into LLM (saves tokens)
luny strip src/main.ts
```

## What Luny Generates vs What You Write

TOON DOSE files have two categories of fields:

### Structural Fields (Luny generates these)

Extracted automatically from AST analysis:

| Field | Description |
|-------|-------------|
| `tokens` | Approximate token count |
| `exports` | Public API (functions, classes, types) |
| `imports` | Dependencies |
| `calls` | Functions this file calls |
| `imported-by` | Files that import this one |
| `called-by` | Functions that call into this file |
| `signatures` | Type signatures for exports |

### Semantic Fields (You write these in @dose comments)

Human knowledge that can't be extracted from code:

| Field | Description | |
|-------|-------------|---|
| `purpose` | What this file does | Required |
| `when-editing` | Critical things to check when modifying | Recommended |
| `invariants` | Rules that must ALWAYS hold | Recommended |
| `do-not` | Forbidden patterns (security, bugs) | Recommended |
| `gotchas` | Non-obvious traps | Recommended |
| `error-handling` | How to handle each error type | Optional |
| `constraints` | Prerequisites and requirements | Optional |
| `flows` | Multi-step operation sequences | Optional |
| `testing` | How to test this code | Optional |
| `common-mistakes` | Historical bugs to avoid | Optional |
| `change-impacts` | Non-obvious blast radius | Optional |

## Writing @dose Comments

Add `@dose` blocks to your source files. Luny extracts the semantic content and combines it with AST data.

### TypeScript/JavaScript

```typescript
/** @dose
purpose: Auth context managing session state, token refresh, and platform storage.

when-editing:
    - !Always validate token expiry before checking permissions
    - Update User type if adding new claims

invariants:
    - Access tokens expire in 15 minutes
    - Only one refresh request in flight at a time
    - Web uses HTTP-only cookies; mobile uses SecureStore

do-not:
    - Never store tokens in localStorage (XSS vulnerability)
    - Never skip loading state check before accessing user

gotchas:
    - isWeb check uses typeof window - breaks in SSR
    - Don't destructure useAuth() at module level

error-handling:
    - 401: Attempt refresh first, then onUnauthorized if fails
    - Network timeout: Retry once, then surface error

flows:
    - login: oauth → callback → persistSession → authenticated
    - refresh: timer fires → check expiry → API call → update tokens
*/
export function AuthProvider({ children }: Props) {
  // ...
}

/** @dose invariant: caller must check isAuthenticated before calling */
export async function refreshToken(): Promise<void> {
  // ...
}

/** @dose gotcha: returns null during SSR, not undefined */
export function useAuth(): AuthContextValue | null {
  // ...
}
```

### Python

```python
"""@dose
purpose: Database connection pool with automatic reconnection and health checks.

when-editing:
    - !Connection limits are per-environment in config
    - Always use context managers for connections

invariants:
    - Pool size never exceeds MAX_CONNECTIONS
    - Unhealthy connections removed within 30 seconds

do-not:
    - Never create connections outside the pool
    - Never hold connections across async boundaries

gotchas:
    - close() is async - must be awaited
    - Health checks run on background thread
"""

class ConnectionPool:
    # ...

    # @dose invariant: always returns a valid connection or raises
    async def acquire(self) -> Connection:
        # ...

    # @dose gotcha: must be called even if acquire() raised
    async def release(self, conn: Connection) -> None:
        # ...
```

### Rust

```rust
//! @dose
//! purpose: Thread-safe LRU cache with TTL support.
//!
//! when-editing:
//!     - !Lock ordering: always acquire read before write
//!     - Eviction runs on background thread
//!
//! invariants:
//!     - Cache size never exceeds configured maximum
//!     - Expired entries are never returned
//!
//! do-not:
//!     - Never hold lock across await points
//!     - Never bypass TTL checks
//!
//! gotchas:
//!     - get() clones values - expensive for large types
//!     - clear() blocks until eviction completes

pub struct Cache<K, V> {
    // ...
}

impl<K, V> Cache<K, V> {
    /// @dose invariant: returns None for expired entries, never stale data
    pub fn get(&self, key: &K) -> Option<V> {
        // ...
    }

    /// @dose gotcha: overwrites existing entry without returning old value
    pub fn insert(&mut self, key: K, value: V, ttl: Duration) {
        // ...
    }
}
```

## Generated TOON Format

Luny generates `.toon` files in the `.ai/` directory:

```
src/auth/provider.ts  →  .ai/src/auth/provider.ts.toon
```

Example output (from the TypeScript example above):

```toon
purpose: Auth context managing session state, token refresh, and platform storage.
tokens: ~652
exports[6]: User(interface), AuthContextValue(interface), AuthContext(context), refreshToken(fn), useAuth(hook), AuthProvider(fn)
signatures[5]:
  User(interface): { id: string; email: string; roles: string[] }
  AuthContextValue(interface): { user: User | null; isAuthenticated: boolean; ... }
  refreshToken(fn): () : Promise<void>
  useAuth(hook): () : AuthContextValue | null
  AuthProvider(fn): ({ children }: { children: React.ReactNode })
when-editing: !Always validate token expiry before checking permissions; Update User type if adding new claims
invariants: Access tokens expire in 15 minutes; Only one refresh request in flight; Web uses HTTP-only cookies; mobile uses SecureStore
do-not: Never store tokens in localStorage (XSS vulnerability); Never skip loading state check before accessing user
imports[3]{from,items}: react,createContext|useContext|useState|useEffect; ./api-client,ApiClient; ./secure-store,SecureStore
calls[1]{target,methods}: react,createContext|useContext|useState|useEffect
error-handling: 401: Attempt refresh first, then onUnauthorized if fails; Network timeout: Retry once, then surface error
flows: login: oauth → callback → persistSession → authenticated; refresh: timer fires → check expiry → API call → update tokens
fn:refreshToken: invariants: caller must check isAuthenticated before calling
fn:useAuth: gotchas: returns null during SSR, not undefined
gotchas: isWeb check uses typeof window - breaks in SSR; Don't destructure useAuth() at module level
```

Note the `fn:` prefixed lines show per-function annotations from inline `@dose` comments.

## Commands

### `luny generate`

```bash
luny generate [PATH...]           # Generate for specific paths
luny generate                     # Generate for current directory
luny generate --dry-run           # Preview without writing
luny generate --force             # Regenerate existing files
luny generate --token-warn 500    # Warning threshold (default: 500)
luny generate --token-error 1000  # Error threshold (default: 1000)
```

### `luny validate`

```bash
luny validate [PATH...]    # Validate specific files
luny validate              # Validate all .toon files
luny validate --fix        # Regenerate invalid files
luny validate --strict     # Treat warnings as errors
```

### `luny strip`

Remove `@dose` comments when feeding source to an LLM. Since the LLM already has semantic context from the `.toon` file, the embedded comments are redundant—stripping them saves tokens.

```bash
luny strip <FILE>              # Output to stdout
luny strip <FILE> -o out.ts    # Output to file
luny strip - --ext ts          # Read from stdin
```

**LLM Integration**: Configure your AI tool to pipe files through `luny strip` when reading source files that have corresponding `.toon` DOSE files. This avoids duplicate context and reduces token usage.

## Supported Languages

| Language   | Extensions      | Comment Syntax |
|------------|-----------------|----------------|
| TypeScript | `.ts`, `.tsx`   | `/** @dose */` |
| JavaScript | `.js`, `.jsx`   | `/** @dose */` |
| Python     | `.py`           | `"""@dose"""` or `# @dose` |
| Ruby       | `.rb`           | `# @dose` |
| C#         | `.cs`           | `/** @dose */` or `/// @dose` |
| Go         | `.go`           | `/* @dose */` |
| Rust       | `.rs`           | `/*! @dose */` or `//! @dose` |

## Token Budgets

| File Complexity | Target | Max | Description |
|-----------------|--------|-----|-------------|
| Simple | 150 | 350 | Single export, ≤3 imports |
| Standard | 250 | 500 | Typical component/module |
| Complex | 400 | 800 | Many exports, flows + error handling |

## Development

```bash
make build      # Build debug version
make release    # Build optimized release
make test       # Run tests (276 tests)
make check      # Run fmt + clippy
make docs       # Generate rustdoc
make install    # Install to ~/.cargo/bin
```

## MSRV (Minimum Supported Rust Version)

Luny's MSRV is **Rust 1.85** (enforced via `rust-version = "1.85"` in `Cargo.toml` and checked in CI).

## References

- [TOON Format Specification](https://toonformat.dev/)
- [TOON GitHub Repository](https://github.com/toon-format/toon)

### Research Papers

- Liu et al. (2023). "[Lost in the Middle: How Language Models Use Long Contexts](https://arxiv.org/abs/2307.03172)" - TACL
- Hsieh et al. (2024). "[RULER: What's the Real Context Size of Your Long-Context Language Models?](https://arxiv.org/abs/2404.06654)" - COLM
- Struppek et al. (2025). "[Focused Chain-of-Thought](https://arxiv.org/abs/2511.22176)" - arXiv
- Gwak et al. (2025). "[Revisiting the Uniform Information Density Hypothesis](https://arxiv.org/abs/2510.06953)" - arXiv
- Sukumaran et al. (2024). "[Meta Knowledge for Retrieval Augmented Large Language Models](https://arxiv.org/abs/2408.09017)" - arXiv
- Du et al. (2023). "[ClassEval: A Manually-Crafted Benchmark for Evaluating LLMs on Class-level Code Generation](https://arxiv.org/abs/2308.01861)" - arXiv

## License

MIT License - see [LICENSE](LICENSE) for details.
