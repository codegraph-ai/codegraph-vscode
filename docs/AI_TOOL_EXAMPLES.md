# CodeGraph AI Tool Examples

This document provides detailed examples of how AI agents can use CodeGraph tools effectively. These tools are registered via VS Code's Language Model Tools API and are available to any AI assistant (GitHub Copilot, Claude, etc.) operating in VS Code.

---

## Why Use CodeGraph Tools?

CodeGraph tools provide **semantic understanding** rather than text matching. This leads to:

- **75-80% fewer tool calls** compared to grep/read operations
- **75-78% reduction in tokens** consumed
- **Higher accuracy** — no false positives from text matching
- **Intent-aware context** — get relevant code based on what you're trying to do

---

## Tool Overview (17 Tools)

### Core Analysis Tools (9)
| Tool | Purpose |
|------|---------|
| `codegraph_get_dependency_graph` | Understand file/module dependencies |
| `codegraph_get_call_graph` | Trace function call relationships |
| `codegraph_analyze_impact` | Assess change impact before modifying code |
| `codegraph_get_ai_context` | Get intent-aware code context |
| `codegraph_find_related_tests` | Find tests covering code |
| `codegraph_get_symbol_info` | Get symbol metadata and documentation |
| `codegraph_analyze_complexity` | Measure code complexity metrics |
| `codegraph_find_unused_code` | Detect dead code |
| `codegraph_analyze_coupling` | Analyze module coupling/cohesion |

### AI Agent Query Tools (8)
| Tool | Purpose |
|------|---------|
| `codegraph_symbol_search` | Fast text-based symbol search with BM25 ranking |
| `codegraph_find_by_imports` | Find code by imported libraries |
| `codegraph_find_entry_points` | Discover HTTP handlers, CLI commands, etc. |
| `codegraph_traverse_graph` | Custom graph traversal with filters |
| `codegraph_get_callers` | Find all callers of a function |
| `codegraph_get_callees` | Find all functions called by a function |
| `codegraph_get_detailed_symbol` | Rich metadata for any symbol |
| `codegraph_find_by_signature` | Find functions by signature pattern |

---

## Tool Reference

### 1. `codegraph_get_dependency_graph`

**Purpose:** Understand what files/modules a piece of code depends on, or what depends on it.

**When to use:**
- Understanding module architecture
- Planning refactoring scope
- Finding circular dependencies
- Analyzing import chains

**Parameters:**
```json
{
  "uri": "file:///path/to/file.ts",    // Required: file to analyze
  "depth": 3,                           // Optional: traversal depth (1-10)
  "includeExternal": false,             // Optional: include node_modules
  "direction": "both"                   // Optional: "imports", "importedBy", or "both"
}
```

**Example — "What does UserService depend on?"**

```
Tool: codegraph_get_dependency_graph
Input: {
  "uri": "file:///project/src/services/UserService.ts",
  "depth": 2,
  "direction": "imports"
}
```

**Output:**
```markdown
# Dependency Graph

Found 8 files/modules with 12 dependencies.

## Dependencies (12)
- UserService.ts → DatabaseClient.ts (import)
- UserService.ts → Logger.ts (import)
- UserService.ts → UserRepository.ts (import)
- UserService.ts → ValidationUtils.ts (import)
- UserRepository.ts → DatabaseClient.ts (import)
- UserRepository.ts → UserModel.ts (import)
...

## Files/Modules
- **UserService.ts** (module, typescript)
- **DatabaseClient.ts** (module, typescript)
- **Logger.ts** (module, typescript)
...
```

**Comparison — Traditional Approach:**
```
Without CodeGraph:
1. read_file UserService.ts (top 50 lines for imports)
2. grep_search for each import to find file location
3. read_file each dependency
4. Repeat for transitive dependencies

= 8-12 tool calls, ~6,000-8,000 tokens

With CodeGraph:
1. codegraph_get_dependency_graph

= 1 tool call, ~500 tokens
```

---

### 2. `codegraph_get_call_graph`

**Purpose:** Understand function call relationships — what calls a function, and what it calls.

**When to use:**
- Tracing execution flow
- Understanding function usage patterns
- Planning function signature changes
- Debugging call chains

**Parameters:**
```json
{
  "uri": "file:///path/to/file.ts",    // Required: file containing function
  "line": 45,                           // Required: line number (0-indexed)
  "character": 0,                       // Optional: character position
  "depth": 3,                           // Optional: traversal depth
  "direction": "both"                   // Optional: "callers", "callees", or "both"
}
```

**Example — "What functions call processPayment?"**

```
Tool: codegraph_get_call_graph
Input: {
  "uri": "file:///project/src/payments/PaymentProcessor.ts",
  "line": 120,
  "direction": "callers",
  "depth": 2
}
```

