# PDF-AST Node.js Bindings

This package provides experimental Node.js bindings for `pdf-core` and its
published `pdf-ast` crate. It exposes structural parsing and selected
preflight checks; it is not a complete PDF 2.0 or compliance implementation.

## Features

- **Structural parsing**: Parse supported PDF objects into an AST
- **Experimental preflight**: Run selected PDF/A and PDF/UA checks
- **Plugin system**: Extensible architecture for custom document processing
- **Performance optimized**: Fast parsing with native Rust implementation
- **TypeScript support**: Full TypeScript definitions included
- **Scope**: See the repository capability and compliance matrices

## Installation

```bash
npm install pdf-ast
```

## Quick Start

```javascript
const pdfAst = require('pdf-ast');
const fs = require('fs');

// Parse a PDF document
const pdfBuffer = fs.readFileSync('document.pdf');
const document = pdfAst.parseDocument(pdfBuffer);

// Get document statistics
const stats = document.getStatistics();
console.log(`Document has ${stats.totalNodes} nodes`);
console.log(`PDF version: ${stats.version}`);

// Get all pages
const pages = document.getNodesByType('Page');
console.log(`Document has ${pages.length} pages`);

// Run selected PDF/A-1b preflight checks
const report = document.validate('PDF/A-1b');
if (report.isValid()) {
    console.log('No implemented PDF/A-1b preflight rule failed');
} else {
    console.log(`Validation failed with ${report.getIssues().length} issues`);
    report.getIssues().forEach(issue => {
        console.log(`  ${issue.getSeverity()}: ${issue.getMessage()}`);
    });
}

// Use plugin system
const pluginManager = new pdfAst.PluginManager();
const result = pluginManager.executePlugins(document);
console.log(`Executed ${result.totalPlugins} plugins in ${result.executionTimeMs}ms`);
```

## TypeScript Usage

```typescript
import * as pdfAst from 'pdf-ast';
import { readFileSync } from 'fs';

// Parse a PDF document
const pdfBuffer: Buffer = readFileSync('document.pdf');
const document: pdfAst.PdfDocument = pdfAst.parseDocument(pdfBuffer);

// Get document statistics
const stats: pdfAst.DocumentStatistics = document.getStatistics();
console.log(`Document has ${stats.totalNodes} nodes`);

// Type-safe node type filtering
const pages: pdfAst.AstNode[] = document.getNodesByType('Page');
const fonts: pdfAst.AstNode[] = document.getNodesByType('Font');

// Validation with proper typing
const report: pdfAst.ValidationReport = document.validate('PDF/A-1b');
const issues: pdfAst.ValidationIssue[] = report.getIssues();
```

## Advanced Usage

### Working with AST Nodes

```javascript
// Get the document catalog
const root = document.getRoot();
if (root) {
    console.log(`Root node ID: ${root.getId()}`);
    console.log(`Root node type: ${root.getType()}`);
    
    // Check for specific properties
    if (root.hasProperty('Type')) {
        console.log(`Type: ${root.getProperty('Type')}`);
    }
    
    // Get metadata
    const metadata = root.getMetadata();
    if (metadata) {
        console.log(`Byte offset: ${metadata.byteOffset}`);
        console.log(`Byte length: ${metadata.byteLength}`);
    }
}

// Get children of a node
const children = document.getChildren(root.getId());
children.forEach(child => {
    console.log(`Child: ${child.getType()}`);
});
```

### Schema Validation

```javascript
// Get available schemas
const schemas = pdfAst.getAvailableSchemas();
console.log('Available schemas:', schemas);

// Validate against multiple schemas
const validationResults = [];
['PDF-2.0', 'PDF/A-1b', 'PDF/X-1a'].forEach(schema => {
    if (schemas.includes(schema)) {
        const report = document.validate(schema);
        validationResults.push({
            schema,
            valid: report.isValid(),
            issues: report.getIssues().length
        });
    }
});

console.table(validationResults);
```

### Plugin Management

```javascript
// Create plugin manager
const manager = new pdfAst.PluginManager();

// List available plugins
const plugins = manager.listPlugins();
plugins.forEach(plugin => {
    console.log(`Plugin: ${plugin.name} v${plugin.version}`);
    console.log(`  Description: ${plugin.description}`);
    console.log(`  Author: ${plugin.author}`);
    console.log(`  Tags: ${plugin.tags.join(', ')}`);
});

// Execute plugins on document
const result = manager.executePlugins(document);
console.log(`Plugin execution summary:`);
console.log(`  Total: ${result.totalPlugins}`);
console.log(`  Successful: ${result.successfulPlugins}`);
console.log(`  Failed: ${result.failedPlugins}`);
console.log(`  Time: ${result.executionTimeMs}ms`);

// Check individual plugin results
Object.entries(result.pluginResults).forEach(([name, result]) => {
    console.log(`  ${name}: ${result}`);
});
```

