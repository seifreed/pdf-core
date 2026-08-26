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

#[derive(serde::Deserialize)]
struct VeraPdfReport {
    report: VeraPdfReportBody,
}

#[derive(serde::Deserialize)]
struct VeraPdfReportBody {
    jobs: Vec<VeraPdfJob>,
}

#[derive(serde::Deserialize)]
struct VeraPdfJob {
    #[serde(rename = "validationResult")]
    validation_result: Option<Vec<VeraPdfValidation>>,
}

#[derive(serde::Deserialize)]
struct VeraPdfValidation {
    #[serde(rename = "jobEndStatus")]
    job_end_status: String,
    compliant: bool,
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

#[test]
fn compare_strict_parser_with_verapdf_when_available() {
    let Some(verapdf) = verapdf_binary() else {
        eprintln!("Skipping veraPDF comparison: set VERAPDF_BIN to enable it");
        return;
    };

    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = if let Some(external_root) = std::env::var_os("PDF_EXTERNAL_CORPUS") {
        let mut files = Vec::new();
        collect_pdfs(Path::new(&external_root), &mut files);
        files
    } else {
        let manifest: CorpusManifest = serde_json::from_str(
            &fs::read_to_string(root.join("pdfs/CORPUS.json")).expect("corpus manifest exists"),
        )
        .expect("corpus manifest is valid");
        manifest
            .files
            .into_iter()
            .map(|entry| root.join("pdfs").join(entry.file))
            .collect()
    };
    files.sort();
    if let Ok(max_files) = std::env::var("PDF_EXTERNAL_MAX_FILES") {
        if let Ok(max_files) = max_files.parse::<usize>() {
            files.truncate(max_files);
        }
    }
    let parser = PdfParser::strict();
    let mut checked = 0;
    let mut divergences = 0;
    let mut conformant = 0;

    for path in files {
        let bytes = fs::read(&path).expect("corpus PDF is readable");
        let core_accepts = parser.parse_bytes(&bytes).is_ok();
        let verapdf_output = Command::new(&verapdf)
            .args(["--format", "json", "--flavour", "1b"])
            .arg(&path)
            .output()
            .expect("veraPDF should run");
        let report: VeraPdfReport =
            serde_json::from_slice(&verapdf_output.stdout).unwrap_or_else(|error| {
                panic!(
                    "veraPDF returned invalid JSON for {}: {error}",
                    path.display()
                )
            });
        let validation = report
            .report
            .jobs
            .first()
            .and_then(|job| job.validation_result.as_ref())
            .and_then(|results| results.first());
        let verapdf_parsed =
            validation.is_some_and(|validation| validation.job_end_status == "normal");
        conformant += validation.is_some_and(|validation| validation.compliant) as usize;

        if core_accepts != verapdf_parsed {
            divergences += 1;
            eprintln!(
                "veraPDF parser divergence for {}: pdf-core={}, veraPDF={}",
                path.display(),
                core_accepts,
                verapdf_parsed
            );
        }
        checked += 1;
    }

    assert!(checked > 0, "veraPDF comparison checked no corpus files");
    assert_eq!(
        divergences, 0,
        "veraPDF comparison found {divergences} parser divergences"
    );
    eprintln!(
        "veraPDF comparison: {checked} checked, {conformant} PDF/A-1b conformant, {divergences} parser divergences"
    );
}