**Output:**
```markdown
# Call Graph

Found 5 functions with 7 call relationships.

## Target Function
**processPayment** (async processPayment(order: Order, method: PaymentMethod): Promise<Receipt>)
Location: file:///project/src/payments/PaymentProcessor.ts

## Callers (3)
Functions that call this:
- **checkout** at src/checkout/CheckoutService.ts
- **retryPayment** at src/orders/OrderService.ts
- **processRefund** at src/payments/RefundHandler.ts
```

**Comparison — Traditional Approach:**
```
Without CodeGraph:
1. grep_search for "processPayment(" across codebase
2. Filter out definition, keep only usages
3. read_file surrounding context for each match
4. Manually trace call chains

= 5-7 tool calls, ~5,000-7,000 tokens

With CodeGraph:
1. codegraph_get_call_graph

= 1 tool call, ~400-600 tokens
```

---

### 3. `codegraph_analyze_impact`

**Purpose:** Before making changes, understand what will break. Shows direct impacts, indirect impacts, and affected tests.

**When to use:**
- Before refactoring
- Before deleting code
- Before renaming symbols
- Assessing change risk

**Parameters:**
```json
{
  "uri": "file:///path/to/file.ts",    // Required: file containing symbol
  "line": 45,                           // Required: line number (0-indexed)
  "character": 0,                       // Optional: character position
  "changeType": "modify"                // Optional: "modify", "delete", or "rename"
}
```

**Example — "What breaks if I delete validateInput?"**

```
Tool: codegraph_analyze_impact
Input: {
  "uri": "file:///project/src/utils/validation.ts",
  "line": 23,
  "changeType": "delete"
}
```

**Output:**
```markdown
# Impact Analysis

## Summary
- Files Affected: 4
- Breaking Changes: 6
- Warnings: 2

## Direct Impact (6)
Immediate usages that will be affected:
🔴 BREAKING: **reference** at UserService.ts:45
🔴 BREAKING: **reference** at OrderService.ts:89
🔴 BREAKING: **reference** at PaymentProcessor.ts:34
🔴 BREAKING: **reference** at CheckoutController.ts:67
🔴 BREAKING: **reference** at ApiHandler.ts:123
🔴 BREAKING: **reference** at FormValidator.ts:12

## Indirect Impact (2)
Transitive dependencies that will be affected:
🟡 CheckoutFlow.ts
  Dependency path: CheckoutFlow → CheckoutController → validateInput
🟡 OrderWorkflow.ts
  Dependency path: OrderWorkflow → OrderService → validateInput

## Affected Tests (3)
Tests that may need updating:
🧪 **validateInput.test.ts** at tests/utils/validateInput.test.ts
🧪 **UserService.test.ts** at tests/services/UserService.test.ts
🧪 **integration.test.ts** at tests/integration/integration.test.ts
```

**Comparison — Traditional Approach:**
```
Without CodeGraph:
1. grep_search for "validateInput" usage
2. read_file surrounding context for each match
3. Manually identify which are breaking vs warnings
4. grep_search in test files
5. Manually trace indirect impacts

= 5-8 tool calls, ~4,000-5,000 tokens, manual analysis required

With CodeGraph:
1. codegraph_analyze_impact

= 1 tool call, ~500-1,200 tokens, automatic categorization
```

---

### 4. `codegraph_get_ai_context`

**Purpose:** Get comprehensive code context optimized for AI analysis. Automatically selects relevant related code based on your intent.

**When to use:**
- Understanding unfamiliar code
- Before modifying code
- Debugging issues
- Writing tests

**Parameters:**
```json
{
  "uri": "file:///path/to/file.ts",    // Required: file to analyze
  "line": 45,                           // Required: line number (0-indexed)
  "character": 0,                       // Optional: character position
  "intent": "explain",                  // Optional: "explain", "modify", "debug", "test"
  "maxTokens": 4000                     // Optional: token budget
}
```

**Intent-Aware Context Selection:**

| Intent | Prioritizes |
|--------|-------------|
| `explain` | Type definitions, interfaces, documentation, usage examples |
| `modify` | Callers, dependents, contracts that must be maintained |
| `debug` | Call chain, data flow, error handling, initialization |
| `test` | Related tests, mocks, test utilities, coverage |

**Example — "Help me understand the AuthMiddleware"**

```
Tool: codegraph_get_ai_context
Input: {
  "uri": "file:///project/src/middleware/AuthMiddleware.ts",
  "line": 15,
  "intent": "explain",
  "maxTokens": 4000
}
```

**Output:**
```markdown
# Code Context

## Primary Code
**class: AuthMiddleware**
Language: typescript
Location: file:///project/src/middleware/AuthMiddleware.ts

```typescript
export class AuthMiddleware {
  constructor(
    private tokenService: TokenService,
    private userRepository: UserRepository
  ) {}

