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
https://github.com/seifreed/pdf-core-corpus and pinned by the corpus workflow;
the corpus contains 2,806 upstream fixtures across PDF/A, PDF/UA, and Isartor
profiles,
plus three local parser regressions. The PDF/A-1b comparison gate uses its 569
profile fixtures; the other profiles are available for parser and preflight
campaigns.

The complete serialized Isartor-to-veraPDF mapping is maintained in
[`RULE-MAPPINGS.json`](https://github.com/seifreed/pdf-core-corpus/blob/bb9d353181797cf61e81294c071737a09f155d6d/RULE-MAPPINGS.json)
and covers 205 negative fixtures with 95 distinct veraPDF rule IDs. The
clause-level ISO 32000 inventory is maintained in
[ISO-32000-MATRIX.md](ISO-32000-MATRIX.md). It records implementation scope
and known boundaries for both PDF 1.7 and PDF 2.0; it is not a conformance
certificate.

## Feature Matrix

| Standard area | Rule or feature | Status | Evidence |
|---|---|---|---|
| ISO 32000-1/2 | Basic objects, arrays, dictionaries | Implemented | Parser and parser tests |
| ISO 32000-1/2 | Classic xref and trailer | Implemented | Xref tests and corpus gate |
| ISO 32000-1/2 | Xref streams and hybrid xref | Partial | Bounded decoder; `test_hybrid_xref_table_and_stream` |
| ISO 32000-1/2 | Incremental updates and `/Prev` | Partial | `test_incremental_xref_chain`; no complete conformance suite |
| ISO 32000-1/2 | Object streams | Partial | Checked offsets and bounded decoding |
| ISO 32000-1/2 | Inherited page resources | Partial | `test_page_resources_are_inherited_from_pages_node`; category merge with child override |
| ISO 32000-1/2 | CMap and ToUnicode mappings | Partial | Variable-width code spaces, bfrange, and UTF-16BE tests |
| ISO 19005-1:2005 | 6.3.4 embedded font programs | Preflight | `PDF_A_FONT_EMBEDDING` |
| ISO 19005-1:2005 | 6.5.2 annotation types and 6.6.1 actions | Preflight | `PDF_A_MULTIMEDIA`, `PDF_A_JAVASCRIPT` |
| ISO 14289-1:2014 | 7.1 structure tree | Preflight | `NO_TAGGED_STRUCTURE`, `STRUCT_ELEM_MISSING` |
| ISO 14289-1:2014 | 7.2 document language | Preflight | `LANG_MISSING`, `LANG_EMPTY` |
| ISO 19005 / ISO 14289 | Full profile conformance | Not implemented | Requires rule-complete validation and veraPDF comparison |
| ISO 32000-1/2 | JBIG2 pixel decoding | Unsupported | Raw stream inspection only |
| ISO 32000-1/2 | JPX pixel decoding | Unsupported | JP2 container/codestream inspection only |

## Published Rule Matrix

The local identifiers below are preflight rules, not replacements for the
veraPDF validation model. Synthetic AST fixtures in
`tests/validation_tests.rs` prove both branches of each local rule; serialized
upstream fixtures in `tests/verapdf_rule_mapping_tests.rs` add parser and
corpus coverage for the PDF/A and PDF/UA cases listed below.

| Local rule | Profile and ISO clause | Positive fixture | Negative fixture | veraPDF rule mapping |
|---|---|---|---|---|
| `PDF_A_FONT_EMBEDDING` | PDF/A-1b, ISO 19005-1:2005 6.3.4 | `test_pdfa_font_validation` with embedded font | Same test with missing `FontFile` | `ISO_19005_1:6.3.4:1` |
| `PDF_A_MULTIMEDIA` | PDF/A-1b, ISO 19005-1:2005 6.5.2 | `fixture_pdfa_multimedia_rule_has_positive_and_negative_cases` clean document | Same test with `Movie` annotation | `ISO_19005_1:6.5.2:1` |
| `PDF_A_JAVASCRIPT` | PDF/A-1b, ISO 19005-1:2005 6.6.1 | `test_pdfa_javascript_validation` clean document | Same test with JavaScript action | `ISO_19005_1:6.6.1:1` |
| `NO_TAGGED_STRUCTURE` | PDF/UA-1, ISO 14289-1:2014 7.1 | `fixture_pdfua_structure_rule_has_positive_and_negative_cases` marked catalog with `StructTreeRoot` | Same test with untagged catalog | `ISO_14289_1:7.1:11` |
| `STRUCT_ELEM_MISSING` | PDF/UA-1, ISO 14289-1:2014 7.1 | Same test with a `StructElem` | Same test with `StructTreeRoot` but no `StructElem` | Aggregate of `ISO_14289_1:7.1:*`; no 1:1 veraPDF rule |
| `LANG_MISSING` | PDF/UA-1, ISO 14289-1:2014 7.2 | `fixture_pdfua_language_rule_has_positive_and_negative_cases` with `en-US` | Same test without `Lang` | Aggregate of `ISO_14289_1:7.2:2,21-34`; depends on content object |
| `LANG_EMPTY` | PDF/UA-1, ISO 14289-1:2014 7.2 | Same test with `en-US` | Same test with an empty `Lang` string | Aggregate of `ISO_14289_1:7.2:*`; no 1:1 veraPDF rule |

The corpus also contains serialized Isartor negatives for the three PDF/A
clauses above and upstream PDF/UA cases for tagged structure and document
language. `tests/verapdf_rule_mapping_tests.rs` rechecks the exact veraPDF
IDs when `VERAPDF_BIN` and `PDF_COMPLIANCE_CORPUS` are configured:
`6.3.4:1`, `6.5.2:1`, and `6.6.1:1`; the local PDF/A and PDF/UA validators are
then run against those serialized bytes.

The latest local parser gate (2026-08-26 UTC, corpus commit
`bb9d353181797cf61e81294c071737a09f155d6d`) processed 2,809 files and
161,227,640 bytes without a panic. It recorded 577 controlled parse errors,
`peak_rss_kib=202240`, and p50/p95/p99 latencies of 3/53/129 ms. Reproduce it
with `PDF_EXTERNAL_CORPUS=/path/to/pdf-core-corpus/fixtures PDF_EXTERNAL_MAX_FILES=3000 cargo test --test external_corpus_tests --locked -- --test-threads=1 --nocapture`.

## Required Before Conformance Claims

- The local positive/negative fixture branch is covered by the matrix above;
  serialized upstream positives and negatives cover the current PDF/A and
  PDF/UA preflight rules.
- Run serialized versions of the same fixtures through a pinned veraPDF
  release and record pass/fail/divergence results per veraPDF rule ID.
- Use `VERAPDF_BIN=/path/to/verapdf cargo test --test verapdf_tests` to run the
  optional acceptance comparison against the checked-in corpus.
- Publish the exact profile, rule coverage, parser mode, and corpus revision
  with every report.
