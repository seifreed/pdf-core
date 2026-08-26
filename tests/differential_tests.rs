use pdf_ast::parser::PdfParser;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use std::process::Command;

#[derive(serde::Deserialize)]
struct CorpusManifest {
    files: Vec<CorpusFile>,
}

#[derive(serde::Deserialize)]
struct CorpusFile {
    file: String,
    sha256: String,
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
        eprintln!("Skipping differential test: qpdf and mutool are required");
        return;
    }

    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest: CorpusManifest = serde_json::from_str(
        &fs::read_to_string(root.join("pdfs/CORPUS.json")).expect("corpus manifest exists"),
    )
    .expect("corpus manifest is valid");
    let mut checked = 0;

    for entry in manifest.files {
        let path = root.join("pdfs").join(&entry.file);
        let bytes = fs::read(&path).expect("corpus PDF is readable");
        let digest = format!("{:x}", Sha256::digest(&bytes));
        assert_eq!(digest, entry.sha256, "corpus file changed: {}", entry.file);
        assert!(
            PdfParser::strict().parse_bytes(&bytes).is_ok(),
            "pdf-core rejected {} in strict mode",
            entry.file
        );
        assert!(
            Command::new("qpdf")
                .args(["--check", path.to_str().unwrap()])
                .status()
                .expect("qpdf should run")
                .success(),
            "qpdf rejected {}",
            entry.file
        );
        assert!(
            Command::new("mutool")
                .args(["info", path.to_str().unwrap()])
                .status()
                .expect("mutool should run")
                .success(),
            "MuPDF rejected {}",
            entry.file
        );
        checked += 1;
    }

    assert!(checked > 0, "differential test checked no corpus files");
}