  async authenticate(req: Request, res: Response, next: NextFunction) {
    const token = this.extractToken(req);
    if (!token) {
      return res.status(401).json({ error: 'No token provided' });
    }

    try {
      const decoded = await this.tokenService.verify(token);
      req.user = await this.userRepository.findById(decoded.userId);
      next();
    } catch (error) {
      return res.status(401).json({ error: 'Invalid token' });
    }
  }
  // ...
}
```

## Related Code (3)

### 1. type_definition (relevance: 95%)
**TokenService**
```typescript
interface TokenService {
  verify(token: string): Promise<TokenPayload>;
  sign(payload: TokenPayload): string;
}
```

### 2. dependency (relevance: 88%)
**UserRepository**
```typescript
class UserRepository {
  async findById(id: string): Promise<User | null>;
  // ...
}
```

### 3. usage (relevance: 75%)
**app.ts**
```typescript
app.use('/api', authMiddleware.authenticate);
```

## Architecture Context
- Module: middleware
- Neighbors: TokenService, UserRepository, Express
```

**Comparison — Traditional Approach:**
```
Without CodeGraph:
1. read_file AuthMiddleware.ts (full file)
2. grep_search for TokenService definition
3. read_file TokenService.ts
4. grep_search for UserRepository
5. read_file UserRepository.ts
6. grep_search for middleware usage
7. read_file app.ts

= 7+ tool calls, ~8,000-10,000 tokens, manual context assembly

With CodeGraph:
1. codegraph_get_ai_context (intent: explain)

= 1 tool call, ~3,000-4,000 tokens, pre-assembled context
```

---

### 5. `codegraph_find_related_tests`

**Purpose:** Discover tests that cover a piece of code. Essential for understanding test coverage and identifying tests that need updating.

**When to use:**
- Before modifying code (what tests to run?)
- After modifying code (what tests to update?)
- Assessing test coverage
- Finding test examples

**Parameters:**
```json
{
  "uri": "file:///path/to/file.ts",    // Required: file to find tests for
  "line": 0                             // Optional: specific line (0-indexed)
}
```

**Example — "Find tests for PaymentProcessor"**

```
Tool: codegraph_find_related_tests
Input: {
  "uri": "file:///project/src/payments/PaymentProcessor.ts",
  "line": 0
}
```

**Output:**
```markdown
# Related Tests

Found 3 related test(s):

## 1. PaymentProcessor.test.ts
Relationship: direct_test
Relevance: 98%
```typescript
describe('PaymentProcessor', () => {
  it('should process valid payment', async () => {
    const processor = new PaymentProcessor(mockGateway);
    const result = await processor.processPayment(validOrder, 'card');
    expect(result.success).toBe(true);
  });
  // ...
});
```

## 2. integration/checkout.test.ts
Relationship: integration_test
Relevance: 72%
```typescript
describe('Checkout Flow', () => {
  it('should complete checkout with payment', async () => {
    // Uses PaymentProcessor internally
    const result = await checkout.complete(cart, paymentMethod);
    // ...
  });
});
```

## 3. e2e/payment.e2e.ts
Relationship: e2e_test
Relevance: 65%
```typescript
test('user can complete payment', async ({ page }) => {
  // End-to-end payment test
});
```
```

---

### 6. `codegraph_get_symbol_info`

**Purpose:** Get metadata about a symbol — its type, signature, documentation, and optionally usage statistics.

**When to use:**
- Quick symbol lookup
- Understanding function signatures
- Finding documentation
- Assessing symbol usage (with `includeReferences: true`)

**Parameters:**
```json
{
  "uri": "file:///path/to/file.ts",    // Required: file containing symbol
  "line": 45,                           // Required: line number (0-indexed)
  "character": 0,                       // Optional: character position
  "includeReferences": false            // Optional: include all references (can be slow)
}
```

**Performance note:** Reference search can be slow on large workspaces. By default, references are not included. Set `includeReferences: true` only when you need usage statistics. For dependency analysis, consider using `codegraph_analyze_impact` instead.

**Example — "What is calculateDiscount?" (fast, no references)**

```
Tool: codegraph_get_symbol_info
Input: {
  "uri": "file:///project/src/pricing/discounts.ts",
  "line": 34,
  "character": 15
}
```

**Output:**
```markdown
# Symbol Information

Location: file:///project/src/pricing/discounts.ts:35:16

## Documentation & Type Information
```typescript
/**
 * Calculates the discount amount based on order total and customer tier.
 * @param total - The order total before discount
 * @param tier - Customer loyalty tier
 * @returns The discount amount to subtract
 */
function calculateDiscount(total: number, tier: CustomerTier): number
```

## Definition
- /project/src/pricing/discounts.ts:35

## References
_References not included. Set `includeReferences: true` to find usages (may be slow)._
```

**Example — "Where is calculateDiscount used?" (with references)**

```
Tool: codegraph_get_symbol_info
Input: {
  "uri": "file:///project/src/pricing/discounts.ts",
  "line": 34,
  "character": 15,
  "includeReferences": true
}
```

