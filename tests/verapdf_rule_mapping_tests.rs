use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use pdf_ast::validation::pdfa::PdfA1bValidator;
use pdf_ast::validation::SchemaRegistry;
use pdf_ast::PdfParser;

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

const LOCAL_CASES: &[(&str, &str)] = &[
    (
        "PDFA-1b/6.3 Fonts/6.3.4 Embedded font programs/isartor-6-3-4-t01-fail-a.pdf",
        "PDF_A_FONT_EMBEDDING",
    ),
    (
        "PDFA-1b/6.5 Annotations/6.5.2 Annotation types/isartor-6-5-2-t01-fail-a.pdf",
        "PDF_A_MULTIMEDIA",
    ),
    (
        "PDFA-1b/6.6 Actions/6.6.1 General/isartor-6-6-1-t01-fail-f.pdf",
        "PDF_A_JAVASCRIPT",
    ),
];

const PDFUA_CASES: &[(&str, &str, bool)] = &[
    (
        "verapdf-pdfua-1/7.1 General/7.1-t11-fail-a.pdf",
        "NO_TAGGED_STRUCTURE",
        true,
    ),
    (
        "verapdf-pdfua-1/7.1 General/7.1-t01-fail-a.pdf",
        "NO_TAGGED_STRUCTURE",
        false,
    ),
    (
        "verapdf-pdfua-1/7.1 General/7.1-t11-fail-a.pdf",
        "STRUCT_ELEM_MISSING",
        true,
    ),
    (
        "verapdf-pdfua-1/7.1 General/7.1-t08-fail-a.pdf",
        "ACCESSIBILITY_METADATA_MISSING",
        true,
    ),
    (
        "verapdf-pdfua-1/7.1 General/7.1-t08-pass-a.pdf",
        "ACCESSIBILITY_METADATA_MISSING",
        false,
    ),
    (
        "verapdf-pdfua-1/7.2 Text/7.2-t24-pass-a.pdf",
        "STRUCT_ELEM_MISSING",
        false,
    ),
    (
        "verapdf-pdfua-1/7.2 Text/7.2-t29-fail-o.pdf",
        "LANG_MISSING",
        true,
    ),
    (
        "verapdf-pdfua-1/7.2 Text/7.2-t24-pass-a.pdf",
        "LANG_MISSING",
        false,
    ),
    (
        "verapdf-pdfua-1/7.2 Text/7.2-t29-fail-n.pdf",
        "LANG_EMPTY",
        true,
    ),
    (
        "verapdf-pdfua-1/7.2 Text/7.2-t29-fail-b.pdf",
        "LANG_INVALID",
        true,
    ),
    (
        "verapdf-pdfua-1/7.2 Text/7.2-t24-pass-a.pdf",
        "LANG_EMPTY",
        false,
    ),
    (
        "verapdf-pdfua-1/7.2 Text/7.2-t24-pass-a.pdf",
        "LANG_INVALID",
        false,
    ),
    (
        "verapdf-pdfua-1/7.3 Graphics/7.3-t01-fail-a.pdf",
        "ALT_TEXT_MISSING",
        true,
    ),
    (
        "verapdf-pdfua-1/7.3 Graphics/7.3-t01-pass-a.pdf",
        "ALT_TEXT_MISSING",
        false,
    ),
];

