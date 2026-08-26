# ISO 32000 Coverage Matrix

This is an implementation inventory, not a conformance statement. `PDF 1.7`
means ISO 32000-1:2008 and `PDF 2.0` means ISO 32000-2:2020. The two editions
share the main clause numbering, while PDF 2.0 adds and changes requirements
inside several clauses. A row marked `partial` must not be read as support for
the complete clause.

Statuses:

- `implemented`: the repository has an implementation and focused tests for
  the listed scope, but this is not a complete standards claim.
- `partial`: only the named subset is implemented or tested.
- `pass-through`: the parser preserves or exposes data without interpreting it.
- `unsupported`: the input is rejected or reported as unsupported.
- `not-applicable`: the clause is normative for processors or writers rather
  than the structural parser scope.

The matrix is maintained against the public PDF Association clause index and
errata for PDF 2.0, together with the ISO 32000-1 reference PDF:

- <https://pdf-issues.pdfa.org/32000-2-2020/>
- <https://pdfa.org/how-to-get-started-with-pdf-2-0/>
- <https://developer.adobe.com/document-services/docs/assets/35e4369068f86065372c18787171a17e/PDF_ISO_32000-1.pdf>

## Main Clauses

| Clause | Area | Status | Current evidence and boundary |
|---|---|---|---|
| 1 | Scope | not-applicable | Defines the standard scope; it is not a runtime parser feature. |
| 2 | Normative references | not-applicable | Reference material only; no runtime behavior. |
| 3 | Terms and definitions | not-applicable | Used as specification vocabulary. |
| 4 | Notation | not-applicable | No processor feature. |
| 5 | Version designations | implemented | `PdfVersion`, header parsing, and version tests. |
| 6 | Conformance | partial | Experimental PDF/A, PDF/UA, and PDF 2.0 preflight schemas; no complete conformance validator. |
| 7.1 | Syntax, general | partial | Bounded lexer/object parser with strict and tolerant modes; recovery is observable. |
| 7.2 | Lexical conventions | implemented | Names, numbers, strings, comments, whitespace, and delimiters are parsed by `parser/lexer.rs`. |
| 7.3 | Objects | implemented | Null, booleans, numbers, strings, names, arrays, dictionaries, streams, and references. |
| 7.4 | Filters | partial | Flate, ASCII, LZW, RunLength, CCITT, DCT, and bounded JPX inspection; JBIG2 decoding is unsupported. |
| 7.5 | File structure | partial | Classic xref, xref streams, trailers, object streams, incremental `/Prev`, and bounded recovery; hybrid revisions lack complete conformance coverage. |
| 7.6 | Encryption | partial | Standard security parsing and optional crypto paths; not every PDF 2.0 security extension is supported. |
| 7.7 | Document structure | partial | Catalog, page tree, names, outlines, forms, embedded files, and signatures are represented; inheritance and edge cases remain incomplete. |
| 7.8 | Content streams and resources | partial | Operators and resource nodes are exposed; full graphics semantics and inherited-resource equivalence are not proven. |
| 7.9 | Common data structures | partial | Dictionaries, arrays, rectangles, matrices, streams, and dates are represented; malformed-value recovery is mode-dependent. |
| 7.10 | Functions | partial | Function types are parsed and annotated; complete execution and all limits are not guaranteed. |
| 7.11 | File specifications | partial | Embedded files and file specifications are inspected; complete relationship and portability rules are absent. |
| 7.12 | Extensions dictionary | partial | Extension dictionaries are preserved when parsed; extension-specific semantics are not generally implemented. |
| 8.1 | Graphics, general | partial | Graphics-related AST nodes and content operators are exposed; there is no renderer. |
| 8.2 | Graphics objects | partial | Paths, images, forms, shadings, patterns, and graphics operators are recognized. |
| 8.3 | Coordinate systems | partial | Matrix and coordinate operands are preserved; no complete device rendering model. |
| 8.4 | Graphics state | partial | ExtGState dictionaries and common parameters are annotated. |
| 8.5 | Path construction and painting | partial | Content operators are parsed; painting semantics are not fully evaluated. |
| 8.6 | Colour spaces | partial | Device, calibrated, ICCBased, indexed, separation, and DeviceN structures are inspected; inheritance and all profiles remain partial. |
| 8.7 | Patterns | partial | Pattern nodes and dictionaries are recognized; full tiling/shading evaluation is absent. |
| 8.8 | External objects | partial | XObject and Form XObject structures are represented; complete resource resolution is not guaranteed. |
| 8.9 | Images | partial | Image dictionaries and DCT/JPX/CCITT paths are inspected; codec coverage is incomplete. |
| 8.10 | Form XObjects | partial | Form XObject nodes and resources are represented; full graphics execution is absent. |
| 8.11 | Optional content | partial | OCG/OCProperties/OCMD structures are parsed and annotated. |
| 9.1 | Text, general | partial | Text operators and extraction paths exist; layout and shaping are not complete. |
| 9.2 | Organisation and use of fonts | partial | Font nodes, embedding checks, encodings, and descendant fonts are inspected; metrics and edge cases remain partial. |
| 9.3 | Text state parameters and operators | partial | Operators and operands are parsed; no complete text rendering model. |
| 9.4 | Text objects | partial | BT/ET and text-show operators are parsed and retained. |
| 9.5 | Font data structures | partial | Type 1, TrueType, Type 3, CID, CMap, and ToUnicode nodes are exposed; semantic validation is incomplete. |
| 9.6 | Simple fonts | partial | Simple font dictionaries and encodings are inspected. |
| 9.7 | Composite fonts | partial | Type 0/CID structures and descendant relationships are inspected; complete CMap and font metric validation is pending. |
| 9.8 | Font descriptors | partial | Font descriptor dictionaries and embedding metadata are inspected; complete glyph, metric, and encoding validation is pending. |
| 9.9 | Embedded font programs | partial | Embedded program presence is checked; complete glyph and program validation is pending. |
| 9.10 | Extraction of text content | partial | Text extraction supports common encodings and CMaps; it is not a full Unicode shaping engine. |
| 10 | Rendering | pass-through | Rendering operators and resources are parsed, but no renderer or pixel-equivalence claim exists. |
| 11 | Transparency | partial | Transparency groups, ExtGState, and related dictionaries are recognized; full compositing is absent. |
| 12.1 | Interactive features, general | partial | Actions, annotations, forms, outlines, and destinations are represented. |
| 12.2 | Viewer preferences | partial | Catalog preferences are preserved when encountered; processor behavior is not implemented. |
| 12.3 | Document-level navigation | partial | Catalog-level outlines, destinations, and name trees are represented. |
| 12.4 | Page-level navigation | partial | Page destinations and links are represented; complete navigation behavior is not implemented. |
| 12.5 | Annotations | partial | Common annotation dictionaries and selected restrictions are inspected; complete annotation conformance is pending. |
| 12.6 | Actions | partial | JavaScript, URI, launch, submit, and related action nodes are detected; all action semantics are not implemented. |
| 12.7 | Forms | partial | AcroForm fields, XFA packets, and hybrid forms are inspected; full calculation/appearance behavior is absent. |
| 12.8 | Digital signatures | partial | Signature dictionaries and CMS structures are parsed; verification is optional and not a complete processor implementation. |
| 12.9 | Measurement properties | partial | Measurement dictionaries are preserved when encountered; complete geospatial and measurement validation is pending. |
| 12.10 | Geospatial features | partial | Geospatial dictionaries are preserved when encountered; no coordinate transformation engine is implemented. |
| 12.11 | Document requirements | partial | Requirement dictionaries are preserved when encountered; requirement enforcement is not implemented. |
| 13.1 | Multimedia, general | partial | Audio, video, 3D, and RichMedia structures are detected and annotated. |
| 13.2 | Multimedia data | partial | Media dictionaries are preserved; playback and complete media validation are absent. |
| 13.3 | Sounds | partial | Sound annotations are detected. |
| 13.4 | Movies | partial | Movie annotations are detected. |
| 13.5 | Alternate presentations | partial | Related dictionaries are pass-through only. |
| 13.6 | 3D artwork | partial | U3D/PRC/3D nodes and metadata are inspected; no 3D renderer. |
| 13.7 | Rich media | partial | RichMedia annotations, assets, and scripts are inspected. |
| 14.1 | Document interchange, general | partial | Document-level metadata and structure are exposed. |
| 14.2 | Procedure sets | pass-through | Preserved as PDF values when encountered; not executed. |
| 14.3 | Metadata | partial | XMP and document information metadata are parsed; full synchronization/conformance rules are pending. |
| 14.4 | File identifiers | partial | Trailer identifiers are parsed; complete generation and conformance checks are pending. |
| 14.5 | Page-piece dictionaries | partial | Page-piece structures are preserved when recognized. |
| 14.6 | Marked content | partial | Marked-content operators and properties are parsed; full association validation is pending. |
| 14.7 | Logical structure | partial | Structure tree and structure elements are represented; complete parent/child validation is pending. |
| 14.8 | Tagged PDF | partial | PDF/UA preflight checks cover selected structure, language, and alternative-text rules only. |
| 14.9 | Repurposing and accessibility support | partial | Text extraction and accessibility findings exist; no complete reading-order or assistive-technology model. |
| 14.10 | Web capture | pass-through | Data is preserved when parsed; no web-capture processor. |
| 14.11 | Prepress support | partial | Output intents and selected ICC metadata are inspected; complete prepress validation is absent. |
| 14.12 | Document parts | partial | Revision and document-part-related structures are represented where supported. |
| 14.13 | Associated files | partial | Embedded/associated file structures are detected; complete relationship validation is pending. |