**Output (with references):**
```markdown
# Symbol Information

Location: file:///project/src/pricing/discounts.ts:35:16

## Documentation & Type Information
[...same as above...]

## Definition
- /project/src/pricing/discounts.ts:35

## References (12 usages)
- **CheckoutService.ts** (3 references)
  Line 45, Line 89, Line 123
- **PriceCalculator.ts** (4 references)
  Line 12, Line 34, Line 56, Line 78
- **discounts.test.ts** (5 references)
  Line 10, Line 25, Line 40
  ... and 2 more
```

---

### 7. `codegraph_analyze_complexity`

**Purpose:** Analyze cyclomatic and cognitive complexity of code to identify areas needing refactoring.

**When to use:**
- Assessing code quality
- Finding complex functions that need simplification
- Before refactoring efforts
- Code review preparation

**Parameters:**
```json
{
  "uri": "file:///path/to/file.ts",    // Required: file to analyze
  "line": 45,                           // Optional: specific function line (0-indexed)
  "threshold": 10,                      // Optional: complexity threshold for flagging
  "summary": false                      // Optional: return condensed summary
}
```

**Example — "Find complex functions in PaymentService"**

```
Tool: codegraph_analyze_complexity
Input: {
  "uri": "file:///project/src/services/PaymentService.ts",
  "threshold": 10
}
```

**Output:**
```markdown
# Complexity Analysis

File: src/services/PaymentService.ts

## Summary
- Total functions: 12
- High complexity (>10): 3
- Average complexity: 7.2

## High Complexity Functions

### 1. processPayment (line 45)
- Cyclomatic complexity: 15
- Cognitive complexity: 22
- Recommendation: Consider breaking into smaller functions

### 2. validateTransaction (line 120)
- Cyclomatic complexity: 12
- Cognitive complexity: 18
- Recommendation: Extract validation rules into separate functions

### 3. handleRefund (line 200)
- Cyclomatic complexity: 11
- Cognitive complexity: 14
- Recommendation: Simplify conditional logic
```

---

### 8. `codegraph_find_unused_code`

**Purpose:** Detect unused functions, variables, and imports that can be safely removed.

**When to use:**
- Code cleanup efforts
- Reducing bundle size
- Removing dead code
- Maintenance and refactoring

**Parameters:**
```json
{
  "uri": "file:///path/to/file.ts",    // Optional: specific file
  "scope": "file",                      // Optional: "file", "module", or "workspace"
  "includeTests": false,                // Optional: include test files
  "confidence": 0.7                     // Optional: minimum confidence threshold (0-1)
}
```

**Example — "Find unused code in the utils module"**

```
Tool: codegraph_find_unused_code
Input: {
  "uri": "file:///project/src/utils",
  "scope": "module",
  "confidence": 0.8
}
```

**Output:**
```markdown
# Unused Code Analysis

Scope: src/utils/ (module)
Confidence threshold: 80%

## Unused Functions (4)
🔴 **formatCurrency** at utils/formatters.ts:34 (confidence: 95%)
   - No references found in codebase
   - Recommendation: Safe to remove

🔴 **deprecated_helper** at utils/legacy.ts:12 (confidence: 98%)
   - Marked as deprecated, no usages
   - Recommendation: Safe to remove

🟡 **validateInput** at utils/validation.ts:78 (confidence: 82%)
   - Only used in test files
   - Recommendation: Move to test utilities or remove

🟡 **parseConfig** at utils/config.ts:45 (confidence: 80%)
   - Referenced dynamically (uncertain)
   - Recommendation: Verify before removing

## Unused Imports (6)
- lodash (utils/formatters.ts:1)
- moment (utils/dates.ts:2)
- ...and 4 more
```

**Cross-File Resolution (v0.3.1+):**

The tool now properly detects when symbols are used across files:
- ✅ Exported classes/functions that are imported elsewhere
- ✅ Cross-file function calls
- ✅ Framework entry points (VS Code `activate`/`deactivate`)
- ✅ Trait implementations and LSP protocol methods

**Known Limitations:**
- Instance method calls (e.g., `obj.method()`) are not tracked through object instances
- Dynamic dispatch and reflection-based calls cannot be detected
- Class methods are only detected as "used" if the class itself is imported

---

### 9. `codegraph_analyze_coupling`

**Purpose:** Analyze module coupling and cohesion to improve architecture.

**When to use:**
- Architecture reviews
- Planning refactoring
- Identifying tightly coupled modules
- Improving code organization

**Parameters:**
```json
{
  "uri": "file:///path/to/file.ts",    // Required: file to analyze
  "includeExternal": false,             // Optional: include external dependencies
  "depth": 2,                           // Optional: depth of analysis
  "summary": false                      // Optional: return condensed summary
}
```

**Example — "Analyze coupling for UserService"**

```
Tool: codegraph_analyze_coupling
Input: {
  "uri": "file:///project/src/services/UserService.ts",
  "depth": 2
}
```

