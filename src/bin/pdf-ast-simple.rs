use clap::{Parser, Subcommand};
use log::{error, info};
use std::fs;
use std::io::BufReader;
use std::path::PathBuf;
use std::time::Instant;

#[derive(Parser)]
#[command(name = "pdf-ast-simple")]
#[command(about = "Experimental PDF analysis tool")]
#[command(version = env!("CARGO_PKG_VERSION"))]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Enable verbose logging
    #[arg(short, long, global = true)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Parse PDF files and generate AST
    Parse {
        /// Input PDF file
        #[arg(value_name = "INPUT")]
        input: PathBuf,

        /// Output file (default: stdout)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Analyze PDF structure and metadata
    Analyze {
        /// Input PDF file
        #[arg(value_name = "INPUT")]
        input: PathBuf,

        /// Include detailed object analysis
        #[arg(long)]
        detailed: bool,
    },

    /// Performance benchmarking
    Benchmark {
        /// Input PDF file
        #[arg(value_name = "INPUT")]
        input: PathBuf,

        /// Number of iterations
        #[arg(short, long, default_value_t = 1)]
        iterations: usize,
    },
}

fn main() {
    let cli = Cli::parse();

    // Initialize logging
    let log_level = if cli.verbose { "debug" } else { "info" };
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(log_level))
        .format_timestamp_secs()
        .init();

    info!(
        "PDF-AST Simple CLI Tool v{} starting",
        env!("CARGO_PKG_VERSION")
    );

    let result = match cli.command {
        Commands::Parse { input, output } => handle_parse(input, output),
        Commands::Analyze { input, detailed } => handle_analyze(input, detailed),
        Commands::Benchmark { input, iterations } => handle_benchmark(input, iterations),
    };

    match result {
        Ok(_) => {
            info!("Operation completed successfully");
            std::process::exit(0);
        }
        Err(e) => {
            error!("Operation failed: {}", e);
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}

fn handle_parse(input: PathBuf, output: Option<PathBuf>) -> Result<(), String> {
    info!("Parsing file: {}", input.display());

    let start_time = Instant::now();

    // Read the PDF file
    let file = fs::File::open(&input)
        .map_err(|e| format!("Could not open file {}: {}", input.display(), e))?;

    let buf_reader = BufReader::new(file);

    // Create parser
    let parser = pdf_ast::parser::PdfParser::new();

    // Parse the PDF into AST
    let document = parser
        .parse(buf_reader)
        .map_err(|e| format!("Parse error for {}: {:?}", input.display(), e))?;

    let parse_duration = start_time.elapsed();

    // Generate AST JSON
    let json = pdf_ast::serialization::to_json(&document)
        .map_err(|e| format!("JSON serialization error: {:?}", e))?;

    let total_duration = start_time.elapsed();

    // Output results
    match output {
        Some(output_path) => {
            fs::write(&output_path, &json)
                .map_err(|e| format!("Could not write to {}: {}", output_path.display(), e))?;
            println!("AST saved to: {}", output_path.display());
        }
        None => {
            println!("{}", json);
        }
    }

    // Performance metrics
    eprintln!("Parse time: {:?}", parse_duration);
    eprintln!("Total time: {:?}", total_duration);
    eprintln!(
        "Document version: {}.{}",
        document.version.major, document.version.minor
    );
    eprintln!("Total objects: {}", document.ast.get_all_nodes().len());

    Ok(())
}

fn handle_analyze(input: PathBuf, detailed: bool) -> Result<(), String> {
    info!("Analyzing file: {}", input.display());

    let file = fs::File::open(&input).map_err(|e| format!("Could not open file: {}", e))?;

    let buf_reader = BufReader::new(file);
    let parser = pdf_ast::parser::PdfParser::new();
    let document = parser
        .parse(buf_reader)
        .map_err(|e| format!("Parse error: {:?}", e))?;

    // Basic analysis
    println!("PDF Analysis Report");
    println!("==================");
    println!("File: {}", input.display());
    println!(
        "PDF Version: {}.{}",
        document.version.major, document.version.minor
    );
    println!("Total Objects: {}", document.ast.get_all_nodes().len());

    if let Some(root_id) = document.ast.get_root() {
        println!("Root Object ID: {:?}", root_id);
    }

    // File size
    if let Ok(metadata) = fs::metadata(&input) {
        println!("File Size: {} bytes", metadata.len());
    }

    // Detailed object analysis
    if detailed {
        println!("\nDetailed Object Analysis:");
        let nodes = document.ast.get_all_nodes();
        let mut type_counts = std::collections::HashMap::new();

        for node in nodes {
            *type_counts
                .entry(format!("{:?}", node.node_type))
                .or_insert(0) += 1;
        }

        for (obj_type, count) in type_counts {
            println!("  {}: {}", obj_type, count);
        }

        // Basic security analysis
        println!("\nBasic Security Analysis:");
        let mut has_javascript = false;
        let mut has_forms = false;
        let mut has_embedded_files = false;

        let nodes = document.ast.get_all_nodes();
        for node in nodes {
            if let pdf_ast::types::PdfValue::Dictionary(dict) = &node.value {
                if dict.get("JS").is_some() || dict.get("JavaScript").is_some() {
                    has_javascript = true;
                }
                if let Some(pdf_ast::types::PdfValue::Name(name)) = dict.get("Type") {
                    if name.as_str() == "/Annot" {
                        if let Some(pdf_ast::types::PdfValue::Name(subtype)) = dict.get("Subtype") {
                            if subtype.as_str() == "/Widget" {
                                has_forms = true;
                            }
                        }
                    }
                }
                if dict.get("EF").is_some() {
                    has_embedded_files = true;
                }
            }
        }

        println!(
            "  JavaScript: {}",
            if has_javascript {
                "Present"
            } else {
                "Not detected"
            }
        );
        println!(
            "  Forms: {}",
            if has_forms { "Present" } else { "Not detected" }
        );
        if document.metadata.form_field_count > 0 {
            println!("  Form Fields: {}", document.metadata.form_field_count);
        }
        println!(
            "  XFA: {}",
            if document.metadata.has_xfa {
                format!("Present ({} packets)", document.metadata.xfa_packets)
            } else {
                "Not detected".to_string()
            }
        );
        if document.metadata.has_xfa_scripts {
            println!("  XFA Scripts: {}", document.metadata.xfa_script_nodes);
        }
        println!(
            "  Hybrid Forms: {}",
            if document.metadata.has_hybrid_forms {
                "Detected"
            } else {
                "Not detected"
            }
        );
        println!(
            "  Embedded Files: {}",
            if has_embedded_files {
                "Present"
            } else {
                "Not detected"
            }
        );
        if document.metadata.has_richmedia {
            println!(
                "  RichMedia: {} annots, {} assets, {} scripts",
                document.metadata.richmedia_annotations,
                document.metadata.richmedia_assets,
                document.metadata.richmedia_scripts
            );
        }
        if document.metadata.has_3d {
            println!(
                "  3D: {} annots (U3D: {}, PRC: {})",
                document.metadata.threed_annotations,
                document.metadata.threed_u3d,
                document.metadata.threed_prc
            );
        }
        if document.metadata.has_audio {
            println!("  Audio: {} annots", document.metadata.audio_annotations);
        }
        if document.metadata.has_video {
            println!("  Video: {} annots", document.metadata.video_annotations);
        }
        if document.metadata.has_dss {
            println!(
                "  DSS: VRI {}, Certs {}, OCSP {}, CRL {}, TS {}",
                document.metadata.dss_vri_count,
                document.metadata.dss_certs,
                document.metadata.dss_ocsp,
                document.metadata.dss_crl,
                document.metadata.dss_timestamps
            );
        }
    }

    Ok(())
}

fn handle_benchmark(input: PathBuf, iterations: usize) -> Result<(), String> {
    info!("Starting benchmark with {} iterations", iterations);

    let mut parse_times = Vec::new();
    let mut total_times = Vec::new();

    for i in 1..=iterations {
        println!("Iteration {}/{}", i, iterations);

        let start_time = Instant::now();

        let file = fs::File::open(&input).map_err(|e| format!("Could not open file: {}", e))?;

        let buf_reader = BufReader::new(file);
        let parser = pdf_ast::parser::PdfParser::new();

        let parse_start = Instant::now();
        let document = parser
            .parse(buf_reader)
            .map_err(|e| format!("Parse error: {:?}", e))?;
        let parse_time = parse_start.elapsed();

        let _json = pdf_ast::serialization::to_json(&document)
            .map_err(|e| format!("JSON serialization error: {:?}", e))?;

        let total_time = start_time.elapsed();

        parse_times.push(parse_time);
        total_times.push(total_time);
    }

    // Calculate statistics
    let avg_parse = parse_times.iter().sum::<std::time::Duration>() / iterations as u32;
    let avg_total = total_times.iter().sum::<std::time::Duration>() / iterations as u32;

    let min_parse = parse_times.iter().min().unwrap();
    let max_parse = parse_times.iter().max().unwrap();
    let min_total = total_times.iter().min().unwrap();
    let max_total = total_times.iter().max().unwrap();

    // Results
    println!("\nBenchmark Results:");
    println!("================");
    println!("Iterations: {}", iterations);
    println!(
        "Parse time - Avg: {:?}, Min: {:?}, Max: {:?}",
        avg_parse, min_parse, max_parse
    );
    println!(
        "Total time - Avg: {:?}, Min: {:?}, Max: {:?}",
        avg_total, min_total, max_total
    );

    Ok(())
}
