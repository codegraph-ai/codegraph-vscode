import { Position, Range, Location } from 'vscode-languageclient';

// ==========================================
// Dependency Graph Types
// ==========================================

export interface DependencyGraphParams {
    uri: string;
    depth?: number;
    includeExternal?: boolean;
    direction?: 'imports' | 'importedBy' | 'both';
}

export interface DependencyNode {
    id: string;
    label: string;
    type: 'module' | 'package' | 'file';
    language: string;
    uri: string;
    metadata?: Record<string, unknown>;
}

export interface DependencyEdge {
    from: string;
    to: string;
    type: 'import' | 'require' | 'use';
    metadata?: Record<string, unknown>;
}

export interface DependencyGraphResponse {
    nodes: DependencyNode[];
    edges: DependencyEdge[];
}

// ==========================================
// Call Graph Types
// ==========================================

export interface CallGraphParams {
    uri: string;
    position: Position;
    direction?: 'callers' | 'callees' | 'both';
    depth?: number;
    includeExternal?: boolean;
}

export interface FunctionNode {
    id: string;
    name: string;
    signature: string;
    uri: string;
    range: Range;
    language: string;
    metrics?: {
        complexity?: number;
        linesOfCode?: number;
        callCount?: number;
    };
}

export interface CallEdge {
    from: string;
    to: string;
    callSites: Location[];
    isRecursive?: boolean;
}

export interface CallGraphResponse {
    root: FunctionNode;
    nodes: FunctionNode[];
    edges: CallEdge[];
}

// ==========================================
// AI Context Types
// ==========================================

export interface AIContextParams {
    uri: string;
    position: Position;
    contextType: 'explain' | 'modify' | 'debug' | 'test';
    maxTokens?: number;
}

export interface PrimaryContext {
    type: 'function' | 'class' | 'module';
    name: string;
    code: string;
    language: string;
    location: Location;
}

export interface RelatedSymbol {
    name: string;
    relationship: 'calls' | 'called_by' | 'uses' | 'used_by' | 'inherits' | 'implements' | 'tests' | 'similar';
    code: string;
    location: Location;
    relevanceScore: number;
}

export interface DependencyInfo {
    name: string;
    type: 'import' | 'type_dependency';
    code?: string;
}

export interface UsageExample {
    code: string;
    location: Location;
    description?: string;
}

export interface ArchitectureInfo {
    module: string;
    layer?: string;
    neighbors: string[];
}

export interface AIContextResponse {
    primaryContext: PrimaryContext;
    relatedSymbols: RelatedSymbol[];
    dependencies: DependencyInfo[];
    usageExamples?: UsageExample[];
    architecture?: ArchitectureInfo;
    metadata: {
        totalTokens: number;
        queryTime: number;
    };
}

// ==========================================
// Impact Analysis Types
// ==========================================

export interface ImpactAnalysisParams {
    uri: string;
    position: Position;
    analysisType: 'modify' | 'delete' | 'rename';
}

export interface DirectImpact {
    uri: string;
    range: Range;
    type: 'caller' | 'reference' | 'subclass' | 'implementation';
    severity: 'breaking' | 'warning' | 'info';
}

export interface IndirectImpact {
    uri: string;
    path: string[];
    severity: 'breaking' | 'warning' | 'info';
}

export interface AffectedTest {
    uri: string;
    testName: string;
}

export interface ImpactAnalysisResponse {
    directImpact: DirectImpact[];
    indirectImpact: IndirectImpact[];
    affectedTests: AffectedTest[];
    summary: {
        filesAffected: number;
        breakingChanges: number;
        warnings: number;
    };
}

// ==========================================
// Related Tests Types
// ==========================================

export interface RelatedTestsParams {
    uri: string;
    position: Position;
    limit?: number;
}

export interface RelatedTest {
    uri: string;
    testName: string;
    relationship: string;
    range: Range;
}

export interface RelatedTestsResponse {
    tests: RelatedTest[];
    truncated?: boolean;
}

// ==========================================
// Parser Metrics Types
// ==========================================

export interface ParserMetricsParams {
    language?: string;
}

export interface ParserMetric {
    language: string;
    filesAttempted: number;
    filesSucceeded: number;
    filesFailed: number;
    totalEntities: number;
    totalRelationships: number;
    totalParseTimeMs: number;
    avgParseTimeMs: number;
}

export interface ParserMetricsResponse {
    metrics: ParserMetric[];
    totals: {
        filesAttempted: number;
        filesSucceeded: number;
        filesFailed: number;
        totalEntities: number;
        successRate: number;
    };
}

// ==========================================
// Graph Visualization Types (for webview)
// ==========================================

export interface GraphNode {
    id: string;
    label: string;
    type: string;
    language?: string;
    x?: number;
    y?: number;
}

export interface GraphEdge {
    from: string;
    to: string;
    type: string;
}

export interface GraphData {
    nodes: GraphNode[];
    edges: GraphEdge[];
}

// ==========================================
// Code Metrics Types
// ==========================================

export interface ComplexityParams {
    uri: string;
    line?: number;
    threshold?: number;
    includeMetrics?: boolean;
}

export interface ComplexityDetails {
    branches: number;
    loops: number;
    conditions: number;
    nestingDepth: number;
    linesOfCode: number;
}

