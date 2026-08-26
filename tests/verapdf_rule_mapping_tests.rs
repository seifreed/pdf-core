use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;

const CASES: &[(&str, &str)] = &[
    (
        "PDFA-1b/6.3 Fonts/6.3.4 Embedded font programs/isartor-6-3-4-t01-fail-a.pdf",
        "ISO 19005-1:2005:6.3.4:1",
    ),
    (
        "PDFA-1b/6.5 Annotations/6.5.2 Annotation types/isartor-6-5-2-t01-fail-a.pdf",
        "ISO 19005-1:2005:6.5.2:1",
    ),
    (
        "PDFA-1b/6.6 Actions/6.6.1 General/isartor-6-6-1-t01-fail-a.pdf",
        "ISO 19005-1:2005:6.6.1:1",
    ),
];

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

fn corpus_root() -> Option<PathBuf> {
    std::env::var_os("PDF_COMPLIANCE_CORPUS")
        .or_else(|| std::env::var_os("PDF_EXTERNAL_CORPUS"))
        .map(PathBuf::from)
}

#[test]
fn isartor_rules_match_verapdf_ids() {
    let Some(verapdf) = verapdf_binary() else {
        eprintln!("Skipping veraPDF rule mapping: veraPDF is not available");
        return;
    };
    let Some(root) = corpus_root() else {
        eprintln!("Skipping veraPDF rule mapping: corpus root is not configured");
        return;
    };

    let files: Vec<PathBuf> = CASES
        .iter()
        .map(|(relative, _)| root.join("isartor").join(relative))
        .collect();
    for path in &files {
        assert!(
            path.is_file(),
            "missing compliance fixture: {}",
            path.display()
        );
    }

    let output = Command::new(verapdf)
        .args(["--format", "json", "--flavour", "1b"])
        .args(&files)
        .output()
        .expect("veraPDF should run");
    let report: Value = serde_json::from_slice(&output.stdout).expect("valid veraPDF JSON");
    let jobs = report["report"]["jobs"].as_array().expect("veraPDF jobs");
    assert_eq!(jobs.len(), CASES.len());

    for (job, (_, expected_rule)) in jobs.iter().zip(CASES) {
        let validation = &job["validationResult"][0];
        assert_eq!(validation["jobEndStatus"], "normal");
        assert_eq!(validation["compliant"], false);
        let rules = validation["details"]["ruleSummaries"]
            .as_array()
            .expect("veraPDF rule summaries");
        assert!(
            rules.iter().any(|rule| {
                format!(
                    "{}:{}:{}",
                    rule["specification"].as_str().unwrap_or_default(),
                    rule["clause"].as_str().unwrap_or_default(),
                    rule["testNumber"].as_u64().unwrap_or_default()
                ) == *expected_rule
            }),
            "veraPDF rule {expected_rule} missing"
        );
    }
}
