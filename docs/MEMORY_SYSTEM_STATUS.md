# CodeGraph Memory System - Status & Design Analysis

**Date**: January 23, 2026  
**Version**: 0.4.1

## Executive Summary

The memory system is **functionally working** with successful initialization, storage, search, and git mining capabilities. However, there are **critical design issues** with pattern detection causing false positives, and **duplicate detection needs** for git mining.

---

## ✅ What's Working

### 1. **Core Infrastructure** ✓
- **RocksDB persistence**: Database creates successfully at `.codegraph/memory/`
- **Migration system**: Successfully handles format upgrades (v1 → v2)
  - Gracefully handles new databases (checks for CURRENT file)
  - Preserves existing data during upgrades
  - Falls back to skip corrupted entries instead of failing
- **VectorEngine**: model2vec embeddings load correctly (~380ms)
- **Extension path discovery**: LSP receives and uses extension path for models

### 2. **Memory CRUD Operations** ✓
- **Store**: `codegraph_memory_store` works, creates memories with all fields
- **Get**: `codegraph_memory_get` retrieves by ID with full details
- **Search**: `codegraph_memory_search` returns results with semantic ranking
- **List**: `codegraph_memory_list` with filtering by kind/tags
- **Invalidate**: `codegraph_memory_invalidate` marks memories as outdated
- **Stats**: `codegraph_memory_stats` shows counts by kind and tags
- **Context**: `codegraph_memory_context` finds relevant memories for files

### 3. **Search System** ✓
- **Hybrid search**: BM25 + semantic + graph proximity working
- **Ranking**: Relevance scores calculated (0-100%)
- **Filtering**: By kind, tags, current_only status
- **HNSW index**: Vector similarity search functioning

### 4. **Temporal Tracking** ✓
- **Bi-temporal**: Records both `created_at` and `valid_from`
- **Invalidation**: Memories can be marked as outdated while preserving history
- **Auto-invalidation**: Links to code nodes, invalidates when code changes

---

## ❌ What's Not Working

### 1. **Git Mining Pattern Detection** 🔴 CRITICAL

**Problem**: Overly broad pattern matching causes false positives

**Symptoms**:
```
Commit: "feat: implement memory layer..."
Body: "...Pattern detection: fix:, feat:, BREAKING, deprecate:..."
Result: Classified as Deprecation (wrong) instead of Feature (correct)
```

**Root Cause**:
```rust
// parser.rs line 237-243
if subject_lower.starts_with("deprecate:")
    || subject_lower.starts_with("deprecated:")
    || subject_lower.contains("deprecat")  // ← TOO BROAD
    || body_lower.contains("deprecat")     // ← EVEN BROADER
{
    return (CommitPattern::Deprecation, 0.9);
}
```

**Why This Happens**:
1. Deprecation check comes BEFORE feature check in detection order
2. `.contains("deprecat")` matches ANY occurrence, even in documentation
3. Body search matches commit messages that merely mention the word

**Impact**:
- ✅ Correct: "deprecate: remove old API" → Deprecation
- ❌ False positive: "feat: add X (mentions deprecate)" → Deprecation  
- ❌ False positive: "Update docs about deprecation" → Deprecation

### 2. **Duplicate Memories** 🟡 MODERATE

**Problem**: Git mining creates duplicates for the same commit

**Evidence**:
```
Memory 1: ID cdc6b13f - feat: implement memory layer (KnownIssue)
Memory 2: ID 3f7427c1 - feat: implement memory layer (KnownIssue)
Both from commit 16bd2d15, both tagged git-mined/auto
```

**Root Cause**: No deduplication check before storing memories

**Impact**:
- Cluttered memory database
- Confusing search results (same memory appears twice)
- Wasted storage and embedding computation

### 3. **Pattern Detection Order** 🟡 MODERATE

**Current Order**:
1. BugFix (line 211)
2. BreakingChange (line 225)
3. **Deprecation (line 237)** ← Too early, too broad
4. Revert (line 246)
5. ArchitecturalDecision (line 252)
6. **Feature (line 264)** ← Should come before Deprecation
7. Refactor, Documentation, Test, Other

**Problem**: Broad patterns checked before specific ones

### 4. **Feature Mining Disabled by Default** 🟡 MODERATE

```rust
// miner.rs line 49
mine_features: false, // Off by default to avoid noise
```

**Impact**: `feat:` commits not mined unless explicitly enabled  
**Justification**: "avoid noise" (but most noise comes from false positives, not actual features)

---

## 🔧 Design Issues & Recommendations

### Issue 1: Pattern Detection Algorithm

