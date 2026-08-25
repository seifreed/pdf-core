use crate::Severity;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Threat {
    pub threat_type: ThreatType,
    pub severity: Severity,
    pub title: String,
    pub description: String,
    pub location: String,
    pub evidence: Vec<String>,
    pub mitigation: String,
    pub cve_references: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThreatType {
    JavaScript,
    EmbeddedExecutable,
    SuspiciousAction,
    FormDataExfiltration,
    UnencryptedSensitiveData,
    OutboundConnection,
    FileSystemAccess,
    Exploit,
    Malware,
    PrivacyRisk,
    DataLeakage,
    PhishingRisk,
    SocialEngineering,
}

impl Threat {
    pub fn javascript(location: String, script_content: &str) -> Self {
        let severity = if Self::is_suspicious_javascript(script_content) {
            Severity::High
        } else {
            Severity::Medium
        };

        Threat {
            threat_type: ThreatType::JavaScript,
            severity,
            title: "JavaScript Code Detected".to_string(),
            description: "PDF contains executable JavaScript code which could be used for malicious purposes.".to_string(),
            location,
            evidence: vec![format!("JavaScript: {}", Self::truncate_evidence(script_content))],
            mitigation: "Review JavaScript code for malicious intent. Consider disabling JavaScript in PDF viewer.".to_string(),
            cve_references: vec!["CVE-2009-0927".to_string(), "CVE-2010-1241".to_string()],
        }
    }

    pub fn embedded_executable(location: String, file_name: &str, file_type: &str) -> Self {
        Threat {
            threat_type: ThreatType::EmbeddedExecutable,
            severity: Severity::Critical,
            title: "Embedded Executable File".to_string(),
            description:
                "PDF contains an embedded executable file that could be automatically launched."
                    .to_string(),
            location,
            evidence: vec![format!("File: {} (Type: {})", file_name, file_type)],
            mitigation: "Do not open embedded executables. Scan with antivirus before extraction."
                .to_string(),
            cve_references: vec!["CVE-2010-0188".to_string()],
        }
    }

    pub fn suspicious_action(location: String, action_type: &str, details: &str) -> Self {
        let severity = match action_type {
            "Launch" => Severity::Critical,
            "URI" => Severity::Medium,
            "SubmitForm" => Severity::High,
            "ImportData" => Severity::High,
            _ => Severity::Low,
        };

        Threat {
            threat_type: ThreatType::SuspiciousAction,
            severity,
            title: format!("{} Action Detected", action_type),
            description: format!(
                "PDF contains a {} action which could be used maliciously.",
                action_type
            ),
            location,
            evidence: vec![details.to_string()],
            mitigation: match action_type {
                "Launch" => "Never allow PDFs to launch external applications.".to_string(),
                "URI" => "Verify URL destination before allowing navigation.".to_string(),
                "SubmitForm" => "Review form submission destination for legitimacy.".to_string(),
                _ => "Review action for malicious intent.".to_string(),
            },
            cve_references: vec![],
        }
    }

    pub fn outbound_connection(location: String, url: &str) -> Self {
        let severity = if Self::is_suspicious_url(url) {
            Severity::High
        } else {
            Severity::Medium
        };

        Threat {
            threat_type: ThreatType::OutboundConnection,
            severity,
            title: "Outbound Network Connection".to_string(),
            description: "PDF attempts to connect to external servers.".to_string(),
            location,
            evidence: vec![format!("URL: {}", url)],
            mitigation: "Review destination for legitimacy. Block network access if suspicious."
                .to_string(),
            cve_references: vec![],
        }
    }

    pub fn form_data_exfiltration(location: String, submit_url: &str) -> Self {
        Threat {
            threat_type: ThreatType::FormDataExfiltration,
            severity: Severity::High,
            title: "Form Data Exfiltration Risk".to_string(),
            description: "PDF form configured to submit data to external server.".to_string(),
            location,
            evidence: vec![format!("Submit URL: {}", submit_url)],
            mitigation: "Verify form submission destination. Do not enter sensitive data."
                .to_string(),
            cve_references: vec![],
        }
    }

    pub fn privacy_risk(location: String, risk_type: &str, details: &str) -> Self {
        Threat {
            threat_type: ThreatType::PrivacyRisk,
            severity: Severity::Medium,
            title: format!("Privacy Risk: {}", risk_type),
            description: "PDF contains features that may compromise user privacy.".to_string(),
            location,
            evidence: vec![details.to_string()],
            mitigation: "Review privacy implications before sharing document.".to_string(),
            cve_references: vec![],
        }
    }

    fn is_suspicious_javascript(script: &str) -> bool {
        let suspicious_patterns = [
            "eval(",
            "document.write",
            "unescape(",
            "ActiveXObject",
            "WScript.Shell",
            "new Array(",
            "fromCharCode",
            "shellcode",
            "exploit",
            "payload",
            "app.launchURL",
            "this.print",
            "app.execDialog",
        ];

        let script_lower = script.to_lowercase();
        suspicious_patterns
            .iter()
            .any(|pattern| script_lower.contains(pattern))
    }

    fn is_suspicious_url(url: &str) -> bool {
        let suspicious_domains = ["bit.ly", "tinyurl.com", "t.co", "goo.gl", "ow.ly", "is.gd"];

        let suspicious_schemes = ["ftp://", "file://", "javascript:", "data:"];

        let url_lower = url.to_lowercase();

        suspicious_domains.iter().any(|domain| url_lower.contains(domain)) ||
        suspicious_schemes.iter().any(|scheme| url_lower.starts_with(scheme)) ||
        url_lower.contains("..") || // Path traversal
        url_lower.contains("localhost") ||
        url_lower.contains("127.0.0.1") ||
        url_lower.contains("192.168.") ||
        url_lower.contains("10.") ||
        url_lower.matches('.').count() > 6 // Suspiciously long domain
    }

    fn truncate_evidence(text: &str) -> String {
        if text.len() > 200 {
            format!("{}...", &text[..197])
        } else {
            text.to_string()
        }
    }
}

impl std::fmt::Display for ThreatType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ThreatType::JavaScript => write!(f, "JavaScript"),
            ThreatType::EmbeddedExecutable => write!(f, "Embedded Executable"),
            ThreatType::SuspiciousAction => write!(f, "Suspicious Action"),
            ThreatType::FormDataExfiltration => write!(f, "Form Data Exfiltration"),
            ThreatType::UnencryptedSensitiveData => write!(f, "Unencrypted Sensitive Data"),
            ThreatType::OutboundConnection => write!(f, "Outbound Connection"),
            ThreatType::FileSystemAccess => write!(f, "File System Access"),
            ThreatType::Exploit => write!(f, "Exploit"),
            ThreatType::Malware => write!(f, "Malware"),
            ThreatType::PrivacyRisk => write!(f, "Privacy Risk"),
            ThreatType::DataLeakage => write!(f, "Data Leakage"),
            ThreatType::PhishingRisk => write!(f, "Phishing Risk"),
            ThreatType::SocialEngineering => write!(f, "Social Engineering"),
        }
    }
}