**Output:**
```markdown
# Coupling Analysis

Module: src/services/UserService.ts

## Metrics
- Afferent Coupling (Ca): 8 (modules that depend on this)
- Efferent Coupling (Ce): 5 (modules this depends on)
- Instability (I): 0.38 (Ce / (Ca + Ce))
- Abstractness (A): 0.2

## Incoming Dependencies (8)
Modules that depend on UserService:
- AuthController.ts
- ProfileController.ts
- AdminService.ts
- NotificationService.ts
- ...and 4 more

## Outgoing Dependencies (5)
Modules that UserService depends on:
- DatabaseClient.ts
- Logger.ts
- EmailService.ts
- CacheService.ts
- ValidationUtils.ts

## Recommendations
⚠️ High afferent coupling - changes here may have wide impact
✅ Moderate instability - reasonable balance
💡 Consider abstracting shared interfaces to reduce coupling
```

---

### 10. `codegraph_symbol_search`

**Purpose:** Fast text-based symbol search with BM25 ranking. Ideal for finding code by name or keyword.

**When to use:**
- Finding functions by name
- Exploring unfamiliar codebases
- Locating specific implementations
- Searching for patterns in symbol names

**Parameters:**
```json
{
  "query": "validate email",            // Required: search keywords
  "symbolType": "any",                  // Optional: "function", "class", "method", "variable", etc.
  "limit": 20,                          // Optional: max results
  "includePrivate": true                // Optional: include private symbols
}
```

**Example — "Find email validation functions"**

```
Tool: codegraph_symbol_search
Input: {
  "query": "validate email",
  "symbolType": "function",
  "limit": 10
}
```

**Output:**
```markdown
# Symbol Search Results

Query: "validate email"
Found: 5 functions

## Results

### 1. validateEmail (score: 9.5)
- Location: src/utils/validators.ts:45
- Signature: `function validateEmail(email: string): boolean`
- Type: function

### 2. isValidEmail (score: 8.2)
- Location: src/auth/validation.ts:23
- Signature: `function isValidEmail(input: string): ValidationResult`
- Type: function

### 3. validateEmailFormat (score: 7.8)
- Location: src/forms/validators.ts:67
- Signature: `function validateEmailFormat(email: string, strict?: boolean): boolean`
- Type: function

### 4. checkEmailValid (score: 6.5)
- Location: src/api/handlers.ts:112
- Signature: `async function checkEmailValid(email: string): Promise<boolean>`
- Type: function

### 5. emailValidator (score: 5.2)
- Location: src/shared/validation.ts:34
- Signature: `const emailValidator: Validator<string>`
- Type: variable
```

---

### 11. `codegraph_find_by_imports`

**Purpose:** Find all code that imports or uses specific modules or packages.

**When to use:**
- Understanding library usage patterns
- Planning migrations (e.g., replacing a library)
- Finding consumers of internal modules
- Auditing dependency usage

**Parameters:**
```json
{
  "moduleName": "lodash",              // Required: module/package name
  "matchMode": "contains",              // Optional: "exact", "prefix", "contains", "fuzzy"
  "limit": 50                           // Optional: max results
}
```

**Example — "Find all code using lodash"**

```
Tool: codegraph_find_by_imports
Input: {
  "moduleName": "lodash",
  "matchMode": "prefix"
}
```

**Output:**
```markdown
# Import Search Results

Module: lodash (prefix match)
Found: 12 files

## Files Importing lodash

### src/utils/helpers.ts
```typescript
import { debounce, throttle } from 'lodash';
import _ from 'lodash';
```
Used symbols: debounce, throttle, _

### src/services/DataProcessor.ts
```typescript
import { groupBy, sortBy, uniqBy } from 'lodash';
```
Used symbols: groupBy, sortBy, uniqBy

### src/components/Table.tsx
```typescript
import { orderBy } from 'lodash/orderBy';
```
Used symbols: orderBy

...and 9 more files
```

---

### 12. `codegraph_find_entry_points`

**Purpose:** Discover application entry points — main functions, HTTP handlers, CLI commands, event handlers.

**When to use:**
- Understanding application architecture
- Tracing request flow
- Finding all API endpoints
- Mapping CLI commands

**Parameters:**
```json
{
  "entryType": "all",                   // Optional: "main", "http_handler", "cli_command", "event_handler", "test", "all"
  "framework": "express",               // Optional: filter by framework
  "limit": 50                           // Optional: max results
}
```

**Example — "Find all HTTP endpoints"**

```
Tool: codegraph_find_entry_points
Input: {
  "entryType": "http_handler"
}
```

