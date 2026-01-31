# @memoryx/codegraph-mcp

CodeGraph MCP server - Cross-language code intelligence for AI assistants.

## Installation

```bash
npm install -g @memoryx/codegraph-mcp
```

## Configuration

### Claude Desktop

Add to `~/.claude.json`:

```json
{
  "mcpServers": {
    "codegraph": {
      "command": "codegraph-mcp"
    }
  }
}
```

### Cursor

Add to your settings:

```json
{
  "mcp.servers": {
    "codegraph": {
      "command": "codegraph-mcp"
    }
  }
}
```

### Claude Code CLI

Add to `~/.claude.json`:

```json
{
  "mcpServers": {
    "codegraph": {
      "command": "codegraph-mcp"
    }
  }
}
```

## Available Tools

### Code Analysis
- `codegraph_symbol_search` - Search for symbols by name or pattern
- `codegraph_get_dependency_graph` - Analyze file import/dependency relationships
- `codegraph_get_call_graph` - Map function call relationships
- `codegraph_analyze_impact` - Predict blast radius of code changes
- `codegraph_analyze_complexity` - Measure code complexity metrics
- `codegraph_analyze_coupling` - Measure module coupling
- `codegraph_find_unused_code` - Detect dead code

### Code Navigation
- `codegraph_get_callers` - Find all functions that call a target
- `codegraph_get_callees` - Find all functions called by a target
- `codegraph_get_symbol_info` - Get quick metadata about any symbol
- `codegraph_get_detailed_symbol` - Get comprehensive symbol details
- `codegraph_get_ai_context` - Gather code context optimized for AI

### Code Search
- `codegraph_find_by_imports` - Find files importing a module
- `codegraph_find_entry_points` - Discover application entry points
- `codegraph_find_by_signature` - Find functions by signature patterns
- `codegraph_find_related_tests` - Discover tests for specific code
- `codegraph_traverse_graph` - Advanced graph traversal

### Memory System
- `codegraph_memory_store` - Store debugging insights and decisions
- `codegraph_memory_search` - Search stored memories
- `codegraph_memory_get` - Retrieve memory by ID
- `codegraph_memory_context` - Find memories for current code
- `codegraph_memory_list` - List all memories
- `codegraph_memory_invalidate` - Mark memory as outdated
- `codegraph_memory_stats` - Get memory statistics
- `codegraph_mine_git_history` - Create memories from git history
- `codegraph_mine_git_file` - Mine history for specific file

## Supported Languages

- TypeScript/JavaScript
- Python
- Rust
- Go
- Java
- C/C++
- And more via Tree-sitter

## Platform Support

- macOS (arm64, x64)
- Linux (x64)
- Windows (x64)

## License

Apache-2.0
