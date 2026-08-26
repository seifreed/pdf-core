use pdf_ast::parser::PdfParser;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Instant;

#[derive(serde::Deserialize)]
struct CorpusManifest {
    files: Vec<CorpusFile>,
}

#[derive(serde::Deserialize)]
struct CorpusFile {
    file: String,
    sha256: String,
}

fn collect_pdfs(path: &Path, files: &mut Vec<std::path::PathBuf>) {
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

fn tool_available(tool: &str) -> bool {
    Command::new(tool)
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

#[test]
fn corpus_acceptance_matches_reference_parsers() {
    if !tool_available("qpdf") || !tool_available("mutool") {
        if std::env::var_os("CI").is_some() {
            panic!("qpdf and mutool are required in CI for differential testing");
        }
        eprintln!("Skipping differential test: qpdf and mutool are required");
        return;
    }

    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let files: Vec<(std::path::PathBuf, Option<String>)> =
        if let Some(external_root) = std::env::var_os("PDF_EXTERNAL_CORPUS") {
            let mut files = Vec::new();
            collect_pdfs(Path::new(&external_root), &mut files);
            files.sort();
            let max_files = std::env::var("PDF_EXTERNAL_MAX_FILES")
                .ok()
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(1000);
            files.truncate(max_files);
            files.into_iter().map(|path| (path, None)).collect()
        } else {
            let manifest: CorpusManifest = serde_json::from_str(
                &fs::read_to_string(root.join("pdfs/CORPUS.json")).expect("corpus manifest exists"),
            )
            .expect("corpus manifest is valid");
            manifest
                .files
                .into_iter()
                .map(|entry| (root.join("pdfs").join(entry.file), Some(entry.sha256)))
                .collect()
        };
    assert!(
        !files.is_empty(),
        "differential corpus contains no PDF files"
    );
    let mut checked = 0;
    let started = Instant::now();
    let mut total_bytes = 0u64;
    let mut core_accepted = 0;
    let mut qpdf_accepted = 0;
    let mut mutool_accepted = 0;

    let mut divergences = 0;
    for (path, expected_sha256) in files {
        let bytes = fs::read(&path).expect("corpus PDF is readable");
        total_bytes += bytes.len() as u64;
        if let Some(expected_sha256) = expected_sha256 {
            let digest = format!("{:x}", Sha256::digest(&bytes));
            assert_eq!(
                digest,
                expected_sha256,
                "corpus file changed: {}",
                path.display()
            );
        }
        let core_accepts = PdfParser::strict().parse_bytes(&bytes).is_ok();
        let qpdf_accepts = Command::new("qpdf")
            .arg("--check")
            .arg(&path)
            .status()
            .expect("qpdf should run")
            .success();
        let mutool_accepts = Command::new("mutool")
            .arg("info")
            .arg(&path)
            .status()
            .expect("mutool should run")
            .success();
        core_accepted += core_accepts as usize;
        qpdf_accepted += qpdf_accepts as usize;
        mutool_accepted += mutool_accepts as usize;
        if core_accepts != qpdf_accepts || core_accepts != mutool_accepts {
            divergences += 1;
            eprintln!(
                "differential divergence: file={}, pdf_core={}, qpdf={}, mutool={}",
                path.display(),
                core_accepts,
                qpdf_accepts,
                mutool_accepts
            );
        }
        checked += 1;
    }

    assert!(checked > 0, "differential test checked no corpus files");
    assert_eq!(
        divergences, 0,
        "differential testing found {divergences} parser divergences"
    );
    eprintln!(
        "differential metrics: files={}, bytes={}, pdf_core_accepts={}, qpdf_accepts={}, mutool_accepts={}, divergences={}, wall_ms={}",
        checked,
        total_bytes,
        core_accepted,
        qpdf_accepted,
        mutool_accepted,
        divergences,
        started.elapsed().as_millis()
    );
}
