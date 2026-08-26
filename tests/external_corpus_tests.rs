use pdf_ast::parser::PdfParser;
use std::fs;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::time::Instant;

fn collect_pdfs(path: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_pdfs(&path, files);
        } else if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf"))
        {
            files.push(path);
        }
    }
}

fn percentile_ms(samples: &mut [u128], percentile: usize) -> u128 {
    if samples.is_empty() {
        return 0;
    }
    samples.sort_unstable();
    let index = (samples.len() - 1) * percentile / 100;
    samples[index]
}

#[test]
fn external_corpus_has_no_parser_panics() {
    let Some(root) = std::env::var_os("PDF_EXTERNAL_CORPUS") else {
        eprintln!("Skipping external corpus: PDF_EXTERNAL_CORPUS is not set");
        return;
    };

    let mut files = Vec::new();
    collect_pdfs(Path::new(&root), &mut files);
    files.sort();
    let max_files = std::env::var("PDF_EXTERNAL_MAX_FILES")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(1000);
    files.truncate(max_files);

    assert!(!files.is_empty(), "external corpus contains no PDF files");

    let parser = PdfParser::new();
    let started = Instant::now();
    let mut total_bytes = 0u64;
    let mut parse_errors = 0usize;
    let mut durations_ms = Vec::with_capacity(files.len());

    for path in &files {
        let file_started = Instant::now();
        let bytes = fs::read(path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
        total_bytes += bytes.len() as u64;
        let result = catch_unwind(AssertUnwindSafe(|| parser.parse_bytes(&bytes)));
        assert!(result.is_ok(), "parser panicked on {}", path.display());
        if result.is_ok_and(|result| result.is_err()) {
            parse_errors += 1;
        }
        durations_ms.push(file_started.elapsed().as_millis());
    }

    let p50_ms = percentile_ms(&mut durations_ms, 50);
    let p95_ms = percentile_ms(&mut durations_ms, 95);
    let p99_ms = percentile_ms(&mut durations_ms, 99);
    eprintln!(
        "external corpus metrics: files={}, bytes={}, parse_errors={}, wall_ms={}, p50_ms={}, p95_ms={}, p99_ms={}",
        files.len(),
        total_bytes,
        parse_errors,
        started.elapsed().as_millis(),
        p50_ms,
        p95_ms,
        p99_ms
    );
}
