#![recursion_limit = "256"]

use clap::{Parser, Subcommand, ValueEnum};
use log::{error, info, warn};
use pdf_ast::api::{QueryEngine, QueryParser};
use pdf_ast::ast::PdfAstGraph;
use pdf_ast::crypto::signature_verification::{SignatureInfo, SignatureVerifier};
use pdf_ast::crypto::CryptoConfig;
use pdf_ast::parser::PdfParser;
use pdf_ast::schema::{
    generate_json_schema, SchemaExporter, SchemaMigrator, SchemaVersion, StableAstSchema,
};
use pdf_ast::serialization::{
    to_json, GraphDeserializer, SerializableDocument, SerializableDocumentMetadata,
    SerializableEdge, SerializableGraph, SerializableNode, SerializableValue,
    SerializableXRefEntry,
};
use pdf_ast::streaming::parse_large_pdf;
use pdf_ast::validation::SchemaRegistry;
use pdf_ast::{format_security_report, security_info_to_report, SecurityOutputFormat};
use quick_xml::events::BytesText;
use quick_xml::Writer;

use std::fs;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::time::Instant;

// External crates for serialization formats
extern crate quick_xml;
extern crate serde_yaml;
extern crate toml;

#[derive(Parser)]
#[command(name = "pdf-ast")]
#[command(about = "Experimental PDF analysis and AST generation tool")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(author = "Marc Rivero López")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Enable verbose logging
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Output format
    #[arg(short, long, value_enum, default_value_t = OutputFormat::Pretty, global = true)]
    format: OutputFormat,

    /// Disable colored output
    #[arg(long, global = true)]
    no_color: bool,
}

#[derive(Clone, ValueEnum)]
enum OutputFormat {
    Pretty,
    Json,
    Compact,
    Table,
    Yaml,
    Toml,
}

#[derive(Subcommand)]
enum Commands {
    /// Parse PDF files and generate AST
    Parse {
        /// Input PDF file or directory
        #[arg(value_name = "INPUT")]
        input: PathBuf,

        /// Output file or directory (default: stdout)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Include stream data in AST
        #[arg(long)]
        include_streams: bool,

        /// Resolve indirect references
        #[arg(long, default_value_t = true)]
        resolve_refs: bool,

        /// Process recursively if input is directory
        #[arg(short, long)]
        recursive: bool,
    },

    /// Validate PDF files against schemas
    Validate {
        /// Input PDF file or directory
        #[arg(value_name = "INPUT")]
        input: PathBuf,

        /// Validation schema to use
        #[arg(short, long, value_enum, default_value_t = ValidationSchema::Pdf20)]
        schema: ValidationSchema,

        /// Output validation report
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Strict validation mode
        #[arg(long)]
        strict: bool,

        /// Output a versioned validation report envelope
        #[arg(long)]
        versioned_report: bool,

        /// Process recursively if input is directory
        #[arg(short, long)]
        recursive: bool,
    },

    /// Analyze PDF structure and metadata
    Analyze {
        /// Input PDF file
        #[arg(value_name = "INPUT")]
        input: PathBuf,

        /// Include detailed object analysis
        #[arg(long)]
        detailed: bool,

        /// Include security analysis
        #[arg(long)]
        security: bool,

        /// Include performance metrics
        #[arg(long)]
        metrics: bool,

        /// Write security report to file (auto-detect format by extension)
        #[arg(long)]
        security_report: Option<PathBuf>,

        /// Enable TSA chain validation for RFC3161 timestamps
        #[arg(long)]
        enable_tsa_chain_validation: bool,

        /// Enable TSA OCSP/CRL checks during timestamp validation
        #[arg(long)]
        enable_tsa_revocation_checks: bool,

        /// Allow-list TSA certificate fingerprint (SHA-256). Can be repeated or comma-separated.
        #[arg(long, value_delimiter = ',', value_name = "SHA256")]
        tsa_allow_fingerprint: Vec<String>,

        /// Block-list TSA certificate fingerprint (SHA-256). Can be repeated or comma-separated.
        #[arg(long, value_delimiter = ',', value_name = "SHA256")]
        tsa_block_fingerprint: Vec<String>,
    },

    /// Convert between AST formats
    Convert {
        /// Input AST file (JSON)
        #[arg(value_name = "INPUT")]
        input: PathBuf,

        /// Output file
        #[arg(short, long)]
        output: PathBuf,

        /// Target format
        #[arg(short = 't', long = "to", value_enum, default_value_t = ConvertFormat::Json)]
        target_format: ConvertFormat,

        /// Pretty print output
        #[arg(long)]
        pretty: bool,
    },

    /// Stream and parse large PDFs incrementally
    Stream {
        /// Input PDF file
        #[arg(value_name = "INPUT")]
        input: PathBuf,

        /// Output AST file (default: stdout)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Output streaming report JSON
        #[arg(long)]
        report: Option<PathBuf>,
    },
    /// Performance benchmarking
    Benchmark {
        /// Input PDF file or directory
        #[arg(value_name = "INPUT")]
        input: PathBuf,

        /// Number of iterations
        #[arg(short, long, default_value_t = 1)]
        iterations: usize,

        /// Output benchmark results
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Include memory profiling
        #[arg(long)]
        memory: bool,
    },

    /// Query AST using CSS-like selectors
    Query {
        /// Input AST file (JSON)
        #[arg(value_name = "INPUT")]
        input: PathBuf,

        /// Query selector
        #[arg(short, long)]
        query: String,

        /// Output format
        #[arg(short, long, value_enum, default_value_t = OutputFormat::Pretty)]
        format: OutputFormat,

        /// Limit number of results
        #[arg(long)]
        limit: Option<usize>,
    },

    /// Generate or migrate JSON schema
    Schema {
        /// Generate schema instead of migrating
        #[arg(long)]
        generate: bool,

        /// Input AST file for migration
        input: Option<PathBuf>,

        /// Output file
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Target schema version for migration
        #[arg(long, default_value = "1.0.0")]
        version: String,
    },
}

#[derive(Debug, Clone, ValueEnum)]
enum ValidationSchema {
    Pdf20,
    PdfA1b,
    PdfA2b,
    PdfA3b,
    PdfX1a,
    PdfX4,
    PdfUA1,
}

#[derive(Clone, ValueEnum)]
enum ConvertFormat {
    Json,
    Yaml,
    Toml,
    Xml,
}

