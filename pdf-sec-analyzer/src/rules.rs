use crate::threats::ThreatType;
use crate::{Severity, Threat, Warning};
use pdf_ast::{PdfDictionary, PdfValue};
use regex::Regex;

pub struct SecurityRules;

impl SecurityRules {
    pub fn analyze_javascript(script: &str) -> Vec<Threat> {
        let mut threats = Vec::new();

        // Obfuscation detection
        if Self::is_obfuscated_javascript(script) {
            threats.push(Threat {
                threat_type: ThreatType::JavaScript,
                severity: Severity::High,
                title: "Obfuscated JavaScript Detected".to_string(),
                description: "JavaScript code appears to be heavily obfuscated, indicating possible malicious intent.".to_string(),
                location: "JavaScript analysis".to_string(),
                evidence: vec!["High obfuscation score detected".to_string()],
                mitigation: "Do not execute obfuscated JavaScript. Consider malicious.".to_string(),
                cve_references: vec![],
            });
        }

        // Shellcode patterns
        if Self::contains_shellcode_patterns(script) {
            threats.push(Threat {
                threat_type: ThreatType::Exploit,
                severity: Severity::Critical,
                title: "Shellcode Pattern in JavaScript".to_string(),
                description: "JavaScript contains patterns consistent with shellcode injection."
                    .to_string(),
                location: "JavaScript analysis".to_string(),
                evidence: vec!["Shellcode-like patterns detected".to_string()],
                mitigation: "Block immediately. High likelihood of exploit attempt.".to_string(),
                cve_references: vec!["CVE-2009-0927".to_string(), "CVE-2010-1241".to_string()],
            });
        }

        // Heap spray detection
        if Self::contains_heap_spray(script) {
            threats.push(Threat {
                threat_type: ThreatType::Exploit,
                severity: Severity::Critical,
                title: "Heap Spray Technique Detected".to_string(),
                description: "JavaScript contains heap spraying technique used in memory corruption exploits.".to_string(),
                location: "JavaScript analysis".to_string(),
                evidence: vec!["Heap spray pattern detected".to_string()],
                mitigation: "Block immediately. Classic exploit technique.".to_string(),
                cve_references: vec!["CVE-2008-2992".to_string(), "CVE-2009-0658".to_string()],
            });
        }

        threats
    }

    pub fn analyze_url(url: &str) -> Vec<Threat> {
        let mut threats = Vec::new();

        // Suspicious URL patterns
        if Self::is_suspicious_url_pattern(url) {
            threats.push(Threat {
                threat_type: ThreatType::PhishingRisk,
                severity: Severity::High,
                title: "Suspicious URL Pattern".to_string(),
                description:
                    "URL contains patterns commonly used in phishing or malware distribution."
                        .to_string(),
                location: "URL analysis".to_string(),
                evidence: vec![format!("URL: {}", url)],
                mitigation: "Verify URL legitimacy before accessing.".to_string(),
                cve_references: vec![],
            });
        }

        // Data exfiltration URLs
        if Self::is_data_exfiltration_url(url) {
            threats.push(Threat {
                threat_type: ThreatType::DataLeakage,
                severity: Severity::High,
                title: "Data Exfiltration URL".to_string(),
                description: "URL appears designed for data collection or exfiltration."
                    .to_string(),
                location: "URL analysis".to_string(),
                evidence: vec![format!("URL: {}", url)],
                mitigation: "Block data submission to this URL.".to_string(),
                cve_references: vec![],
            });
        }

        threats
    }

    pub fn analyze_embedded_file(file_dict: &PdfDictionary) -> Vec<Threat> {
        let mut threats = Vec::new();

        // Check file type
        if let Some(PdfValue::String(filename)) = file_dict.get("F") {
            let filename = filename.to_string_lossy();

            // Double extension check
            if Self::has_double_extension(&filename) {
                threats.push(Threat {
                    threat_type: ThreatType::SocialEngineering,
                    severity: Severity::High,
                    title: "Double Extension Filename".to_string(),
                    description: "Embedded file uses double extension to disguise file type."
                        .to_string(),
                    location: "Embedded file analysis".to_string(),
                    evidence: vec![format!("Filename: {}", filename)],
                    mitigation: "Verify actual file type before opening.".to_string(),
                    cve_references: vec![],
                });
            }

            // Hidden executable extensions
            if Self::is_hidden_executable(&filename) {
                threats.push(Threat {
                    threat_type: ThreatType::EmbeddedExecutable,
                    severity: Severity::Critical,
                    title: "Hidden Executable File".to_string(),
                    description:
                        "Embedded file appears to be an executable disguised as another file type."
                            .to_string(),
                    location: "Embedded file analysis".to_string(),
                    evidence: vec![format!("Filename: {}", filename)],
                    mitigation: "Do not execute. Scan with antivirus.".to_string(),
                    cve_references: vec![],
                });
            }
        }

        threats
    }

