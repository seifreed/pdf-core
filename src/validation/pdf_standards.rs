use super::*;
use crate::ast::{PdfDocument, PdfVersion};

/// PDF/A compliance levels
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfALevel {
    PdfA1a,
    PdfA1b,
    PdfA2a,
    PdfA2b,
    PdfA2u,
    PdfA3a,
    PdfA3b,
    PdfA3u,
}

impl PdfALevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            PdfALevel::PdfA1a => "PDF/A-1a",
            PdfALevel::PdfA1b => "PDF/A-1b",
            PdfALevel::PdfA2a => "PDF/A-2a",
            PdfALevel::PdfA2b => "PDF/A-2b",
            PdfALevel::PdfA2u => "PDF/A-2u",
            PdfALevel::PdfA3a => "PDF/A-3a",
            PdfALevel::PdfA3b => "PDF/A-3b",
            PdfALevel::PdfA3u => "PDF/A-3u",
        }
    }

    pub fn requires_tagging(&self) -> bool {
        matches!(
            self,
            PdfALevel::PdfA1a | PdfALevel::PdfA2a | PdfALevel::PdfA3a
        )
    }

    pub fn allows_transparency(&self) -> bool {
        !matches!(self, PdfALevel::PdfA1a | PdfALevel::PdfA1b)
    }

    pub fn allows_embedded_files(&self) -> bool {
        matches!(
            self,
            PdfALevel::PdfA3a | PdfALevel::PdfA3b | PdfALevel::PdfA3u
        )
    }
}

/// PDF/X compliance levels
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfXLevel {
    PdfX1a,
    PdfX3,
    PdfX4,
    PdfX4p,
    PdfX5g,
    PdfX5n,
    PdfX5pg,
}

impl PdfXLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            PdfXLevel::PdfX1a => "PDF/X-1a",
            PdfXLevel::PdfX3 => "PDF/X-3",
            PdfXLevel::PdfX4 => "PDF/X-4",
            PdfXLevel::PdfX4p => "PDF/X-4p",
            PdfXLevel::PdfX5g => "PDF/X-5g",
            PdfXLevel::PdfX5n => "PDF/X-5n",
            PdfXLevel::PdfX5pg => "PDF/X-5pg",
        }
    }
}

/// PDF/UA compliance levels
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfUALevel {
    PdfUA1,
    PdfUA2,
}

impl PdfUALevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            PdfUALevel::PdfUA1 => "PDF/UA-1",
            PdfUALevel::PdfUA2 => "PDF/UA-2",
        }
    }
}

/// PDF 2.0 base schema
pub struct Pdf20Schema;

impl Default for Pdf20Schema {
    fn default() -> Self {
        Self::new()
    }
}

impl Pdf20Schema {
    pub fn new() -> Self {
        Self
    }
}

impl PdfSchema for Pdf20Schema {
    fn name(&self) -> &str {
        "PDF-2.0"
    }

    fn version(&self) -> &str {
        "2.0"
    }

    fn description(&self) -> &str {
        "Experimental structural checks for selected PDF 2.0 constraints"
    }

    fn reference_url(&self) -> Option<&str> {
        Some("https://www.iso.org/standard/63534.html")
    }

    fn supports_pdf_version(&self, version: &PdfVersion) -> bool {
        version.major >= 2 || (version.major == 1 && version.minor >= 4)
    }

    fn validate(&self, document: &PdfDocument) -> ValidationReport {
        let mut report = ValidationReport::new(self.name().to_string(), self.version().to_string());
        let context = ValidationContext::new(document, &mut report);

        // Run all constraints
        for constraint in self.get_constraints() {
            if let Some(reference) = constraint.iso_reference() {
                context
                    .report
                    .metadata
                    .insert(format!("iso.{}", constraint.name()), reference.to_string());
            }
            constraint.check(document, context.report);
        }

        context.report.finalize();
        report
    }

