use crate::ast::{EdgeType, NodeId, NodeType, PdfAstGraph, PdfDocument};
use crate::types::PdfValue;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const AST_SCHEMA_VERSION: &str = "1.1.0";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableDocument {
    pub version: String,
    pub schema_version: String,
    pub ast: SerializableGraph,
    pub catalog: Option<usize>,
    pub info: Option<usize>,
    pub trailer: SerializableValue,
    pub xref_entries: HashMap<String, SerializableXRefEntry>,
    pub metadata: SerializableDocumentMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableXRefEntry {
    pub offset: Option<u64>,
    pub generation: u16,
    pub entry_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableDocumentMetadata {
    pub file_size: Option<u64>,
    pub linearized: bool,
    pub encrypted: bool,
    pub has_forms: bool,
    pub has_xfa: bool,
    pub xfa_packets: usize,
    pub has_xfa_scripts: bool,
    pub xfa_script_nodes: usize,
    pub has_hybrid_forms: bool,
    pub form_field_count: usize,
    pub has_javascript: bool,
    pub has_embedded_files: bool,
    pub has_signatures: bool,
    pub has_richmedia: bool,
    pub richmedia_annotations: usize,
    pub richmedia_assets: usize,
    pub richmedia_scripts: usize,
    pub has_3d: bool,
    pub threed_annotations: usize,
    pub threed_u3d: usize,
    pub threed_prc: usize,
    pub has_audio: bool,
    pub audio_annotations: usize,
    pub has_video: bool,
    pub video_annotations: usize,
    pub has_dss: bool,
    pub dss_vri_count: usize,
    pub dss_certs: usize,
    pub dss_ocsp: usize,
    pub dss_crl: usize,
    pub dss_timestamps: usize,
    pub page_count: usize,
    pub producer: Option<String>,
    pub creator: Option<String>,
    pub creation_date: Option<String>,
    pub modification_date: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableGraph {
    pub nodes: Vec<SerializableNode>,
    pub edges: Vec<SerializableEdge>,
    pub root: Option<usize>,
    pub metadata: GraphMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableNode {
    pub id: usize,
    pub node_type: String,
    pub value: SerializableValue,
    pub object_id: Option<(u32, u16)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableEdge {
    pub from: usize,
    pub to: usize,
    pub edge_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphMetadata {
    pub node_count: usize,
    pub edge_count: usize,
    pub is_cyclic: bool,
    pub serialization_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum SerializableValue {
    Null,
    Boolean(bool),
    Integer(i64),
    Real(f64),
    String(String),
    Name(String),
    Array(Vec<SerializableValue>),
    Dictionary(HashMap<String, SerializableValue>),
    Stream {
        dictionary: HashMap<String, SerializableValue>,
        data: Vec<u8>,
        lazy: Option<crate::types::StreamReference>,
    },
    Reference {
        object_id: u32,
        generation: u16,
    },
}

impl SerializableGraph {
    pub fn from_ast(ast: &PdfAstGraph) -> Self {
        let serializer = GraphSerializer::new();
        serializer.serialize(ast)
    }

    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }

    pub fn to_cbor(&self) -> serde_cbor::Result<Vec<u8>> {
        serde_cbor::to_vec(self)
    }

    pub fn from_json(json: &str) -> serde_json::Result<Self> {
        serde_json::from_str(json)
    }

    pub fn from_cbor(data: &[u8]) -> serde_cbor::Result<Self> {
        serde_cbor::from_slice(data)
    }
}

struct GraphSerializer {
    nodes: Vec<SerializableNode>,
    edges: Vec<SerializableEdge>,
    node_id_map: HashMap<NodeId, usize>,
    next_serial_id: usize,
}

impl GraphSerializer {
    fn new() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
            node_id_map: HashMap::new(),
            next_serial_id: 0,
        }
    }

    fn serialize(mut self, ast: &PdfAstGraph) -> SerializableGraph {
        // Serialize all nodes
        for node in ast.get_all_nodes() {
            let serial_id = self.next_serial_id;
            self.next_serial_id += 1;

            self.node_id_map.insert(node.id, serial_id);

            // Extract object_id if this is an Object node
            let object_id = match &node.node_type {
                crate::ast::NodeType::Object(obj_id) => Some((obj_id.number, obj_id.generation)),
                _ => None,
            };

            let serialized_node = SerializableNode {
                id: serial_id,
                node_type: node_type_name(&node.node_type).to_string(),
                value: Self::serialize_value(&node.value),
                object_id,
            };

            self.nodes.push(serialized_node);
        }

        // Serialize all edges - FIXED: Now properly serializes edges
        for edge in ast.get_all_edges() {
            if let (Some(&from_id), Some(&to_id)) = (
                self.node_id_map.get(&edge.from),
                self.node_id_map.get(&edge.to),
            ) {
                let serialized_edge = SerializableEdge {
                    from: from_id,
                    to: to_id,
                    edge_type: edge_type_name(edge.edge_type).to_string(),
                };
                self.edges.push(serialized_edge);
            }
        }

        // Find root
        let root_serial_id = ast
            .root
            .and_then(|root_node_id| self.node_id_map.get(&root_node_id).copied());

        SerializableGraph {
            nodes: self.nodes,
            edges: self.edges,
            root: root_serial_id,
            metadata: GraphMetadata {
                node_count: ast.node_count(),
                edge_count: ast.edge_count(),
                is_cyclic: ast.is_cyclic(),
                serialization_version: AST_SCHEMA_VERSION.to_string(),
            },
        }
    }

    fn serialize_value(value: &PdfValue) -> SerializableValue {
        match value {
            PdfValue::Null => SerializableValue::Null,
            PdfValue::Boolean(b) => SerializableValue::Boolean(*b),
            PdfValue::Integer(i) => SerializableValue::Integer(*i),
            PdfValue::Real(r) => SerializableValue::Real(*r),
            PdfValue::String(s) => SerializableValue::String(s.to_string_lossy()),
            PdfValue::Name(n) => SerializableValue::Name(n.as_str().to_string()),
            PdfValue::Array(arr) => {
                let items: Vec<SerializableValue> = arr.iter().map(Self::serialize_value).collect();
                SerializableValue::Array(items)
            }
            PdfValue::Dictionary(dict) => {
                let mut map = HashMap::new();
                for (key, val) in dict.iter() {
                    map.insert(key.to_string(), Self::serialize_value(val));
                }
                SerializableValue::Dictionary(map)
            }
            PdfValue::Stream(stream) => {
                let mut dict_map = HashMap::new();
                for (key, val) in stream.dict.iter() {
                    dict_map.insert(key.to_string(), Self::serialize_value(val));
                }
                SerializableValue::Stream {
                    dictionary: dict_map,
                    data: match &stream.data {
                        crate::types::StreamData::Raw(bytes) => bytes.clone(),
                        crate::types::StreamData::Decoded(bytes) => bytes.clone(),
                        crate::types::StreamData::Lazy(_) => Vec::new(),
                    },
                    lazy: match &stream.data {
                        crate::types::StreamData::Lazy(reference) => Some(reference.clone()),
                        _ => None,
                    },
                }
            }
            PdfValue::Reference(r) => SerializableValue::Reference {
                object_id: r.object_number,
                generation: r.generation_number,
            },
        }
    }
}

pub struct GraphDeserializer;

impl GraphDeserializer {
    pub fn deserialize(serialized: SerializableGraph) -> Result<PdfAstGraph, String> {
        if serialized.metadata.serialization_version != AST_SCHEMA_VERSION {
            return Err(format!(
                "Unsupported AST serialization version: {}; expected {}",
                serialized.metadata.serialization_version, AST_SCHEMA_VERSION
            ));
        }
        if serialized.metadata.node_count != serialized.nodes.len() {
            return Err(format!(
                "AST node count mismatch: metadata={}, actual={}",
                serialized.metadata.node_count,
                serialized.nodes.len()
            ));
        }
        if serialized.metadata.edge_count != serialized.edges.len() {
            return Err(format!(
                "AST edge count mismatch: metadata={}, actual={}",
                serialized.metadata.edge_count,
                serialized.edges.len()
            ));
        }

        let mut ast = PdfAstGraph::new();
        let mut id_map: HashMap<usize, NodeId> = HashMap::new();

        // First pass: create all nodes
        for serialized_node in &serialized.nodes {
            if id_map.contains_key(&serialized_node.id) {
                return Err(format!(
                    "Duplicate serialized node ID: {}",
                    serialized_node.id
                ));
            }
            let node_type =
                Self::parse_node_type(&serialized_node.node_type, serialized_node.object_id)?;
            let value = Self::deserialize_value(&serialized_node.value)?;
            let node_id = ast.create_node(node_type, value);

            id_map.insert(serialized_node.id, node_id);
        }

        // Second pass: create all edges
        for serialized_edge in &serialized.edges {
            let from_id = id_map
                .get(&serialized_edge.from)
                .ok_or_else(|| format!("Invalid from node ID: {}", serialized_edge.from))?;
            let to_id = id_map
                .get(&serialized_edge.to)
                .ok_or_else(|| format!("Invalid to node ID: {}", serialized_edge.to))?;
            let edge_type = Self::parse_edge_type(&serialized_edge.edge_type)?;

            ast.add_edge(*from_id, *to_id, edge_type);
        }

        // Set root if it exists
        if let Some(root_serial_id) = serialized.root {
            let root_id = id_map
                .get(&root_serial_id)
                .ok_or_else(|| format!("Invalid root node ID: {}", root_serial_id))?;
            ast.set_root(*root_id);
        }

        Ok(ast)
    }

    fn parse_node_type(
        type_str: &str,
        object_id: Option<(u32, u16)>,
    ) -> Result<crate::ast::NodeType, String> {
        use crate::types::ObjectId;
        match type_str {
            "Root" => Ok(crate::ast::NodeType::Root),
            "Catalog" => Ok(crate::ast::NodeType::Catalog),
            "Pages" => Ok(crate::ast::NodeType::Pages),
            "Page" => Ok(crate::ast::NodeType::Page),
            "Resource" => Ok(crate::ast::NodeType::Resource),
            "Font" => Ok(crate::ast::NodeType::Font),
            "Image" => Ok(crate::ast::NodeType::Image),
            "ContentStream" => Ok(crate::ast::NodeType::ContentStream),
            "Annotation" => Ok(crate::ast::NodeType::Annotation),
            "Action" => Ok(crate::ast::NodeType::Action),
            "Metadata" => Ok(crate::ast::NodeType::Metadata),
            "EmbeddedFile" => Ok(crate::ast::NodeType::EmbeddedFile),
            "Signature" => Ok(crate::ast::NodeType::Signature),
            "Object" => {
                let (num, gen) =
                    object_id.ok_or_else(|| "Object node is missing its object_id".to_string())?;
                Ok(crate::ast::NodeType::Object(ObjectId::new(num, gen)))
            }
            "Unknown" => Ok(crate::ast::NodeType::Unknown),
            "Stream" => Ok(crate::ast::NodeType::Stream),
            "FilteredStream" => Ok(crate::ast::NodeType::FilteredStream),
            "DecodedStream" => Ok(crate::ast::NodeType::DecodedStream),
            "XObject" => Ok(crate::ast::NodeType::XObject),
            "FormXObject" => Ok(crate::ast::NodeType::FormXObject),
            "ImageXObject" => Ok(crate::ast::NodeType::ImageXObject),
            "Type1Font" => Ok(crate::ast::NodeType::Type1Font),
            "TrueTypeFont" => Ok(crate::ast::NodeType::TrueTypeFont),
            "Type3Font" => Ok(crate::ast::NodeType::Type3Font),
            "CIDFont" => Ok(crate::ast::NodeType::CIDFont),
            "JavaScriptAction" => Ok(crate::ast::NodeType::JavaScriptAction),
            "GoToAction" => Ok(crate::ast::NodeType::GoToAction),
            "URIAction" => Ok(crate::ast::NodeType::URIAction),
            "LaunchAction" => Ok(crate::ast::NodeType::LaunchAction),
            "SubmitFormAction" => Ok(crate::ast::NodeType::SubmitFormAction),
            "AcroForm" => Ok(crate::ast::NodeType::AcroForm),
            "Field" => Ok(crate::ast::NodeType::Field),
            "Encrypt" => Ok(crate::ast::NodeType::Encrypt),
            "Permission" => Ok(crate::ast::NodeType::Permission),
            "ContentOperator" => Ok(crate::ast::NodeType::ContentOperator),
            "TextOperator" => Ok(crate::ast::NodeType::TextOperator),
            "GraphicsOperator" => Ok(crate::ast::NodeType::GraphicsOperator),
            "EmbeddedJS" => Ok(crate::ast::NodeType::EmbeddedJS),
            "SuspiciousAction" => Ok(crate::ast::NodeType::SuspiciousAction),
            "ExternalReference" => Ok(crate::ast::NodeType::ExternalReference),
            "EncodedContent" => Ok(crate::ast::NodeType::EncodedContent),
            "Outline" => Ok(crate::ast::NodeType::Outline),
            "OutlineItem" => Ok(crate::ast::NodeType::OutlineItem),
            "NameTree" => Ok(crate::ast::NodeType::NameTree),
            "StructTreeRoot" => Ok(crate::ast::NodeType::StructTreeRoot),
            "StructElem" => Ok(crate::ast::NodeType::StructElem),
            "ColorSpace" => Ok(crate::ast::NodeType::ColorSpace),
            "ICCBased" => Ok(crate::ast::NodeType::ICCBased),
            "Separation" => Ok(crate::ast::NodeType::Separation),
            "DeviceN" => Ok(crate::ast::NodeType::DeviceN),
            "Indexed" => Ok(crate::ast::NodeType::Indexed),
            "Pattern" => Ok(crate::ast::NodeType::Pattern),
            "Shading" => Ok(crate::ast::NodeType::Shading),
            "ExtGState" => Ok(crate::ast::NodeType::ExtGState),
            "Function" => Ok(crate::ast::NodeType::Function),
            "CMap" => Ok(crate::ast::NodeType::CMap),
            "ToUnicode" => Ok(crate::ast::NodeType::ToUnicode),
            "Encoding" => Ok(crate::ast::NodeType::Encoding),
            "OCG" => Ok(crate::ast::NodeType::OCG),
            "OCProperties" => Ok(crate::ast::NodeType::OCProperties),
            "OCMD" => Ok(crate::ast::NodeType::OCMD),
            "RichMedia" => Ok(crate::ast::NodeType::RichMedia),
            "Rendition" => Ok(crate::ast::NodeType::Rendition),
            "Screen" => Ok(crate::ast::NodeType::Screen),
            "Sound" => Ok(crate::ast::NodeType::Sound),
            "Movie" => Ok(crate::ast::NodeType::Movie),
            "ThreeD" => Ok(crate::ast::NodeType::ThreeD),
            "U3D" => Ok(crate::ast::NodeType::U3D),
            "PRC" => Ok(crate::ast::NodeType::PRC),
            "OutputIntent" => Ok(crate::ast::NodeType::OutputIntent),
            "LinkAnnotation" => Ok(crate::ast::NodeType::LinkAnnotation),
            "WidgetAnnotation" => Ok(crate::ast::NodeType::WidgetAnnotation),
            "FileAttachmentAnnotation" => Ok(crate::ast::NodeType::FileAttachmentAnnotation),
            "InlineImage" => Ok(crate::ast::NodeType::InlineImage),
            "Form" => Ok(crate::ast::NodeType::Form),
            "Structure" => Ok(crate::ast::NodeType::Structure),
            "Multimedia" => Ok(crate::ast::NodeType::Multimedia),
            "JavaScript" => Ok(crate::ast::NodeType::JavaScript),
            "Encryption" => Ok(crate::ast::NodeType::Encryption),
            "Content" => Ok(crate::ast::NodeType::Content),
            "Other" => Ok(crate::ast::NodeType::Other),
            _ => Err(format!("Unknown node type: {}", type_str)),
        }
    }

    fn parse_edge_type(type_str: &str) -> Result<EdgeType, String> {
        match type_str {
            "Child" => Ok(EdgeType::Child),
            "Reference" => Ok(EdgeType::Reference),
            "Parent" => Ok(EdgeType::Parent),
            "Resource" => Ok(EdgeType::Resource),
            "Annotation" => Ok(EdgeType::Annotation),
            "Content" => Ok(EdgeType::Content),
            _ => Err(format!("Unknown edge type: {}", type_str)),
        }
    }

    fn deserialize_value(value: &SerializableValue) -> Result<PdfValue, String> {
        match value {
            SerializableValue::Null => Ok(PdfValue::Null),
            SerializableValue::Boolean(b) => Ok(PdfValue::Boolean(*b)),
            SerializableValue::Integer(i) => Ok(PdfValue::Integer(*i)),
            SerializableValue::Real(r) => Ok(PdfValue::Real(*r)),
            SerializableValue::String(s) => Ok(PdfValue::String(
                crate::types::PdfString::new_literal(s.as_bytes()),
            )),
            SerializableValue::Name(n) => Ok(PdfValue::Name(crate::types::PdfName::new(n))),
            SerializableValue::Array(items) => {
                let mut array = crate::types::PdfArray::new();
                for item in items {
                    array.push(Self::deserialize_value(item)?);
                }
                Ok(PdfValue::Array(array))
            }
            SerializableValue::Dictionary(map) => {
                let mut dict = crate::types::PdfDictionary::new();
                for (key, val) in map {
                    dict.insert(key.as_str(), Self::deserialize_value(val)?);
                }
                Ok(PdfValue::Dictionary(dict))
            }
            SerializableValue::Stream {
                dictionary,
                data,
                lazy,
            } => {
                let mut dict = crate::types::PdfDictionary::new();
                for (key, val) in dictionary {
                    dict.insert(key.as_str(), Self::deserialize_value(val)?);
                }
                let stream = if let Some(reference) = lazy {
                    crate::types::PdfStream::new_lazy(dict, reference.clone())
                } else {
                    crate::types::PdfStream {
                        dict,
                        data: crate::types::StreamData::Raw(data.clone()),
                    }
                };
                Ok(PdfValue::Stream(stream))
            }
            SerializableValue::Reference {
                object_id,
                generation,
            } => Ok(PdfValue::Reference(crate::types::PdfReference {
                object_number: *object_id,
                generation_number: *generation,
            })),
        }
    }
}

pub(crate) fn node_type_name(node_type: &NodeType) -> &'static str {
    match node_type {
        NodeType::Root => "Root",
        NodeType::Catalog => "Catalog",
        NodeType::Pages => "Pages",
        NodeType::Page => "Page",
        NodeType::Resource => "Resource",
        NodeType::Font => "Font",
        NodeType::Image => "Image",
        NodeType::ContentStream => "ContentStream",
        NodeType::Annotation => "Annotation",
        NodeType::Action => "Action",
        NodeType::Metadata => "Metadata",
        NodeType::EmbeddedFile => "EmbeddedFile",
        NodeType::Signature => "Signature",
        NodeType::Object(_) => "Object",
        NodeType::Unknown => "Unknown",
        NodeType::Stream => "Stream",
        NodeType::FilteredStream => "FilteredStream",
        NodeType::DecodedStream => "DecodedStream",
        NodeType::XObject => "XObject",
        NodeType::FormXObject => "FormXObject",
        NodeType::ImageXObject => "ImageXObject",
        NodeType::Type1Font => "Type1Font",
        NodeType::TrueTypeFont => "TrueTypeFont",
        NodeType::Type3Font => "Type3Font",
        NodeType::CIDFont => "CIDFont",
        NodeType::JavaScriptAction => "JavaScriptAction",
        NodeType::GoToAction => "GoToAction",
        NodeType::URIAction => "URIAction",
        NodeType::LaunchAction => "LaunchAction",
        NodeType::SubmitFormAction => "SubmitFormAction",
        NodeType::AcroForm => "AcroForm",
        NodeType::Field => "Field",
        NodeType::Encrypt => "Encrypt",
        NodeType::Permission => "Permission",
        NodeType::ContentOperator => "ContentOperator",
        NodeType::TextOperator => "TextOperator",
        NodeType::GraphicsOperator => "GraphicsOperator",
        NodeType::EmbeddedJS => "EmbeddedJS",
        NodeType::SuspiciousAction => "SuspiciousAction",
        NodeType::ExternalReference => "ExternalReference",
        NodeType::EncodedContent => "EncodedContent",
        NodeType::Outline => "Outline",
        NodeType::OutlineItem => "OutlineItem",
        NodeType::NameTree => "NameTree",
        NodeType::StructTreeRoot => "StructTreeRoot",
        NodeType::StructElem => "StructElem",
        NodeType::ColorSpace => "ColorSpace",
        NodeType::ICCBased => "ICCBased",
        NodeType::Separation => "Separation",
        NodeType::DeviceN => "DeviceN",
        NodeType::Indexed => "Indexed",
        NodeType::Pattern => "Pattern",
        NodeType::Shading => "Shading",
        NodeType::ExtGState => "ExtGState",
        NodeType::Function => "Function",
        NodeType::CMap => "CMap",
        NodeType::ToUnicode => "ToUnicode",
        NodeType::Encoding => "Encoding",
        NodeType::OCG => "OCG",
        NodeType::OCProperties => "OCProperties",
        NodeType::OCMD => "OCMD",
        NodeType::RichMedia => "RichMedia",
        NodeType::Rendition => "Rendition",
        NodeType::Screen => "Screen",
        NodeType::Sound => "Sound",
        NodeType::Movie => "Movie",
        NodeType::ThreeD => "ThreeD",
        NodeType::U3D => "U3D",
        NodeType::PRC => "PRC",
        NodeType::OutputIntent => "OutputIntent",
        NodeType::LinkAnnotation => "LinkAnnotation",
        NodeType::WidgetAnnotation => "WidgetAnnotation",
        NodeType::FileAttachmentAnnotation => "FileAttachmentAnnotation",
        NodeType::InlineImage => "InlineImage",
        NodeType::Form => "Form",
        NodeType::Structure => "Structure",
        NodeType::Multimedia => "Multimedia",
        NodeType::JavaScript => "JavaScript",
        NodeType::Encryption => "Encryption",
        NodeType::Content => "Content",
        NodeType::Other => "Other",
    }
}

pub(crate) fn edge_type_name(edge_type: EdgeType) -> &'static str {
    match edge_type {
        EdgeType::Child => "Child",
        EdgeType::Reference => "Reference",
        EdgeType::Parent => "Parent",
        EdgeType::Resource => "Resource",
        EdgeType::Annotation => "Annotation",
        EdgeType::Content => "Content",
    }
}

/// Convert a PdfDocument to JSON string
pub fn to_json(document: &PdfDocument) -> Result<String, serde_json::Error> {
    let serializable = SerializableDocument::from_document(document);
    serde_json::to_string_pretty(&serializable)
}

impl SerializableDocument {
    pub fn from_document(document: &PdfDocument) -> Self {
        let ast_serializable = SerializableGraph::from_ast(&document.ast);

        // Convert XRef entries
        let mut xref_entries = HashMap::new();
        for (obj_id, entry) in &document.xref.entries {
            let key = format!("{}_{}", obj_id.number, obj_id.generation);
            let serializable_entry = match entry {
                crate::ast::document::XRefEntry::InUse { offset, generation } => {
                    SerializableXRefEntry {
                        offset: Some(*offset),
                        generation: *generation,
                        entry_type: "InUse".to_string(),
                    }
                }
                crate::ast::document::XRefEntry::Free { generation, .. } => SerializableXRefEntry {
                    offset: None,
                    generation: *generation,
                    entry_type: "Free".to_string(),
                },
                crate::ast::document::XRefEntry::Compressed { .. } => SerializableXRefEntry {
                    offset: None,
                    generation: 0,
                    entry_type: "Compressed".to_string(),
                },
            };
            xref_entries.insert(key, serializable_entry);
        }

        // Convert catalog and info to serial IDs
        let catalog_serial_id = document.catalog.and_then(|node_id| {
            ast_serializable
                .nodes
                .iter()
                .find(|node| node.id == node_id.0)
                .map(|node| node.id)
        });

        let info_serial_id = document.info.and_then(|node_id| {
            ast_serializable
                .nodes
                .iter()
                .find(|node| node.id == node_id.0)
                .map(|node| node.id)
        });

        SerializableDocument {
            version: document.version.to_string(),
            schema_version: AST_SCHEMA_VERSION.to_string(),
            ast: ast_serializable,
            catalog: catalog_serial_id,
            info: info_serial_id,
            trailer: GraphSerializer::serialize_value(&PdfValue::Dictionary(
                document.trailer.clone(),
            )),
            xref_entries,
            metadata: SerializableDocumentMetadata {
                file_size: document.metadata.file_size,
                linearized: document.metadata.linearized,
                encrypted: document.metadata.encrypted,
                has_forms: document.metadata.has_forms,
                has_xfa: document.metadata.has_xfa,
                xfa_packets: document.metadata.xfa_packets,
                has_xfa_scripts: document.metadata.has_xfa_scripts,
                xfa_script_nodes: document.metadata.xfa_script_nodes,
                has_hybrid_forms: document.metadata.has_hybrid_forms,
                form_field_count: document.metadata.form_field_count,
                has_javascript: document.metadata.has_javascript,
                has_embedded_files: document.metadata.has_embedded_files,
                has_signatures: document.metadata.has_signatures,
                has_richmedia: document.metadata.has_richmedia,
                richmedia_annotations: document.metadata.richmedia_annotations,
                richmedia_assets: document.metadata.richmedia_assets,
                richmedia_scripts: document.metadata.richmedia_scripts,
                has_3d: document.metadata.has_3d,
                threed_annotations: document.metadata.threed_annotations,
                threed_u3d: document.metadata.threed_u3d,
                threed_prc: document.metadata.threed_prc,
                has_audio: document.metadata.has_audio,
                audio_annotations: document.metadata.audio_annotations,
                has_video: document.metadata.has_video,
                video_annotations: document.metadata.video_annotations,
                has_dss: document.metadata.has_dss,
                dss_vri_count: document.metadata.dss_vri_count,
                dss_certs: document.metadata.dss_certs,
                dss_ocsp: document.metadata.dss_ocsp,
                dss_crl: document.metadata.dss_crl,
                dss_timestamps: document.metadata.dss_timestamps,
                page_count: document.metadata.page_count,
                producer: document.metadata.producer.clone(),
                creator: document.metadata.creator.clone(),
                creation_date: document.metadata.creation_date.clone(),
                modification_date: document.metadata.modification_date.clone(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{NodeType, PdfAstGraph, PdfDocument, PdfVersion};
    use crate::types::{PdfDictionary, PdfValue};

    #[test]
    fn test_graph_serialization() {
        let mut ast = PdfAstGraph::new();
        let root_value = PdfValue::Dictionary(PdfDictionary::new());
        let root_id = ast.create_node(NodeType::Root, root_value);
        let object_id = crate::types::ObjectId::new(42, 7);
        let object_node_id = ast.create_node(NodeType::Object(object_id), PdfValue::Integer(1));
        ast.set_root(root_id);

        let serialized = SerializableGraph::from_ast(&ast);
        assert_eq!(serialized.nodes.len(), 2);
        assert_eq!(serialized.edges.len(), 0);
        assert!(serialized.root.is_some());
        let object_node = serialized
            .nodes
            .iter()
            .find(|node| node.id == object_node_id.index())
            .unwrap();
        assert_eq!(object_node.node_type, "Object");
        assert_eq!(object_node.object_id, Some((42, 7)));

        let json = serialized.to_json().unwrap();
        assert!(json.contains("Root"));

        let deserialized = SerializableGraph::from_json(&json).unwrap();
        assert_eq!(deserialized.nodes.len(), 2);
        let restored = GraphDeserializer::deserialize(deserialized).unwrap();
        assert!(restored.get_node_by_object(object_id).is_some());
    }

    #[test]
    fn rejects_object_nodes_without_identity() {
        let graph = SerializableGraph {
            nodes: vec![SerializableNode {
                id: 0,
                node_type: "Object".to_string(),
                value: SerializableValue::Null,
                object_id: None,
            }],
            edges: Vec::new(),
            root: Some(0),
            metadata: GraphMetadata {
                node_count: 1,
                edge_count: 0,
                is_cyclic: false,
                serialization_version: AST_SCHEMA_VERSION.to_string(),
            },
        };

        let error = GraphDeserializer::deserialize(graph).unwrap_err();
        assert!(error.contains("missing its object_id"));
    }

    #[test]
    fn rejects_unsupported_schema_versions() {
        let mut ast = PdfAstGraph::new();
        let root_id = ast.create_node(NodeType::Root, PdfValue::Null);
        ast.set_root(root_id);
        let mut graph = SerializableGraph::from_ast(&ast);

        for version in ["1", "1.0.0", "1.2.0", "2.0.0"] {
            graph.metadata.serialization_version = version.to_string();
            let error = GraphDeserializer::deserialize(graph.clone()).unwrap_err();
            assert!(error.contains("Unsupported AST serialization version"));
        }
    }

    #[test]
    fn rejects_inconsistent_graph_metadata_and_topology() {
        let mut ast = PdfAstGraph::new();
        let root_id = ast.create_node(NodeType::Root, PdfValue::Null);
        ast.set_root(root_id);
        let graph = SerializableGraph::from_ast(&ast);

        let mut count_mismatch = graph.clone();
        count_mismatch.metadata.node_count += 1;
        let error = GraphDeserializer::deserialize(count_mismatch).unwrap_err();
        assert!(error.contains("node count mismatch"));

        let mut duplicate_id = graph.clone();
        duplicate_id.nodes.push(duplicate_id.nodes[0].clone());
        duplicate_id.metadata.node_count += 1;
        let error = GraphDeserializer::deserialize(duplicate_id).unwrap_err();
        assert!(error.contains("Duplicate serialized node ID"));

        let mut invalid_root = graph;
        invalid_root.root = Some(99);
        let error = GraphDeserializer::deserialize(invalid_root).unwrap_err();
        assert!(error.contains("Invalid root node ID"));
    }

    #[test]
    fn test_document_serialization() {
        let version = PdfVersion::new(1, 7);
        let document = PdfDocument::new(version);

        let json = to_json(&document).unwrap();
        assert!(json.contains("1.7"));
        assert!(json.contains("ast"));
        assert!(json.contains("metadata"));
        assert!(json.contains("schema_version"));
    }

    #[test]
    fn test_cbor_serialization() {
        let mut ast = PdfAstGraph::new();
        let root_value = PdfValue::Dictionary(PdfDictionary::new());
        let root_id = ast.create_node(NodeType::Root, root_value);
        ast.set_root(root_id);

        let serialized = SerializableGraph::from_ast(&ast);
        let cbor_data = serialized.to_cbor().unwrap();
        assert!(!cbor_data.is_empty());

        let deserialized = SerializableGraph::from_cbor(&cbor_data).unwrap();
        assert_eq!(deserialized.nodes.len(), 1);
    }
}