**Output:**
```markdown
# Entry Points

Type: HTTP Handlers
Found: 15 endpoints

## Endpoints

### Authentication
| Method | Route | Handler | Location |
|--------|-------|---------|----------|
| POST | /api/login | loginHandler | src/api/auth.ts:23 |
| POST | /api/register | registerHandler | src/api/auth.ts:45 |
| POST | /api/logout | logoutHandler | src/api/auth.ts:67 |

### Users
| Method | Route | Handler | Location |
|--------|-------|---------|----------|
| GET | /api/users | listUsers | src/api/users.ts:12 |
| GET | /api/users/:id | getUser | src/api/users.ts:34 |
| PUT | /api/users/:id | updateUser | src/api/users.ts:56 |
| DELETE | /api/users/:id | deleteUser | src/api/users.ts:78 |

### Orders
| Method | Route | Handler | Location |
|--------|-------|---------|----------|
| GET | /api/orders | listOrders | src/api/orders.ts:15 |
| POST | /api/orders | createOrder | src/api/orders.ts:45 |
...and 6 more
```

---

### 13. `codegraph_traverse_graph`

**Purpose:** Advanced code exploration by traversing the code graph with custom filters.

**When to use:**
- Tracing execution flow
- Finding all code reachable from a function
- Custom relationship exploration
- Complex dependency analysis

**Parameters:**
```json
{
  "uri": "file:///path/to/file.ts",    // Preferred: file URI
  "line": 45,                           // Preferred: line number (0-indexed)
  "startNodeId": "abc123",              // Alternative: node ID from symbol_search
  "direction": "outgoing",              // Optional: "outgoing", "incoming", "both"
  "edgeTypes": ["calls", "imports"],    // Optional: filter edge types
  "nodeTypes": ["function"],            // Optional: filter node types
  "maxDepth": 3,                        // Optional: traversal depth
  "limit": 100                          // Optional: max nodes
}
```

**Example — "Trace all functions called by authenticate"**

```
Tool: codegraph_traverse_graph
Input: {
  "uri": "file:///project/src/auth/middleware.ts",
  "line": 25,
  "direction": "outgoing",
  "edgeTypes": ["calls"],
  "maxDepth": 3
}
```

**Output:**
```markdown
# Graph Traversal

Starting from: authenticate (src/auth/middleware.ts:25)
Direction: outgoing
Edge types: calls
Max depth: 3

## Traversal Results (12 nodes)

### Depth 1
- **extractToken** at src/auth/middleware.ts:45
- **verifyToken** at src/auth/tokens.ts:23
- **logRequest** at src/utils/logger.ts:12

### Depth 2
- **decodeJWT** at src/auth/jwt.ts:34 (via verifyToken)
- **validateSignature** at src/auth/jwt.ts:67 (via verifyToken)
- **getUserById** at src/db/users.ts:23 (via verifyToken)

### Depth 3
- **queryDatabase** at src/db/client.ts:45 (via getUserById)
- **formatUser** at src/db/users.ts:89 (via getUserById)
- **checkExpiry** at src/auth/jwt.ts:90 (via decodeJWT)
...and 3 more
```

---

### 14. `codegraph_get_callers`

**Purpose:** Find all functions that call a specific function.

**When to use:**
- Understanding function usage
- Before modifying function signature
- Impact analysis
- Debugging call chains

**Parameters:**
```json
{
  "uri": "file:///path/to/file.ts",    // Preferred: file URI
  "line": 45,                           // Preferred: line number (0-indexed)
  "nodeId": "abc123",                   // Alternative: node ID from symbol_search
  "depth": 1                            // Optional: caller chain depth
}
```

**Example — "Who calls validatePayment?"**

```
Tool: codegraph_get_callers
Input: {
  "uri": "file:///project/src/payments/validator.ts",
  "line": 34,
  "depth": 2
}
```

**Output:**
```markdown
# Callers of validatePayment

Function: validatePayment (src/payments/validator.ts:34)

## Direct Callers (Depth 1)
| Caller | Location | Context |
|--------|----------|---------|
| processPayment | src/payments/processor.ts:67 | `if (validatePayment(order)) { ... }` |
| checkoutHandler | src/api/checkout.ts:45 | `await validatePayment(cart.payment)` |
| retryPayment | src/payments/retry.ts:23 | `validatePayment(failedOrder)` |

## Indirect Callers (Depth 2)
| Caller | Via | Location |
|--------|-----|----------|
| handleCheckout | processPayment | src/api/checkout.ts:12 |
| scheduleRetry | retryPayment | src/jobs/payments.ts:34 |
| adminRefund | processPayment | src/admin/refunds.ts:56 |
```

---

### 15. `codegraph_get_callees`

**Purpose:** Find all functions that a specific function calls.

**When to use:**
- Understanding function behavior
- Tracing execution flow
- Debugging
- Analyzing dependencies

**Parameters:**
```json
{
  "uri": "file:///path/to/file.ts",    // Preferred: file URI
  "line": 45,                           // Preferred: line number (0-indexed)
  "nodeId": "abc123",                   // Alternative: node ID from symbol_search
  "depth": 1                            // Optional: callee chain depth
}
```

**Example — "What does processOrder call?"**

```
Tool: codegraph_get_callees
Input: {
  "uri": "file:///project/src/orders/service.ts",
  "line": 78,
  "depth": 2
}
```

