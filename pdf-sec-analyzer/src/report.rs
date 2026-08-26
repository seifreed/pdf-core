use crate::{InfoItem, ScanResult, Severity, Threat};
use serde_json;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct SecurityReport {
    pub result: ScanResult,
}

impl SecurityReport {
    pub fn new(result: ScanResult) -> Self {
        SecurityReport { result }
    }

    pub fn risk_score(&self) -> u32 {
        self.result
            .threats
            .iter()
            .map(|t| match t.severity {
                Severity::Critical => 100,
                Severity::High => 50,
                Severity::Medium => 20,
                Severity::Low => 5,
            })
            .sum::<u32>()
            .min(1000) // Cap at 1000
    }

    pub fn risk_level(&self) -> &'static str {
        match self.risk_score() {
            0..=20 => "LOW",
            21..=100 => "MEDIUM",
            101..=200 => "HIGH",
            _ => "CRITICAL",
        }
    }

    pub fn is_safe(&self) -> bool {
        self.risk_score() <= 20
            && !self
                .result
                .threats
                .iter()
                .any(|t| t.severity >= Severity::High)
    }

    pub fn critical_threats(&self) -> Vec<&Threat> {
        self.result
            .threats
            .iter()
            .filter(|t| t.severity == Severity::Critical)
            .collect()
    }

    pub fn high_threats(&self) -> Vec<&Threat> {
        self.result
            .threats
            .iter()
            .filter(|t| t.severity == Severity::High)
            .collect()
    }

    pub fn summary(&self) -> String {
        format!(
            "Security Report Summary:\n\
             Risk Level: {} (Score: {})\n\
             Threats: {} (Critical: {}, High: {}, Medium: {}, Low: {})\n\
             Warnings: {}\n\
             Scan Duration: {}ms\n\
             Nodes Scanned: {}",
            self.risk_level(),
            self.risk_score(),
            self.result.threats.len(),
            self.result
                .threats
                .iter()
                .filter(|t| t.severity == Severity::Critical)
                .count(),
            self.result
                .threats
                .iter()
                .filter(|t| t.severity == Severity::High)
                .count(),
            self.result
                .threats
                .iter()
                .filter(|t| t.severity == Severity::Medium)
                .count(),
            self.result
                .threats
                .iter()
                .filter(|t| t.severity == Severity::Low)
                .count(),
            self.result.warnings.len(),
            self.result.metadata.scan_duration_ms,
            self.result.metadata.total_nodes_scanned,
        )
    }

    pub fn detailed_report(&self) -> String {
        let mut report = String::new();

        report.push_str("PDF Security Analysis Report\n");
        report.push_str("============================\n\n");

        report.push_str(&self.summary());
        report.push_str("\n\n");

        if !self.result.threats.is_empty() {
            report.push_str("THREATS DETECTED:\n");
            report.push_str("-----------------\n");

            for (i, threat) in self.result.threats.iter().enumerate() {
                report.push_str(&format!(
                    "{}. [{}] {} ({})\n",
                    i + 1,
                    threat.severity,
                    threat.title,
                    threat.threat_type
                ));
                report.push_str(&format!("   Location: {}\n", threat.location));
                report.push_str(&format!("   Description: {}\n", threat.description));

                if !threat.evidence.is_empty() {
                    report.push_str("   Evidence:\n");
                    for evidence in &threat.evidence {
                        report.push_str(&format!("   - {}\n", evidence));
                    }
                }

                report.push_str(&format!("   Mitigation: {}\n", threat.mitigation));

                if !threat.cve_references.is_empty() {
                    report.push_str("   CVE References: ");
                    report.push_str(&threat.cve_references.join(", "));
                    report.push('\n');
                }

                report.push('\n');
            }
        }

        if !self.result.warnings.is_empty() {
            report.push_str("WARNINGS:\n");
            report.push_str("---------\n");

            for (i, warning) in self.result.warnings.iter().enumerate() {
                report.push_str(&format!(
                    "{}. [{}] {}\n",
                    i + 1,
                    warning.severity,
                    warning.message
                ));
                if let Some(location) = &warning.location {
                    report.push_str(&format!("   Location: {}\n", location));
                }
                report.push_str(&format!("   Recommendation: {}\n", warning.recommendation));
                report.push('\n');
            }
        }

        if !self.result.info.is_empty() {
            report.push_str("INFORMATION:\n");
            report.push_str("------------\n");

            let mut categories: HashMap<String, Vec<&InfoItem>> = HashMap::new();
            for info in &self.result.info {
                categories
                    .entry(info.category.clone())
                    .or_default()
                    .push(info);
            }

            for (category, items) in categories {
                report.push_str(&format!("{}:\n", category));
                for item in items {
                    report.push_str(&format!("  - {}: {}\n", item.message, item.value));
                }
                report.push('\n');
            }
        }

        report.push_str(&format!(
            "Scan completed at: {}\n",
            self.result.metadata.scan_time
        ));
        report.push_str(&format!(
            "Rules applied: {}\n",
            self.result.metadata.rules_applied.join(", ")
        ));

        report
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(&self.result)
    }

    pub fn to_json_compact(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(&self.result)
    }

    pub fn recommendations(&self) -> Vec<String> {
        let mut recommendations = Vec::new();

        if self.is_safe() {
            recommendations.push("Document appears safe to use.".to_string());
        } else {
            recommendations.push(format!(
                "Document has {} risk level - exercise caution.",
                self.risk_level()
            ));
        }

        if !self.critical_threats().is_empty() {
            recommendations.push(
                "CRITICAL: Do not open this document without proper security measures.".to_string(),
            );
        }

        if !self.high_threats().is_empty() {
            recommendations.push(
                "HIGH RISK: Scan with multiple antivirus engines before opening.".to_string(),
            );
        }

        let has_js = self
            .result
            .threats
            .iter()
            .any(|t| t.title.contains("JavaScript"));
        let has_network = self
            .result
            .threats
            .iter()
            .any(|t| t.title.contains("Connection") || t.title.contains("URI"));

        if has_js {
            recommendations.push("Disable JavaScript in your PDF viewer.".to_string());
        }

        if has_network {
            recommendations.push(
                "Block network access for PDF viewer or use in isolated environment.".to_string(),
            );
        }

        if self
            .result
            .threats
            .iter()
            .any(|t| t.title.contains("Embedded"))
        {
            recommendations.push("Do not extract or execute embedded files.".to_string());
        }

        recommendations
    }

    pub fn executive_summary(&self) -> String {
        let risk_level = self.risk_level();
        let critical_count = self.critical_threats().len();
        let high_count = self.high_threats().len();

        if critical_count > 0 {
            format!(
                "CRITICAL SECURITY RISK: Document contains {} critical threat(s). \
                 DO NOT OPEN without proper security isolation. \
                 Document appears to be malicious.",
                critical_count
            )
        } else if high_count > 0 {
            format!(
                "HIGH SECURITY RISK: Document contains {} high-risk threat(s). \
                 Exercise extreme caution. Scan with antivirus before opening.",
                high_count
            )
        } else if risk_level == "MEDIUM" {
            "MEDIUM RISK: Document contains potentially suspicious content. \
             Review warnings and disable JavaScript/network access."
                .to_string()
        } else {
            "LOW RISK: Document appears relatively safe but monitor for unexpected behavior."
                .to_string()
        }
    }
}

impl std::fmt::Display for SecurityReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.detailed_report())
    }
}