### Error Handling

```javascript
try {
    const document = pdfAst.parseDocument(pdfBuffer);
    console.log('PDF parsed successfully');
} catch (error) {
    console.error('Failed to parse PDF:', error.message);
}

try {
    const report = document.validate('InvalidSchema');
} catch (error) {
    console.error('Unknown schema:', error.message);
}
```

## API Reference

### Classes

#### `PdfDocument`
- `getVersion()`: Get PDF version as `{major: number, minor: number}`
- `getAllNodes()`: Get array of all AST nodes
- `getRoot()`: Get root node or null
- `getNode(nodeId: number)`: Get specific node by ID
- `getChildren(nodeId: number)`: Get child nodes of a specific node
- `getNodesByType(nodeType: string)`: Get all nodes of a specific type
- `validate(schemaName: string)`: Validate against a schema
- `getStatistics()`: Get document statistics

#### `AstNode`
- `getId()`: Get node ID
- `getType()`: Get node type
- `getValue()`: Get string representation of node value
- `getMetadata()`: Get node metadata or null
- `hasProperty(key: string)`: Check if node has a property
- `getProperty(key: string)`: Get property value or null

#### `ValidationReport`
- `isValid()`: Check if document is valid
- `getSchemaName()`: Get schema name
- `getSchemaVersion()`: Get schema version
- `getIssues()`: Get array of validation issues
- `getStatistics()`: Get validation statistics

#### `ValidationIssue`
- `getSeverity()`: Get issue severity
- `getCode()`: Get error code
- `getMessage()`: Get error message
- `getNodeId()`: Get associated node ID or null
- `getLocation()`: Get error location or null
- `getSuggestion()`: Get fix suggestion or null

#### `PluginManager`
- `executePlugins(document)`: Execute plugins on document
- `listPlugins()`: Get array of plugin metadata

### Functions

- `parseDocument(buffer: Buffer)`: Parse PDF from buffer
- `getAvailableSchemas()`: Get array of available validation schemas
- `getNodeTypes()`: Get array of supported node types

### Constants

- `VERSION`: Library version string
- `AUTHOR`: Library author string

## Node Types

All standard PDF node types are supported:

```javascript
const nodeTypes = pdfAst.getNodeTypes();
// Returns: ['Catalog', 'Pages', 'Page', 'ContentStream', 'Font', ...]
```

- `Catalog`: Document catalog
- `Pages`: Pages tree
- `Page`: Individual page
- `ContentStream`: Page content stream
- `Font`, `Type1Font`, `TrueTypeFont`, `Type3Font`: Font objects
- `Image`: Image XObject
- `Annotation`: Annotations
- `Outline`: Document outline
- `Action`: Actions
- `Encryption`: Security settings
- `Metadata`: Document metadata
- `Structure`: Structured content
- `Form`: Interactive forms
- `JavaScript`: JavaScript code
- `Multimedia`: Multimedia content
- `ColorSpace`: Color spaces
- `Pattern`: Patterns
- `Shading`: Shading
- `XObject`: External objects
- `EmbeddedFile`: Embedded files
- `Other`: Other object types

## Standards Support

The binding exposes experimental checks for selected standards:

- **PDF 2.0**: Selected structural constraints
- **PDF/A-1b**: Selected preflight rules
- **PDF/UA-1**: Selected preflight rules

## Performance

The library is optimized for performance:

- Native Rust implementation for fast parsing
- Efficient memory usage
- Minimal JavaScript overhead
- Suitable for processing large documents

## Platform Support

Pre-built binaries are available for:

- **macOS**: x64, ARM64
- **Linux**: x64, ARM64
- **Windows**: x64

## Building from Source

If pre-built binaries are not available for your platform:

```bash
# Install Rust toolchain
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Clone repository
git clone https://github.com/seifreed/pdf-core.git
cd pdf-core/javascript

# Install dependencies and build
npm install
npm run build
```

## Examples

See the `examples/` directory for more usage examples:

- `basic-parsing.js`: Basic PDF parsing
- `validation.js`: Document validation
- `plugins.js`: Plugin system usage
- `typescript-example.ts`: TypeScript usage

## License

This project is licensed under the MIT License.