## Annexes

| Annex | Area | Status | Boundary |
|---|---|---|---|
| A | Operator summary | partial | Operators are parsed, not rendered or fully semantically executed. |
| B | Type 4 functions | partial | Function dictionaries are parsed; complete execution is not guaranteed. |
| C | Portability advice | not-applicable | Guidance rather than parser behavior. |
| D | Character sets and encodings | partial | Common encodings and CMaps are handled; full coverage is pending. |
| E | Extending PDF | partial | Extension dictionaries are preserved; extension semantics are not general. |
| F | Linearized PDF | partial | Linearization dictionaries are parsed and bounded; complete access-strategy validation is pending. |
| G | Linearized access strategies | not-applicable | No streaming renderer/access planner claim. |
| H | Example PDF files | not-applicable | Examples are test inputs, not implementation requirements. |
| I | PDF versions and compatibility | partial | Version headers are parsed; full cross-version conformance is not implemented. |
| J | XObject comparison | not-applicable | No renderer comparison. |
| K | XFA forms | partial | XFA XML packets and scripts are inspected; full XFA execution is unsupported. |
| L | PDF 2.0 structure namespace | partial | Structure nodes are exposed; complete parent-child namespace validation is pending. |
| M | Structure namespace differences | not-applicable | Reference material; no automatic migration claim. |
| N | Halftones | pass-through | Halftone dictionaries are preserved where parsed; no renderer. |
| O | Fragment identifiers | unsupported | No fragment-identifier resolver. |
| P | Actual blending colour space | partial | Transparency/colour metadata are exposed; no complete blending calculation. |
| Q | Page transparency method | partial | Transparency features are detected; no complete page-level rendering calculation. |

## Reproducible Evidence

The parser tests, corpus tests, and differential tests are the executable
evidence for the statuses above. The pinned external corpus is maintained at
<https://github.com/seifreed/pdf-core-corpus>. A green test does not upgrade a
`partial` row to `implemented`; that requires clause-specific positive and
negative fixtures, a documented parser mode, and comparison against the
relevant reference behavior.
