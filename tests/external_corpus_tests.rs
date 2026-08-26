use pdf_ast::parser::PdfParser;
use sha2::{Digest, Sha256};
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

fn peak_rss_kib() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let status = fs::read_to_string("/proc/self/status").ok()?;
        let value = status
            .lines()
            .find(|line| line.starts_with("VmHWM:"))?
            .split_whitespace()
            .nth(1)?;
        return value.parse().ok();
    }

    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        let mut usage = std::mem::MaybeUninit::<libc::rusage>::zeroed();
        let result = unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) };
        if result != 0 {
            return None;
        }
        let bytes = unsafe { usage.assume_init().ru_maxrss };
        u64::try_from(bytes).ok().map(|bytes| bytes / 1024)
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "ios")))]
    {
        None
    }
}

fn verify_checksums(root: &Path) -> usize {
    let checksum_path = root.join("SHA256SUMS");
    let checksums = fs::read_to_string(&checksum_path)
        .unwrap_or_else(|err| panic!("read checksum manifest {}: {err}", checksum_path.display()));

    let mut checked = 0;
    for line in checksums.lines().filter(|line| !line.trim().is_empty()) {
        let (expected, relative_path) = line
            .split_once("  ")
            .unwrap_or_else(|| panic!("invalid checksum line: {line}"));
        let relative_path = relative_path.strip_prefix("./").unwrap_or(relative_path);
        let relative_path = Path::new(relative_path);
        assert!(
            !relative_path.is_absolute(),
            "checksum path must be relative"
        );
        assert!(
            !relative_path
                .components()
                .any(|component| component == std::path::Component::ParentDir),
            "checksum path must stay inside corpus"
        );

        let bytes = fs::read(root.join(relative_path)).unwrap_or_else(|err| {
            panic!("read checksum target {}: {err}", relative_path.display())
        });
        let actual = format!("{:x}", Sha256::digest(&bytes));
        assert_eq!(
            actual,
            expected,
            "checksum mismatch for {}",
            relative_path.display()
        );
        checked += 1;
    }

    checked
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
    let hashes_verified = verify_checksums(Path::new(&root));
    assert!(
        hashes_verified >= files.len(),
        "checksum manifest must cover the selected external corpus PDFs"
    );

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
    let peak_rss_kib = peak_rss_kib();
    eprintln!(
        "external corpus metrics: files={}, hashes_verified={}, bytes={}, parse_errors={}, wall_ms={}, peak_rss_kib={:?}, p50_ms={}, p95_ms={}, p99_ms={}",
        files.len(),
        hashes_verified,
        total_bytes,
        parse_errors,
        started.elapsed().as_millis(),
        peak_rss_kib,
        p50_ms,
        p95_ms,
        p99_ms
    );
}
