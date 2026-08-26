<p align="center">
  <img src="https://img.shields.io/badge/pdf--core-PDF%20Security%20Analysis-blue?style=for-the-badge" alt="pdf-core">
</p>

<h1 align="center">pdf-core</h1>

<p align="center">
  <strong>Experimental PDF parser and AST for structural, forensic, and security analysis</strong>
</p>

<p align="center">
  <a href="https://github.com/seifreed/pdf-core/blob/main/LICENSE"><img src="https://img.shields.io/badge/license-MIT-green?style=flat-square" alt="License"></a>
  <a href="https://github.com/seifreed/pdf-core/actions"><img src="https://img.shields.io/github/actions/workflow/status/seifreed/pdf-core/ci.yml?style=flat-square&logo=github&label=CI" alt="CI Status"></a>
  <img src="https://img.shields.io/badge/rust-stable-orange?style=flat-square" alt="Rust Stable">
</p>

<p align="center">
  <a href="https://github.com/seifreed/pdf-core/stargazers"><img src="https://img.shields.io/github/stars/seifreed/pdf-core?style=flat-square" alt="GitHub Stars"></a>
  <a href="https://github.com/seifreed/pdf-core/issues"><img src="https://img.shields.io/github/issues/seifreed/pdf-core?style=flat-square" alt="GitHub Issues"></a>
</p>

---

## Overview

**pdf-core** is an experimental Rust library for parsing PDF documents into a rich Abstract Syntax Tree (AST). It targets structural, forensic, and security analysis. The crate name remains `pdf-ast` for compatibility.

### Key Features

| Feature | Description |
|---------|-------------|
| **PDF parser** | Objects, streams, xref tables, linearization, and incremental updates; corpus coverage is limited |
| **Rich AST** | Typed nodes for common PDF structures; schema/API are experimental |
| **Security Analysis** | JavaScript, forms, embedded files, and suspicious actions |
| **PDF/A Validation** | Experimental preflight checks for selected requirements |
| **Stream Decoding** | Flate, LZW, CCITT, DCT, and JPX container inspection; some codecs are partial |
| **XFA + AcroForm** | XML packets, scripts, and field trees |
| **Signature Support** | CMS/PKCS#7 parsing and optional crypto verification; experimental |
| **CLI + Library** | Use from the command line or embed in Rust apps |

---

## Capability Matrix

| Area | Status |
|------|--------|
| Basic objects, arrays, dictionaries | Implemented |
| Classic xref | Implemented |
| Xref streams and incremental updates | Implemented with limited corpus coverage |
| Object streams | Partial, bounded and validated |
| Flate, ASCII85, LZW, RunLength | Implemented |
| CCITT | Experimental |
| JBIG2 | Unsupported; raw stream inspection only |
| JPX | Container inspection, not full JPEG 2000 decoding |
| Text extraction | Experimental |
| PDF/A and PDF/UA | Partial preflight checks, not certifiable |
| Streaming parser | Prototype |
| Python and JavaScript bindings | Experimental |

This project is not production-ready for untrusted PDFs without process
isolation and an application-level resource policy.

---

## Supported Use Cases

- **Threat Intelligence**: detect suspicious actions, embedded scripts, and attachments.
- **Malware Research**: inspect object graphs and content streams for obfuscation.
- **Compliance & Archival**: run experimental PDF/A preflight checks.
- **Forensics**: extract full AST for offline analysis and correlation.
- **Pipeline Integration**: run automated PDF parsing in CI/CD or batch jobs.

---

## Installation

Use the repository as a Git dependency (registry publication is not enabled yet):

```toml
[dependencies]
pdf-ast = { git = "https://github.com/seifreed/pdf-core" }
```

Or with Cargo:

```bash
cargo add --git https://github.com/seifreed/pdf-core pdf-ast
```

### Feature Flags

