/**
 * PDF-AST TypeScript Definitions
 * Experimental AST bindings for selected PDF structures.
 */

export interface NodeMetadata {
  byteOffset: number;
  byteLength: number;
  depth: number;
  objectId?: string;
}

export interface DocumentStatistics {
  totalNodes: number;
  nodeTypes: { [nodeType: string]: number };
  version: string;
}

export interface ValidationStatistics {
  totalChecks: number;
  passedChecks: number;
  failedChecks: number;
  infoCount: number;
  warningCount: number;
  errorCount: number;
  criticalCount: number;
}

export interface PluginMetadata {
  name: string;
  version: string;
  description: string;
  author: string;
  tags: string[];
}

export interface PluginExecutionResult {
  totalPlugins: number;
  successfulPlugins: number;
  failedPlugins: number;
  executionTimeMs: number;
  pluginResults: { [pluginName: string]: string };
}

export type NodeType = 
  | "Catalog"
  | "Pages"
  | "Page"
  | "ContentStream"
  | "Font"
  | "Type1Font"
  | "TrueTypeFont"
  | "Type3Font"
  | "Image"
  | "Annotation"
  | "Outline"
  | "Action"
  | "Encryption"
  | "Metadata"
  | "Structure"
  | "Form"
  | "JavaScript"
  | "Multimedia"
  | "ColorSpace"
  | "Pattern"
  | "Shading"
  | "XObject"
  | "EmbeddedFile"
  | "Other";

export type ValidationSeverity = "Info" | "Warning" | "Error" | "Critical";

/**
 * Represents a single node in the PDF AST
 */
export class AstNode {
  /**
   * Get the unique identifier of this node
   */
  getId(): number;

  /**
   * Get the type of this node
   */
  getType(): NodeType;

  /**
   * Get the string representation of this node's value
   */
  getValue(): string;

  /**
   * Get metadata associated with this node
   */
  getMetadata(): NodeMetadata | null;

  /**
   * Check if this node has a specific property
   */
  hasProperty(key: string): boolean;

  /**
   * Get the value of a specific property
   */
  getProperty(key: string): string | null;
}

/**
 * Represents a validation issue found during document validation
 */
export class ValidationIssue {
  /**
   * Get the severity of this issue
   */
  getSeverity(): ValidationSeverity;

  /**
   * Get the error code
   */
  getCode(): string;

  /**
   * Get the human-readable error message
   */
  getMessage(): string;

  /**
   * Get the ID of the node associated with this issue
   */
  getNodeId(): number | null;

  /**
   * Get the location where this issue occurred
   */
  getLocation(): string | null;

  /**
   * Get a suggestion for fixing this issue
   */
  getSuggestion(): string | null;
}

/**
 * Represents the result of validating a PDF document against a schema
 */
export class ValidationReport {
  /**
   * Check if the document passed validation
   */
  isValid(): boolean;

  /**
   * Get the name of the schema used for validation
   */
  getSchemaName(): string;

  /**
   * Get the version of the schema used for validation
   */
  getSchemaVersion(): string;

  /**
   * Get all validation issues found
   */
  getIssues(): ValidationIssue[];

  /**
   * Get validation statistics
   */
  getStatistics(): ValidationStatistics;
}

/**
 * Manages and executes plugins on PDF documents
 */
export class PluginManager {
  /**
   * Create a new plugin manager
   */
  constructor();

  /**
   * Execute all loaded plugins on a document
   */
  executePlugins(document: PdfDocument): PluginExecutionResult;

  /**
   * List all available plugins
   */
  listPlugins(): PluginMetadata[];
}

/**
 * Represents a PDF document and its AST
 */
export class PdfDocument {
  /**
   * Create a new empty PDF document
   */
  constructor();

  /**
   * Get the PDF version of this document
   */
  getVersion(): { major: number; minor: number };

  /**
   * Get all nodes in the document
   */
  getAllNodes(): AstNode[];

  /**
   * Get the root node of the document
   */
  getRoot(): AstNode | null;

  /**
   * Get a specific node by its ID
   */
  getNode(nodeId: number): AstNode | null;

  /**
   * Get all children of a specific node
   */
  getChildren(nodeId: number): AstNode[];

  /**
   * Get all nodes of a specific type
   */
  getNodesByType(nodeType: NodeType): AstNode[];

  /**
   * Validate this document against a specific schema
   */
  validate(schemaName: string): ValidationReport;

  /**
   * Get statistics about this document
   */
  getStatistics(): DocumentStatistics;
}

/**
 * Parse a PDF document from a buffer
 */
export function parseDocument(buffer: Buffer): PdfDocument;

/**
 * Get a list of all available validation schemas
 */
export function getAvailableSchemas(): string[];

/**
 * Get a list of all supported node types
 */
export function getNodeTypes(): NodeType[];

/**
 * Library version
 */
export const VERSION: string;

/**
 * Library author
 */
export const AUTHOR: string;
