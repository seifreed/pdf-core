This directory contains the checked-in regression corpus for pdf-core.

To fetch the pinned veraPDF PDF/A-1b corpus without adding external binaries to
Git, run:

    tools/fetch-verapdf-corpus.sh PDF_A-1b .external-corpus/verapdf
    PDF_EXTERNAL_CORPUS=.external-corpus/verapdf cargo test --test external_corpus_tests

The fetcher records the source commit in `SOURCE.json` and writes SHA-256
checksums to `SHA256SUMS`. The external campaign is also scheduled in CI.