```toml
[dependencies]
pdf-ast = {
    git = "https://github.com/seifreed/pdf-core",
    features = ["crypto", "parallel", "async"]
}
```

Available features:
- `crypto`: cryptographic support (signatures, encryption, timestamps, OCSP/CRL)
- `parallel`: multi-threading with Rayon
- `async`: async parsing with Tokio
- `python`: Python bindings via PyO3
- `javascript`: Node.js bindings via Neon
- `full`: all features enabled

### OpenSSL (for `crypto`)

The `crypto` feature requires OpenSSL headers and libraries. On Windows, set `OPENSSL_DIR`, `OPENSSL_LIB_DIR`, and `OPENSSL_INCLUDE_DIR` if OpenSSL is not in a standard location.

---

## Quick Start

```bash
# Build CLI tools
cargo build --release

# Parse a PDF into JSON AST
./target/release/pdf-ast-simple parse document.pdf -o output.json

# Analyze security signals
./target/release/pdf-ast-simple analyze document.pdf --detailed
```

---

## CLI Usage

### pdf-ast-simple (Experimental)

```bash
# Parse to AST JSON
pdf-ast-simple parse document.pdf -o output.json

# Security analysis
pdf-ast-simple analyze document.pdf --detailed

# Benchmark parsing
pdf-ast-simple benchmark large-file.pdf -i 10
```

### pdf-ast (Advanced)

```bash
# Parse with full options
pdf-ast parse input.pdf --include-streams --resolve-refs

# PDF/A validation
pdf-ast validate input.pdf --schema pdf-a-1b --strict

# Security analysis
pdf-ast analyze input.pdf --security --metrics

# TSA controls for RFC3161 timestamps
pdf-ast analyze input.pdf --security --tsa-allow-fingerprint <SHA256>
pdf-ast analyze input.pdf --security --disable-tsa-revocation-checks

# Security report output formats
pdf-ast analyze input.pdf --security --format yaml
pdf-ast analyze input.pdf --security --format toml

# Write security report to a file
pdf-ast analyze input.pdf --security --security-report report.json
```

---

## Library Usage

### Basic Parsing

```rust
use pdf_ast::{PdfParser, PdfDocument};
use std::fs::File;

let mut file = File::open("document.pdf")?;
let parser = PdfParser::new();
let document: PdfDocument = parser.parse(&mut file)?;

println!("PDF Version: {}", document.version);
println!("Object Count: {}", document.ast.node_count());
```

### Security Analysis

```rust
use pdf_ast::{PdfParser, security::SecurityAnalyzer};
use std::fs::File;

let mut file = File::open("document.pdf")?;
let parser = PdfParser::new();
let document = parser.parse(&mut file)?;

let analyzer = SecurityAnalyzer::new();
let report = analyzer.analyze(&document);
println!("Security Score: {}/100", report.score);
```

### Working with the AST

```rust
use pdf_ast::ast::{NodeType, EdgeType};

let catalog = document.ast.find_nodes(|node| {
    matches!(node.node_type, NodeType::Catalog)
}).next().unwrap();

for edge in document.ast.edges_from(catalog.id) {
    if edge.edge_type == EdgeType::Reference {
        let target = document.ast.get_node(edge.target).unwrap();
        println!("Catalog references: {:?}", target.node_type);
    }
}
```

---

## Project Structure

```
pdf-core/
├── src/            # Core library
├── tests/          # Test suite
├── examples/       # Usage examples
├── include/        # C header (pdf_ast.h)
└── scripts/        # Utilities and helpers
```

---

## Contributing

Contributions are welcome:

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

---


## Support the Project

If you find pdf-core useful, consider supporting its development:

<a href="https://buymeacoffee.com/seifreed" target="_blank">
  <img src="https://cdn.buymeacoffee.com/buttons/v2/default-yellow.png" alt="Buy Me A Coffee" height="50">
</a>

---

<p align="center">
  <sub>Built for secure PDF analysis and research</sub>
</p>
