# Compliance Scope

The validators in this repository are experimental preflight checks. They are
not conformance claims. A report with no findings means only that the
implemented checks did not find a violation.

The `pdf-compliance` workspace crate is an adapter over the root
`pdf_ast::validation::SchemaRegistry`; it does not maintain a second rule
implementation. `validate_profile` exposes the registered experimental
profiles while preserving their rule identifiers, severity, expected action,
node ID, and source offset when the AST provides them. Registry reports also
include `iso.<constraint>` metadata for constraints that declare a normative
reference. The reproducible fixture corpus is maintained at
https://github.com/seifreed/pdf-core-corpus and pinned by the corpus workflow.

## Feature Matrix

| Standard area | Rule or feature | Status | Evidence |
|---|---|---|---|
| ISO 32000-1/2 | Basic objects, arrays, dictionaries | Implemented | Parser and parser tests |
| ISO 32000-1/2 | Classic xref and trailer | Implemented | Xref tests and corpus gate |
| ISO 32000-1/2 | Xref streams | Partial | Bounded decoder; limited corpus |
| ISO 32000-1/2 | Incremental updates and `/Prev` | Partial | Revision tests; no complete conformance suite |
| ISO 32000-1/2 | Object streams | Partial | Checked offsets and bounded decoding |
| ISO 19005-1:2005 | 6.3.5 font embedding | Preflight | `PDF_A_FONT_EMBEDDING` |
| ISO 19005-1:2005 | 6.6 interactive content | Preflight | `PDF_A_MULTIMEDIA`, `PDF_A_JAVASCRIPT` |
| ISO 14289-1:2014 | 7.1 structure tree | Preflight | `NO_TAGGED_STRUCTURE`, `STRUCT_ELEM_MISSING` |
| ISO 14289-1:2014 | 7.2 document language | Preflight | `LANG_MISSING`, `LANG_EMPTY` |
| ISO 19005 / ISO 14289 | Full profile conformance | Not implemented | Requires rule-complete validation and veraPDF comparison |
| ISO 32000-1/2 | JBIG2 pixel decoding | Unsupported | Raw stream inspection only |
| ISO 32000-1/2 | JPX pixel decoding | Unsupported | JP2 container/codestream inspection only |

## Required Before Conformance Claims

- Add positive and negative fixtures for every published rule.
- Run the same fixtures through a pinned veraPDF release and record
  pass/fail/divergence results.
- Use `VERAPDF_BIN=/path/to/verapdf cargo test --test verapdf_tests` to run the
  optional acceptance comparison against the checked-in corpus.
- Publish the exact profile, rule coverage, parser mode, and corpus revision
  with every report.
