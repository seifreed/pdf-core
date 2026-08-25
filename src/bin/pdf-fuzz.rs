use clap::Parser;
use pdf_ast::parser::PdfParser;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Parser, Debug)]
#[command(author, version, about = "Deterministic PDF mutation smoke test")]
struct Args {
    #[arg(long, default_value = "pdfs")]
    corpus_dir: String,

    #[arg(long, default_value_t = 10)]
    max_files: usize,

    #[arg(long, default_value_t = 10)]
    mutations_per_file: usize,

    #[arg(long, default_value_t = 123456789)]
    seed: u64,

    #[arg(long, default_value_t = 0.002)]
    flip_ratio: f64,
}

struct XorShift64 {
    state: u64,
}

impl XorShift64 {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    fn next_usize(&mut self, max: usize) -> usize {
        if max == 0 {
            0
        } else {
            (self.next() as usize) % max
        }
    }
}

fn collect_pdfs(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("pdf") {
                files.push(path);
            }
        }
    }
    files
}

fn mutate_bytes(mut data: Vec<u8>, rng: &mut XorShift64, flips: usize) -> Vec<u8> {
    let len = data.len();
    if len == 0 {
        return data;
    }
    for _ in 0..flips {
        let pos = rng.next_usize(len);
        let bit = 1u8 << (rng.next_usize(8) as u8);
        data[pos] ^= bit;
    }
    data
}

fn main() {
    let args = Args::parse();
    let corpus_dir = PathBuf::from(&args.corpus_dir);
    let files = collect_pdfs(&corpus_dir);
    let mut rng = XorShift64::new(args.seed);
    let parser = PdfParser::new();

    if files.is_empty() {
        eprintln!("No PDFs found in {}", corpus_dir.display());
        return;
    }

    let mut total = 0usize;
    let mut errors = 0usize;

    for path in files.into_iter().take(args.max_files) {
        let data = match fs::read(&path) {
            Ok(d) => d,
            Err(_) => continue,
        };

        let flips = ((data.len() as f64) * args.flip_ratio).ceil() as usize;
        let flips = std::cmp::max(1, flips);

        for _ in 0..args.mutations_per_file {
            let mutated = mutate_bytes(data.clone(), &mut rng, flips);
            total += 1;
            if parser.parse_bytes(&mutated).is_err() {
                errors += 1;
            }
        }

        if data.len() > 8 {
            let truncated = data[..data.len() / 2].to_vec();
            total += 1;
            if parser.parse_bytes(&truncated).is_err() {
                errors += 1;
            }
        }
    }

    println!(
        "Fuzzed {} inputs; parse errors: {} (ratio {:.2}%)",
        total,
        errors,
        if total == 0 {
            0.0
        } else {
            (errors as f64 / total as f64) * 100.0
        }
    );
}
