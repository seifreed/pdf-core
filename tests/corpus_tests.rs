use pdf_ast::parser::PdfParser;
use pdf_ast::types::PdfValue;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::BufReader;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(serde::Deserialize)]
struct CorpusFile {
    file: String,
    size: u64,
    sha256: String,
}

#[derive(serde::Deserialize)]
struct CorpusManifest {
    corpus_version: String,
    files: Vec<CorpusFile>,
}

fn corpus_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("pdfs")
}

fn load_manifest() -> Option<CorpusManifest> {
    let corpus_dir = corpus_path();
    let manifest_path = corpus_dir.join("CORPUS.json");
    let manifest_data = match fs::read_to_string(&manifest_path) {
        Ok(data) => data,
        Err(err) if err.kind() == ErrorKind::NotFound => return None,
        Err(err) => panic!("Failed to read CORPUS.json: {}", err),
    };
    let manifest: CorpusManifest =
        serde_json::from_str(&manifest_data).expect("CORPUS.json should be valid JSON");
    Some(manifest)
}

#[test]
fn corpus_manifest_matches_files() {
    let corpus_dir = corpus_path();
    let Some(manifest) = load_manifest() else {
        return;
    };

    assert_eq!(manifest.corpus_version, "1.0");
    assert!(
        !manifest.files.is_empty(),
        "Corpus manifest must contain files"
    );

    for entry in manifest.files {
        let file_path = corpus_dir.join(&entry.file);
        let data = fs::read(&file_path).expect("Corpus file should be readable");
        assert_eq!(
            data.len() as u64,
            entry.size,
            "Size mismatch for {}",
            entry.file
        );

        let mut hasher = Sha256::new();
        hasher.update(&data);
        let digest = format!("{:x}", hasher.finalize());
        assert_eq!(digest, entry.sha256, "SHA256 mismatch for {}", entry.file);
        assert_eq!(
            Path::new(&entry.file)
                .file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or_default(),
            entry.sha256,
            "Filename should match sha256 for {}",
            entry.file
        );
    }
}

#[test]
fn corpus_parses_with_tolerant_parser() {
    let corpus_dir = corpus_path();
    let Some(manifest) = load_manifest() else {
        return;
    };

    let parser = PdfParser::new();
    let started = Instant::now();
    let mut total_bytes = 0u64;
    let mut parsed = 0usize;

    for entry in manifest.files {
        let file_path = corpus_dir.join(&entry.file);
        let file =
            fs::File::open(&file_path).unwrap_or_else(|_| panic!("Failed to open {}", entry.file));
        let reader = BufReader::new(file);
        let result = parser.parse(reader);
        assert!(result.is_ok(), "Failed to parse {}", entry.file);
        total_bytes += entry.size;
        parsed += 1;
    }

    eprintln!(
        "corpus metrics: files={}, bytes={}, tolerant_parse_ms={}",
        parsed,
        total_bytes,
        started.elapsed().as_millis()
    );
}

#[test]
fn corpus_streams_decode_with_limits() {
    let corpus_dir = corpus_path();
    let Some(manifest) = load_manifest() else {
        return;
    };

    let parser = PdfParser::new();

    let mut total_attempted = 0usize;
    let mut total_decoded_ok = 0usize;

    for entry in manifest.files {
        let file_path = corpus_dir.join(&entry.file);
        let file =
            fs::File::open(&file_path).unwrap_or_else(|_| panic!("Failed to open {}", entry.file));
        let reader = BufReader::new(file);
        let document = parser
            .parse(reader)
            .unwrap_or_else(|_| panic!("Failed to parse {}", entry.file));
        for node in document.ast.get_all_nodes() {
            if let PdfValue::Stream(stream) = &node.value {
                if stream.is_lazy() {
                    continue;
                }
                let filters = stream.get_filters_with_params();
                if filters.is_empty() {
                    continue;
                }
                total_attempted += 1;
                if stream.decode_with_limits(5 * 1024 * 1024, 50).is_ok() {
                    total_decoded_ok += 1;
                }
            }
        }
    }

    assert!(total_attempted > 0, "No filtered streams found in corpus");
    assert!(
        total_decoded_ok > 0,
        "No filtered streams decoded successfully"
    );
}