const PDFUA_VERAPDF_CASES: &[(&str, &str)] = &[
    (
        "verapdf-pdfua-1/7.1 General/7.1-t11-fail-a.pdf",
        "ISO 14289-1:2014:7.1:11",
    ),
    (
        "verapdf-pdfua-1/7.2 Text/7.2-t02-fail-a.pdf",
        "ISO 14289-1:2014:7.2:2",
    ),
    (
        "verapdf-pdfua-1/7.2 Text/7.2-t29-fail-n.pdf",
        "ISO 14289-1:2014:7.2:29",
    ),
    (
        "verapdf-pdfua-1/7.3 Graphics/7.3-t01-fail-a.pdf",
        "ISO 14289-1:2014:7.3:1",
    ),
    (
        "verapdf-pdfua-1/7.3 Graphics/7.3-t01-fail-b.pdf",
        "ISO 14289-1:2014:7.3:1",
    ),
    (
        "verapdf-pdfua-1/7.1 General/7.1-t08-fail-a.pdf",
        "ISO 14289-1:2014:7.1:8",
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

fn manifest_fixture_root(root: &Path) -> PathBuf {
    root.parent()
        .filter(|parent| parent.join("fixtures").is_dir())
        .map(PathBuf::from)
        .unwrap_or_else(|| root.to_path_buf())
}

fn manifest_fixture_path(root: &Path, relative: &str) -> PathBuf {
    let path = manifest_fixture_root(root).join(relative);
    if path.is_file() {
        return path;
    }
    relative
        .strip_prefix("fixtures/")
        .map(|relative| root.join(relative))
        .unwrap_or(path)
}

fn local_rule_is_present(path: &Path, local_rule: &str) -> bool {
    let bytes = fs::read(path)
        .unwrap_or_else(|error| panic!("read local coverage fixture {}: {error}", path.display()));
    let document = PdfParser::new()
        .parse_bytes(&bytes)
        .unwrap_or_else(|error| panic!("parse local coverage fixture {}: {error}", path.display()));
    let report = if local_rule.starts_with("PDF_A_") {
        PdfA1bValidator::new()
            .with_strict_mode(false)
            .validate(&document)
    } else {
        SchemaRegistry::new()
            .validate(&document, "PDF/UA-1")
            .unwrap_or_else(|| {
                panic!(
                    "PDF/UA-1 does not support parsed version {}.{} for {}",
                    document.version.major,
                    document.version.minor,
                    path.display()
                )
            })
    };
    report.issues.iter().any(|issue| issue.code == local_rule)
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

#[test]
fn complete_isartor_manifest_matches_verapdf_ids() {
    let Some(verapdf) = verapdf_binary() else {
        eprintln!("Skipping complete veraPDF manifest mapping: veraPDF is not available");
        return;
    };
    let Some(root) = corpus_root() else {
        eprintln!("Skipping complete veraPDF manifest mapping: corpus root is not configured");
        return;
    };
    let manifest_path = root
        .parent()
        .expect("corpus fixtures should have a parent directory")
        .join("RULE-MAPPINGS.json");
    if !manifest_path.is_file() {
        eprintln!(
            "Skipping complete veraPDF manifest mapping: {} is not available",
            manifest_path.display()
        );
        return;
    }

    let manifest: Value =
        serde_json::from_slice(&fs::read(&manifest_path).expect("read mapping manifest"))
            .expect("valid mapping manifest");
    let mappings = manifest["mappings"].as_array().expect("mapping entries");
    assert_eq!(
        manifest["fixture_count"].as_u64(),
        Some(mappings.len() as u64)
    );
    assert_eq!(mappings.len(), 205);

    let files: Vec<PathBuf> = mappings
        .iter()
        .map(|mapping| {
            manifest_fixture_path(&root, mapping["fixture"].as_str().expect("fixture path"))
        })
        .collect();
    for path in &files {
        assert!(path.is_file(), "missing mapped fixture: {}", path.display());
    }

    let output = Command::new(verapdf)
        .args(["--format", "json", "--flavour", "1b"])
        .args(&files)
        .output()
        .expect("veraPDF should run");
    let report: Value = serde_json::from_slice(&output.stdout).expect("valid veraPDF JSON");
    let jobs = report["report"]["jobs"].as_array().expect("veraPDF jobs");
    assert_eq!(jobs.len(), mappings.len());

    for (job, mapping) in jobs.iter().zip(mappings) {
        let validation = &job["validationResult"][0];
        assert_eq!(validation["jobEndStatus"], "normal");
        assert_eq!(validation["compliant"], false);
        let actual_rules: Vec<String> = validation["details"]["ruleSummaries"]
            .as_array()
            .expect("veraPDF rule summaries")
            .iter()
            .filter(|rule| rule["ruleStatus"] == "FAILED")
            .map(|rule| {
                format!(
                    "{}:{}:{}",
                    rule["specification"].as_str().unwrap_or_default(),
                    rule["clause"].as_str().unwrap_or_default(),
                    rule["testNumber"].as_u64().unwrap_or_default()
                )
            })
            .collect();
        for expected_rule in mapping["veraPDF_rules"].as_array().expect("mapped rules") {
            let expected_rule = expected_rule.as_str().expect("rule ID");
            assert!(
                actual_rules.iter().any(|actual| actual == expected_rule),
                "veraPDF rule {expected_rule} missing for {}",
                mapping["fixture"].as_str().unwrap_or_default()
            );
        }
    }
}

#[test]
fn local_pdfa_rules_match_serialized_isartor_cases() {
    let Some(root) = corpus_root() else {
        eprintln!("Skipping local PDF/A fixture mapping: corpus root is not configured");
        return;
    };
    let parser = PdfParser::new();
    let validator = PdfA1bValidator::new().with_strict_mode(false);

    for (relative, expected_code) in LOCAL_CASES {
        let path = root.join("isartor").join(relative);
        let bytes = fs::read(&path).unwrap_or_else(|error| {
            panic!("read local compliance fixture {}: {error}", path.display())
        });
        let document = parser.parse_bytes(&bytes).unwrap_or_else(|error| {
            panic!("parse local compliance fixture {}: {error}", path.display())
        });
        let report = validator.validate(&document);
        assert!(
            report
                .issues
                .iter()
                .any(|issue| issue.code == *expected_code),
            "local rule {expected_code} missing for {}",
            path.display()
        );
    }
}

#[test]
fn local_pdfua_rules_match_serialized_upstream_cases() {
    let Some(root) = corpus_root() else {
        eprintln!("Skipping local PDF/UA fixture mapping: corpus root is not configured");
        return;
    };
    let parser = PdfParser::new();
    let registry = SchemaRegistry::new();

    for (relative, expected_code, should_have_issue) in PDFUA_CASES {
        let path = root.join(relative);
        let bytes = fs::read(&path).unwrap_or_else(|error| {
            panic!("read local PDF/UA fixture {}: {error}", path.display())
        });
        let document = parser.parse_bytes(&bytes).unwrap_or_else(|error| {
            panic!("parse local PDF/UA fixture {}: {error}", path.display())
        });
        let report = registry.validate(&document, "PDF/UA-1").unwrap_or_else(|| {
            panic!(
                "PDF/UA-1 does not support parsed version {}.{} for {}",
                document.version.major,
                document.version.minor,
                path.display()
            )
        });
        let has_issue = report
            .issues
            .iter()
            .any(|issue| issue.code == *expected_code);
        assert_eq!(
            has_issue,
            *should_have_issue,
            "local rule {expected_code} mismatch for {}",
            path.display()
        );
    }
}

#[test]
fn pdfua_rules_match_verapdf_ids() {
    let Some(verapdf) = verapdf_binary() else {
        eprintln!("Skipping veraPDF PDF/UA rule mapping: veraPDF is not available");
        return;
    };
    let Some(root) = corpus_root() else {
        eprintln!("Skipping veraPDF PDF/UA rule mapping: corpus root is not configured");
        return;
    };

    let files: Vec<PathBuf> = PDFUA_VERAPDF_CASES
        .iter()
        .map(|(relative, _)| root.join(relative))
        .collect();
    for path in &files {
        assert!(path.is_file(), "missing PDF/UA fixture: {}", path.display());
    }

    let output = Command::new(verapdf)
        .args(["--format", "json", "--flavour", "ua1"])
        .args(&files)
        .output()
        .expect("veraPDF should run");
    let report: Value = serde_json::from_slice(&output.stdout).expect("valid veraPDF JSON");
    let jobs = report["report"]["jobs"].as_array().expect("veraPDF jobs");
    assert_eq!(jobs.len(), PDFUA_VERAPDF_CASES.len());

    for (job, (_, expected_rule)) in jobs.iter().zip(PDFUA_VERAPDF_CASES) {
        let validation = &job["validationResult"][0];
        assert_eq!(validation["jobEndStatus"], "normal");
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

#[test]
fn published_rule_coverage_has_positive_and_negative_verapdf_evidence() {
    let Some(verapdf) = verapdf_binary() else {
        eprintln!("Skipping positive veraPDF rule coverage: veraPDF is not available");
        return;
    };
    let Some(root) = corpus_root() else {
        eprintln!("Skipping positive veraPDF rule coverage: corpus root is not configured");
        return;
    };
    let manifest_path = root
        .parent()
        .expect("corpus fixtures should have a parent directory")
        .join("RULE-COVERAGE.json");
    if !manifest_path.is_file() {
        eprintln!(
            "Skipping positive veraPDF rule coverage: {} is not available",
            manifest_path.display()
        );
        return;
    }

    let manifest: Value =
        serde_json::from_slice(&fs::read(&manifest_path).expect("read coverage manifest"))
            .expect("valid coverage manifest");
    let mappings = manifest["mappings"].as_array().expect("coverage mappings");
    assert_eq!(mappings.len(), 20);

    for mapping in mappings {
        let positive = manifest_fixture_path(
            &root,
            mapping["positive_fixture"]
                .as_str()
                .expect("positive fixture"),
        );
        let negative = manifest_fixture_path(
            &root,
            mapping["negative_fixture"]
                .as_str()
                .expect("negative fixture"),
        );
        assert!(
            positive.is_file(),
            "missing positive fixture: {}",
            positive.display()
        );
        assert!(
            negative.is_file(),
            "missing negative fixture: {}",
            negative.display()
        );
        let local_rule = mapping["local_rule"].as_str().expect("local rule");
        assert!(
            !local_rule_is_present(&positive, local_rule),
            "local rule {local_rule} unexpectedly reported for positive fixture {}",
            positive.display()
        );
        assert!(
            local_rule_is_present(&negative, local_rule),
            "local rule {local_rule} missing for negative fixture {}",
            negative.display()
        );

        let flavour = if local_rule.starts_with("PDF_A_") {
            "1b"
        } else {
            "ua1"
        };
        let output = Command::new(&verapdf)
            .args(["--format", "json", "--flavour", flavour])
            .arg(&positive)
            .arg(&negative)
            .output()
            .expect("veraPDF should run");
        let report: Value = serde_json::from_slice(&output.stdout).expect("valid veraPDF JSON");
        let jobs = report["report"]["jobs"].as_array().expect("veraPDF jobs");
        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[0]["validationResult"][0]["jobEndStatus"], "normal");
        assert_eq!(jobs[1]["validationResult"][0]["jobEndStatus"], "normal");
        assert_eq!(jobs[0]["validationResult"][0]["compliant"], true);
        assert_eq!(jobs[1]["validationResult"][0]["compliant"], false);

        if let Some(expected_rule) = mapping["veraPDF_rule"].as_str() {
            let rules = jobs[1]["validationResult"][0]["details"]["ruleSummaries"]
                .as_array()
                .expect("veraPDF rule summaries");
            assert!(
                rules.iter().any(|rule| {
                    format!(
                        "{}:{}:{}",
                        rule["specification"].as_str().unwrap_or_default(),
                        rule["clause"].as_str().unwrap_or_default(),
                        rule["testNumber"].as_u64().unwrap_or_default()
                    ) == expected_rule
                }),
                "veraPDF rule {expected_rule} missing for {}",
                negative.display()
            );
        }
    }
}
