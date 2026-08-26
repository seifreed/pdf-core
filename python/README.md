# PDF-AST Python Bindings

This package provides experimental Python bindings for `pdf-core` and its
Rust crate. It exposes structural parsing and selected
preflight checks; it is not a complete PDF 2.0 or compliance implementation.

## Features

- **Structural parsing**: Parse supported PDF objects into a structured AST
- **Experimental preflight**: Run selected PDF/A and PDF/UA checks
- **Plugin system**: Extensible architecture for custom document processing
- **Performance optimized**: Fast parsing with optional parallel processing
- **Scope**: See the repository capability and compliance matrices

## Installation

```bash
python -m pip install ./python
```

## Quick Start

```python
import pdf_ast

# Parse a PDF document
with open("document.pdf", "rb") as f:
    document = pdf_ast.parse_pdf(f.read())

# Get document statistics
stats = document.get_statistics()
print(f"Document has {stats['total_nodes']} nodes")
print(f"PDF version: {stats['version']}")

# Get all pages
pages = document.get_nodes_by_type("Page")
print(f"Document has {len(pages)} pages")

# Run selected PDF/A-1b preflight checks
report = document.validate("PDF/A-1b")
if report.is_valid():
    print("No implemented PDF/A-1b preflight rule failed")
else:
    print(f"Validation failed with {len(report.get_issues())} issues")
    for issue in report.get_issues():
        print(f"  {issue.get_severity()}: {issue.get_message()}")

# Use plugin system
plugin_manager = pdf_ast.PluginManager()
plugin_manager.execute_plugins(document)
```

## Advanced Usage

### Working with AST Nodes

```python
# Get the document catalog
root = document.get_root()
if root:
    print(f"Root node ID: {root.get_id()}")
    print(f"Root node type: {root.get_type()}")
    
    # Check for specific properties
    if root.has_property("Type"):
        print(f"Type: {root.get_property('Type')}")

# Get children of a node
children = document.get_children(root.get_id())
for child in children:
    print(f"Child: {child.get_type()}")
```

### Schema Validation

```python
# Get available schemas
schemas = pdf_ast.get_available_schemas()
print("Available schemas:", schemas)

# Validate against multiple schemas
for schema in ["PDF-2.0", "PDF/A-1b", "PDF/X-1a"]:
    if schema in schemas:
        report = document.validate(schema)
        print(f"{schema}: {'✓' if report.is_valid() else '✗'}")
```

### Plugin Management

```python
# Create plugin manager
manager = pdf_ast.PluginManager()

# List available plugins
plugins = manager.list_plugins()
for plugin in plugins:
    print(f"Plugin: {plugin['name']} v{plugin['version']}")
    print(f"  Description: {plugin['description']}")
    print(f"  Author: {plugin['author']}")
    print(f"  Tags: {', '.join(plugin['tags'])}")

# Execute plugins on document
result = manager.execute_plugins(document)
print(f"Executed {result['total_plugins']} plugins")
print(f"Success: {result['successful_plugins']}")
print(f"Failed: {result['failed_plugins']}")
print(f"Execution time: {result['execution_time_ms']}ms")
```

## API Reference

### Classes

#### `PdfDocument`
- `from_bytes(data: bytes) -> PdfDocument`: Parse PDF from bytes
- `get_version() -> tuple[int, int]`: Get PDF version
- `get_all_nodes() -> list[AstNode]`: Get all nodes
- `get_root() -> AstNode | None`: Get root node
- `get_node(node_id: int) -> AstNode | None`: Get node by ID
- `get_children(node_id: int) -> list[AstNode]`: Get child nodes
- `get_nodes_by_type(node_type: str) -> list[AstNode]`: Get nodes by type
- `validate(schema_name: str) -> ValidationReport`: Validate document
- `get_statistics() -> dict`: Get document statistics

#### `AstNode`
- `get_id() -> int`: Get node ID
- `get_type() -> str`: Get node type
- `get_value() -> str`: Get node value representation
- `get_metadata() -> dict | None`: Get node metadata
- `has_property(key: str) -> bool`: Check if node has property
- `get_property(key: str) -> str | None`: Get property value

#### `ValidationReport`
- `is_valid() -> bool`: Check if document is valid
- `get_schema_name() -> str`: Get schema name
- `get_schema_version() -> str`: Get schema version
- `get_issues() -> list[ValidationIssue]`: Get validation issues
- `get_statistics() -> dict`: Get validation statistics

#### `ValidationIssue`
- `get_severity() -> str`: Get issue severity
- `get_code() -> str`: Get error code
- `get_message() -> str`: Get error message
- `get_node_id() -> int | None`: Get associated node ID
- `get_location() -> str | None`: Get error location
- `get_suggestion() -> str | None`: Get fix suggestion

#### `PluginManager`
- `load_plugins_from_file(path: str) -> list[str]`: Load plugins from file
- `execute_plugins(document: PdfDocument) -> dict`: Execute plugins
- `list_plugins() -> list[dict]`: List available plugins

### Functions

- `parse_pdf(data: bytes) -> PdfDocument`: Parse PDF document
- `get_available_schemas() -> list[str]`: Get available validation schemas
- `validate_document(document: PdfDocument, schema: str) -> ValidationReport`: Validate document

## Node Types

The library supports all standard PDF node types:

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

- Fast parsing using native Rust implementation
- Optional parallel processing for large documents
- Memory-efficient AST representation
- Lazy loading of document components

## Error Handling

```python
try:
    document = pdf_ast.parse_pdf(pdf_data)
except ValueError as e:
    print(f"Failed to parse PDF: {e}")

try:
    report = document.validate("PDF/A-1b")
except ValueError as e:
    print(f"Unknown schema: {e}")
```

## License

This project is licensed under the MIT License.