export interface LocationInfo {
    uri: string;
    range: Range;
}

export interface FunctionComplexity {
    name: string;
    complexity: number;
    grade: string;
    location: LocationInfo;
    details: ComplexityDetails;
}

export interface FileSummary {
    totalFunctions: number;
    averageComplexity: number;
    maxComplexity: number;
    functionsAboveThreshold: number;
    overallGrade: string;
}

export interface ComplexityResponse {
    functions: FunctionComplexity[];
    fileSummary: FileSummary;
    recommendations: string[];
}

// ==========================================
// Unused Code Detection Types
// ==========================================

export interface UnusedCodeParams {
    uri?: string;
    scope: 'file' | 'module' | 'workspace';
    includeTests?: boolean;
    confidence?: number;
}

export interface UnusedItem {
    itemType: string;
    name: string;
    location: LocationInfo;
    confidence: number;
    reason: string;
    safeToRemove: boolean;
}

export interface UnusedByType {
    functions: number;
    classes: number;
    imports: number;
    variables: number;
}

export interface UnusedSummary {
    totalItems: number;
    byType: UnusedByType;
    safeDeletions: number;
    estimatedLinesRemovable: number;
}

export interface UnusedCodeResponse {
    unusedItems: UnusedItem[];
    summary: UnusedSummary;
}

// ==========================================
// Coupling Analysis Types
// ==========================================

export interface CouplingParams {
    uri: string;
    includeExternal?: boolean;
    depth?: number;
}

export interface CouplingMetrics {
    afferent: number;
    efferent: number;
    instability: number;
    dependents: string[];
    dependencies: string[];
}

export interface CohesionMetrics {
    score: number;
    cohesionType: string;
    internalReferenceRatio: number;
}

export interface ArchViolation {
    violationType: string;
    severity: string;
    description: string;
    suggestion: string;
}

export interface CouplingResponse {
    coupling: CouplingMetrics;
    cohesion: CohesionMetrics;
    violations: ArchViolation[];
    recommendations: string[];
}

// ==========================================
// AI Agent Query Primitives Types
// ==========================================

export interface SymbolSearchParams {
    query: string;
    scope?: 'workspace' | 'module' | 'file';
    symbolTypes?: ('function' | 'class' | 'variable' | 'module' | 'interface' | 'type')[];
    limit?: number;
    includePrivate?: boolean;
}

export interface SymbolLocation {
    file: string;
    line: number;
    column: number;
    endLine: number;
    endColumn: number;
}

export interface SymbolInfo {
    name: string;
    kind: string;
    location: SymbolLocation;
    signature?: string;
    docstring?: string;
    isPublic: boolean;
}

export interface SymbolMatch {
    nodeId: string;
    symbol: SymbolInfo;
    score: number;
    matchReason: string;
}

export interface SymbolSearchResponse {
    results: SymbolMatch[];
    totalMatches: number;
    queryTimeMs: number;
}

export interface FindByImportsParams {
    libraries: string[];
    matchMode?: 'exact' | 'prefix' | 'fuzzy';
}

export interface FindByImportsResponse {
    results: SymbolMatch[];
    queryTimeMs: number;
}

export interface FindEntryPointsParams {
    entryType?: 'http_handler' | 'cli_command' | 'public_api' | 'event_handler' | 'test_entry' | 'main';
}

export interface EntryPoint {
    nodeId: string;
    entryType: string;
    route?: string;
    method?: string;
    description?: string;
    symbol: SymbolInfo;
}

export interface FindEntryPointsResponse {
    entryPoints: EntryPoint[];
    totalFound: number;
}

export interface TraverseGraphParams {
    startNodeId?: string;
    uri?: string;
    line?: number;
    direction?: 'outgoing' | 'incoming' | 'both';
    depth?: number;
    filterSymbolTypes?: ('function' | 'class' | 'variable' | 'module' | 'interface' | 'type')[];
    maxNodes?: number;
}

export interface TraversalNode {
    nodeId: string;
    depth: number;
    path: string[];
    edgeType: string;
    symbol: SymbolInfo;
}

export interface TraverseGraphResponse {
    nodes: TraversalNode[];
    queryTimeMs: number;
}

export interface GetCallersParams {
    nodeId?: string;
    uri?: string;
    line?: number;
    depth?: number;
}

export interface CallInfo {
    nodeId: string;
    symbol: SymbolInfo;
    callSite: SymbolLocation;
    depth: number;
}

export interface GetCallersResponse {
    callers: CallInfo[];
    queryTimeMs: number;
}

export interface GetDetailedInfoParams {
    nodeId?: string;
    uri?: string;
    line?: number;
    includeCallers?: boolean;
    includeCallees?: boolean;
}

export interface DetailedSymbolResponse {
    symbol: SymbolInfo;
    callers: CallInfo[];
    callees: CallInfo[];
    complexity?: number;
    linesOfCode: number;
    isPublic: boolean;
    isDeprecated: boolean;
    referenceCount: number;
}

export interface FindBySignatureParams {
    namePattern?: string;
    returnType?: string;
    paramCount?: {
        min: number;
        max: number;
    };
    modifiers?: ('public' | 'private' | 'static' | 'async' | 'const')[];
}

export interface FindBySignatureResponse {
    results: SymbolMatch[];
    queryTimeMs: number;
}