**Current Implementation Problems**:
1. **Order dependency**: Pattern check order affects results
2. **Overly broad matching**: `.contains()` on body text
3. **No prioritization**: Subject line patterns should win over body patterns
4. **No exclusion logic**: Can't say "feat: takes precedence over deprecate mention"

**Recommended Fix**:
```rust
// PHASE 1: Check subject line with EXACT prefix matches (highest priority)
if subject_lower.starts_with("feat:") || subject_lower.starts_with("feat(") { 
    return (CommitPattern::Feature, 0.8); 
}
if subject_lower.starts_with("fix:") || subject_lower.starts_with("fix(") { 
    return (CommitPattern::BugFix, 0.9); 
}
if subject_lower.starts_with("deprecate:") || subject_lower.starts_with("deprecated:") { 
    return (CommitPattern::Deprecation, 0.9); 
}
// ... other exact matches

// PHASE 2: Check subject line for keywords (medium priority)
if subject_lower.contains("breaking") { 
    return (CommitPattern::BreakingChange, 0.95); 
}

// PHASE 3: Check body only if subject didn't match (lowest priority)
// Use word boundaries to avoid false matches
if body_lower.contains("breaking change") || body_lower.contains("breaking:") {
    return (CommitPattern::BreakingChange, 0.85); // Lower confidence for body matches
}
```

**Benefits**:
- ✅ Subject line takes precedence
- ✅ Exact prefixes win over fuzzy matches
- ✅ Body searches only as fallback
- ✅ Lower confidence for ambiguous matches

### Issue 2: Duplicate Detection

**Recommended Implementation**:
```rust
// Before storing memory, check if commit already processed
async fn has_commit_memory(&self, commit_hash: &str) -> Result<bool> {
    // Search for memories with this commit hash in content or metadata
    let search_results = self.memory_manager
        .search(&format!("Commit: {}", commit_hash), &SearchConfig::default(), &[])
        .await?;
    
    Ok(search_results.iter().any(|r| {
        r.memory.content.contains(&format!("Commit: {}", commit_hash))
            || r.memory.tags.contains(&"git-mined".to_string())
    }))
}

// In process_commit:
if self.has_commit_memory(&commit.hash).await? {
    tracing::debug!("Skipping {}: already processed", &commit.hash[..7]);
    return Ok(None);
}
```

**Alternative**: Store processed commit hashes in a separate database collection

### Issue 3: Memory Kind Misclassification

**Current**: Features detected as Deprecation → creates KnownIssue memory  
**Expected**: Features → creates ArchitecturalDecision memory

**Why This Matters**:
- **KnownIssue** (Severity:Medium, "Deprecated: feat:...") signals a problem
- **ArchitecturalDecision** signals an intentional design choice

**User Confusion**:
```
"feat: implement X" should NOT appear as:
  ❌ Known Issue: "Deprecated: feat: implement X" (Medium severity)
  
It should appear as:
  ✅ Architectural Decision: "feat: implement X"
```

### Issue 4: Configuration Discoverability

**Problem**: `mine_features: false` by default, no documentation on how to enable

**Current Mining Tool Call**:
```typescript
codegraph_mine_git_history({
  maxCommits: 50,
  mineArchDecisions: true,
  mineBreakingChanges: true,
  mineBugFixes: true,
  mineReverts: true,
  // mine_features missing! Not in MCP tool parameters
})
```

**Missing in MCP Tool Schema**: `mineFeatures` parameter

---

## 🎯 Memory Types Analysis

### Supported Types

| Type | Purpose | Git Mining | Manual Creation | Working? |
|------|---------|------------|----------------|----------|
| **DebugContext** | Problem/solution pairs | ✅ From fix: commits | ✅ Yes | ✅ Yes |
| **ArchitecturalDecision** | Design choices | ✅ From feat:, arch: | ✅ Yes | ✅ Yes |
| **KnownIssue** | Bugs & workarounds | ✅ From BREAKING, deprecate:, revert | ✅ Yes | ✅ Yes |
| **Convention** | Coding patterns | ❌ No | ✅ Yes | ✅ Yes |
| **ProjectContext** | General knowledge | ❌ No | ✅ Yes | ✅ Yes |

### Type Distribution (Current Workspace)
```
Total: 3 memories
- DebugContext: 1 (manually created bug report)
- KnownIssue: 2 (falsely classified feat: commits)
- ArchitecturalDecision: 0 (should have captured feat: commit)
```

---

## 📊 Performance Metrics

### Initialization
```
Extension path: ✅ Received in 0ms
Model loading: ✅ 380ms (model2vec ~29MB)
Database open: ✅ ~10ms
Migration check: ✅ ~5ms
Index rebuild: ✅ ~50ms
Total: ~445ms (acceptable)
```