**Output:**
```markdown
# Callees of processOrder

Function: processOrder (src/orders/service.ts:78)

## Direct Callees (Depth 1)
| Function | Location | Call Site |
|----------|----------|-----------|
| validateOrder | src/orders/validator.ts:12 | Line 82 |
| calculateTotal | src/orders/pricing.ts:45 | Line 85 |
| chargePayment | src/payments/processor.ts:23 | Line 90 |
| sendConfirmation | src/notifications/email.ts:67 | Line 95 |
| updateInventory | src/inventory/service.ts:34 | Line 100 |

## Indirect Callees (Depth 2)
| Function | Via | Location |
|----------|-----|----------|
| applyDiscount | calculateTotal | src/orders/pricing.ts:78 |
| validateCard | chargePayment | src/payments/validator.ts:45 |
| renderTemplate | sendConfirmation | src/notifications/templates.ts:23 |
```

---

### 16. `codegraph_get_detailed_symbol`

**Purpose:** Get comprehensive information about a symbol including source code, documentation, and relationships.

**When to use:**
- Deep dive into a specific function/class
- Understanding implementation details
- Getting full context for code review
- Documentation lookup

**Parameters:**
```json
{
  "uri": "file:///path/to/file.ts",    // Preferred: file URI
  "line": 45,                           // Preferred: line number (0-indexed)
  "nodeId": "abc123",                   // Alternative: node ID from symbol_search
  "includeSource": true,                // Optional: include full source code
  "includeCallers": true,               // Optional: include callers list
  "includeCallees": true                // Optional: include callees list
}
```

**Example — "Get full details for UserService.createUser"**

```
Tool: codegraph_get_detailed_symbol
Input: {
  "uri": "file:///project/src/services/UserService.ts",
  "line": 45,
  "includeSource": true,
  "includeCallers": true,
  "includeCallees": true
}
```

**Output:**
```markdown
# Symbol Details

## createUser
- **Type:** async method
- **Location:** src/services/UserService.ts:45-78
- **Signature:** `async createUser(data: CreateUserInput): Promise<User>`
- **Visibility:** public
- **Modifiers:** async

## Documentation
```typescript
/**
 * Creates a new user account.
 * @param data - User creation input containing email, password, and profile
 * @returns The created user object
 * @throws ValidationError if input is invalid
 * @throws DuplicateError if email already exists
 */
```

## Source Code
```typescript
async createUser(data: CreateUserInput): Promise<User> {
  // Validate input
  const validated = await this.validator.validate(data);

  // Check for existing user
  const existing = await this.repository.findByEmail(validated.email);
  if (existing) {
    throw new DuplicateError('Email already registered');
  }

  // Hash password
  const hashedPassword = await this.hasher.hash(validated.password);

  // Create user
  const user = await this.repository.create({
    ...validated,
    password: hashedPassword,
  });

  // Send welcome email
  await this.emailService.sendWelcome(user);

  return user;
}
```

## Callers (3)
- registerHandler (src/api/auth.ts:34)
- adminCreateUser (src/admin/users.ts:56)
- importUsers (src/jobs/import.ts:78)

## Callees (5)
- validator.validate (src/validation/UserValidator.ts:12)
- repository.findByEmail (src/db/UserRepository.ts:45)
- hasher.hash (src/utils/password.ts:23)
- repository.create (src/db/UserRepository.ts:67)
- emailService.sendWelcome (src/email/EmailService.ts:89)
```

---

### 17. `codegraph_find_by_signature`

**Purpose:** Find functions by their signature pattern — parameter count, return type, or modifiers.

**When to use:**
- Finding all async functions
- Finding functions with specific return types
- Finding functions with certain parameter counts
- Pattern-based code search

**Parameters:**
```json
{
  "namePattern": "get*",                // Optional: function name pattern (wildcards)
  "paramCount": 2,                      // Optional: exact parameter count
  "minParams": 1,                       // Optional: minimum parameters
  "maxParams": 3,                       // Optional: maximum parameters
  "returnType": "Promise",              // Optional: return type to match
  "modifiers": ["async"],               // Optional: required modifiers
  "limit": 50                           // Optional: max results
}
```

**Example — "Find all async functions returning Promise"**

```
Tool: codegraph_find_by_signature
Input: {
  "modifiers": ["async"],
  "returnType": "Promise",
  "limit": 20
}
```

**Output:**
```markdown
# Signature Search Results

Criteria:
- Modifiers: async
- Return type: Promise

Found: 45 functions (showing first 20)

## Results

| Function | Signature | Location |
|----------|-----------|----------|
| fetchUsers | `async fetchUsers(): Promise<User[]>` | src/api/users.ts:12 |
| createOrder | `async createOrder(data: OrderInput): Promise<Order>` | src/orders/service.ts:34 |
| authenticate | `async authenticate(token: string): Promise<AuthResult>` | src/auth/service.ts:23 |
| sendEmail | `async sendEmail(to: string, template: string): Promise<void>` | src/email/service.ts:45 |
| processPayment | `async processPayment(order: Order): Promise<Receipt>` | src/payments/processor.ts:67 |
| validateInput | `async validateInput<T>(data: T): Promise<ValidationResult>` | src/validation/validator.ts:12 |
...and 14 more
```