    fn is_obfuscated_javascript(script: &str) -> bool {
        let obfuscation_indicators = [
            // High ratio of special characters
            (script
                .chars()
                .filter(|c| !c.is_alphanumeric() && !c.is_whitespace())
                .count() as f32
                / script.len() as f32)
                > 0.4,
            // Long variable names with numbers/underscores
            script.contains("var "),
            Regex::new(r"[a-zA-Z_$][a-zA-Z0-9_$]{20,}")
                .unwrap()
                .is_match(script),
            // Frequent use of escape sequences
            script.matches("\\x").count() > 10,
            script.matches("\\u").count() > 5,
            // String concatenation patterns
            script.matches(" + ").count() > 20,
            // eval/unescape patterns
            script.contains("eval(")
                && (script.contains("unescape(") || script.contains("fromCharCode")),
        ];

        obfuscation_indicators
            .iter()
            .filter(|&&indicator| indicator)
            .count()
            >= 3
    }

    fn contains_shellcode_patterns(script: &str) -> bool {
        let shellcode_patterns = [
            r"\\x[0-9a-fA-F]{2}", // Hex encoding
            r"\\u[0-9a-fA-F]{4}", // Unicode encoding
            r"String\.fromCharCode",
            r"unescape\(",
            r"eval\(",
        ];

        let hex_pattern = Regex::new(r"\\x[0-9a-fA-F]{2}").unwrap();
        let hex_count = hex_pattern.find_iter(script).count();

        // High density of hex sequences indicates shellcode
        hex_count > 50
            && shellcode_patterns
                .iter()
                .filter(|&&pattern| Regex::new(pattern).unwrap().is_match(script))
                .count()
                >= 2
    }

    fn contains_heap_spray(script: &str) -> bool {
        // Look for patterns indicating heap spraying
        let heap_spray_indicators = ["new Array(", ".length", "for(", "String("];

        let has_large_array = script.contains("new Array(")
            && (script.contains("0x") || Regex::new(r"\d{4,}").unwrap().is_match(script));

        let has_loop = script.contains("for(") || script.contains("while(");

        has_large_array
            && has_loop
            && heap_spray_indicators
                .iter()
                .filter(|&&pattern| script.contains(pattern))
                .count()
                >= 3
    }

    fn is_suspicious_url_pattern(url: &str) -> bool {
        let url_lower = url.to_lowercase();

        // IP addresses instead of domains
        let ip_pattern = Regex::new(r"https?://\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}").unwrap();
        if ip_pattern.is_match(&url_lower) {
            return true;
        }

        // Suspicious TLDs
        let suspicious_tlds = [".tk", ".ml", ".ga", ".cf", ".cc", ".info", ".biz"];
        if suspicious_tlds.iter().any(|&tld| url_lower.contains(tld)) {
            return true;
        }

        // URL shorteners
        let shorteners = ["bit.ly", "tinyurl", "t.co", "goo.gl", "ow.ly", "is.gd"];
        if shorteners
            .iter()
            .any(|&shortener| url_lower.contains(shortener))
        {
            return true;
        }

        // Suspicious paths
        let suspicious_paths = ["/download", "/exe", "/install", "/setup", "/update"];
        if suspicious_paths
            .iter()
            .any(|&path| url_lower.contains(path))
        {
            return true;
        }

        false
    }

    fn is_data_exfiltration_url(url: &str) -> bool {
        let url_lower = url.to_lowercase();

        // Common data collection parameters
        let data_params = ["?data=", "&data=", "?info=", "&info=", "?user=", "&user="];
        if data_params.iter().any(|&param| url_lower.contains(param)) {
            return true;
        }

        // Base64 encoded parameters (possible data exfiltration)
        if url_lower.contains("=") && url.len() > 100 {
            let query_part = url.split('?').nth(1).unwrap_or("");
            if query_part.len() > 50
                && query_part
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "=&".contains(c))
            {
                return true;
            }
        }

        false
    }

    fn has_double_extension(filename: &str) -> bool {
        let extensions = [".txt", ".pdf", ".doc", ".jpg", ".png", ".zip"];
        let hidden_exts = [".exe", ".scr", ".bat", ".com", ".pif"];

        let filename_lower = filename.to_lowercase();

        extensions.iter().any(|&ext1| {
            hidden_exts
                .iter()
                .any(|&ext2| filename_lower.ends_with(&format!("{}{}", ext1, ext2)))
        })
    }

    fn is_hidden_executable(filename: &str) -> bool {
        let filename_lower = filename.to_lowercase();

        // Files with spaces before extension (hiding technique)
        if filename_lower.matches(" .exe").count() > 0 {
            return true;
        }

        // Right-to-left override character (Unicode U+202E)
        if filename.contains('\u{202E}') {
            return true;
        }

        false
    }
}