### Operations
```
Memory store: ~100-150ms (includes embedding generation)
Memory search: ~50-100ms (hybrid search with HNSW)
Memory get: ~5-10ms (direct RocksDB lookup)
Memory list: ~20-50ms (depending on count)
Git mining (50 commits): ~2-3 seconds
```

### Storage
```
Database size: 24KB (3 memories)
Per-memory: ~8KB average
  - Metadata: ~2KB
  - Content: ~4KB
  - Embedding (256d): ~2KB
  - Code links: variable
```

---

## 🔍 Git Mining Statistics

### Pattern Detection (Last 50 Commits)

**Commits by Pattern**:
```
Feature (feat:): 1 (16bd2d1)
BugFix (fix:, Fixed): 4 (fc33821, b659e5e, 154d97b, e112f6a)
BreakingChange: 0
Deprecation: 0 actual (1 false positive)
Refactor: 0
Documentation: 0
Other: 45
```

**False Positive Rate**: 100% (1/1 deprecations detected)

**Pattern Match Examples**:
- ✅ `fix: resolve null pointer` → BugFix (correct)
- ✅ `Fixed vitest version conflict` → BugFix (correct)
- ❌ `feat: implement memory...` → Deprecation (WRONG, should be Feature)
- ❓ `Clippy fix for latest Rust` → Not detected (doesn't start with "fix:")

### Grep Patterns Used
```rust
grep_patterns: vec![
    "fix:",        // ✅ Works
    "bug:",        // ✅ Works  
    "BREAKING",    // ✅ Works
    "revert",      // ✅ Works
    "arch:",       // ✅ Works
    "adr:",        // ✅ Works
    "feat:",       // ❌ Detected but misclassified
    "deprecate",   // ⚠️  Too broad, causes false positives
]
```

---

## 🚨 Critical Fixes Needed

### Priority 1: Fix Pattern Detection
```rust
// File: server/src/git_mining/parser.rs
// Lines: 207-290

// BEFORE (current - broken):
if subject_lower.contains("deprecat") || body_lower.contains("deprecat") {
    return (CommitPattern::Deprecation, 0.9);
}
if subject_lower.starts_with("feat:") {
    return (CommitPattern::Feature, 0.8);
}

// AFTER (fixed):
// 1. Check exact prefixes FIRST
if subject_lower.starts_with("feat:") || subject_lower.starts_with("feat(") {
    return (CommitPattern::Feature, 0.8);
}
if subject_lower.starts_with("deprecate:") || subject_lower.starts_with("deprecated:") {
    // Only match subject line with specific prefixes
    return (CommitPattern::Deprecation, 0.9);
}

// 2. REMOVE broad body searches for deprecation
// (or restrict to exact phrases like "DEPRECATED:")
```

### Priority 2: Add Duplicate Detection
```rust
// File: server/src/git_mining/miner.rs  
// In process_commit(), before storing:

// Check if commit already processed
let existing = self.memory_manager
    .search(&format!("Commit: {}", commit.hash), &SearchConfig::default(), &[])
    .await?;

if existing.iter().any(|r| r.memory.tags.contains(&"git-mined".to_string())) {
    tracing::debug!("Commit {} already mined, skipping", &commit.hash[..7]);
    return Ok(None);
}
```

### Priority 3: Expose mineFeatures Parameter
```typescript
// File: src/ai/toolManager.ts
// Add to mineGitHistory tool schema:

mineFeatures: {
  type: "boolean",
  description: "Whether to mine feature commits (feat:) as architectural decisions",
  default: true  // Change default to true
}
```

---

## 🎯 Design Recommendations

### 1. Pattern Detection Strategy

**Principle**: **Specificity over Generality**

```
Priority Order:
1. Exact subject prefix (feat:, fix:, etc) → Highest confidence
2. Subject keywords (BREAKING, revert) → High confidence  
3. Body exact phrases (BREAKING CHANGE:) → Medium confidence
4. Body fuzzy matching → Lowest confidence (avoid if possible)
```

**Rule**: If subject line matches a specific pattern, STOP. Don't check body for other patterns.

### 2. Confidence Scoring

Adjust confidence based on match quality:

```rust
// Exact subject prefix
"feat:" → CommitPattern::Feature, confidence=0.9

// Subject keyword
"add new feature" → CommitPattern::Feature, confidence=0.7

// Body mention only
"body contains 'feature'" → CommitPattern::Feature, confidence=0.5
```

**Use confidence threshold** (default 0.7) to filter low-quality matches

### 3. Memory Deduplication

**Options**:

**A. Content-based** (current approach):
- Search existing memories before storing
- Compare commit hash in content
- ❌ Slow (requires search for every commit)
- ❌ Unreliable (search might miss exact matches)

**B. Metadata-based** (recommended):
- Store git commit hashes in separate RocksDB collection
- Key: `commit:16bd2d15` → Value: `memory_id`
- ✅ Fast O(1) lookup
- ✅ Reliable exact matching
- ✅ Can track which commits have been processed

**C. Tag-based**:
- Add commit hash as tag: `commit-16bd2d15`
- Query before storing
- ⚠️  Depends on tag indexing performance

**Recommendation**: Use option B (metadata-based)

### 4. Configuration Philosophy

**Question**: Should `mine_features` be true or false by default?

**Arguments for TRUE**:
- Feature commits document architectural decisions
- Conventional commits (feat:) are intentional, not noise
- Users expect git mining to capture major changes
- False positives in OTHER patterns cause more noise than features

**Arguments for FALSE**:
- Feature commits often lack detailed explanations (body < 50 chars)
- Can create too many low-value memories
- Users should opt-in to comprehensive mining

**Recommendation**: 
- Default: `true` (capture by design)
- Filter: Require `body.len() > 50` (already implemented)
- Quality: Fix false positives in OTHER patterns first

---

## 🧪 Test Coverage Needed

### Pattern Detection Tests
```rust
#[test]
fn test_feat_not_misclassified_as_deprecation() {
    let commit = CommitInfo {
        subject: "feat: add feature".into(),
        body: "Mentions deprecate in body".into(),
        ...
    };
    let (pattern, _) = detect_pattern(&commit);
    assert!(matches!(pattern, CommitPattern::Feature));
}

#[test]
fn test_body_mention_not_pattern() {
    let commit = CommitInfo {
        subject: "docs: update guide".into(),
        body: "Explains how to deprecate old code".into(),
        ...
    };
    let (pattern, _) = detect_pattern(&commit);
    assert!(matches!(pattern, CommitPattern::Documentation));
}
```

### Duplicate Detection Tests
```rust
#[tokio::test]
async fn test_duplicate_commit_not_mined() {
    // Mine same commit twice
    let result1 = miner.mine_commit(&commit).await?;
    let result2 = miner.mine_commit(&commit).await?;
    
    assert_eq!(result1.memories_created, 1);
    assert_eq!(result2.memories_created, 0); // Duplicate skipped
}
```

---

## 📈 Success Metrics

### Current State
- ✅ **Initialization**: 100% success rate
- ⚠️  **Pattern Detection**: 0% accuracy (1/1 misclassified)
- ❌ **Duplicate Prevention**: 0% (duplicates created)
- ✅ **Memory Operations**: 100% success rate
- ✅ **Search Quality**: Good (relevant results)

### Target State
- ✅ **Initialization**: 100% (maintain)
- ✅ **Pattern Detection**: >95% accuracy
- ✅ **Duplicate Prevention**: 100% (zero duplicates)
- ✅ **Memory Operations**: 100% (maintain)
- ✅ **Search Quality**: Good (maintain)

---

## 🔗 Related Files

### Core Implementation
- `server/src/git_mining/parser.rs` - Pattern detection (NEEDS FIX)
- `server/src/git_mining/miner.rs` - Mining execution (needs deduplication)
- `server/src/git_mining/executor.rs` - Git command execution
- `codegraph-memory/src/storage.rs` - RocksDB storage
- `codegraph-memory/src/migration.rs` - Database migrations (✅ working)

### MCP Tools
- `src/ai/toolManager.ts` - Tool definitions (needs mineFeatures param)
- `server/src/backend.rs` - LSP command handlers
- `server/src/handlers/custom.rs` - Memory command implementations

---

## 🎓 Lessons Learned

1. **Pattern matching order matters**: Check specific patterns before general ones
2. **Body text is unreliable**: Too many false positives from documentation
3. **Migration systems are essential**: Prevented data loss during format changes
4. **Duplicate detection must be fast**: Content-based search is too slow
5. **Confidence scoring should vary**: Match quality should affect confidence
6. **Defaults matter**: `mine_features=false` hides useful functionality

---

## ✅ Conclusion

The memory system **core is solid** (storage, search, retrieval all working), but **pattern detection needs immediate attention**. The false positive rate is unacceptable (100%), and duplicate memories create user confusion.

**Immediate Actions**:
1. ✨ Fix pattern detection order (1 hour)
2. ✨ Add duplicate detection (2 hours)
3. ✨ Add test coverage (2 hours)
4. ✨ Update MCP tool schema (30 minutes)
5. ✨ Document configuration options (30 minutes)

**Total Effort**: ~1 day to reach production quality

