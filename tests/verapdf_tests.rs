use pdf_ast::parser::PdfParser;
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
}

fn verapdf_binary() -> Option<String> {
    std::env::var("VERAPDF_BIN")
        .ok()
        .filter(|path| !path.is_empty())
        .or_else(|| {
            Command::new("verapdf")
                .arg("--version")
                .output()
                .ok()
                .filter(|output| output.status.success())
                .map(|_| "verapdf".to_string())
        })
}

#[test]
fn compare_strict_parser_with_verapdf_when_available() {
    let Some(verapdf) = verapdf_binary() else {
        eprintln!("Skipping veraPDF comparison: set VERAPDF_BIN to enable it");
        return;
    };

    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest: CorpusManifest = serde_json::from_str(
        &fs::read_to_string(root.join("pdfs/CORPUS.json")).expect("corpus manifest exists"),
    )
    .expect("corpus manifest is valid");
    let parser = PdfParser::strict();
    let mut checked = 0;
    let mut divergences = 0;

    for entry in manifest.files {
        let path = root.join("pdfs").join(&entry.file);
        let bytes = fs::read(&path).expect("corpus PDF is readable");
        let core_accepts = parser.parse_bytes(&bytes).is_ok();
        let verapdf_accepts = Command::new(&verapdf)
            .args(["--format", "text", "--flavour", "1b"])
            .arg(&path)
            .status()
            .expect("veraPDF should run")
            .success();

        if core_accepts != verapdf_accepts {
            divergences += 1;
            eprintln!(
                "veraPDF divergence for {}: pdf-core={}, veraPDF={}",
                entry.file, core_accepts, verapdf_accepts
            );
        }
        checked += 1;
    }

    assert!(checked > 0, "veraPDF comparison checked no corpus files");
    eprintln!("veraPDF comparison: {checked} checked, {divergences} divergences");
}