fn main() {
    let cli = Cli::parse();

    // Initialize logging
    let log_level = if cli.verbose { "debug" } else { "info" };
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(log_level))
        .format_timestamp_secs()
        .init();

    info!("PDF-AST CLI Tool v{} starting", env!("CARGO_PKG_VERSION"));

    let result = match &cli.command {
        Commands::Parse {
            input,
            output,
            include_streams,
            resolve_refs,
            recursive,
        } => handle_parse(
            input.clone(),
            output.clone(),
            *include_streams,
            *resolve_refs,
            *recursive,
            &cli,
        ),
        Commands::Validate {
            input,
            schema,
            output,
            strict,
            versioned_report,
            recursive,
        } => handle_validate(
            input.clone(),
            schema.clone(),
            output.clone(),
            *strict,
            *versioned_report,
            *recursive,
            &cli,
        ),
        Commands::Analyze {
            input,
            detailed,
            security,
            metrics,
            security_report,
            enable_tsa_chain_validation,
            enable_tsa_revocation_checks,
            tsa_allow_fingerprint,
            tsa_block_fingerprint,
        } => handle_analyze(
            input.clone(),
            *detailed,
            *security,
            *metrics,
            security_report.clone(),
            build_crypto_config(
                *enable_tsa_chain_validation,
                *enable_tsa_revocation_checks,
                tsa_allow_fingerprint,
                tsa_block_fingerprint,
            ),
            &cli,
        ),
        Commands::Convert {
            input,
            output,
            target_format,
            pretty,
        } => handle_convert(
            input.clone(),
            output.clone(),
            target_format.clone(),
            *pretty,
            &cli,
        ),
        Commands::Stream {
            input,
            output,
            report,
        } => handle_stream(input.clone(), output.clone(), report.clone(), &cli),
        Commands::Benchmark {
            input,
            iterations,
            output,
            memory,
        } => handle_benchmark(input.clone(), *iterations, output.clone(), *memory, &cli),
        Commands::Query {
            input,
            query,
            format,
            limit,
        } => handle_query(input.clone(), query.clone(), format.clone(), *limit, &cli),
        Commands::Schema {
            generate,
            input,
            output,
            version,
        } => handle_schema_command(
            *generate,
            input.clone(),
            output.clone(),
            version.clone(),
            &cli,
        ),
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

fn handle_parse(
    input: PathBuf,
    output: Option<PathBuf>,
    include_streams: bool,
    resolve_refs: bool,
    recursive: bool,
    cli: &Cli,
) -> Result<(), String> {
    info!("Starting parse operation");

    if input.is_file() {
        parse_single_file(
            &input,
            output.as_deref(),
            include_streams,
            resolve_refs,
            cli,
        )
    } else if input.is_dir() {
        parse_directory(
            &input,
            output.as_deref(),
            include_streams,
            resolve_refs,
            recursive,
            cli,
        )
    } else {
        Err(format!("Input path does not exist: {}", input.display()))
    }
}

fn parse_single_file(
    input: &Path,
    output: Option<&Path>,
    _include_streams: bool,
    _resolve_refs: bool,
    cli: &Cli,
) -> Result<(), String> {
    info!("Parsing file: {}", input.display());

    let start_time = Instant::now();

    // Read the PDF file
    let file = fs::File::open(input)
        .map_err(|e| format!("Could not open file {}: {}", input.display(), e))?;

    let buf_reader = BufReader::new(file);

    // Create parser with configuration
    let parser = PdfParser::new();

    // Parse the PDF into AST
    let document = parser
        .parse(buf_reader)
        .map_err(|e| format!("Parse error for {}: {:?}", input.display(), e))?;

    let parse_duration = start_time.elapsed();

    // Generate AST JSON
    let json = to_json(&document).map_err(|e| format!("JSON serialization error: {:?}", e))?;

    let total_duration = start_time.elapsed();

    // Output results
    match output {
        Some(output_path) => {
            fs::write(output_path, &json)
                .map_err(|e| format!("Could not write to {}: {}", output_path.display(), e))?;
            info!("AST saved to: {}", output_path.display());
        }
        None => {
            println!("{}", json);
        }
    }

    // Performance metrics
    if cli.verbose {
        println!("Parse time: {:?}", parse_duration);
        println!("Total time: {:?}", total_duration);
        println!(
            "Document version: {}.{}",
            document.version.major, document.version.minor
        );
        println!("Total objects: {}", document.ast.get_all_nodes().len());
    }

    Ok(())
}

fn parse_directory(
    input: &Path,
    output: Option<&Path>,
    include_streams: bool,
    resolve_refs: bool,
    recursive: bool,
    cli: &Cli,
) -> Result<(), String> {
    info!("Parsing directory: {}", input.display());

    let output_dir = output.unwrap_or_else(|| Path::new("output"));
    fs::create_dir_all(output_dir).map_err(|e| {
        format!(
            "Could not create output directory {}: {}",
            output_dir.display(),
            e
        )
    })?;

    let entries = collect_pdf_files(input, recursive)?;
    let mut processed = 0;
    let mut successful = 0;

    for pdf_path in entries {
        processed += 1;

        let filename = pdf_path.file_stem().unwrap_or_default().to_string_lossy();
        let output_path = output_dir.join(format!("{}.ast.json", filename));

        println!("Processing: {}", pdf_path.display());

        match parse_single_file(
            &pdf_path,
            Some(&output_path),
            include_streams,
            resolve_refs,
            cli,
        ) {
            Ok(_) => {
                successful += 1;
                println!("  ✓ Success");
            }
            Err(e) => {
                warn!("Failed to parse {}: {}", pdf_path.display(), e);
                println!("  ✗ Error: {}", e);
            }
        }
    }

    println!("\nSummary:");
    println!("  Files processed: {}", processed);
    println!("  Successful: {}", successful);
    println!(
        "  Success rate: {:.1}%",
        (successful as f64 / processed as f64) * 100.0
    );

    Ok(())
}

fn handle_validate(
    input: PathBuf,
    schema: ValidationSchema,
    output: Option<PathBuf>,
    strict: bool,
    versioned_report: bool,
    recursive: bool,
    cli: &Cli,
) -> Result<(), String> {
    info!("Starting validation operation with schema: {:?}", schema);

    if input.is_file() {
        validate_single_file(
            &input,
            &schema,
            output.as_deref(),
            strict,
            versioned_report,
            cli,
        )
    } else if input.is_dir() {
        validate_directory(
            &input,
            &schema,
            output.as_deref(),
            strict,
            recursive,
            versioned_report,
            cli,
        )
    } else {
        Err(format!("Input path does not exist: {}", input.display()))
    }
}

fn validate_single_file(
    input: &Path,
    schema: &ValidationSchema,
    output: Option<&Path>,
    _strict: bool,
    versioned_report: bool,
    cli: &Cli,
) -> Result<(), String> {
    info!("Validating file: {}", input.display());

    // Parse the document first
    let file = fs::File::open(input)
        .map_err(|e| format!("Could not open file {}: {}", input.display(), e))?;

    let buf_reader = BufReader::new(file);
    let parser = PdfParser::new();
    let document = parser
        .parse(buf_reader)
        .map_err(|e| format!("Parse error: {:?}", e))?;

    // Validate based on schema
    let report = match schema {
        ValidationSchema::PdfA1b => {
            let registry = SchemaRegistry::new();
            registry
                .validate(&document, "PDF/A-1b")
                .ok_or_else(|| "No validation report generated".to_string())?
        }
        ValidationSchema::Pdf20 => {
            let registry = SchemaRegistry::new();
            registry
                .validate(&document, "PDF-2.0")
                .ok_or_else(|| "No validation report generated".to_string())?
        }
        ValidationSchema::PdfA2b => {
            let registry = SchemaRegistry::new();
            registry
                .validate(&document, "PDF/A-2b")
                .ok_or_else(|| "No validation report generated".to_string())?
        }
        ValidationSchema::PdfA3b => {
            let registry = SchemaRegistry::new();
            registry
                .validate(&document, "PDF/A-3b")
                .ok_or_else(|| "No validation report generated".to_string())?
        }
        ValidationSchema::PdfX1a => {
            let registry = SchemaRegistry::new();
            registry
                .validate(&document, "PDF/X-1a")
                .ok_or_else(|| "No validation report generated".to_string())?
        }
        ValidationSchema::PdfX4 => {
            let registry = SchemaRegistry::new();
            registry
                .validate(&document, "PDF/X-4")
                .ok_or_else(|| "No validation report generated".to_string())?
        }
        ValidationSchema::PdfUA1 => {
            let registry = SchemaRegistry::new();
            registry
                .validate(&document, "PDF/UA-1")
                .ok_or_else(|| "No validation report generated".to_string())?
        }
    };

    // Output validation report
    let report_json = if versioned_report {
        let envelope = report.clone().into_envelope();
        serde_json::to_string_pretty(&envelope)
            .map_err(|e| format!("Failed to serialize validation report envelope: {}", e))?
    } else {
        serde_json::to_string_pretty(&report)
            .map_err(|e| format!("Failed to serialize validation report: {}", e))?
    };

    match output {
        Some(output_path) => {
            fs::write(output_path, &report_json)
                .map_err(|e| format!("Could not write to {}: {}", output_path.display(), e))?;
            info!("Validation report saved to: {}", output_path.display());
        }
        None => {
            println!("{}", report_json);
        }
    }

    // Summary
    if cli.verbose {
        println!("Validation completed:");
        println!("  Valid: {}", report.is_valid);
        println!("  Issues: {}", report.issues.len());
        println!("  Error count: {}", report.statistics.error_count);
        println!("  Warning count: {}", report.statistics.warning_count);
    }

    if !report.is_valid {
        return Err("Document failed validation".to_string());
    }

    Ok(())
}

fn validate_directory(
    input: &Path,
    schema: &ValidationSchema,
    output: Option<&Path>,
    _strict: bool,
    recursive: bool,
    versioned_report: bool,
    cli: &Cli,
) -> Result<(), String> {
    info!("Validating directory: {}", input.display());

    let output_dir = output.unwrap_or_else(|| Path::new("validation_reports"));
    fs::create_dir_all(output_dir)
        .map_err(|e| format!("Could not create output directory: {}", e))?;

    let entries = collect_pdf_files(input, recursive)?;
    let mut processed = 0;
    let mut valid = 0;

    for pdf_path in entries {
        processed += 1;

        let filename = pdf_path.file_stem().unwrap_or_default().to_string_lossy();
        let output_path = output_dir.join(format!("{}.validation.json", filename));

        println!("Validating: {}", pdf_path.display());

        match validate_single_file(
            &pdf_path,
            schema,
            Some(&output_path),
            _strict,
            versioned_report,
            cli,
        ) {
            Ok(_) => {
                valid += 1;
                println!("  ✓ Valid");
            }
            Err(e) => {
                warn!("Validation failed for {}: {}", pdf_path.display(), e);
                println!("  ✗ Invalid: {}", e);
            }
        }
    }

    println!("\nValidation Summary:");
    println!("  Files processed: {}", processed);
    println!("  Valid files: {}", valid);
    println!(
        "  Validation rate: {:.1}%",
        (valid as f64 / processed as f64) * 100.0
    );

    Ok(())
}

fn handle_analyze(
    input: PathBuf,
    detailed: bool,
    security: bool,
    metrics: bool,
    security_report: Option<PathBuf>,
    crypto_config: CryptoConfig,
    cli: &Cli,
) -> Result<(), String> {
    info!("Starting analysis of: {}", input.display());

    let file = fs::File::open(&input).map_err(|e| format!("Could not open file: {}", e))?;

    let buf_reader = BufReader::new(file);
    let parser = PdfParser::new();
    let document = parser
        .parse(buf_reader)
        .map_err(|e| format!("Parse error: {:?}", e))?;

    if matches!(cli.format, OutputFormat::Yaml | OutputFormat::Toml) {
        if !security {
            return Err("YAML/TOML output requires --security".to_string());
        }
        let mut reader = fs::File::open(&input)
            .map_err(|e| format!("Could not open file for signature verification: {}", e))?;
        let info = pdf_ast::security::SecurityAnalyzer::analyze_document(
            &document,
            &mut reader,
            crypto_config.clone(),
        );
        let report = security_info_to_report(info);
        let format = match cli.format {
            OutputFormat::Yaml => SecurityOutputFormat::Yaml,
            OutputFormat::Toml => SecurityOutputFormat::Toml,
            _ => SecurityOutputFormat::Json,
        };
        let output = format_security_report(&report, format)?;
        if let Some(path) = &security_report {
            fs::write(path, output).map_err(|e| format!("Write failed: {}", e))?;
        } else {
            println!("{}", output);
        }
        return Ok(());
    }

    if matches!(cli.format, OutputFormat::Json) {
        let nodes = document.ast.get_all_nodes();
        let total_objects = nodes.len();
        let mut type_counts = std::collections::HashMap::new();
        if detailed {
            for node in nodes.iter() {
                *type_counts
                    .entry(format!("{:?}", node.node_type))
                    .or_insert(0) += 1;
            }
        }

        let mut has_javascript = false;
        let mut has_forms = false;
        let mut has_embedded_files = false;
        let signatures = if security {
            analyze_signatures(&document, &input, &crypto_config)
        } else {
            Vec::new()
        };
        if security {
            for node in nodes.iter() {
                if let pdf_ast::types::PdfValue::Dictionary(dict) = &node.value {
                    if dict.get("JS").is_some() || dict.get("JavaScript").is_some() {
                        has_javascript = true;
                    }
                    if let Some(pdf_ast::types::PdfValue::Name(name)) = dict.get("Type") {
                        if name.as_str() == "/Annot" {
                            if let Some(pdf_ast::types::PdfValue::Name(subtype)) =
                                dict.get("Subtype")
                            {
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
        }

        let file_size = if metrics {
            fs::metadata(&input).map(|m| m.len()).unwrap_or(0)
        } else {
            0
        };
        let objects_per_kb = if metrics && file_size > 0 {
            (total_objects as f64) / (file_size as f64 / 1024.0)
        } else {
            0.0
        };

        let analysis = serde_json::json!({
            "report_version": "1.0",
            "file": input.display().to_string(),
            "pdf_version": format!("{}.{}", document.version.major, document.version.minor),
            "total_objects": total_objects,
                "metadata": {
                    "file_size": document.metadata.file_size,
                    "linearized": document.metadata.linearized,
                    "encrypted": document.metadata.encrypted,
                    "has_forms": document.metadata.has_forms,
                    "has_xfa": document.metadata.has_xfa,
                    "xfa_packets": document.metadata.xfa_packets,
                    "has_xfa_scripts": document.metadata.has_xfa_scripts,
                    "xfa_script_nodes": document.metadata.xfa_script_nodes,
                    "has_hybrid_forms": document.metadata.has_hybrid_forms,
                    "form_field_count": document.metadata.form_field_count,
                    "has_javascript": document.metadata.has_javascript,
                    "has_embedded_files": document.metadata.has_embedded_files,
                    "has_signatures": document.metadata.has_signatures,
                    "has_richmedia": document.metadata.has_richmedia,
                    "richmedia_annotations": document.metadata.richmedia_annotations,
                    "richmedia_assets": document.metadata.richmedia_assets,
                    "richmedia_scripts": document.metadata.richmedia_scripts,
                    "has_3d": document.metadata.has_3d,
                    "threed_annotations": document.metadata.threed_annotations,
                    "threed_u3d": document.metadata.threed_u3d,
                    "threed_prc": document.metadata.threed_prc,
                    "has_audio": document.metadata.has_audio,
                    "audio_annotations": document.metadata.audio_annotations,
                    "has_video": document.metadata.has_video,
                    "video_annotations": document.metadata.video_annotations,
                    "has_dss": document.metadata.has_dss,
                    "dss_vri_count": document.metadata.dss_vri_count,
                    "dss_certs": document.metadata.dss_certs,
                    "dss_ocsp": document.metadata.dss_ocsp,
                    "dss_crl": document.metadata.dss_crl,
                    "dss_timestamps": document.metadata.dss_timestamps,
                    "page_count": document.metadata.page_count,
                "producer": document.metadata.producer,
                "creator": document.metadata.creator,
                "creation_date": document.metadata.creation_date,
                "modification_date": document.metadata.modification_date,
                "title": document.metadata.title,
                "author": document.metadata.author,
                "subject": document.metadata.subject,
            },
            "type_counts": if detailed { serde_json::json!(type_counts) } else { serde_json::Value::Null },
            "security": if security {
                serde_json::json!({
                    "javascript": has_javascript,
                    "forms": has_forms,
                    "embedded_files": has_embedded_files,
                    "signatures": signatures.iter().map(signature_info_to_json).collect::<Vec<_>>(),
                })
            } else {
                serde_json::Value::Null
            },
            "metrics": if metrics {
                serde_json::json!({
                    "file_size": file_size,
                    "objects_per_kb": objects_per_kb,
                })
            } else {
                serde_json::Value::Null
            }
        });

        println!(
            "{}",
            serde_json::to_string_pretty(&analysis)
                .map_err(|e| format!("JSON serialization error: {}", e))?
        );

        if let Some(path) = &security_report {
            let report =
                security_info_to_report(pdf_ast::security::SecurityAnalyzer::analyze_document(
                    &document,
                    &mut fs::File::open(&input).map_err(|e| {
                        format!("Could not open file for signature verification: {}", e)
                    })?,
                    crypto_config.clone(),
                ));
            let format = pdf_ast::security::report_output::output_format_from_path(path)
                .unwrap_or(SecurityOutputFormat::Json);
            let output = format_security_report(&report, format)?;
            fs::write(path, output).map_err(|e| format!("Write failed: {}", e))?;
        }
        return Ok(());
    }

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
    }

    // Security analysis
    if security {
        println!("\nSecurity Analysis:");
        // Check for JavaScript
        let nodes = document.ast.get_all_nodes();
        let mut has_javascript = false;
        let mut has_forms = false;
        let mut has_embedded_files = false;

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
        println!(
            "  Embedded Files: {}",
            if has_embedded_files {
                "Present"
            } else {
                "Not detected"
            }
        );

        let signatures = analyze_signatures(&document, &input, &crypto_config);
        if !signatures.is_empty() {
            println!("  Signatures: {}", signatures.len());
            for sig in signatures {
                println!("    - {}: {:?}", sig.field_name, sig.validity);
            }
        }
    }

    // Performance metrics
    if metrics {
        println!("\nPerformance Metrics:");
        let file_size = fs::metadata(&input).map(|m| m.len()).unwrap_or(0);
        println!("  File Size: {} bytes", file_size);

        if file_size > 0 {
            let objects_per_kb =
                (document.ast.get_all_nodes().len() as f64) / (file_size as f64 / 1024.0);
            println!("  Objects per KB: {:.2}", objects_per_kb);
        }
    }

    Ok(())
}

fn build_crypto_config(
    enable_tsa_chain_validation: bool,
    enable_tsa_revocation_checks: bool,
    tsa_allow_fingerprint: &[String],
    tsa_block_fingerprint: &[String],
) -> CryptoConfig {
    CryptoConfig {
        enable_tsa_chain_validation,
        enable_tsa_revocation_checks,
        tsa_allow_fingerprints: tsa_allow_fingerprint.to_vec(),
        tsa_block_fingerprints: tsa_block_fingerprint.to_vec(),
        ..Default::default()
    }
}

fn analyze_signatures(
    document: &pdf_ast::PdfDocument,
    input: &Path,
    crypto_config: &CryptoConfig,
) -> Vec<SignatureInfo> {
    use pdf_ast::types::PdfValue;
    use pdf_ast::NodeType;

    let mut signatures = Vec::new();
    let Ok(mut file) = fs::File::open(input) else {
        return signatures;
    };
    let mut verifier = SignatureVerifier::new().with_crypto_config(crypto_config.clone());

    for (index, node) in document.ast.get_all_nodes().iter().enumerate() {
        if node.node_type == NodeType::Signature {
            if let PdfValue::Dictionary(dict) = &node.value {
                let name = extract_signature_name(dict, index);
                let info = verifier.verify_signature(dict, &name, &mut file);
                signatures.push(info);
            }
        }
    }

    signatures
}

fn extract_signature_name(dict: &pdf_ast::types::PdfDictionary, index: usize) -> String {
    use pdf_ast::types::PdfValue;
    if let Some(PdfValue::String(s)) = dict.get("T") {
        return s.to_string_lossy();
    }
    if let Some(PdfValue::String(s)) = dict.get("Name") {
        return s.to_string_lossy();
    }
    format!("Signature_{}", index)
}

fn signature_info_to_json(info: &SignatureInfo) -> serde_json::Value {
    serde_json::json!({
        "field_name": info.field_name,
        "signer": {
            "subject": info.signer.subject,
            "issuer": info.signer.issuer,
            "serial_number": info.signer.serial_number,
            "email": info.signer.email,
        },
        "signing_time": info.signing_time.map(|t| t.to_string()),
        "reason": info.reason,
        "location": info.location,
        "contact_info": info.contact_info,
        "byte_range": info.byte_range,
        "filter": info.filter,
        "sub_filter": info.sub_filter,
        "validity": format!("{:?}", info.validity),
        "timestamp": info.timestamp.as_ref().map(|ts| serde_json::json!({
            "time": ts.time.to_string(),
            "policy_oid": ts.policy_oid,
            "hash_algorithm": ts.hash_algorithm,
            "signature_valid": ts.signature_valid,
            "tsa_chain_valid": ts.tsa_chain_valid,
            "tsa_chain_errors": ts.tsa_chain_errors,
            "tsa_chain_warnings": ts.tsa_chain_warnings,
            "tsa_pin_valid": ts.tsa_pin_valid,
            "tsa_pin_reason": ts.tsa_pin_reason,
            "tsa_revocation_events": ts.tsa_revocation_events.iter().map(|ev| {
                serde_json::json!({
                    "cert_index": ev.cert_index,
                    "url": ev.url,
                    "protocol": match ev.protocol {
                        pdf_ast::crypto::certificates::RevocationProtocol::Ocsp => "ocsp",
                        pdf_ast::crypto::certificates::RevocationProtocol::Crl => "crl",
                    },
                    "status": ev.status,
                    "latency_ms": ev.latency_ms,
                    "error": ev.error,
                })
            }).collect::<Vec<_>>(),
        })),
    })
}

fn handle_convert(
    input: PathBuf,
    output: PathBuf,
    format: ConvertFormat,
    pretty: bool,
    _cli: &Cli,
) -> Result<(), String> {
    info!("Converting AST format");

    // Read input JSON
    let input_data =
        fs::read_to_string(&input).map_err(|e| format!("Could not read input file: {}", e))?;

    let input_ast = match serde_json::from_str::<SerializableDocument>(&input_data) {
        Ok(doc) => InputAst::Serializable(doc),
        Err(_) => {
            let stable: StableAstSchema = serde_json::from_str(&input_data)
                .map_err(|e| format!("Could not parse input JSON: {}", e))?;
            InputAst::Stable(stable)
        }
    };

    // Convert to target format
    let output_data = match format {
        ConvertFormat::Json => match input_ast {
            InputAst::Serializable(document) => if pretty {
                serde_json::to_string_pretty(&document)
            } else {
                serde_json::to_string(&document)
            }
            .map_err(|e| format!("JSON conversion error: {}", e))?,
            InputAst::Stable(stable) => if pretty {
                serde_json::to_string_pretty(&stable)
            } else {
                serde_json::to_string(&stable)
            }
            .map_err(|e| format!("JSON conversion error: {}", e))?,
        },
        ConvertFormat::Yaml => match input_ast {
            InputAst::Serializable(document) => serde_yaml::to_string(&document)
                .map_err(|e| format!("YAML conversion error: {}", e))?,
            InputAst::Stable(stable) => serde_yaml::to_string(&stable)
                .map_err(|e| format!("YAML conversion error: {}", e))?,
        },
        ConvertFormat::Toml => match input_ast {
            InputAst::Serializable(document) => {
                toml::to_string(&document).map_err(|e| format!("TOML conversion error: {}", e))?
            }
            InputAst::Stable(stable) => {
                toml::to_string(&stable).map_err(|e| format!("TOML conversion error: {}", e))?
            }
        },
        ConvertFormat::Xml => match input_ast {
            InputAst::Serializable(document) => convert_to_xml(&document)?,
            InputAst::Stable(_) => {
                return Err("XML conversion requires SerializableDocument input".to_string())
            }
        },
    };

    // Write output
    fs::write(&output, output_data).map_err(|e| format!("Could not write output file: {}", e))?;

    info!(
        "Conversion completed: {} -> {}",
        input.display(),
        output.display()
    );
    Ok(())
}

/// Convert a SerializableDocument to XML format
fn convert_to_xml(document: &SerializableDocument) -> Result<String, String> {
    use quick_xml::events::{BytesEnd, BytesStart, Event};
    use quick_xml::Writer;
    use std::io::Cursor;

    let mut writer = Writer::new(Cursor::new(Vec::new()));

    // Write XML declaration
    writer
        .write_event(Event::Decl(quick_xml::events::BytesDecl::new(
            "1.0",
            Some("UTF-8"),
            None,
        )))
        .map_err(|e| format!("XML writing error: {}", e))?;

    // Root element
    let mut pdf_element = BytesStart::new("pdf_document");
    pdf_element.push_attribute(("version", document.version.as_str()));
    writer
        .write_event(Event::Start(pdf_element))
        .map_err(|e| format!("XML writing error: {}", e))?;

    // Write metadata
    write_metadata_xml(&mut writer, &document.metadata)?;

    // Write AST
    write_ast_xml(&mut writer, &document.ast)?;

    // Write trailer
    write_trailer_xml(&mut writer, &document.trailer)?;

    // Write XRef entries
    write_xref_xml(&mut writer, &document.xref_entries)?;

    // Close root element
    writer
        .write_event(Event::End(BytesEnd::new("pdf_document")))
        .map_err(|e| format!("XML writing error: {}", e))?;

    let result = writer.into_inner().into_inner();
    String::from_utf8(result).map_err(|e| format!("UTF-8 conversion error: {}", e))
}

/// Write metadata section to XML
fn write_metadata_xml<W: std::io::Write>(
    writer: &mut Writer<W>,
    metadata: &SerializableDocumentMetadata,
) -> Result<(), String> {
    use quick_xml::events::{BytesEnd, BytesStart, Event};

    writer
        .write_event(Event::Start(BytesStart::new("metadata")))
        .map_err(|e| format!("XML writing error: {}", e))?;

    // Helper macro for simple elements
    macro_rules! write_element {
        ($name:expr, $value:expr) => {
            writer
                .write_event(Event::Start(BytesStart::new($name)))
                .map_err(|e| format!("XML writing error: {}", e))?;
            writer
                .write_event(Event::Text(BytesText::new(&$value.to_string())))
                .map_err(|e| format!("XML writing error: {}", e))?;
            writer
                .write_event(Event::End(BytesEnd::new($name)))
                .map_err(|e| format!("XML writing error: {}", e))?;
        };
    }

    if let Some(file_size) = metadata.file_size {
        write_element!("file_size", file_size);
    }
    write_element!("linearized", metadata.linearized);
    write_element!("encrypted", metadata.encrypted);
    write_element!("has_forms", metadata.has_forms);
    write_element!("has_xfa", metadata.has_xfa);
    write_element!("xfa_packets", metadata.xfa_packets);
    write_element!("has_xfa_scripts", metadata.has_xfa_scripts);
    write_element!("xfa_script_nodes", metadata.xfa_script_nodes);
    write_element!("has_hybrid_forms", metadata.has_hybrid_forms);
    write_element!("form_field_count", metadata.form_field_count);
    write_element!("has_javascript", metadata.has_javascript);
    write_element!("has_embedded_files", metadata.has_embedded_files);
    write_element!("has_signatures", metadata.has_signatures);
    write_element!("has_richmedia", metadata.has_richmedia);
    write_element!("richmedia_annotations", metadata.richmedia_annotations);
    write_element!("richmedia_assets", metadata.richmedia_assets);
    write_element!("richmedia_scripts", metadata.richmedia_scripts);
    write_element!("has_3d", metadata.has_3d);
    write_element!("threed_annotations", metadata.threed_annotations);
    write_element!("threed_u3d", metadata.threed_u3d);
    write_element!("threed_prc", metadata.threed_prc);
    write_element!("has_audio", metadata.has_audio);
    write_element!("audio_annotations", metadata.audio_annotations);
    write_element!("has_video", metadata.has_video);
    write_element!("video_annotations", metadata.video_annotations);
    write_element!("has_dss", metadata.has_dss);
    write_element!("dss_vri_count", metadata.dss_vri_count);
    write_element!("dss_certs", metadata.dss_certs);
    write_element!("dss_ocsp", metadata.dss_ocsp);
    write_element!("dss_crl", metadata.dss_crl);
    write_element!("dss_timestamps", metadata.dss_timestamps);
    write_element!("page_count", metadata.page_count);

    if let Some(producer) = &metadata.producer {
        write_element!("producer", producer);
    }
    if let Some(creator) = &metadata.creator {
        write_element!("creator", creator);
    }
    if let Some(creation_date) = &metadata.creation_date {
        write_element!("creation_date", creation_date);
    }
    if let Some(modification_date) = &metadata.modification_date {
        write_element!("modification_date", modification_date);
    }

    writer
        .write_event(Event::End(BytesEnd::new("metadata")))
        .map_err(|e| format!("XML writing error: {}", e))?;

    Ok(())
}

/// Write AST section to XML
fn write_ast_xml<W: std::io::Write>(
    writer: &mut Writer<W>,
    ast: &SerializableGraph,
) -> Result<(), String> {
    use quick_xml::events::{BytesEnd, BytesStart, Event};

    let mut ast_element = BytesStart::new("ast");
    if let Some(root) = ast.root {
        ast_element.push_attribute(("root", root.to_string().as_str()));
    }
    writer
        .write_event(Event::Start(ast_element))
        .map_err(|e| format!("XML writing error: {}", e))?;

    // Write graph metadata
    writer
        .write_event(Event::Start(BytesStart::new("graph_metadata")))
        .map_err(|e| format!("XML writing error: {}", e))?;

    macro_rules! write_element {
        ($name:expr, $value:expr) => {
            writer
                .write_event(Event::Start(BytesStart::new($name)))
                .map_err(|e| format!("XML writing error: {}", e))?;
            writer
                .write_event(Event::Text(BytesText::new(&$value.to_string())))
                .map_err(|e| format!("XML writing error: {}", e))?;
            writer
                .write_event(Event::End(BytesEnd::new($name)))
                .map_err(|e| format!("XML writing error: {}", e))?;
        };
    }

    write_element!("node_count", ast.metadata.node_count);
    write_element!("edge_count", ast.metadata.edge_count);
    write_element!("is_cyclic", ast.metadata.is_cyclic);
    write_element!("serialization_version", &ast.metadata.serialization_version);

    writer
        .write_event(Event::End(BytesEnd::new("graph_metadata")))
        .map_err(|e| format!("XML writing error: {}", e))?;

    // Write nodes
    writer
        .write_event(Event::Start(BytesStart::new("nodes")))
        .map_err(|e| format!("XML writing error: {}", e))?;

    for node in &ast.nodes {
        write_node_xml(writer, node)?;
    }

    writer
        .write_event(Event::End(BytesEnd::new("nodes")))
        .map_err(|e| format!("XML writing error: {}", e))?;

    // Write edges
    writer
        .write_event(Event::Start(BytesStart::new("edges")))
        .map_err(|e| format!("XML writing error: {}", e))?;

    for edge in &ast.edges {
        write_edge_xml(writer, edge)?;
    }

    writer
        .write_event(Event::End(BytesEnd::new("edges")))
        .map_err(|e| format!("XML writing error: {}", e))?;

    writer
        .write_event(Event::End(BytesEnd::new("ast")))
        .map_err(|e| format!("XML writing error: {}", e))?;

    Ok(())
}

/// Write a single node to XML
fn write_node_xml<W: std::io::Write>(
    writer: &mut Writer<W>,
    node: &SerializableNode,
) -> Result<(), String> {
    use quick_xml::events::{BytesEnd, BytesStart, Event};

    let mut node_element = BytesStart::new("node");
    node_element.push_attribute(("id", node.id.to_string().as_str()));
    node_element.push_attribute(("type", node.node_type.as_str()));

    if let Some((obj_id, generation)) = node.object_id {
        node_element.push_attribute(("object_id", obj_id.to_string().as_str()));
        node_element.push_attribute(("generation", generation.to_string().as_str()));
    }

    writer
        .write_event(Event::Start(node_element))
        .map_err(|e| format!("XML writing error: {}", e))?;

    // Write the value
    write_value_xml(writer, &node.value, "value")?;

    writer
        .write_event(Event::End(BytesEnd::new("node")))
        .map_err(|e| format!("XML writing error: {}", e))?;

    Ok(())
}

/// Write a single edge to XML  
fn write_edge_xml<W: std::io::Write>(
    writer: &mut Writer<W>,
    edge: &SerializableEdge,
) -> Result<(), String> {
    use quick_xml::events::{BytesEnd, BytesStart, Event};

    let mut edge_element = BytesStart::new("edge");
    edge_element.push_attribute(("from", edge.from.to_string().as_str()));
    edge_element.push_attribute(("to", edge.to.to_string().as_str()));
    edge_element.push_attribute(("type", edge.edge_type.as_str()));

    writer
        .write_event(Event::Start(edge_element))
        .map_err(|e| format!("XML writing error: {}", e))?;
    writer
        .write_event(Event::End(BytesEnd::new("edge")))
        .map_err(|e| format!("XML writing error: {}", e))?;

    Ok(())
}

/// Write a serializable value to XML
fn write_value_xml<W: std::io::Write>(
    writer: &mut Writer<W>,
    value: &SerializableValue,
    element_name: &str,
) -> Result<(), String> {
    use quick_xml::events::{BytesEnd, BytesStart, Event};

    let mut value_element = BytesStart::new(element_name);

    match value {
        SerializableValue::Null => {
            value_element.push_attribute(("type", "null"));
            writer
                .write_event(Event::Start(value_element))
                .map_err(|e| format!("XML writing error: {}", e))?;
        }
        SerializableValue::Boolean(b) => {
            value_element.push_attribute(("type", "boolean"));
            writer
                .write_event(Event::Start(value_element))
                .map_err(|e| format!("XML writing error: {}", e))?;
            writer
                .write_event(Event::Text(BytesText::new(&b.to_string())))
                .map_err(|e| format!("XML writing error: {}", e))?;
        }
        SerializableValue::Integer(i) => {
            value_element.push_attribute(("type", "integer"));
            writer
                .write_event(Event::Start(value_element))
                .map_err(|e| format!("XML writing error: {}", e))?;
            writer
                .write_event(Event::Text(BytesText::new(&i.to_string())))
                .map_err(|e| format!("XML writing error: {}", e))?;
        }
        SerializableValue::Real(r) => {
            value_element.push_attribute(("type", "real"));
            writer
                .write_event(Event::Start(value_element))
                .map_err(|e| format!("XML writing error: {}", e))?;
            writer
                .write_event(Event::Text(BytesText::new(&r.to_string())))
                .map_err(|e| format!("XML writing error: {}", e))?;
        }
        SerializableValue::String(s) => {
            value_element.push_attribute(("type", "string"));
            writer
                .write_event(Event::Start(value_element))
                .map_err(|e| format!("XML writing error: {}", e))?;
            writer
                .write_event(Event::Text(BytesText::new(s)))
                .map_err(|e| format!("XML writing error: {}", e))?;
        }
        SerializableValue::Name(n) => {
            value_element.push_attribute(("type", "name"));
            writer
                .write_event(Event::Start(value_element))
                .map_err(|e| format!("XML writing error: {}", e))?;
            writer
                .write_event(Event::Text(BytesText::new(n)))
                .map_err(|e| format!("XML writing error: {}", e))?;
        }
        SerializableValue::Array(arr) => {
            value_element.push_attribute(("type", "array"));
            writer
                .write_event(Event::Start(value_element))
                .map_err(|e| format!("XML writing error: {}", e))?;
            for (i, item) in arr.iter().enumerate() {
                write_value_xml(writer, item, &format!("item_{}", i))?;
            }
        }
        SerializableValue::Dictionary(dict) => {
            value_element.push_attribute(("type", "dictionary"));
            writer
                .write_event(Event::Start(value_element))
                .map_err(|e| format!("XML writing error: {}", e))?;
            for (key, val) in dict {
                writer
                    .write_event(Event::Start(BytesStart::new("entry")))
                    .map_err(|e| format!("XML writing error: {}", e))?;

                writer
                    .write_event(Event::Start(BytesStart::new("key")))
                    .map_err(|e| format!("XML writing error: {}", e))?;
                writer
                    .write_event(Event::Text(BytesText::new(key)))
                    .map_err(|e| format!("XML writing error: {}", e))?;
                writer
                    .write_event(Event::End(BytesEnd::new("key")))
                    .map_err(|e| format!("XML writing error: {}", e))?;

                write_value_xml(writer, val, "value")?;

                writer
                    .write_event(Event::End(BytesEnd::new("entry")))
                    .map_err(|e| format!("XML writing error: {}", e))?;
            }
        }
        SerializableValue::Stream {
            dictionary, data, ..
        } => {
            value_element.push_attribute(("type", "stream"));
            writer
                .write_event(Event::Start(value_element))
                .map_err(|e| format!("XML writing error: {}", e))?;

            write_value_xml(
                writer,
                &SerializableValue::Dictionary(dictionary.clone()),
                "dictionary",
            )?;

            writer
                .write_event(Event::Start(BytesStart::new("data")))
                .map_err(|e| format!("XML writing error: {}", e))?;
            let base64_data = base64_encode(data);
            writer
                .write_event(Event::Text(BytesText::new(&base64_data)))
                .map_err(|e| format!("XML writing error: {}", e))?;
            writer
                .write_event(Event::End(BytesEnd::new("data")))
                .map_err(|e| format!("XML writing error: {}", e))?;
        }
        SerializableValue::Reference {
            object_id,
            generation,
        } => {
            value_element.push_attribute(("type", "reference"));
            value_element.push_attribute(("object_id", object_id.to_string().as_str()));
            value_element.push_attribute(("generation", generation.to_string().as_str()));
            writer
                .write_event(Event::Start(value_element))
                .map_err(|e| format!("XML writing error: {}", e))?;
        }
    }

    writer
        .write_event(Event::End(BytesEnd::new(element_name)))
        .map_err(|e| format!("XML writing error: {}", e))?;

    Ok(())
}

/// Write trailer section to XML
fn write_trailer_xml<W: std::io::Write>(
    writer: &mut Writer<W>,
    trailer: &SerializableValue,
) -> Result<(), String> {
    use quick_xml::events::{BytesEnd, BytesStart, Event};

    writer
        .write_event(Event::Start(BytesStart::new("trailer")))
        .map_err(|e| format!("XML writing error: {}", e))?;

    write_value_xml(writer, trailer, "trailer_dict")?;

    writer
        .write_event(Event::End(BytesEnd::new("trailer")))
        .map_err(|e| format!("XML writing error: {}", e))?;

    Ok(())
}

/// Write xref entries to XML
fn write_xref_xml<W: std::io::Write>(
    writer: &mut Writer<W>,
    xref_entries: &std::collections::HashMap<String, SerializableXRefEntry>,
) -> Result<(), String> {
    use quick_xml::events::{BytesEnd, BytesStart, Event};

    writer
        .write_event(Event::Start(BytesStart::new("xref_table")))
        .map_err(|e| format!("XML writing error: {}", e))?;

    for (key, entry) in xref_entries {
        let mut entry_element = BytesStart::new("xref_entry");
        entry_element.push_attribute(("key", key.as_str()));
        entry_element.push_attribute(("type", entry.entry_type.as_str()));
        entry_element.push_attribute(("generation", entry.generation.to_string().as_str()));

        if let Some(offset) = entry.offset {
            entry_element.push_attribute(("offset", offset.to_string().as_str()));
        }

        writer
            .write_event(Event::Start(entry_element))
            .map_err(|e| format!("XML writing error: {}", e))?;
        writer
            .write_event(Event::End(BytesEnd::new("xref_entry")))
            .map_err(|e| format!("XML writing error: {}", e))?;
    }

    writer
        .write_event(Event::End(BytesEnd::new("xref_table")))
        .map_err(|e| format!("XML writing error: {}", e))?;

    Ok(())
}

/// Simple base64 encoding for binary data
fn base64_encode(data: &[u8]) -> String {
    // Simple base64 implementation
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = Vec::new();

    let chunks = data.chunks_exact(3);
    let remainder = chunks.remainder();

    for chunk in chunks {
        let b1 = chunk[0] as u32;
        let b2 = chunk[1] as u32;
        let b3 = chunk[2] as u32;
        let n = (b1 << 16) | (b2 << 8) | b3;

        result.push(CHARS[((n >> 18) & 63) as usize]);
        result.push(CHARS[((n >> 12) & 63) as usize]);
        result.push(CHARS[((n >> 6) & 63) as usize]);
        result.push(CHARS[(n & 63) as usize]);
    }

    match remainder.len() {
        1 => {
            let n = (remainder[0] as u32) << 16;
            result.push(CHARS[((n >> 18) & 63) as usize]);
            result.push(CHARS[((n >> 12) & 63) as usize]);
            result.push(b'=');
            result.push(b'=');
        }
        2 => {
            let n = ((remainder[0] as u32) << 16) | ((remainder[1] as u32) << 8);
            result.push(CHARS[((n >> 18) & 63) as usize]);
            result.push(CHARS[((n >> 12) & 63) as usize]);
            result.push(CHARS[((n >> 6) & 63) as usize]);
            result.push(b'=');
        }
        _ => {}
    }

    String::from_utf8(result).unwrap_or_default()
}

fn handle_benchmark(
    input: PathBuf,
    iterations: usize,
    output: Option<PathBuf>,
    memory: bool,
    cli: &Cli,
) -> Result<(), String> {
    info!("Starting benchmark with {} iterations", iterations);

    if input.is_file() {
        benchmark_single_file(&input, iterations, output.as_deref(), memory, cli)
    } else {
        Err("Benchmark currently only supports single files".to_string())
    }
}

fn benchmark_single_file(
    input: &Path,
    iterations: usize,
    output: Option<&Path>,
    _memory: bool,
    _cli: &Cli,
) -> Result<(), String> {
    let mut parse_times = Vec::new();
    let mut total_times = Vec::new();

    for i in 1..=iterations {
        println!("Iteration {}/{}", i, iterations);

        let start_time = Instant::now();

        let file = fs::File::open(input).map_err(|e| format!("Could not open file: {}", e))?;

        let buf_reader = BufReader::new(file);
        let parser = PdfParser::new();

        let parse_start = Instant::now();
        let document = parser
            .parse(buf_reader)
            .map_err(|e| format!("Parse error: {:?}", e))?;
        let parse_time = parse_start.elapsed();

        let _json = to_json(&document).map_err(|e| format!("JSON serialization error: {:?}", e))?;

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

    // Save results if requested
    if let Some(output_path) = output {
        let results = serde_json::json!({
            "iterations": iterations,
            "parse_times_ms": parse_times.iter().map(|d| d.as_millis()).collect::<Vec<_>>(),
            "total_times_ms": total_times.iter().map(|d| d.as_millis()).collect::<Vec<_>>(),
            "avg_parse_ms": avg_parse.as_millis(),
            "avg_total_ms": avg_total.as_millis()
        });

        fs::write(output_path, serde_json::to_string_pretty(&results).unwrap())
            .map_err(|e| format!("Could not save benchmark results: {}", e))?;

        info!("Benchmark results saved to: {}", output_path.display());
    }

    Ok(())
}

fn collect_pdf_files(dir: &Path, recursive: bool) -> Result<Vec<PathBuf>, String> {
    let mut pdf_files = Vec::new();

    let entries = fs::read_dir(dir)
        .map_err(|e| format!("Could not read directory {}: {}", dir.display(), e))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("Directory entry error: {}", e))?;
        let path = entry.path();

        if path.is_file() {
            if is_probable_pdf(&path) {
                pdf_files.push(path);
            }
        } else if path.is_dir() && recursive {
            let mut sub_files = collect_pdf_files(&path, recursive)?;
            pdf_files.append(&mut sub_files);
        }
    }

    Ok(pdf_files)
}

fn is_probable_pdf(path: &Path) -> bool {
    if let Some(ext) = path.extension() {
        if ext.to_string_lossy().to_lowercase() == "pdf" {
            return true;
        }
    }

    // Handle extensionless PDFs by checking the header magic.
    if path.extension().is_none() {
        if let Ok(mut file) = fs::File::open(path) {
            let mut header = [0u8; 5];
            if let Ok(read) = std::io::Read::read(&mut file, &mut header) {
                return read == 5 && &header == b"%PDF-";
            }
        }
    }

    false
}

fn handle_query(
    input: PathBuf,
    query: String,
    format: OutputFormat,
    limit: Option<usize>,
    _cli: &Cli,
) -> Result<(), String> {
    info!("Executing query: {}", query);

    // Read AST file
    let ast_data =
        fs::read_to_string(&input).map_err(|e| format!("Could not read AST file: {}", e))?;

    let graph = load_graph_from_ast_json(&ast_data)?;

    // Parse query selector
    let selector = QueryParser::parse(&query).map_err(|e| format!("Query parsing error: {}", e))?;

    // Execute query
    let mut query_engine = QueryEngine::new(&graph);
    let mut results = query_engine.query(&selector);

    // Apply limit
    if let Some(limit) = limit {
        results.truncate(limit);
    }

    // Output results
    println!("Query: {}", query);
    println!("Results: {} nodes", results.len());

    #[derive(serde::Serialize)]
    struct NodeOutput {
        index: usize,
        node_id: usize,
    }

    for (i, node_id) in results.iter().enumerate() {
        match format {
            OutputFormat::Pretty => println!("{}. Node {}", i + 1, node_id.index()),
            OutputFormat::Json => {
                let node_data = NodeOutput {
                    index: i,
                    node_id: node_id.index(),
                };
                println!("{}", serde_json::to_string(&node_data).unwrap());
            }
            OutputFormat::Yaml => {
                let node_data = NodeOutput {
                    index: i,
                    node_id: node_id.index(),
                };
                println!("{}", serde_yaml::to_string(&node_data).unwrap());
            }
            OutputFormat::Toml => {
                let node_data = NodeOutput {
                    index: i,
                    node_id: node_id.index(),
                };
                println!("{}", toml::to_string(&node_data).unwrap());
            }
            OutputFormat::Compact => print!("{} ", node_id.index()),
            OutputFormat::Table => println!("{:<4} {}", i + 1, node_id.index()),
        }
    }

    if matches!(format, OutputFormat::Compact) {
        println!(); // New line after compact output
    }

    Ok(())
}

fn handle_stream(
    input: PathBuf,
    output: Option<PathBuf>,
    report: Option<PathBuf>,
    cli: &Cli,
) -> Result<(), String> {
    info!("Starting streaming parse: {}", input.display());

    if !input.is_file() {
        return Err("Streaming mode only supports single files".to_string());
    }

    let input_path = input
        .to_str()
        .ok_or_else(|| "Invalid input path".to_string())?;
    let (document, incremental) =
        parse_large_pdf(input_path).map_err(|e| format!("Streaming parse error: {:?}", e))?;

    let json = to_json(&document).map_err(|e| format!("JSON serialization error: {:?}", e))?;

    match output {
        Some(output_path) => {
            fs::write(&output_path, &json)
                .map_err(|e| format!("Could not write to {}: {}", output_path.display(), e))?;
            info!("AST saved to: {}", output_path.display());
        }
        None => {
            println!("{}", json);
        }
    }

    if let Some(report_path) = report {
        let chunks: Vec<serde_json::Value> = incremental
            .chunks_processed
            .iter()
            .map(|chunk| {
                serde_json::json!({
                    "offset": chunk.offset,
                    "size": chunk.size,
                    "chunk_type": format!("{:?}", chunk.chunk_type),
                    "nodes_found": chunk.nodes_found,
                    "processing_time_ms": chunk.processing_time_ms,
                })
            })
            .collect();

        let report_json = serde_json::json!({
            "total_bytes": incremental.total_bytes,
            "total_nodes": incremental.total_nodes,
            "processing_time_ms": incremental.processing_time_ms,
            "memory_peak_mb": incremental.memory_peak_mb,
            "chunks": chunks,
        });

        let report_data = serde_json::to_string_pretty(&report_json)
            .map_err(|e| format!("Report serialization error: {}", e))?;
        fs::write(&report_path, report_data)
            .map_err(|e| format!("Could not write report: {}", e))?;
        info!("Streaming report saved to: {}", report_path.display());
    } else if cli.verbose {
        println!("Streaming summary:");
        println!("  Total bytes: {}", incremental.total_bytes);
        println!("  Total nodes: {}", incremental.total_nodes);
        println!("  Processing time (ms): {}", incremental.processing_time_ms);
        println!("  Memory peak (MB): {:.2}", incremental.memory_peak_mb);
        println!("  Chunks processed: {}", incremental.chunks_processed.len());
    }

    Ok(())
}

fn handle_schema_command(
    generate: bool,
    input: Option<PathBuf>,
    output: Option<PathBuf>,
    version: String,
    _cli: &Cli,
) -> Result<(), String> {
    if generate {
        info!("Generating JSON schema");

        let schema = generate_json_schema();
        let schema_str = serde_json::to_string_pretty(&schema)
            .map_err(|e| format!("Schema serialization error: {}", e))?;

        match output {
            Some(output_path) => {
                fs::write(output_path, schema_str)
                    .map_err(|e| format!("Could not write schema: {}", e))?;
                println!("Schema generated successfully");
            }
            None => println!("{}", schema_str),
        }
    } else {
        // Migration mode
        let input_path = input.ok_or("Input file required for migration")?;
        let output_path = output.ok_or("Output file required for migration")?;

        info!("Migrating AST schema to version {}", version);

        // Read input AST
        let ast_data = fs::read_to_string(&input_path)
            .map_err(|e| format!("Could not read input file: {}", e))?;

        let stable_ast = load_stable_ast_for_migration(&ast_data)?;

        // Migrate
        let migrator = SchemaMigrator::new();
        let target_version = pdf_ast::schema::SchemaVersion::from_string(&version)
            .map_err(|e| format!("Invalid version format: {}", e))?;

        let migrated_ast = migrator
            .migrate(stable_ast, target_version)
            .map_err(|e| format!("Migration failed: {}", e))?;

        // Write output
        let output_data = serde_json::to_string_pretty(&migrated_ast)
            .map_err(|e| format!("Output serialization error: {}", e))?;

        fs::write(&output_path, output_data)
            .map_err(|e| format!("Could not write output file: {}", e))?;

        println!(
            "Migration completed: {} -> {}",
            input_path.display(),
            output_path.display()
        );
    }

    Ok(())
}

#[allow(clippy::large_enum_variant)]
enum InputAst {
    Serializable(SerializableDocument),
    Stable(StableAstSchema),
}

fn load_graph_from_ast_json(ast_data: &str) -> Result<PdfAstGraph, String> {
    if let Ok(stable_ast) = serde_json::from_str::<StableAstSchema>(ast_data) {
        let current_version = SchemaVersion::current();
        let stable_ast = if !stable_ast.version.is_compatible_with(&current_version) {
            let migrator = SchemaMigrator::new();
            migrator
                .migrate(stable_ast, current_version)
                .map_err(|e| format!("Schema migration failed: {}", e))?
        } else {
            stable_ast
        };

        return stable_ast
            .to_graph()
            .map_err(|e| format!("Could not build graph from schema: {}", e));
    }

    let serializable: SerializableDocument =
        serde_json::from_str(ast_data).map_err(|e| format!("Could not parse AST JSON: {}", e))?;

    GraphDeserializer::deserialize(serializable.ast)
}

fn load_stable_ast_for_migration(ast_data: &str) -> Result<StableAstSchema, String> {
    if let Ok(stable_ast) = serde_json::from_str::<StableAstSchema>(ast_data) {
        return Ok(stable_ast);
    }

    let serializable: SerializableDocument =
        serde_json::from_str(ast_data).map_err(|e| format!("Could not parse input AST: {}", e))?;

    let graph = GraphDeserializer::deserialize(serializable.ast)?;
    let mut exporter = SchemaExporter::new(true);
    Ok(exporter.export(&graph))
}