    fn get_constraints(&self) -> Vec<Box<dyn SchemaConstraint>> {
        vec![
            Box::new(HasCatalogConstraint),
            Box::new(HasPagesTreeConstraint),
            Box::new(CatalogHasPagesConstraint),
            Box::new(PageCountConsistencyConstraint),
            Box::new(ValidStructureConstraint),
            Box::new(HasTrailerRootConstraint),
            Box::new(HasTrailerSizeConstraint),
            Box::new(TrailerIdConstraint),
            Box::new(CatalogVersionConstraint),
            Box::new(HasXRefEntriesConstraint),
            Box::new(TrailerSizeConsistencyConstraint),
            Box::new(MetadataStreamConstraint),
            Box::new(FontCMapEncodingConstraint),
        ]
    }
}

/// PDF/A schema implementation
pub struct PdfASchema {
    level: PdfALevel,
}

impl PdfASchema {
    pub fn new(level: PdfALevel) -> Self {
        Self { level }
    }
}

impl PdfSchema for PdfASchema {
    fn name(&self) -> &str {
        self.level.as_str()
    }

    fn version(&self) -> &str {
        match self.level {
            PdfALevel::PdfA1a | PdfALevel::PdfA1b => "1.4",
            PdfALevel::PdfA2a | PdfALevel::PdfA2b | PdfALevel::PdfA2u => "1.7",
            PdfALevel::PdfA3a | PdfALevel::PdfA3b | PdfALevel::PdfA3u => "1.7",
        }
    }

    fn description(&self) -> &str {
        "Experimental preflight checks for selected ISO 19005 PDF/A requirements"
    }

    fn reference_url(&self) -> Option<&str> {
        Some("https://www.iso.org/standard/38920.html")
    }

    fn supports_pdf_version(&self, version: &PdfVersion) -> bool {
        let required_version = self.version().parse::<f32>().unwrap_or(1.4);
        let doc_version = format!("{}.{}", version.major, version.minor)
            .parse::<f32>()
            .unwrap_or(0.0);
        doc_version >= required_version
    }

    fn validate(&self, document: &PdfDocument) -> ValidationReport {
        if self.level == PdfALevel::PdfA1b {
            let mut report = crate::validation::pdfa::PdfA1bValidator::new().validate(document);
            report.schema_name = self.name().to_string();
            report.schema_version = self.version().to_string();
            return report;
        }

        let mut report = ValidationReport::new(self.name().to_string(), self.version().to_string());
        let context = ValidationContext::new(document, &mut report);

        // Run PDF/A specific constraints
        for constraint in self.get_constraints() {
            if let Some(reference) = constraint.iso_reference() {
                context
                    .report
                    .metadata
                    .insert(format!("iso.{}", constraint.name()), reference.to_string());
            }
            constraint.check(document, context.report);
        }

        context.report.finalize();
        report
    }

    fn get_constraints(&self) -> Vec<Box<dyn SchemaConstraint>> {
        let mut constraints: Vec<Box<dyn SchemaConstraint>> = vec![
            Box::new(HasCatalogConstraint),
            Box::new(HasPagesTreeConstraint),
            Box::new(NoEncryptionConstraint),
            Box::new(NoJavaScriptConstraint),
            Box::new(NoExternalReferencesConstraint),
            Box::new(EmbeddedFontsConstraint),
        ];

        if self.level.requires_tagging() {
            constraints.push(Box::new(TaggedStructureConstraint));
        }

        if !self.level.allows_transparency() {
            constraints.push(Box::new(NoTransparencyConstraint));
        }

        if !self.level.allows_embedded_files() {
            constraints.push(Box::new(NoEmbeddedFilesConstraint));
        }

        constraints
    }
}

/// PDF/X schema implementation
pub struct PdfXSchema {
    level: PdfXLevel,
}

impl PdfXSchema {
    pub fn new(level: PdfXLevel) -> Self {
        Self { level }
    }
}

