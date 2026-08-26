This directory contains the checked-in regression corpus for pdf-core.

To fetch the pinned project corpus without adding the external fixture repo to
this repository, run:

    tools/fetch-verapdf-corpus.sh fixtures .external-corpus/verapdf
    PDF_EXTERNAL_CORPUS=.external-corpus/verapdf cargo test --test external_corpus_tests

The fetcher pins `seifreed/pdf-core-corpus`, records the upstream veraPDF
source commit in `SOURCE.json`, and writes SHA-256 checksums to `SHA256SUMS`.
The external campaign is also scheduled in CI.
