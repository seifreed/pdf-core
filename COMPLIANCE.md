# Compliance Scope

The validators in this repository are experimental preflight checks. They are
not conformance claims and have not been compared against veraPDF. A report
with no findings means only that the implemented checks did not find a
violation.

## Feature Matrix

| Standard area | Rule or feature | Status | Evidence |
|---|---|---|---|
| ISO 32000-1/2 | Basic objects, arrays, dictionaries | Implemented | Parser and parser tests |
| ISO 32000-1/2 | Classic xref and trailer | Implemented | Xref tests and corpus gate |
| ISO 32000-1/2 | Xref streams | Partial | Bounded decoder; limited corpus |
| ISO 32000-1/2 | Incremental updates and `/Prev` | Partial | Revision tests; no complete conformance suite |
| ISO 32000-1/2 | Object streams | Partial | Checked offsets and bounded decoding |
| ISO 19005-1:2005 | 6.3.5 font embedding | Preflight | `pdfa-1b.font-embedding` |
| ISO 19005-1:2005 | 6.6 interactive content | Preflight | `pdfa-1b.interactive-content` |
| ISO 14289-1:2014 | 7.1 structure tree | Preflight | `pdfua-1.structure-tree` |
| ISO 14289-1:2014 | 7.2 document language | Preflight | `pdfua-1.language` |
| ISO 19005 / ISO 14289 | Full profile conformance | Not implemented | Requires rule-complete validation and veraPDF comparison |
| ISO 32000-1/2 | JBIG2 pixel decoding | Unsupported | Raw stream inspection only |
| ISO 32000-1/2 | JPX pixel decoding | Unsupported | JP2 container/codestream inspection only |

## Required Before Conformance Claims

- Add positive and negative fixtures for every published rule.
- Run the same fixtures through a pinned veraPDF release and record
  pass/fail/divergence results.
- Publish the exact profile, rule coverage, parser mode, and corpus revision
  with every report.