**Example — "Find handlers with 2-3 parameters"**

```
Tool: codegraph_find_by_signature
Input: {
  "namePattern": "*Handler",
  "minParams": 2,
  "maxParams": 3
}
```

**Output:**
```markdown
# Signature Search Results

Criteria:
- Name pattern: *Handler
- Parameters: 2-3

Found: 8 functions

## Results

| Function | Signature | Location |
|----------|-----------|----------|
| loginHandler | `loginHandler(req: Request, res: Response)` | src/api/auth.ts:23 |
| errorHandler | `errorHandler(err: Error, req: Request, res: Response)` | src/middleware/error.ts:12 |
| webhookHandler | `webhookHandler(event: WebhookEvent, context: Context)` | src/webhooks/handler.ts:34 |
...
```

---

## Best Practices for AI Agents

### 1. Start with Context, Then Narrow Down

```
❌ Don't: Immediately grep for specific strings

✅ Do:
1. codegraph_get_ai_context (understand the area)
2. codegraph_get_dependency_graph (if architecture matters)
3. Then use grep for specific patterns if needed
```

### 2. Always Check Impact Before Changes

```
❌ Don't: Make changes and hope tests catch issues

✅ Do:
1. codegraph_analyze_impact (understand blast radius)
2. codegraph_find_related_tests (know what tests to run)
3. Make changes
4. Run affected tests
```

### 3. Use Intent-Aware Context

```
# For explaining code:
codegraph_get_ai_context with intent: "explain"
→ Gets type definitions, interfaces, documentation

# For modifying code:
codegraph_get_ai_context with intent: "modify"
→ Gets callers, contracts, things that might break

# For debugging:
codegraph_get_ai_context with intent: "debug"
→ Gets call chain, error handling, data flow

# For testing:
codegraph_get_ai_context with intent: "test"
→ Gets related tests, mocks, test utilities
```

### 4. Hybrid Approach

CodeGraph tools excel at **understanding relationships**. Traditional tools excel at **specific text searches**.

```
Use CodeGraph for:
- Understanding code structure
- Impact analysis
- Dependency analysis
- Getting comprehensive context

Use grep/read for:
- Finding specific strings or patterns
- Reading specific file sections
- Custom regex searches
```

---

## Efficiency Comparison

Based on real-world benchmarks:

| Scenario | Without CodeGraph | With CodeGraph | Improvement |
|----------|------------------|----------------|-------------|
| Explain a class | 4 tool calls, 9.8K tokens | 1 call, 4.2K tokens | 75% fewer tools, 57% fewer tokens |
| Find dependencies | 10 tool calls, 7K tokens | 1 call, 500 tokens | 90% fewer tools, 93% fewer tokens |
| Impact analysis | 6 tool calls, 5K tokens | 1 call, 1.2K tokens | 83% fewer tools, 76% fewer tokens |
| Debug investigation | 7 tool calls, 8K tokens | 1 call, 3K tokens | 86% fewer tools, 62% fewer tokens |
| Check for circular deps | 12 tool calls, 10K tokens | 1 call, 800 tokens | 92% fewer tools, 92% fewer tokens |
| Find all async functions | 15 tool calls, 12K tokens | 1 call, 600 tokens | 93% fewer tools, 95% fewer tokens |
| Find all API endpoints | 8 tool calls, 6K tokens | 1 call, 400 tokens | 87% fewer tools, 93% fewer tokens |
| Find unused code | 20+ tool calls, 15K tokens | 1 call, 1K tokens | 95% fewer tools, 93% fewer tokens |

**Overall: 75-95% reduction in tool calls and tokens consumed.**

---

## Error Handling

CodeGraph tools provide helpful error messages when things go wrong:

```markdown
# AI Context Unavailable

❌ No code symbol found at the specified position.

**This could mean:**
- The position is in whitespace, comments, or imports
- The file has not been indexed by CodeGraph yet
- The specified line/character is out of bounds

**Try:**
- Place cursor on a function, class, or variable definition
- Run "CodeGraph: Reindex Workspace" to update the index
- Verify the file is a supported language (TypeScript, JavaScript, Python, Rust, Go)
```

---

## Summary

CodeGraph tools transform how AI agents interact with codebases:

| Traditional | CodeGraph |
|-------------|-----------|
| Text matching | Semantic understanding |
| Many small queries | One comprehensive query |
| Manual relationship tracing | Automatic graph traversal |
| Generic context | Intent-aware context |

**Result: Faster, more accurate, and more efficient code understanding.**
