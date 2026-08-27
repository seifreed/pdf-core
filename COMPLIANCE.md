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
[`RULE-MAPPINGS.json`](https://github.com/seifreed/pdf-core-corpus/blob/e9f4b49f9ad8825883b9b5fe92e38821865940eb)
and covers 205 negative fixtures with 95 distinct veraPDF rule IDs. The
`RULE-COVERAGE.json` records nine positive/negative pairs for the published
local rules, with eight exact veraPDF rule IDs. The positive evidence is
document-level (`compliant=true`); veraPDF emits no passing rule summaries, so
positive rule-level results remain unrecorded for the 95 mapped IDs.
[`RULE-COVERAGE.json`](https://github.com/seifreed/pdf-core-corpus/blob/b3e6f34/RULE-COVERAGE.json)
contains the pair definitions and this limitation explicitly.
clause-level ISO 32000 inventory is maintained in
[ISO-32000-MATRIX.md](ISO-32000-MATRIX.md). It records implementation scope
and known boundaries for both PDF 1.7 and PDF 2.0; it is not a conformance
certificate. Registry-wide validation now gates PDF/A-1 to PDF 1.4 and earlier,
PDF/A-2/3 to PDF 1.7, and the PDF 2.0 schema to PDF 2.0 documents.
PDF/X profiles are likewise gated to their declared PDF 1.3 or 1.6 base
version, and PDF/UA-1/2 are gated to PDF 1.x up to 1.7 and PDF 2.0
respectively.
The direct registry entry point returns no report for an unknown or
version-incompatible profile, matching `validate_all`.

## Feature Matrix

| Standard area | Rule or feature | Status | Evidence |
|---|---|---|---|
| ISO 32000-1/2 | Basic objects, arrays, dictionaries | Implemented | Parser and parser tests |
| ISO 32000-1/2 | Classic xref and trailer | Implemented | Budgeted table/trailer parser, residual-entry rejection, xref tests, and corpus gate |
| ISO 32000-1/2 | Xref streams and hybrid xref | Partial | Bounded table/stream decoder; `test_hybrid_xref_table_and_stream` |
| ISO 32000-1/2 | Incremental updates and `/Prev` | Partial | `test_incremental_xref_chain`; no complete conformance suite |
| ISO 32000-1/2 | Object streams | Partial | Checked offsets and bounded decoding |
| ISO 32000-1/2 | Inherited page resources | Partial | `test_page_resources_are_inherited_from_pages_node`; PDF/A Device color detection also follows `Pages` resources in `test_pdfa_color_space_validation_follows_inherited_page_resources` |
| ISO 32000-1/2 | CMap and ToUnicode mappings | Partial | Variable-width code spaces, bfrange, and UTF-16BE tests |
| ISO 19005-1:2005 | 6.3.4 embedded font programs | Preflight | `PDF_A_FONT_EMBEDDING` |
| ISO 19005-1:2005 | 6.5.2 annotation types and 6.6.1 actions | Preflight | `PDF_A_MULTIMEDIA`, `PDF_A_JAVASCRIPT` |
| ISO 14289-1:2014 / ISO 14289-2 | 7.1 structure tree | Preflight | `NO_TAGGED_STRUCTURE`, `STRUCT_ELEM_MISSING` |
| ISO 14289-1:2014 | 7.2 document language | Preflight | `LANG_MISSING`, `LANG_EMPTY` |
| ISO 19005 / ISO 14289 | Full profile conformance | Not implemented | Requires rule-complete validation and veraPDF comparison |
| ISO 32000-1/2 | JBIG2 pixel decoding | Partial | Bounded pure-Rust embedded/standalone decode with direct and parser-resolved indirect `/JBIG2Globals`; codec coverage remains incomplete |
| ISO 32000-1/2 | JPX pixel decoding | Partial | Bounded pure-Rust JPEG 2000 decode; parser and codec edge coverage remains incomplete |

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
| `ACCESSIBILITY_METADATA_MISSING` | PDF/UA-1, ISO 14289-1:2014 7.1 | `7.1-t08-pass-a.pdf` | `7.1-t08-fail-a.pdf` | `ISO 14289-1:2014:7.1:8` |
| `METADATA_STREAM_INVALID` | PDF/UA-1, ISO 14289-1:2014 7.1 | `fixture_pdfua_metadata_rule_has_positive_and_negative_cases` with `/Type /Metadata` and `/Subtype /XML` | Same test with missing `/Type` | Same clause; local structural refinement |
| `LANG_MISSING` | PDF/UA-1, ISO 14289-1:2014 7.2 | `fixture_pdfua_language_rule_has_positive_and_negative_cases` with `en-US` | Same test without `Lang` | Aggregate local check; isolated upstream evidence: `ISO 14289-1:2014:7.2:2` (`7.2-t02-fail-a.pdf`) |
| `LANG_EMPTY` | PDF/UA-1, ISO 14289-1:2014 7.2 | Same test with `en-US` | Same test with an empty `Lang` string | Aggregate local check; isolated upstream evidence: `ISO 14289-1:2014:7.2:29` (`7.2-t29-fail-n.pdf`) |
| `ALT_TEXT_MISSING` | PDF/UA-1, ISO 14289-1:2014 7.3 | `7.3-t01-pass-a.pdf` | `7.3-t01-fail-a.pdf` | `ISO 14289-1:2014:7.3:1` |

The corpus also contains serialized Isartor negatives for the three PDF/A
clauses above and upstream PDF/UA cases for tagged structure and document
language, and alternative-text cases. `tests/verapdf_rule_mapping_tests.rs`
rechecks the exact veraPDF IDs when `VERAPDF_BIN` and
`PDF_COMPLIANCE_CORPUS` are configured: PDF/A `6.3.4:1`, `6.5.2:1`, and
`6.6.1:1`, plus PDF/UA `7.1:8`, `7.1:11`, `7.2:2`, `7.2:29`, and `7.3:1`; the local
PDF/A and PDF/UA validators are then run against those serialized bytes.
The complete manifest check also passed locally with veraPDF 1.30.2: all 205
Isartor mappings were present and every expected failed rule ID was observed.
The `pdf-compliance` adapter exposes both PDF/UA-1 and PDF/UA-2; both remain
experimental preflight profiles.

The latest local parser gate (2026-08-27 UTC, corpus commit
`e9f4b49f9ad8825883b9b5fe92e38821865940eb`) processed 2,809 files and
161,227,640 bytes without a panic. It recorded 1,898 controlled parse errors,
`peak_rss_kib=310176`, and p50/p95/p99 latencies of 0/9/55 ms. Reproduce it
with `PDF_EXTERNAL_CORPUS=/path/to/pdf-core-corpus/fixtures PDF_EXTERNAL_MAX_FILES=3000 cargo test --test external_corpus_tests --locked -- --test-threads=1 --nocapture`.

The same corpus was run through the tolerant parser and the installed qpdf and
MuPDF reference tools. It processed all 2,809 files and recorded 2,809,
2,419, and 2,801 accepted files respectively, with 397 observed differences;
396 were disagreements between the reference tools and one was a consensus
difference against both. The latest local run took 209,665 ms with
`peak_rss_kib=460112` and p50/p95/p99 latencies of 28/79/182 ms. These
differences are diagnostic evidence, not a conformance claim: the corpus
intentionally includes malformed files and the tools use different recovery
policies. Reproduce it with
`PDF_EXTERNAL_CORPUS=/path/to/pdf-core-corpus/fixtures PDF_EXTERNAL_MAX_FILES=3000 cargo test --test differential_tests --locked -- --test-threads=1 --nocapture`.

The matching local veraPDF 1.30.2 comparison (same date and corpus revision)
checked all 569 PDF/A-1b fixtures: 263 were PDF/A-1b conformant, 6 were
rejected by the strict parser, and the tolerant parser had zero acceptance
divergences. Reproduce it with
`PDF_VERAPDF_CORPUS=/path/to/pdf-core-corpus/fixtures/verapdf-pdfa-1b cargo test --test verapdf_tests --locked -- --test-threads=1`.

## Required Before Conformance Claims

- The local positive/negative fixture branch is covered by the matrix above;
  nine serialized upstream positive/negative pairs are verified, but
  positive rule-level results remain outstanding for the 95 mapped veraPDF
  rule IDs because passing summaries are not emitted.
- Run serialized versions of the same fixtures through a pinned veraPDF
  release and record pass/fail/divergence results per veraPDF rule ID.
- Use `VERAPDF_BIN=/path/to/verapdf cargo test --test verapdf_tests` to run the
  optional acceptance comparison against the checked-in corpus.
- Publish the exact profile, rule coverage, parser mode, and corpus revision
  with every report.
