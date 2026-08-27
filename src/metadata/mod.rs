pub mod icc;
pub mod xmp;

use crate::performance::ResourceBudget;
use crate::types::PdfDictionary;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct XmpMetadata {
    pub raw_xml: String,
    pub properties: HashMap<String, String>,
    pub namespaces: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct PdfInfo {
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub keywords: Option<String>,
    pub creator: Option<String>,
    pub producer: Option<String>,
    pub creation_date: Option<String>,
    pub modification_date: Option<String>,
    pub trapped: Option<String>,
}

impl PdfInfo {
    pub fn from_dict(dict: &PdfDictionary) -> Self {
        PdfInfo {
            title: Self::extract_string(dict, "Title"),
            author: Self::extract_string(dict, "Author"),
            subject: Self::extract_string(dict, "Subject"),
            keywords: Self::extract_string(dict, "Keywords"),
            creator: Self::extract_string(dict, "Creator"),
            producer: Self::extract_string(dict, "Producer"),
            creation_date: Self::extract_string(dict, "CreationDate"),
            modification_date: Self::extract_string(dict, "ModDate"),
            trapped: Self::extract_string(dict, "Trapped"),
        }
    }

    fn extract_string(dict: &PdfDictionary, key: &str) -> Option<String> {
        dict.get(key)
            .and_then(|v| v.as_string())
            .map(|s| s.decode_pdf_encoding())
    }
}

impl XmpMetadata {
    pub fn new() -> Self {
        XmpMetadata {
            raw_xml: String::new(),
            properties: HashMap::new(),
            namespaces: HashMap::new(),
        }
    }

    pub fn parse_from_stream(data: &[u8]) -> Result<Self, String> {
        Self::parse_from_stream_with_budget(data, &ResourceBudget::default())
    }

    pub fn parse_from_stream_with_budget(
        data: &[u8],
        budget: &ResourceBudget,
    ) -> Result<Self, String> {
        xmp::parse_xmp_bytes_with_budget(data, budget)
    }

    pub fn get_property(&self, namespace: &str, property: &str) -> Option<&String> {
        let key = format!("{}:{}", namespace, property);
        self.properties.get(&key)
    }

    pub fn get_dublin_core_property(&self, property: &str) -> Option<&String> {
        self.get_property("dc", property)
    }

    pub fn get_pdf_property(&self, property: &str) -> Option<&String> {
        self.get_property("pdf", property)
    }

    pub fn get_xmp_property(&self, property: &str) -> Option<&String> {
        self.get_property("xmp", property)
    }

    pub fn title(&self) -> Option<&String> {
        self.get_dublin_core_property("title")
    }

    pub fn author(&self) -> Option<&String> {
        self.get_dublin_core_property("creator")
    }

    pub fn subject(&self) -> Option<&String> {
        self.get_dublin_core_property("subject")
    }

    pub fn keywords(&self) -> Option<&String> {
        self.get_pdf_property("Keywords")
    }

    pub fn creator(&self) -> Option<&String> {
        self.get_xmp_property("CreatorTool")
    }

    pub fn producer(&self) -> Option<&String> {
        self.get_pdf_property("Producer")
    }

    pub fn creation_date(&self) -> Option<&String> {
        self.get_xmp_property("CreateDate")
    }

    pub fn modification_date(&self) -> Option<&String> {
        self.get_xmp_property("ModifyDate")
    }
}

impl Default for XmpMetadata {
    fn default() -> Self {
        Self::new()
    }
}
