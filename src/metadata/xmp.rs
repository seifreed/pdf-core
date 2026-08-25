use crate::metadata::XmpMetadata;
use quick_xml::{events::Event, Reader};
use std::collections::HashMap;

/// Comprehensive XMP namespace support with all major namespaces
pub fn parse_xmp(xml: &str) -> Result<XmpMetadata, String> {
    let mut metadata = XmpMetadata::new();
    metadata.raw_xml = xml.to_string();

    let mut parser = EnhancedXmpParser::new(&mut metadata);
    parser.parse(xml)?;

    Ok(metadata)
}

/// Enhanced XMP parser with comprehensive namespace support
struct EnhancedXmpParser<'a> {
    metadata: &'a mut XmpMetadata,
    namespace_registry: NamespaceRegistry,
    current_path: Vec<String>,
    current_text: String,
    in_rdf_description: bool,
    current_lang: Option<String>,
}

impl<'a> EnhancedXmpParser<'a> {
    fn new(metadata: &'a mut XmpMetadata) -> Self {
        EnhancedXmpParser {
            metadata,
            namespace_registry: NamespaceRegistry::new(),
            current_path: Vec::new(),
            current_text: String::new(),
            in_rdf_description: false,
            current_lang: None,
        }
    }

    fn parse(&mut self, xml: &str) -> Result<(), String> {
        let mut reader = Reader::from_str(xml);
        reader.config_mut().trim_text(true);

        let mut buf = Vec::new();

        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(ref e)) => {
                    self.handle_start_element(e)?;
                }
                Ok(Event::Empty(ref e)) => {
                    self.handle_start_element(e)?;
                }
                Ok(Event::End(ref e)) => {
                    self.handle_end_element(e)?;
                }
                Ok(Event::Text(ref e)) => {
                    let decoded = e
                        .decode()
                        .map_err(|e| format!("Text decode error: {}", e))?;
                    let text = quick_xml::escape::unescape(&decoded)
                        .map_err(|e| format!("Text decode error: {}", e))?;
                    self.current_text.push_str(&text);
                }
                Ok(Event::Eof) => break,
                Err(e) => return Err(format!("XML parse error: {}", e)),
                _ => {}
            }
            buf.clear();
        }

        Ok(())
    }

    fn handle_start_element(
        &mut self,
        element: &quick_xml::events::BytesStart,
    ) -> Result<(), String> {
        let binding = element.name();
        let name =
            std::str::from_utf8(binding.as_ref()).map_err(|_| "Invalid UTF-8 in element name")?;

        // Handle namespace declarations
        for attr in element.attributes() {
            let attr = attr.map_err(|e| format!("Attribute error: {}", e))?;
            let key = std::str::from_utf8(attr.key.as_ref()).unwrap_or("");
            let value = std::str::from_utf8(&attr.value).unwrap_or("");

            if key.starts_with("xmlns:") || key == "xmlns" {
                let prefix = if key == "xmlns" {
                    "default".to_string()
                } else {
                    key[6..].to_string()
                };
                self.namespace_registry
                    .register_namespace(prefix, value.to_string());
                self.metadata
                    .namespaces
                    .insert(key.to_string(), value.to_string());
            }

            if key == "xml:lang" {
                self.current_lang = Some(value.to_string());
            }
        }

        // Track path for nested elements
        self.current_path.push(name.to_string());

        // Handle special cases
        if name == "rdf:Description" {
            self.in_rdf_description = true;
            self.handle_description_attributes(element)?;
        }

        // Handle array containers
        if name == "rdf:Bag" || name == "rdf:Seq" || name == "rdf:Alt" {
            // Array container - handled by parent element
        }

        self.current_text.clear();
        Ok(())
    }

    fn handle_end_element(&mut self, element: &quick_xml::events::BytesEnd) -> Result<(), String> {
        let binding = element.name();
        let name =
            std::str::from_utf8(binding.as_ref()).map_err(|_| "Invalid UTF-8 in element name")?;

        if !self.current_text.trim().is_empty() {
            let path = self.current_path.join("/");
            let text = self.current_text.trim().to_string();
            self.store_property(&path, &text);
        }

        if name == "rdf:Description" {
            self.in_rdf_description = false;
        }

        self.current_path.pop();
        self.current_text.clear();
        Ok(())
    }

    fn handle_description_attributes(
        &mut self,
        element: &quick_xml::events::BytesStart,
    ) -> Result<(), String> {
        for attr in element.attributes() {
            let attr = attr.map_err(|e| format!("Attribute error: {}", e))?;
            let key = std::str::from_utf8(attr.key.as_ref()).unwrap_or("");
            let value = std::str::from_utf8(&attr.value).unwrap_or("");

            // Skip namespace and special attributes
            if key.starts_with("xmlns") || key == "rdf:about" || key == "xml:lang" {
                continue;
            }

            // Store as property
            self.store_property(key, value);
        }
        Ok(())
    }

    fn store_property(&mut self, key: &str, value: &str) {
        let normalized_key = self.normalize_property_key(key);
        let decoded_value = self.decode_xml_entities(value);

        // Handle language-specific properties
        let final_key = if let Some(ref lang) = self.current_lang {
            format!("{}[{}]", normalized_key, lang)
        } else {
            normalized_key
        };

        self.metadata.properties.insert(final_key, decoded_value);

        // Also store namespace information
        if let Some(ns_info) = self.namespace_registry.get_namespace_info(key) {
            self.metadata
                .namespaces
                .insert(ns_info.prefix.clone(), ns_info.uri.clone());
        }
    }

    fn normalize_property_key(&self, key: &str) -> String {
        // Remove path separators for simple keys
        if key.contains('/') {
            key.split('/').next_back().unwrap_or(key).to_string()
        } else {
            key.to_string()
        }
    }

    fn decode_xml_entities(&self, s: &str) -> String {
        s.replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&amp;", "&")
            .replace("&quot;", "\"")
            .replace("&apos;", "'")
            .replace("&#39;", "'")
            .replace("&#x27;", "'")
            .replace("&#x2F;", "/")
    }
}

