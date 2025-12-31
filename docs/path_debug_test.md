# Path Debug Test Procedure

## Purpose
Debug the path mismatch issue where getDependencyGraph can't find files after workspace reindexing until they're reopened.

## Test Steps

1. **Restart VS Code Extension**
   - Reload the VS Code window to start fresh
   - This ensures we're testing with the newly built binary

2. **Execute Reindex Workspace**
   - Run the "CodeGraph: Reindex Workspace" command
   - Check the log for messages showing:
     - Total files indexed
     - Sample "Batch indexing file with path: '...'" messages
     - Sample "Stored path in graph: '...'" messages

3. **Test Dependency Graph WITHOUT Reopening File**
   - Open any TypeScript file from the indexed workspace
   - Run "CodeGraph: Get Dependency Graph" command
   - Check the log for:
     - "getDependencyGraph: Searching for path: '...'"
     - "getDependencyGraph: Found N file nodes"
     - If 0 nodes found, check the sample paths listed

4. **Test Dependency Graph AFTER Reopening File**
   - Close the file tab
   - Reopen the same file
   - Check the log for:
     - "did_open: parsing file with path: '...'"
     - "did_open: Stored path in graph: '...'"
   - Run "CodeGraph: Get Dependency Graph" again
   - Check if it works now

## Expected Findings

The logs should reveal:
1. Whether path formats differ between batch indexing and did_open
2. Whether path formats differ between what's stored and what's queried
3. Any path normalization issues (e.g., `/Users/...` vs `file:///Users/...`)

## Next Steps

Based on the log output, we'll:
1. Identify the exact path format mismatch
2. Implement path normalization to ensure consistency
3. Test again to verify the fix