impl PdfSchema for PdfXSchema {
    fn name(&self) -> &str {
        self.level.as_str()
    }

    fn version(&self) -> &str {
        match self.level {
            PdfXLevel::PdfX1a => "1.3",
            PdfXLevel::PdfX3 => "1.3",
            PdfXLevel::PdfX4 | PdfXLevel::PdfX4p => "1.6",
            PdfXLevel::PdfX5g | PdfXLevel::PdfX5n | PdfXLevel::PdfX5pg => "1.6",
        }
    }

    fn description(&self) -> &str {
        "ISO 15930 PDF/X graphics exchange standard"
    }

    fn supports_pdf_version(&self, version: &PdfVersion) -> bool {
        let required_version = self.version().parse::<f32>().unwrap_or(1.3);
        let doc_version = format!("{}.{}", version.major, version.minor)
            .parse::<f32>()
            .unwrap_or(0.0);
        doc_version >= required_version
    }

    fn validate(&self, document: &PdfDocument) -> ValidationReport {
        let mut report = ValidationReport::new(self.name().to_string(), self.version().to_string());
        let context = ValidationContext::new(document, &mut report);

        for constraint in self.get_constraints() {
            if let Some(reference) = constraint.iso_reference() {
                context
                    .report
                    .metadata
                    .insert(format!("iso.{}", constraint.name()), reference.to_string());
            }
            constraint.check(document, context.report);
        }

        context.report.finalize();
        report
    }

    fn get_constraints(&self) -> Vec<Box<dyn SchemaConstraint>> {
        vec![
            Box::new(HasCatalogConstraint),
            Box::new(HasPagesTreeConstraint),
            Box::new(NoEncryptionConstraint),
            Box::new(NoJavaScriptConstraint),
            Box::new(EmbeddedFontsConstraint),
            Box::new(ColorSpaceConstraint),
            Box::new(TrimBoxConstraint),
        ]
    }
}

/// PDF/UA schema implementation
pub struct PdfUASchema {
    level: PdfUALevel,
}

impl PdfUASchema {
    pub fn new(level: PdfUALevel) -> Self {
        Self { level }
    }
}

impl PdfSchema for PdfUASchema {
    fn name(&self) -> &str {
        self.level.as_str()
    }

    fn version(&self) -> &str {
        match self.level {
            PdfUALevel::PdfUA1 => "1.7",
            PdfUALevel::PdfUA2 => "2.0",
        }
    }

    fn description(&self) -> &str {
        "Experimental preflight checks for selected ISO 14289 PDF/UA requirements"
    }

    fn supports_pdf_version(&self, version: &PdfVersion) -> bool {
        let required_version = self.version().parse::<f32>().unwrap_or(1.7);
        let doc_version = format!("{}.{}", version.major, version.minor)
            .parse::<f32>()
            .unwrap_or(0.0);
        doc_version >= required_version
    }

    fn validate(&self, document: &PdfDocument) -> ValidationReport {
        let mut report = ValidationReport::new(self.name().to_string(), self.version().to_string());
        let context = ValidationContext::new(document, &mut report);

        for constraint in self.get_constraints() {
            if let Some(reference) = constraint.iso_reference() {
                context
                    .report
                    .metadata
                    .insert(format!("iso.{}", constraint.name()), reference.to_string());
            }
            constraint.check(document, context.report);
        }

        context.report.finalize();
        report
    }

    fn get_constraints(&self) -> Vec<Box<dyn SchemaConstraint>> {
        vec![
            Box::new(HasCatalogConstraint),
            Box::new(HasPagesTreeConstraint),
            Box::new(TaggedStructureConstraint),
            Box::new(AccessibilityMetadataConstraint),
            Box::new(AltTextConstraint),
            Box::new(LanguageSpecificationConstraint),
            Box::new(LogicalReadingOrderConstraint),
        ]
    }
}