/// Registry for XMP namespaces with comprehensive namespace support
struct NamespaceRegistry {
    namespaces: HashMap<String, NamespaceInfo>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct NamespaceInfo {
    prefix: String,
    uri: String,
    description: String,
    properties: Vec<PropertyInfo>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct PropertyInfo {
    name: String,
    data_type: XmpDataType,
    description: String,
    is_array: bool,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
enum XmpDataType {
    Text,
    Integer,
    Real,
    Boolean,
    Date,
    Uri,
    Choice(Vec<String>),
    Structure,
    Array(Box<XmpDataType>),
}

impl NamespaceRegistry {
    fn new() -> Self {
        let mut registry = Self {
            namespaces: HashMap::new(),
        };

        registry.register_standard_namespaces();
        registry
    }

    fn register_namespace(&mut self, prefix: String, uri: String) {
        self.namespaces
            .entry(prefix.clone())
            .or_insert_with(|| NamespaceInfo {
                prefix: prefix.clone(),
                uri: uri.clone(),
                description: format!("Custom namespace: {}", uri),
                properties: Vec::new(),
            });
    }

    fn get_namespace_info(&self, property_key: &str) -> Option<&NamespaceInfo> {
        if let Some(colon_pos) = property_key.find(':') {
            let prefix = &property_key[..colon_pos];
            self.namespaces.get(prefix)
        } else {
            None
        }
    }

    fn register_standard_namespaces(&mut self) {
        // Core XMP namespaces
        self.add_dublin_core_namespace();
        self.add_xmp_basic_namespace();
        self.add_xmp_rights_namespace();
        self.add_xmp_media_management_namespace();

        // Adobe application namespaces
        self.add_pdf_namespace();
        self.add_photoshop_namespace();
        self.add_camera_raw_namespace();
        self.add_lightroom_namespace();

        // Media and technical namespaces
        self.add_exif_namespace();
        self.add_tiff_namespace();
        self.add_iptc_namespace();
        self.add_dicom_namespace();

        // Creative and workflow namespaces
        self.add_creative_commons_namespace();
        self.add_prism_namespace();
        self.add_dam_namespace();

        // Geospatial and scientific namespaces
        self.add_geospatial_namespace();
        self.add_scientific_namespace();

        // Video and audio namespaces
        self.add_dynamic_media_namespace();
        self.add_audio_namespace();

        // Workflow and asset management
        self.add_workflow_namespace();
        self.add_version_management_namespace();
    }

    fn add_dublin_core_namespace(&mut self) {
        let properties = vec![
            PropertyInfo {
                name: "title".to_string(),
                data_type: XmpDataType::Text,
                description: "Document title".to_string(),
                is_array: true,
            },
            PropertyInfo {
                name: "creator".to_string(),
                data_type: XmpDataType::Text,
                description: "Document creator".to_string(),
                is_array: true,
            },
            PropertyInfo {
                name: "subject".to_string(),
                data_type: XmpDataType::Text,
                description: "Document subject/keywords".to_string(),
                is_array: true,
            },
            PropertyInfo {
                name: "description".to_string(),
                data_type: XmpDataType::Text,
                description: "Document description".to_string(),
                is_array: true,
            },
            PropertyInfo {
                name: "publisher".to_string(),
                data_type: XmpDataType::Text,
                description: "Document publisher".to_string(),
                is_array: true,
            },
            PropertyInfo {
                name: "contributor".to_string(),
                data_type: XmpDataType::Text,
                description: "Document contributor".to_string(),
                is_array: true,
            },
            PropertyInfo {
                name: "date".to_string(),
                data_type: XmpDataType::Date,
                description: "Document date".to_string(),
                is_array: true,
            },
            PropertyInfo {
                name: "type".to_string(),
                data_type: XmpDataType::Text,
                description: "Document type".to_string(),
                is_array: true,
            },
            PropertyInfo {
                name: "format".to_string(),
                data_type: XmpDataType::Text,
                description: "Document format".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "identifier".to_string(),
                data_type: XmpDataType::Text,
                description: "Document identifier".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "source".to_string(),
                data_type: XmpDataType::Text,
                description: "Document source".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "language".to_string(),
                data_type: XmpDataType::Text,
                description: "Document language".to_string(),
                is_array: true,
            },
            PropertyInfo {
                name: "relation".to_string(),
                data_type: XmpDataType::Text,
                description: "Related resources".to_string(),
                is_array: true,
            },
            PropertyInfo {
                name: "coverage".to_string(),
                data_type: XmpDataType::Text,
                description: "Spatial/temporal coverage".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "rights".to_string(),
                data_type: XmpDataType::Text,
                description: "Rights information".to_string(),
                is_array: true,
            },
        ];

        self.namespaces.insert(
            "dc".to_string(),
            NamespaceInfo {
                prefix: "dc".to_string(),
                uri: "http://purl.org/dc/elements/1.1/".to_string(),
                description: "Dublin Core metadata elements".to_string(),
                properties,
            },
        );
    }

    fn add_xmp_basic_namespace(&mut self) {
        let properties = vec![
            PropertyInfo {
                name: "CreateDate".to_string(),
                data_type: XmpDataType::Date,
                description: "Creation date".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "ModifyDate".to_string(),
                data_type: XmpDataType::Date,
                description: "Modification date".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "MetadataDate".to_string(),
                data_type: XmpDataType::Date,
                description: "Metadata modification date".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "CreatorTool".to_string(),
                data_type: XmpDataType::Text,
                description: "Creating application".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "Identifier".to_string(),
                data_type: XmpDataType::Text,
                description: "Unique identifier".to_string(),
                is_array: true,
            },
            PropertyInfo {
                name: "Nickname".to_string(),
                data_type: XmpDataType::Text,
                description: "Informal name".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "Rating".to_string(),
                data_type: XmpDataType::Integer,
                description: "User rating".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "Label".to_string(),
                data_type: XmpDataType::Text,
                description: "User label".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "BaseURL".to_string(),
                data_type: XmpDataType::Uri,
                description: "Base URL for relative URLs".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "Thumbnails".to_string(),
                data_type: XmpDataType::Array(Box::new(XmpDataType::Structure)),
                description: "Thumbnail images".to_string(),
                is_array: true,
            },
        ];

        self.namespaces.insert(
            "xmp".to_string(),
            NamespaceInfo {
                prefix: "xmp".to_string(),
                uri: "http://ns.adobe.com/xap/1.0/".to_string(),
                description: "XMP Basic metadata schema".to_string(),
                properties,
            },
        );
    }

    fn add_xmp_rights_namespace(&mut self) {
        let properties = vec![
            PropertyInfo {
                name: "Certificate".to_string(),
                data_type: XmpDataType::Uri,
                description: "Rights certificate URL".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "Marked".to_string(),
                data_type: XmpDataType::Boolean,
                description: "Rights marked flag".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "Owner".to_string(),
                data_type: XmpDataType::Text,
                description: "Rights owner".to_string(),
                is_array: true,
            },
            PropertyInfo {
                name: "UsageTerms".to_string(),
                data_type: XmpDataType::Text,
                description: "Usage terms".to_string(),
                is_array: true,
            },
            PropertyInfo {
                name: "WebStatement".to_string(),
                data_type: XmpDataType::Uri,
                description: "Web statement URL".to_string(),
                is_array: false,
            },
        ];

        self.namespaces.insert(
            "xmpRights".to_string(),
            NamespaceInfo {
                prefix: "xmpRights".to_string(),
                uri: "http://ns.adobe.com/xap/1.0/rights/".to_string(),
                description: "XMP Rights Management schema".to_string(),
                properties,
            },
        );
    }

    fn add_xmp_media_management_namespace(&mut self) {
        let properties = vec![
            PropertyInfo {
                name: "DerivedFrom".to_string(),
                data_type: XmpDataType::Structure,
                description: "Derived from reference".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "DocumentID".to_string(),
                data_type: XmpDataType::Uri,
                description: "Document identifier".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "History".to_string(),
                data_type: XmpDataType::Array(Box::new(XmpDataType::Structure)),
                description: "Modification history".to_string(),
                is_array: true,
            },
            PropertyInfo {
                name: "Ingredients".to_string(),
                data_type: XmpDataType::Array(Box::new(XmpDataType::Structure)),
                description: "Referenced resources".to_string(),
                is_array: true,
            },
            PropertyInfo {
                name: "InstanceID".to_string(),
                data_type: XmpDataType::Uri,
                description: "Instance identifier".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "ManagedFrom".to_string(),
                data_type: XmpDataType::Structure,
                description: "Asset management reference".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "Manager".to_string(),
                data_type: XmpDataType::Text,
                description: "Asset management system".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "ManageTo".to_string(),
                data_type: XmpDataType::Uri,
                description: "Managed asset URI".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "ManageUI".to_string(),
                data_type: XmpDataType::Uri,
                description: "Management UI URI".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "ManagerVariant".to_string(),
                data_type: XmpDataType::Text,
                description: "Management system variant".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "OriginalDocumentID".to_string(),
                data_type: XmpDataType::Uri,
                description: "Original document ID".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "RenditionClass".to_string(),
                data_type: XmpDataType::Text,
                description: "Rendition class".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "RenditionParams".to_string(),
                data_type: XmpDataType::Text,
                description: "Rendition parameters".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "VersionID".to_string(),
                data_type: XmpDataType::Text,
                description: "Version identifier".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "Versions".to_string(),
                data_type: XmpDataType::Array(Box::new(XmpDataType::Structure)),
                description: "Version history".to_string(),
                is_array: true,
            },
        ];

        self.namespaces.insert(
            "xmpMM".to_string(),
            NamespaceInfo {
                prefix: "xmpMM".to_string(),
                uri: "http://ns.adobe.com/xap/1.0/mm/".to_string(),
                description: "XMP Media Management schema".to_string(),
                properties,
            },
        );
    }

    fn add_pdf_namespace(&mut self) {
        let properties = vec![
            PropertyInfo {
                name: "Keywords".to_string(),
                data_type: XmpDataType::Text,
                description: "PDF keywords".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "Producer".to_string(),
                data_type: XmpDataType::Text,
                description: "PDF producer".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "PDFVersion".to_string(),
                data_type: XmpDataType::Text,
                description: "PDF version".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "Trapped".to_string(),
                data_type: XmpDataType::Choice(vec![
                    "True".to_string(),
                    "False".to_string(),
                    "Unknown".to_string(),
                ]),
                description: "Trapping status".to_string(),
                is_array: false,
            },
        ];

        self.namespaces.insert(
            "pdf".to_string(),
            NamespaceInfo {
                prefix: "pdf".to_string(),
                uri: "http://ns.adobe.com/pdf/1.3/".to_string(),
                description: "Adobe PDF schema".to_string(),
                properties,
            },
        );
    }

    fn add_photoshop_namespace(&mut self) {
        let properties = vec![
            PropertyInfo {
                name: "AuthorsPosition".to_string(),
                data_type: XmpDataType::Text,
                description: "Author's position".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "CaptionWriter".to_string(),
                data_type: XmpDataType::Text,
                description: "Caption writer".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "Category".to_string(),
                data_type: XmpDataType::Text,
                description: "Category".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "City".to_string(),
                data_type: XmpDataType::Text,
                description: "City".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "ColorMode".to_string(),
                data_type: XmpDataType::Integer,
                description: "Color mode".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "Country".to_string(),
                data_type: XmpDataType::Text,
                description: "Country".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "Credit".to_string(),
                data_type: XmpDataType::Text,
                description: "Credit line".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "DateCreated".to_string(),
                data_type: XmpDataType::Date,
                description: "Date created".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "Headline".to_string(),
                data_type: XmpDataType::Text,
                description: "Headline".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "History".to_string(),
                data_type: XmpDataType::Text,
                description: "Edit history".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "ICCProfile".to_string(),
                data_type: XmpDataType::Text,
                description: "ICC profile name".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "Instructions".to_string(),
                data_type: XmpDataType::Text,
                description: "Special instructions".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "Source".to_string(),
                data_type: XmpDataType::Text,
                description: "Source".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "State".to_string(),
                data_type: XmpDataType::Text,
                description: "State/province".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "SupplementalCategories".to_string(),
                data_type: XmpDataType::Text,
                description: "Supplemental categories".to_string(),
                is_array: true,
            },
            PropertyInfo {
                name: "TransmissionReference".to_string(),
                data_type: XmpDataType::Text,
                description: "Original transmission reference".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "Urgency".to_string(),
                data_type: XmpDataType::Integer,
                description: "Urgency level".to_string(),
                is_array: false,
            },
        ];

        self.namespaces.insert(
            "photoshop".to_string(),
            NamespaceInfo {
                prefix: "photoshop".to_string(),
                uri: "http://ns.adobe.com/photoshop/1.0/".to_string(),
                description: "Adobe Photoshop schema".to_string(),
                properties,
            },
        );
    }

    fn add_camera_raw_namespace(&mut self) {
        let properties = vec![
            PropertyInfo {
                name: "AutoBrightness".to_string(),
                data_type: XmpDataType::Boolean,
                description: "Auto brightness".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "AutoContrast".to_string(),
                data_type: XmpDataType::Boolean,
                description: "Auto contrast".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "AutoExposure".to_string(),
                data_type: XmpDataType::Boolean,
                description: "Auto exposure".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "AutoShadows".to_string(),
                data_type: XmpDataType::Boolean,
                description: "Auto shadows".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "BlueHue".to_string(),
                data_type: XmpDataType::Integer,
                description: "Blue hue".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "BlueSaturation".to_string(),
                data_type: XmpDataType::Integer,
                description: "Blue saturation".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "Brightness".to_string(),
                data_type: XmpDataType::Integer,
                description: "Brightness".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "ChromaticAberrationB".to_string(),
                data_type: XmpDataType::Integer,
                description: "Chromatic aberration B".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "ChromaticAberrationR".to_string(),
                data_type: XmpDataType::Integer,
                description: "Chromatic aberration R".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "ColorNoiseReduction".to_string(),
                data_type: XmpDataType::Integer,
                description: "Color noise reduction".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "Contrast".to_string(),
                data_type: XmpDataType::Integer,
                description: "Contrast".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "CropTop".to_string(),
                data_type: XmpDataType::Real,
                description: "Crop top".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "CropLeft".to_string(),
                data_type: XmpDataType::Real,
                description: "Crop left".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "CropBottom".to_string(),
                data_type: XmpDataType::Real,
                description: "Crop bottom".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "CropRight".to_string(),
                data_type: XmpDataType::Real,
                description: "Crop right".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "CropAngle".to_string(),
                data_type: XmpDataType::Real,
                description: "Crop angle".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "Exposure".to_string(),
                data_type: XmpDataType::Real,
                description: "Exposure".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "Highlights".to_string(),
                data_type: XmpDataType::Integer,
                description: "Highlights".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "LuminanceSmoothing".to_string(),
                data_type: XmpDataType::Integer,
                description: "Luminance smoothing".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "RawFileName".to_string(),
                data_type: XmpDataType::Text,
                description: "Raw file name".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "RedHue".to_string(),
                data_type: XmpDataType::Integer,
                description: "Red hue".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "RedSaturation".to_string(),
                data_type: XmpDataType::Integer,
                description: "Red saturation".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "Saturation".to_string(),
                data_type: XmpDataType::Integer,
                description: "Saturation".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "Shadows".to_string(),
                data_type: XmpDataType::Integer,
                description: "Shadows".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "ShadowTint".to_string(),
                data_type: XmpDataType::Integer,
                description: "Shadow tint".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "Sharpness".to_string(),
                data_type: XmpDataType::Integer,
                description: "Sharpness".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "Temperature".to_string(),
                data_type: XmpDataType::Integer,
                description: "Color temperature".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "Tint".to_string(),
                data_type: XmpDataType::Integer,
                description: "Tint".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "ToneCurve".to_string(),
                data_type: XmpDataType::Array(Box::new(XmpDataType::Structure)),
                description: "Tone curve".to_string(),
                is_array: true,
            },
            PropertyInfo {
                name: "Version".to_string(),
                data_type: XmpDataType::Text,
                description: "Camera Raw version".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "VignetteAmount".to_string(),
                data_type: XmpDataType::Integer,
                description: "Vignette amount".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "VignetteMidpoint".to_string(),
                data_type: XmpDataType::Integer,
                description: "Vignette midpoint".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "WhiteBalance".to_string(),
                data_type: XmpDataType::Choice(vec![
                    "As Shot".to_string(),
                    "Auto".to_string(),
                    "Daylight".to_string(),
                    "Cloudy".to_string(),
                    "Shade".to_string(),
                    "Tungsten".to_string(),
                    "Fluorescent".to_string(),
                    "Flash".to_string(),
                    "Custom".to_string(),
                ]),
                description: "White balance".to_string(),
                is_array: false,
            },
        ];

        self.namespaces.insert(
            "crs".to_string(),
            NamespaceInfo {
                prefix: "crs".to_string(),
                uri: "http://ns.adobe.com/camera-raw-settings/1.0/".to_string(),
                description: "Camera Raw Settings schema".to_string(),
                properties,
            },
        );
    }

    fn add_lightroom_namespace(&mut self) {
        let properties = vec![
            PropertyInfo {
                name: "hierarchicalSubject".to_string(),
                data_type: XmpDataType::Text,
                description: "Hierarchical keywords".to_string(),
                is_array: true,
            },
            PropertyInfo {
                name: "privateRTKInfo".to_string(),
                data_type: XmpDataType::Text,
                description: "Private runtime key info".to_string(),
                is_array: false,
            },
        ];

        self.namespaces.insert(
            "lr".to_string(),
            NamespaceInfo {
                prefix: "lr".to_string(),
                uri: "http://ns.adobe.com/lightroom/1.0/".to_string(),
                description: "Adobe Lightroom schema".to_string(),
                properties,
            },
        );
    }

    fn add_exif_namespace(&mut self) {
        let properties = vec![
            PropertyInfo {
                name: "ExifVersion".to_string(),
                data_type: XmpDataType::Text,
                description: "EXIF version".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "FlashpixVersion".to_string(),
                data_type: XmpDataType::Text,
                description: "FlashPix version".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "ColorSpace".to_string(),
                data_type: XmpDataType::Integer,
                description: "Color space".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "ComponentsConfiguration".to_string(),
                data_type: XmpDataType::Text,
                description: "Components configuration".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "CompressedBitsPerPixel".to_string(),
                data_type: XmpDataType::Real,
                description: "Compressed bits per pixel".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "PixelXDimension".to_string(),
                data_type: XmpDataType::Integer,
                description: "Pixel X dimension".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "PixelYDimension".to_string(),
                data_type: XmpDataType::Integer,
                description: "Pixel Y dimension".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "UserComment".to_string(),
                data_type: XmpDataType::Text,
                description: "User comment".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "RelatedSoundFile".to_string(),
                data_type: XmpDataType::Text,
                description: "Related sound file".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "DateTimeOriginal".to_string(),
                data_type: XmpDataType::Date,
                description: "Date/time original".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "DateTimeDigitized".to_string(),
                data_type: XmpDataType::Date,
                description: "Date/time digitized".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "ExposureTime".to_string(),
                data_type: XmpDataType::Real,
                description: "Exposure time".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "FNumber".to_string(),
                data_type: XmpDataType::Real,
                description: "F number".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "ExposureProgram".to_string(),
                data_type: XmpDataType::Integer,
                description: "Exposure program".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "SpectralSensitivity".to_string(),
                data_type: XmpDataType::Text,
                description: "Spectral sensitivity".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "ISOSpeedRatings".to_string(),
                data_type: XmpDataType::Integer,
                description: "ISO speed ratings".to_string(),
                is_array: true,
            },
            PropertyInfo {
                name: "OECF".to_string(),
                data_type: XmpDataType::Text,
                description: "OECF".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "ShutterSpeedValue".to_string(),
                data_type: XmpDataType::Real,
                description: "Shutter speed value".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "ApertureValue".to_string(),
                data_type: XmpDataType::Real,
                description: "Aperture value".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "BrightnessValue".to_string(),
                data_type: XmpDataType::Real,
                description: "Brightness value".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "ExposureBiasValue".to_string(),
                data_type: XmpDataType::Real,
                description: "Exposure bias value".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "MaxApertureValue".to_string(),
                data_type: XmpDataType::Real,
                description: "Max aperture value".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "SubjectDistance".to_string(),
                data_type: XmpDataType::Real,
                description: "Subject distance".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "MeteringMode".to_string(),
                data_type: XmpDataType::Integer,
                description: "Metering mode".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "LightSource".to_string(),
                data_type: XmpDataType::Integer,
                description: "Light source".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "Flash".to_string(),
                data_type: XmpDataType::Integer,
                description: "Flash".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "FocalLength".to_string(),
                data_type: XmpDataType::Real,
                description: "Focal length".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "SubjectArea".to_string(),
                data_type: XmpDataType::Integer,
                description: "Subject area".to_string(),
                is_array: true,
            },
            PropertyInfo {
                name: "FlashEnergy".to_string(),
                data_type: XmpDataType::Real,
                description: "Flash energy".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "SpatialFrequencyResponse".to_string(),
                data_type: XmpDataType::Text,
                description: "Spatial frequency response".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "FocalPlaneXResolution".to_string(),
                data_type: XmpDataType::Real,
                description: "Focal plane X resolution".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "FocalPlaneYResolution".to_string(),
                data_type: XmpDataType::Real,
                description: "Focal plane Y resolution".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "FocalPlaneResolutionUnit".to_string(),
                data_type: XmpDataType::Integer,
                description: "Focal plane resolution unit".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "SubjectLocation".to_string(),
                data_type: XmpDataType::Integer,
                description: "Subject location".to_string(),
                is_array: true,
            },
            PropertyInfo {
                name: "ExposureIndex".to_string(),
                data_type: XmpDataType::Real,
                description: "Exposure index".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "SensingMethod".to_string(),
                data_type: XmpDataType::Integer,
                description: "Sensing method".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "FileSource".to_string(),
                data_type: XmpDataType::Integer,
                description: "File source".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "SceneType".to_string(),
                data_type: XmpDataType::Integer,
                description: "Scene type".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "CFAPattern".to_string(),
                data_type: XmpDataType::Text,
                description: "CFA pattern".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "CustomRendered".to_string(),
                data_type: XmpDataType::Integer,
                description: "Custom rendered".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "ExposureMode".to_string(),
                data_type: XmpDataType::Integer,
                description: "Exposure mode".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "WhiteBalance".to_string(),
                data_type: XmpDataType::Integer,
                description: "White balance".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "DigitalZoomRatio".to_string(),
                data_type: XmpDataType::Real,
                description: "Digital zoom ratio".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "FocalLengthIn35mmFilm".to_string(),
                data_type: XmpDataType::Integer,
                description: "Focal length in 35mm film".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "SceneCaptureType".to_string(),
                data_type: XmpDataType::Integer,
                description: "Scene capture type".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "GainControl".to_string(),
                data_type: XmpDataType::Integer,
                description: "Gain control".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "Contrast".to_string(),
                data_type: XmpDataType::Integer,
                description: "Contrast".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "Saturation".to_string(),
                data_type: XmpDataType::Integer,
                description: "Saturation".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "Sharpness".to_string(),
                data_type: XmpDataType::Integer,
                description: "Sharpness".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "DeviceSettingDescription".to_string(),
                data_type: XmpDataType::Text,
                description: "Device setting description".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "SubjectDistanceRange".to_string(),
                data_type: XmpDataType::Integer,
                description: "Subject distance range".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "ImageUniqueID".to_string(),
                data_type: XmpDataType::Text,
                description: "Image unique ID".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "GPSVersionID".to_string(),
                data_type: XmpDataType::Text,
                description: "GPS version ID".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "GPSLatitudeRef".to_string(),
                data_type: XmpDataType::Text,
                description: "GPS latitude reference".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "GPSLatitude".to_string(),
                data_type: XmpDataType::Text,
                description: "GPS latitude".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "GPSLongitudeRef".to_string(),
                data_type: XmpDataType::Text,
                description: "GPS longitude reference".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "GPSLongitude".to_string(),
                data_type: XmpDataType::Text,
                description: "GPS longitude".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "GPSAltitudeRef".to_string(),
                data_type: XmpDataType::Integer,
                description: "GPS altitude reference".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "GPSAltitude".to_string(),
                data_type: XmpDataType::Real,
                description: "GPS altitude".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "GPSTimeStamp".to_string(),
                data_type: XmpDataType::Text,
                description: "GPS time stamp".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "GPSSatellites".to_string(),
                data_type: XmpDataType::Text,
                description: "GPS satellites".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "GPSStatus".to_string(),
                data_type: XmpDataType::Text,
                description: "GPS status".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "GPSMeasureMode".to_string(),
                data_type: XmpDataType::Text,
                description: "GPS measure mode".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "GPSDOP".to_string(),
                data_type: XmpDataType::Real,
                description: "GPS DOP".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "GPSSpeedRef".to_string(),
                data_type: XmpDataType::Text,
                description: "GPS speed reference".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "GPSSpeed".to_string(),
                data_type: XmpDataType::Real,
                description: "GPS speed".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "GPSTrackRef".to_string(),
                data_type: XmpDataType::Text,
                description: "GPS track reference".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "GPSTrack".to_string(),
                data_type: XmpDataType::Real,
                description: "GPS track".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "GPSImgDirectionRef".to_string(),
                data_type: XmpDataType::Text,
                description: "GPS image direction reference".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "GPSImgDirection".to_string(),
                data_type: XmpDataType::Real,
                description: "GPS image direction".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "GPSMapDatum".to_string(),
                data_type: XmpDataType::Text,
                description: "GPS map datum".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "GPSDestLatitudeRef".to_string(),
                data_type: XmpDataType::Text,
                description: "GPS destination latitude reference".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "GPSDestLatitude".to_string(),
                data_type: XmpDataType::Text,
                description: "GPS destination latitude".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "GPSDestLongitudeRef".to_string(),
                data_type: XmpDataType::Text,
                description: "GPS destination longitude reference".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "GPSDestLongitude".to_string(),
                data_type: XmpDataType::Text,
                description: "GPS destination longitude".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "GPSDestBearingRef".to_string(),
                data_type: XmpDataType::Text,
                description: "GPS destination bearing reference".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "GPSDestBearing".to_string(),
                data_type: XmpDataType::Real,
                description: "GPS destination bearing".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "GPSDestDistanceRef".to_string(),
                data_type: XmpDataType::Text,
                description: "GPS destination distance reference".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "GPSDestDistance".to_string(),
                data_type: XmpDataType::Real,
                description: "GPS destination distance".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "GPSProcessingMethod".to_string(),
                data_type: XmpDataType::Text,
                description: "GPS processing method".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "GPSAreaInformation".to_string(),
                data_type: XmpDataType::Text,
                description: "GPS area information".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "GPSDateStamp".to_string(),
                data_type: XmpDataType::Date,
                description: "GPS date stamp".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "GPSDifferential".to_string(),
                data_type: XmpDataType::Integer,
                description: "GPS differential".to_string(),
                is_array: false,
            },
        ];

        self.namespaces.insert(
            "exif".to_string(),
            NamespaceInfo {
                prefix: "exif".to_string(),
                uri: "http://ns.adobe.com/exif/1.0/".to_string(),
                description: "EXIF schema for digital photography".to_string(),
                properties,
            },
        );
    }

    fn add_tiff_namespace(&mut self) {
        let properties = vec![
            PropertyInfo {
                name: "ImageWidth".to_string(),
                data_type: XmpDataType::Integer,
                description: "Image width".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "ImageLength".to_string(),
                data_type: XmpDataType::Integer,
                description: "Image length".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "BitsPerSample".to_string(),
                data_type: XmpDataType::Integer,
                description: "Bits per sample".to_string(),
                is_array: true,
            },
            PropertyInfo {
                name: "Compression".to_string(),
                data_type: XmpDataType::Integer,
                description: "Compression".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "PhotometricInterpretation".to_string(),
                data_type: XmpDataType::Integer,
                description: "Photometric interpretation".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "Orientation".to_string(),
                data_type: XmpDataType::Integer,
                description: "Orientation".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "SamplesPerPixel".to_string(),
                data_type: XmpDataType::Integer,
                description: "Samples per pixel".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "PlanarConfiguration".to_string(),
                data_type: XmpDataType::Integer,
                description: "Planar configuration".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "YCbCrSubSampling".to_string(),
                data_type: XmpDataType::Integer,
                description: "YCbCr sub sampling".to_string(),
                is_array: true,
            },
            PropertyInfo {
                name: "YCbCrPositioning".to_string(),
                data_type: XmpDataType::Integer,
                description: "YCbCr positioning".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "XResolution".to_string(),
                data_type: XmpDataType::Real,
                description: "X resolution".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "YResolution".to_string(),
                data_type: XmpDataType::Real,
                description: "Y resolution".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "ResolutionUnit".to_string(),
                data_type: XmpDataType::Integer,
                description: "Resolution unit".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "TransferFunction".to_string(),
                data_type: XmpDataType::Integer,
                description: "Transfer function".to_string(),
                is_array: true,
            },
            PropertyInfo {
                name: "WhitePoint".to_string(),
                data_type: XmpDataType::Real,
                description: "White point".to_string(),
                is_array: true,
            },
            PropertyInfo {
                name: "PrimaryChromaticities".to_string(),
                data_type: XmpDataType::Real,
                description: "Primary chromaticities".to_string(),
                is_array: true,
            },
            PropertyInfo {
                name: "YCbCrCoefficients".to_string(),
                data_type: XmpDataType::Real,
                description: "YCbCr coefficients".to_string(),
                is_array: true,
            },
            PropertyInfo {
                name: "ReferenceBlackWhite".to_string(),
                data_type: XmpDataType::Real,
                description: "Reference black white".to_string(),
                is_array: true,
            },
            PropertyInfo {
                name: "DateTime".to_string(),
                data_type: XmpDataType::Date,
                description: "Date time".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "ImageDescription".to_string(),
                data_type: XmpDataType::Text,
                description: "Image description".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "Make".to_string(),
                data_type: XmpDataType::Text,
                description: "Camera make".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "Model".to_string(),
                data_type: XmpDataType::Text,
                description: "Camera model".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "Software".to_string(),
                data_type: XmpDataType::Text,
                description: "Software".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "Artist".to_string(),
                data_type: XmpDataType::Text,
                description: "Artist".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "Copyright".to_string(),
                data_type: XmpDataType::Text,
                description: "Copyright".to_string(),
                is_array: false,
            },
        ];

        self.namespaces.insert(
            "tiff".to_string(),
            NamespaceInfo {
                prefix: "tiff".to_string(),
                uri: "http://ns.adobe.com/tiff/1.0/".to_string(),
                description: "TIFF schema".to_string(),
                properties,
            },
        );
    }

    fn add_iptc_namespace(&mut self) {
        let properties = vec![
            PropertyInfo {
                name: "CiAdrCity".to_string(),
                data_type: XmpDataType::Text,
                description: "Contact info city".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "CiAdrCtry".to_string(),
                data_type: XmpDataType::Text,
                description: "Contact info country".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "CiAdrExtadr".to_string(),
                data_type: XmpDataType::Text,
                description: "Contact info address".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "CiAdrPcode".to_string(),
                data_type: XmpDataType::Text,
                description: "Contact info postal code".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "CiAdrRegion".to_string(),
                data_type: XmpDataType::Text,
                description: "Contact info region".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "CiEmailWork".to_string(),
                data_type: XmpDataType::Text,
                description: "Contact info email".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "CiTelWork".to_string(),
                data_type: XmpDataType::Text,
                description: "Contact info phone".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "CiUrlWork".to_string(),
                data_type: XmpDataType::Text,
                description: "Contact info URL".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "CountryCode".to_string(),
                data_type: XmpDataType::Text,
                description: "Country code".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "IntellectualGenre".to_string(),
                data_type: XmpDataType::Text,
                description: "Intellectual genre".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "Location".to_string(),
                data_type: XmpDataType::Text,
                description: "Location".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "Scene".to_string(),
                data_type: XmpDataType::Text,
                description: "Scene".to_string(),
                is_array: true,
            },
            PropertyInfo {
                name: "SubjectCode".to_string(),
                data_type: XmpDataType::Text,
                description: "Subject code".to_string(),
                is_array: true,
            },
        ];

        self.namespaces.insert(
            "Iptc4xmpCore".to_string(),
            NamespaceInfo {
                prefix: "Iptc4xmpCore".to_string(),
                uri: "http://iptc.org/std/Iptc4xmpCore/1.0/xmlns/".to_string(),
                description: "IPTC Core schema".to_string(),
                properties,
            },
        );
    }

    fn add_dicom_namespace(&mut self) {
        let properties = vec![
            PropertyInfo {
                name: "PatientName".to_string(),
                data_type: XmpDataType::Text,
                description: "Patient name".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "PatientID".to_string(),
                data_type: XmpDataType::Text,
                description: "Patient ID".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "PatientBirthDate".to_string(),
                data_type: XmpDataType::Date,
                description: "Patient birth date".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "PatientSex".to_string(),
                data_type: XmpDataType::Text,
                description: "Patient sex".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "StudyInstanceUID".to_string(),
                data_type: XmpDataType::Text,
                description: "Study instance UID".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "StudyDate".to_string(),
                data_type: XmpDataType::Date,
                description: "Study date".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "StudyTime".to_string(),
                data_type: XmpDataType::Text,
                description: "Study time".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "StudyID".to_string(),
                data_type: XmpDataType::Text,
                description: "Study ID".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "SeriesInstanceUID".to_string(),
                data_type: XmpDataType::Text,
                description: "Series instance UID".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "SeriesNumber".to_string(),
                data_type: XmpDataType::Integer,
                description: "Series number".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "InstanceNumber".to_string(),
                data_type: XmpDataType::Integer,
                description: "Instance number".to_string(),
                is_array: false,
            },
        ];

        self.namespaces.insert(
            "DICOM".to_string(),
            NamespaceInfo {
                prefix: "DICOM".to_string(),
                uri: "http://ns.adobe.com/DICOM/".to_string(),
                description: "DICOM schema for medical imaging".to_string(),
                properties,
            },
        );
    }

    fn add_creative_commons_namespace(&mut self) {
        let properties = vec![
            PropertyInfo {
                name: "license".to_string(),
                data_type: XmpDataType::Uri,
                description: "Creative Commons license".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "attributionName".to_string(),
                data_type: XmpDataType::Text,
                description: "Attribution name".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "attributionURL".to_string(),
                data_type: XmpDataType::Uri,
                description: "Attribution URL".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "morePermissions".to_string(),
                data_type: XmpDataType::Uri,
                description: "More permissions URL".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "useGuidelines".to_string(),
                data_type: XmpDataType::Uri,
                description: "Use guidelines URL".to_string(),
                is_array: false,
            },
        ];

        self.namespaces.insert(
            "cc".to_string(),
            NamespaceInfo {
                prefix: "cc".to_string(),
                uri: "http://creativecommons.org/ns#".to_string(),
                description: "Creative Commons schema".to_string(),
                properties,
            },
        );
    }

    fn add_prism_namespace(&mut self) {
        let properties = vec![
            PropertyInfo {
                name: "publicationName".to_string(),
                data_type: XmpDataType::Text,
                description: "Publication name".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "issn".to_string(),
                data_type: XmpDataType::Text,
                description: "ISSN".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "isbn".to_string(),
                data_type: XmpDataType::Text,
                description: "ISBN".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "doi".to_string(),
                data_type: XmpDataType::Text,
                description: "DOI".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "volume".to_string(),
                data_type: XmpDataType::Text,
                description: "Volume".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "number".to_string(),
                data_type: XmpDataType::Text,
                description: "Issue number".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "startingPage".to_string(),
                data_type: XmpDataType::Text,
                description: "Starting page".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "endingPage".to_string(),
                data_type: XmpDataType::Text,
                description: "Ending page".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "publicationDate".to_string(),
                data_type: XmpDataType::Date,
                description: "Publication date".to_string(),
                is_array: false,
            },
        ];

        self.namespaces.insert(
            "prism".to_string(),
            NamespaceInfo {
                prefix: "prism".to_string(),
                uri: "http://prismstandard.org/namespaces/basic/2.0/".to_string(),
                description: "PRISM schema for publishing".to_string(),
                properties,
            },
        );
    }

    fn add_dam_namespace(&mut self) {
        let properties = vec![
            PropertyInfo {
                name: "sha1".to_string(),
                data_type: XmpDataType::Text,
                description: "SHA1 hash".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "size".to_string(),
                data_type: XmpDataType::Integer,
                description: "File size".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "MIMEType".to_string(),
                data_type: XmpDataType::Text,
                description: "MIME type".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "AssetID".to_string(),
                data_type: XmpDataType::Text,
                description: "Asset ID".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "VersionComment".to_string(),
                data_type: XmpDataType::Text,
                description: "Version comment".to_string(),
                is_array: false,
            },
        ];

        self.namespaces.insert(
            "dam".to_string(),
            NamespaceInfo {
                prefix: "dam".to_string(),
                uri: "http://www.day.com/dam/1.0".to_string(),
                description: "Digital Asset Management schema".to_string(),
                properties,
            },
        );
    }

    fn add_geospatial_namespace(&mut self) {
        let properties = vec![
            PropertyInfo {
                name: "lat".to_string(),
                data_type: XmpDataType::Real,
                description: "Latitude".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "lon".to_string(),
                data_type: XmpDataType::Real,
                description: "Longitude".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "alt".to_string(),
                data_type: XmpDataType::Real,
                description: "Altitude".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "CoordinateSystem".to_string(),
                data_type: XmpDataType::Text,
                description: "Coordinate system".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "BoundingBox".to_string(),
                data_type: XmpDataType::Text,
                description: "Bounding box".to_string(),
                is_array: false,
            },
        ];

        self.namespaces.insert(
            "geo".to_string(),
            NamespaceInfo {
                prefix: "geo".to_string(),
                uri: "http://www.w3.org/2003/01/geo/wgs84_pos#".to_string(),
                description: "Geospatial metadata schema".to_string(),
                properties,
            },
        );
    }

    fn add_scientific_namespace(&mut self) {
        let properties = vec![
            PropertyInfo {
                name: "experimentID".to_string(),
                data_type: XmpDataType::Text,
                description: "Experiment ID".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "instrument".to_string(),
                data_type: XmpDataType::Text,
                description: "Instrument used".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "methodology".to_string(),
                data_type: XmpDataType::Text,
                description: "Methodology".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "dataType".to_string(),
                data_type: XmpDataType::Text,
                description: "Data type".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "units".to_string(),
                data_type: XmpDataType::Text,
                description: "Measurement units".to_string(),
                is_array: false,
            },
        ];

        self.namespaces.insert(
            "sci".to_string(),
            NamespaceInfo {
                prefix: "sci".to_string(),
                uri: "http://ns.example.com/scientific/1.0/".to_string(),
                description: "Scientific data schema".to_string(),
                properties,
            },
        );
    }

    fn add_dynamic_media_namespace(&mut self) {
        let properties = vec![
            PropertyInfo {
                name: "duration".to_string(),
                data_type: XmpDataType::Text,
                description: "Duration".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "videoFrameRate".to_string(),
                data_type: XmpDataType::Real,
                description: "Video frame rate".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "videoFrameSize".to_string(),
                data_type: XmpDataType::Text,
                description: "Video frame size".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "videoPixelAspectRatio".to_string(),
                data_type: XmpDataType::Text,
                description: "Video pixel aspect ratio".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "videoColorSpace".to_string(),
                data_type: XmpDataType::Text,
                description: "Video color space".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "videoCompressor".to_string(),
                data_type: XmpDataType::Text,
                description: "Video compressor".to_string(),
                is_array: false,
            },
        ];

        self.namespaces.insert(
            "xmpDM".to_string(),
            NamespaceInfo {
                prefix: "xmpDM".to_string(),
                uri: "http://ns.adobe.com/xmp/1.0/DynamicMedia/".to_string(),
                description: "Dynamic Media (video/audio) schema".to_string(),
                properties,
            },
        );
    }

    fn add_audio_namespace(&mut self) {
        let properties = vec![
            PropertyInfo {
                name: "audioSampleRate".to_string(),
                data_type: XmpDataType::Integer,
                description: "Audio sample rate".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "audioSampleType".to_string(),
                data_type: XmpDataType::Text,
                description: "Audio sample type".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "audioChannelType".to_string(),
                data_type: XmpDataType::Text,
                description: "Audio channel type".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "audioCompressor".to_string(),
                data_type: XmpDataType::Text,
                description: "Audio compressor".to_string(),
                is_array: false,
            },
        ];

        self.namespaces.insert(
            "xmpDM".to_string(),
            NamespaceInfo {
                prefix: "xmpDM".to_string(),
                uri: "http://ns.adobe.com/xmp/1.0/DynamicMedia/".to_string(),
                description: "Dynamic Media (audio) schema".to_string(),
                properties,
            },
        );
    }

    fn add_workflow_namespace(&mut self) {
        let properties = vec![
            PropertyInfo {
                name: "workflowState".to_string(),
                data_type: XmpDataType::Text,
                description: "Workflow state".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "assignedTo".to_string(),
                data_type: XmpDataType::Text,
                description: "Assigned to".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "dueDate".to_string(),
                data_type: XmpDataType::Date,
                description: "Due date".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "priority".to_string(),
                data_type: XmpDataType::Text,
                description: "Priority".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "approvedBy".to_string(),
                data_type: XmpDataType::Text,
                description: "Approved by".to_string(),
                is_array: false,
            },
        ];

        self.namespaces.insert(
            "workflow".to_string(),
            NamespaceInfo {
                prefix: "workflow".to_string(),
                uri: "http://ns.example.com/workflow/1.0/".to_string(),
                description: "Workflow management schema".to_string(),
                properties,
            },
        );
    }

    fn add_version_management_namespace(&mut self) {
        let properties = vec![
            PropertyInfo {
                name: "versionNumber".to_string(),
                data_type: XmpDataType::Text,
                description: "Version number".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "parentVersion".to_string(),
                data_type: XmpDataType::Text,
                description: "Parent version".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "versionComment".to_string(),
                data_type: XmpDataType::Text,
                description: "Version comment".to_string(),
                is_array: false,
            },
            PropertyInfo {
                name: "branchName".to_string(),
                data_type: XmpDataType::Text,
                description: "Branch name".to_string(),
                is_array: false,
            },
        ];

        self.namespaces.insert(
            "version".to_string(),
            NamespaceInfo {
                prefix: "version".to_string(),
                uri: "http://ns.example.com/version/1.0/".to_string(),
                description: "Version management schema".to_string(),
                properties,
            },
        );
    }
}

impl XmpMetadata {
    /// Get comprehensive namespace information
    pub fn get_namespace_info(&self) -> Vec<(String, String, Vec<String>)> {
        let mut namespace_info = Vec::new();

        for (prefix, uri) in &self.namespaces {
            let mut properties = Vec::new();

            // Collect properties for this namespace
            for prop_key in self.properties.keys() {
                if prop_key.starts_with(&format!("{}:", prefix)) {
                    properties.push(prop_key.clone());
                }
            }

            namespace_info.push((prefix.clone(), uri.clone(), properties));
        }

        namespace_info
    }

    /// Get properties by namespace
    pub fn get_properties_by_namespace(&self, namespace_prefix: &str) -> HashMap<String, String> {
        self.properties
            .iter()
            .filter(|(key, _)| key.starts_with(&format!("{}:", namespace_prefix)))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    /// Get all Dublin Core properties
    pub fn get_dublin_core_properties(&self) -> HashMap<String, String> {
        self.get_properties_by_namespace("dc")
    }

    /// Get all EXIF properties
    pub fn get_exif_properties(&self) -> HashMap<String, String> {
        self.get_properties_by_namespace("exif")
    }

    /// Get all rights management properties
    pub fn get_rights_properties(&self) -> HashMap<String, String> {
        self.get_properties_by_namespace("xmpRights")
    }

    /// Check if the document has specific types of metadata
    pub fn has_geographic_metadata(&self) -> bool {
        self.properties
            .iter()
            .any(|(key, _)| key.starts_with("exif:GPS") || key.starts_with("geo:"))
    }

    pub fn has_camera_metadata(&self) -> bool {
        self.properties.iter().any(|(key, _)| {
            key.starts_with("exif:")
                && (key.contains("Camera")
                    || key.contains("Lens")
                    || key.contains("Exposure")
                    || key.contains("Aperture")
                    || key.contains("Make")
                    || key.contains("Model"))
        })
    }

    pub fn has_creative_commons_license(&self) -> bool {
        self.properties.contains_key("cc:license")
    }

    pub fn has_workflow_metadata(&self) -> bool {
        self.properties
            .iter()
            .any(|(key, _)| key.starts_with("workflow:"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_comprehensive_namespace_registry() {
        let registry = NamespaceRegistry::new();

        // Test that all standard namespaces are registered
        assert!(registry.namespaces.contains_key("dc"));
        assert!(registry.namespaces.contains_key("xmp"));
        assert!(registry.namespaces.contains_key("pdf"));
        assert!(registry.namespaces.contains_key("exif"));
        assert!(registry.namespaces.contains_key("tiff"));
        assert!(registry.namespaces.contains_key("photoshop"));
        assert!(registry.namespaces.contains_key("crs"));
        assert!(registry.namespaces.contains_key("Iptc4xmpCore"));
        assert!(registry.namespaces.contains_key("cc"));
        assert!(registry.namespaces.contains_key("prism"));

        // Test namespace properties
        let dc_ns = registry.namespaces.get("dc").unwrap();
        assert_eq!(dc_ns.uri, "http://purl.org/dc/elements/1.1/");
        assert!(dc_ns.properties.iter().any(|p| p.name == "title"));
        assert!(dc_ns.properties.iter().any(|p| p.name == "creator"));
        assert!(dc_ns.properties.iter().any(|p| p.name == "subject"));
    }

    #[test]
    fn test_enhanced_xmp_parsing() {
        let xmp_xml = r#"<?xml version="1.0"?>
        <x:xmpmeta xmlns:x="adobe:ns:meta/">
            <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
                     xmlns:dc="http://purl.org/dc/elements/1.1/"
                     xmlns:xmp="http://ns.adobe.com/xap/1.0/"
                     xmlns:exif="http://ns.adobe.com/exif/1.0/"
                     xmlns:tiff="http://ns.adobe.com/tiff/1.0/"
                     xmlns:photoshop="http://ns.adobe.com/photoshop/1.0/"
                     xmlns:cc="http://creativecommons.org/ns#">
                <rdf:Description rdf:about=""
                    dc:title="Enhanced XMP Test Document"
                    dc:creator="Test Author"
                    xmp:CreateDate="2024-01-01T12:00:00Z"
                    exif:Make="Canon"
                    exif:Model="EOS 5D"
                    tiff:Orientation="1"
                    photoshop:ColorMode="3"
                    cc:license="http://creativecommons.org/licenses/by/4.0/">
                    <dc:subject>
                        <rdf:Bag>
                            <rdf:li>test</rdf:li>
                            <rdf:li>xmp</rdf:li>
                            <rdf:li>metadata</rdf:li>
                        </rdf:Bag>
                    </dc:subject>
                </rdf:Description>
            </rdf:RDF>
        </x:xmpmeta>"#;

        let result = parse_xmp(xmp_xml);
        assert!(result.is_ok());

        let metadata = result.unwrap();

        // Check that namespaces are properly registered
        assert!(metadata.namespaces.len() >= 7);

        // Check that properties are extracted
        assert!(metadata.properties.contains_key("dc:title"));
        assert!(metadata.properties.contains_key("dc:creator"));
        assert!(metadata.properties.contains_key("xmp:CreateDate"));
        assert!(metadata.properties.contains_key("exif:Make"));
        assert!(metadata.properties.contains_key("cc:license"));

        // Check property values
        assert_eq!(
            metadata.properties.get("dc:title"),
            Some(&"Enhanced XMP Test Document".to_string())
        );
        assert_eq!(
            metadata.properties.get("exif:Make"),
            Some(&"Canon".to_string())
        );
        assert_eq!(
            metadata.properties.get("cc:license"),
            Some(&"http://creativecommons.org/licenses/by/4.0/".to_string())
        );

        // Test namespace-specific methods
        assert!(metadata.has_camera_metadata());
        assert!(metadata.has_creative_commons_license());
        assert!(!metadata.has_geographic_metadata());
    }

    #[test]
    fn test_geographic_metadata_detection() {
        let xmp_xml = r#"<?xml version="1.0"?>
        <x:xmpmeta xmlns:x="adobe:ns:meta/">
            <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
                     xmlns:exif="http://ns.adobe.com/exif/1.0/"
                     xmlns:geo="http://www.w3.org/2003/01/geo/wgs84_pos#">
                <rdf:Description rdf:about=""
                    exif:GPSLatitude="37.7749"
                    exif:GPSLongitude="-122.4194"
                    geo:lat="37.7749"
                    geo:lon="-122.4194"/>
            </rdf:RDF>
        </x:xmpmeta>"#;

        let result = parse_xmp(xmp_xml);
        assert!(result.is_ok());

        let metadata = result.unwrap();
        assert!(metadata.has_geographic_metadata());
    }

    #[test]
    fn test_workflow_metadata_detection() {
        let xmp_xml = r#"<?xml version="1.0"?>
        <x:xmpmeta xmlns:x="adobe:ns:meta/">
            <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
                     xmlns:workflow="http://ns.example.com/workflow/1.0/">
                <rdf:Description rdf:about=""
                    workflow:workflowState="approved"
                    workflow:assignedTo="editor@example.com"/>
            </rdf:RDF>
        </x:xmpmeta>"#;

        let result = parse_xmp(xmp_xml);
        assert!(result.is_ok());

        let metadata = result.unwrap();
        assert!(metadata.has_workflow_metadata());
    }

    #[test]
    fn test_namespace_property_organization() {
        let xmp_xml = r#"<?xml version="1.0"?>
        <x:xmpmeta xmlns:x="adobe:ns:meta/">
            <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
                     xmlns:dc="http://purl.org/dc/elements/1.1/"
                     xmlns:exif="http://ns.adobe.com/exif/1.0/">
                <rdf:Description rdf:about=""
                    dc:title="Test Document"
                    dc:creator="Test Author"
                    exif:Make="Canon"
                    exif:Model="EOS 5D"/>
            </rdf:RDF>
        </x:xmpmeta>"#;

        let result = parse_xmp(xmp_xml);
        assert!(result.is_ok());

        let metadata = result.unwrap();

        let dc_properties = metadata.get_dublin_core_properties();
        assert!(dc_properties.contains_key("dc:title"));
        assert!(dc_properties.contains_key("dc:creator"));

        let exif_properties = metadata.get_exif_properties();
        assert!(exif_properties.contains_key("exif:Make"));
        assert!(exif_properties.contains_key("exif:Model"));

        let namespace_info = metadata.get_namespace_info();
        assert!(namespace_info.len() >= 2);
    }
}
