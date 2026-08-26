use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use pdf_ast::parser::PdfParser;
use std::fs;
use std::io::BufReader;
use std::path::{Path, PathBuf};

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("pdfs")
}

fn list_corpus_files(limit: usize) -> Vec<PathBuf> {
    let dir = corpus_dir();
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .expect("pdfs dir should exist")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && path.extension() == Some(std::ffi::OsStr::new("pdf")))
        .collect();
    files.sort();
    files.truncate(limit);
    files
}

fn bench_parse_corpus(c: &mut Criterion) {
    let parser = PdfParser::new();
    let count = std::env::var("PDF_AST_BENCH_COUNT")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(5);
    let files = list_corpus_files(count);

    let mut group = c.benchmark_group("parse_corpus");
    for path in files {
        let label = path.file_name().unwrap().to_string_lossy().to_string();
        group.bench_with_input(BenchmarkId::from_parameter(label), &path, |b, p| {
            b.iter(|| {
                let file = fs::File::open(p).expect("file open");
                let reader = BufReader::new(file);
                let _ = parser.parse(reader).expect("parse");
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_parse_corpus);
criterion_main!(benches);
