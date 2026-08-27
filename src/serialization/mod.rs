use crate::ast::{
    AstNode, CrossReferenceTable, DocumentMetadata, DocumentRevision, EdgeType, ForensicSnapshot,
    LinearizationInfo, NodeId, NodeType, PdfAstGraph, PdfDocument, PdfVersion, XRefEntry,
    XRefStream,
};
use crate::performance::{ResourceBudget, ResourceBudgetError};
use crate::types::{ObjectId, PdfValue};
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
    #[serde(default)]
    pub xref_prev_offset: Option<u64>,
    #[serde(default)]
    pub xref_hybrid_mode: bool,
    #[serde(default)]
    pub xref_streams: Vec<SerializableXRefStream>,
    #[serde(default)]
    pub original_bytes: Option<Vec<u8>>,
    #[serde(default)]
    pub revisions: Vec<SerializableRevision>,
    #[serde(default)]
    pub diagnostics: Vec<crate::ast::ParseDiagnostic>,
    #[serde(default)]
    pub forensic: Option<SerializableForensicSnapshot>,
    #[serde(default)]
    pub linearization: Option<SerializableLinearizationInfo>,
    pub metadata: SerializableDocumentMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableXRefEntry {
    pub offset: Option<u64>,
    pub generation: u16,
    pub entry_type: String,
    #[serde(default)]
    pub next_free_object: Option<u32>,
    #[serde(default)]
    pub stream_object: Option<u32>,
    #[serde(default)]
    pub index: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableXRefStream {
    pub object_id: (u32, u16),
    pub dict: SerializableValue,
    pub entries: Vec<XRefEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableLinearizationInfo {
    pub version: f64,
    pub file_length: u64,
    pub hint_stream_offset: u64,
    pub hint_stream_length: Option<u64>,
    pub object_count: u32,
    pub first_page_object_number: u32,
    pub first_page_end_offset: u64,
    pub main_xref_table_entries: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableRevision {
    pub revision_number: u32,
    pub xref_offset: u64,
    pub trailer: SerializableValue,
    pub modified_objects: Vec<(u32, u16)>,
    pub added_objects: Vec<(u32, u16)>,
    pub deleted_objects: Vec<(u32, u16)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableForensicSnapshot {
    pub declared_xref: HashMap<String, crate::ast::XRefEntry>,
    pub recovered_xref: HashMap<String, crate::ast::XRefEntry>,
    pub duplicate_objects: Vec<(u32, u16)>,
    pub overwritten_objects: Vec<(u32, u16)>,
    pub residual_ranges: Vec<(u64, u64)>,
}

fn serialize_xref_entry(entry: XRefEntry) -> SerializableXRefEntry {
    match entry {
        XRefEntry::InUse { offset, generation } => SerializableXRefEntry {
            offset: Some(offset),
            generation,
            entry_type: "InUse".to_string(),
            next_free_object: None,
            stream_object: None,
            index: None,
        },
        XRefEntry::Free {
            next_free_object,
            generation,
        } => SerializableXRefEntry {
            offset: None,
            generation,
            entry_type: "Free".to_string(),
            next_free_object: Some(next_free_object),
            stream_object: None,
            index: None,
        },
        XRefEntry::Compressed {
            stream_object,
            index,
        } => SerializableXRefEntry {
            offset: None,
            generation: 0,
            entry_type: "Compressed".to_string(),
            next_free_object: None,
            stream_object: Some(stream_object),
            index: Some(index),
        },
    }
}

fn deserialize_xref_entry(entry: &SerializableXRefEntry) -> Result<XRefEntry, String> {
    match entry.entry_type.as_str() {
        "InUse" => Ok(XRefEntry::InUse {
            offset: entry
                .offset
                .ok_or_else(|| "InUse xref entry is missing offset".to_string())?,
            generation: entry.generation,
        }),
        "Free" => Ok(XRefEntry::Free {
            next_free_object: entry
                .next_free_object
                .ok_or_else(|| "Free xref entry is missing next_free_object".to_string())?,
            generation: entry.generation,
        }),
        "Compressed" => {
            if entry.generation != 0 {
                return Err("Compressed xref entry has non-zero generation".to_string());
            }
            Ok(XRefEntry::Compressed {
                stream_object: entry
                    .stream_object
                    .ok_or_else(|| "Compressed xref entry is missing stream_object".to_string())?,
                index: entry
                    .index
                    .ok_or_else(|| "Compressed xref entry is missing index".to_string())?,
            })
        }
        entry_type => Err(format!("Unknown xref entry type: {entry_type}")),
    }
}

fn parse_object_id(value: &str) -> Result<ObjectId, String> {
    let (number, generation) = value
        .split_once('_')
        .ok_or_else(|| format!("Invalid object ID: {value}"))?;
    let number = number
        .parse::<u32>()
        .map_err(|_| format!("Invalid object number: {number}"))?;
    let generation = generation
        .parse::<u16>()
        .map_err(|_| format!("Invalid object generation: {generation}"))?;
    Ok(ObjectId::new(number, generation))
}

fn serial_object_id(id: ObjectId) -> (u32, u16) {
    (id.number, id.generation)
}

fn object_id_from_tuple((number, generation): (u32, u16)) -> ObjectId {
    ObjectId::new(number, generation)
}

impl From<&crate::ast::ForensicSnapshot> for SerializableForensicSnapshot {
    fn from(snapshot: &crate::ast::ForensicSnapshot) -> Self {
        let serialize_xref = |entries: &HashMap<crate::types::ObjectId, crate::ast::XRefEntry>| {
            entries
                .iter()
                .map(|(object_id, entry)| {
                    (
                        format!("{}_{}", object_id.number, object_id.generation),
                        *entry,
                    )
                })
                .collect()
        };
        Self {
            declared_xref: serialize_xref(&snapshot.declared_xref),
            recovered_xref: serialize_xref(&snapshot.recovered_xref),
            duplicate_objects: snapshot
                .duplicate_objects
                .iter()
                .map(|id| (id.number, id.generation))
                .collect(),
            overwritten_objects: snapshot
                .overwritten_objects
                .iter()
                .map(|id| (id.number, id.generation))
                .collect(),
            residual_ranges: snapshot.residual_ranges.clone(),
        }
    }
}

fn deserialize_forensic(
    snapshot: &SerializableForensicSnapshot,
) -> Result<ForensicSnapshot, String> {
    let deserialize_xref = |entries: &HashMap<String, XRefEntry>| {
        entries
            .iter()
            .map(|(key, entry)| Ok((parse_object_id(key)?, *entry)))
            .collect::<Result<HashMap<_, _>, String>>()
    };
    Ok(ForensicSnapshot {
        declared_xref: deserialize_xref(&snapshot.declared_xref)?,
        recovered_xref: deserialize_xref(&snapshot.recovered_xref)?,
        duplicate_objects: snapshot
            .duplicate_objects
            .iter()
            .copied()
            .map(object_id_from_tuple)
            .collect(),
        overwritten_objects: snapshot
            .overwritten_objects
            .iter()
            .copied()
            .map(object_id_from_tuple)
            .collect(),
        residual_ranges: snapshot.residual_ranges.clone(),
    })
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
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub subject: Option<String>,
    #[serde(default)]
    pub compliance: Vec<crate::ast::ComplianceProfile>,
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
    /// Stable node identity; absent in the historical 1.0/1.1 envelope.
    #[serde(default)]
    pub original_id: Option<usize>,
    pub id: usize,
    pub node_type: String,
    pub value: SerializableValue,
    pub object_id: Option<(u32, u16)>,
    #[serde(default)]
    pub offset: Option<u64>,
    #[serde(default)]
    pub size: Option<usize>,
    #[serde(default)]
    pub errors: Vec<crate::ast::ParseError>,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub properties: HashMap<String, String>,
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
        #[serde(default)]
        decoded: bool,
        #[serde(default)]
        original_bytes: Option<Vec<u8>>,
        #[serde(default)]
        declared_length: Option<u64>,
        #[serde(default)]
        observed_length: Option<usize>,
        #[serde(default)]
        parse_errors: Vec<String>,
        #[serde(default)]
        recovery_actions: Vec<String>,
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

    /// Serializes an AST after charging its nodes, edges, and byte payloads to
    /// the supplied resource budget.
    pub fn from_ast_with_budget(
        ast: &PdfAstGraph,
        budget: &ResourceBudget,
    ) -> Result<Self, ResourceBudgetError> {
        for node in ast.get_all_nodes() {
            budget.consume_node()?;
            check_value_budget(&node.value, budget)?;
        }
        for _ in ast.get_all_edges() {
            budget.consume_edge()?;
        }
        Ok(Self::from_ast(ast))
    }

    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }

    pub fn to_json_with_budget(&self, budget: &ResourceBudget) -> Result<String, String> {
        check_serializable_graph_budget(self, budget).map_err(|error| error.to_string())?;
        let output = self.to_json().map_err(|error| error.to_string())?;
        budget
            .consume_decoded(output.len() as u64)
            .map_err(|error| error.to_string())?;
        Ok(output)
    }

    pub fn to_cbor(&self) -> serde_cbor::Result<Vec<u8>> {
        serde_cbor::to_vec(self)
    }

    pub fn to_cbor_with_budget(&self, budget: &ResourceBudget) -> Result<Vec<u8>, String> {
        check_serializable_graph_budget(self, budget).map_err(|error| error.to_string())?;
        let output = self.to_cbor().map_err(|error| error.to_string())?;
        budget
            .consume_decoded(output.len() as u64)
            .map_err(|error| error.to_string())?;
        Ok(output)
    }

    pub fn from_json(json: &str) -> serde_json::Result<Self> {
        serde_json::from_str(json)
    }

    pub fn from_json_with_budget(json: &str, budget: &ResourceBudget) -> Result<Self, String> {
        budget.check().map_err(|error| error.to_string())?;
        if json.len() as u64 > budget.max_input_bytes {
            return Err(ResourceBudgetError::InputBytes.to_string());
        }
        let graph = Self::from_json(json).map_err(|error| error.to_string())?;
        check_serializable_graph_budget(&graph, budget).map_err(|error| error.to_string())?;
        Ok(graph)
    }

    pub fn from_cbor(data: &[u8]) -> serde_cbor::Result<Self> {
        serde_cbor::from_slice(data)
    }

    pub fn from_cbor_with_budget(data: &[u8], budget: &ResourceBudget) -> Result<Self, String> {
        budget.check().map_err(|error| error.to_string())?;
        if data.len() as u64 > budget.max_input_bytes {
            return Err(ResourceBudgetError::InputBytes.to_string());
        }
        let graph = Self::from_cbor(data).map_err(|error| error.to_string())?;
        check_serializable_graph_budget(&graph, budget).map_err(|error| error.to_string())?;
        Ok(graph)
    }

    /// Deserializes an AST after charging its nodes, edges, and byte payloads
    /// to the supplied resource budget.
    pub fn deserialize_with_budget(&self, budget: &ResourceBudget) -> Result<PdfAstGraph, String> {
        check_serializable_graph_budget(self, budget).map_err(|error| error.to_string())?;
        GraphDeserializer::deserialize(self.clone())
    }
}

fn check_value_budget(
    value: &PdfValue,
    budget: &ResourceBudget,
) -> Result<(), ResourceBudgetError> {
    match value {
        PdfValue::String(value) => budget.consume_input(value.to_string_lossy().len() as u64)?,
        PdfValue::Name(value) => budget.consume_input(value.as_str().len() as u64)?,
        PdfValue::Array(values) => {
            for value in values {
                check_value_budget(value, budget)?;
            }
        }
        PdfValue::Dictionary(dictionary) => {
            for (key, value) in dictionary.iter() {
                budget.consume_input(key.as_str().len() as u64)?;
                check_value_budget(value, budget)?;
            }
        }
        PdfValue::Stream(stream) => {
            if let Some(data) = stream.data.as_bytes() {
                budget.consume_input(data.len() as u64)?;
            }
            if let Some(data) = stream.original_data() {
                budget.consume_input(data.len() as u64)?;
            }
            check_value_budget(&PdfValue::Dictionary(stream.dict.clone()), budget)?;
        }
        PdfValue::Null
        | PdfValue::Boolean(_)
        | PdfValue::Integer(_)
        | PdfValue::Real(_)
        | PdfValue::Reference(_) => {}
    }
    Ok(())
}

fn check_serializable_value_budget(
    value: &SerializableValue,
    budget: &ResourceBudget,
) -> Result<(), ResourceBudgetError> {
    match value {
        SerializableValue::String(value) | SerializableValue::Name(value) => {
            budget.consume_input(value.len() as u64)?
        }
        SerializableValue::Array(values) => {
            for value in values {
                check_serializable_value_budget(value, budget)?;
            }
        }
        SerializableValue::Dictionary(dictionary) => {
            for (key, value) in dictionary {
                budget.consume_input(key.len() as u64)?;
                check_serializable_value_budget(value, budget)?;
            }
        }
        SerializableValue::Stream {
            dictionary,
            data,
            original_bytes,
            ..
        } => {
            budget.consume_input(data.len() as u64)?;
            if let Some(original_bytes) = original_bytes {
                budget.consume_input(original_bytes.len() as u64)?;
            }
            for (key, value) in dictionary {
                budget.consume_input(key.len() as u64)?;
                check_serializable_value_budget(value, budget)?;
            }
        }
        SerializableValue::Null
        | SerializableValue::Boolean(_)
        | SerializableValue::Integer(_)
        | SerializableValue::Real(_)
        | SerializableValue::Reference { .. } => {}
    }
    Ok(())
}

fn check_serializable_graph_budget(
    graph: &SerializableGraph,
    budget: &ResourceBudget,
) -> Result<(), ResourceBudgetError> {
    for node in &graph.nodes {
        budget.consume_node()?;
        check_serializable_value_budget(&node.value, budget)?;
    }
    for _ in &graph.edges {
        budget.consume_edge()?;
    }
    Ok(())
}

fn check_serializable_document_budget(
    document: &SerializableDocument,
    budget: &ResourceBudget,
) -> Result<(), ResourceBudgetError> {
    check_serializable_graph_budget(&document.ast, budget)?;
    if let Some(bytes) = &document.original_bytes {
        budget.consume_input(bytes.len() as u64)?;
    }
    check_serializable_value_budget(&document.trailer, budget)?;
    for _ in &document.xref_entries {
        budget.consume_object()?;
    }
    for stream in &document.xref_streams {
        budget.consume_object()?;
        check_serializable_value_budget(&stream.dict, budget)?;
    }
    for revision in &document.revisions {
        budget.consume_object()?;
        check_serializable_value_budget(&revision.trailer, budget)?;
    }
    Ok(())
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

            let object_id = ast
                .get_object_id(node.id)
                .map(|object_id| (object_id.number, object_id.generation));

            let serialized_node = SerializableNode {
                original_id: Some(node.id.0),
                id: serial_id,
                node_type: node_type_name(&node.node_type).to_string(),
                value: Self::serialize_value(&node.value),
                object_id,
                offset: node.metadata.offset,
                size: node.metadata.size,
                errors: node.metadata.errors.clone(),
                warnings: node.metadata.warnings.clone(),
                properties: node.metadata.properties.clone(),
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
                    decoded: matches!(stream.data, crate::types::StreamData::Decoded(_)),
                    original_bytes: stream.lossless.original_bytes.clone(),
                    declared_length: stream.lossless.declared_length,
                    observed_length: Some(stream.lossless.observed_length),
                    parse_errors: stream.lossless.parse_errors.clone(),
                    recovery_actions: stream.lossless.recovery_actions.clone(),
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
        let serialized = Self::migrate(serialized)?;
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
        let mut restored_ids = std::collections::HashSet::new();

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
            let node_id = NodeId(serialized_node.original_id.unwrap_or(serialized_node.id));
            if !restored_ids.insert(node_id) {
                return Err(format!("Duplicate restored node ID: {}", node_id.0));
            }
            ast.add_node(AstNode::new(node_id, node_type, value));
            if let Some((number, generation)) = serialized_node.object_id {
                ast.register_object_node(ObjectId::new(number, generation), node_id);
            }
            let node = ast
                .get_node_mut(node_id)
                .ok_or_else(|| format!("Failed to restore node {}", serialized_node.id))?;
            node.metadata.offset = serialized_node.offset;
            node.metadata.size = serialized_node.size;
            node.metadata.errors = serialized_node.errors.clone();
            node.metadata.warnings = serialized_node.warnings.clone();
            node.metadata.properties = serialized_node.properties.clone();

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

    fn migrate(mut serialized: SerializableGraph) -> Result<SerializableGraph, String> {
        match serialized.metadata.serialization_version.as_str() {
            "1.0" | "1.0.0" => {
                if serialized
                    .nodes
                    .iter()
                    .any(|node| node.node_type == "Object" && node.object_id.is_none())
                {
                    return Err("Cannot migrate AST 1.0 object node without object_id".to_string());
                }
                serialized.metadata.serialization_version = AST_SCHEMA_VERSION.to_string();
                Ok(serialized)
            }
            AST_SCHEMA_VERSION => Ok(serialized),
            _ => Err(format!(
                "Unsupported AST serialization version: {}; expected {}",
                serialized.metadata.serialization_version, AST_SCHEMA_VERSION
            )),
        }
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
                decoded,
                original_bytes,
                declared_length,
                observed_length,
                parse_errors,
                recovery_actions,
            } => {
                let mut dict = crate::types::PdfDictionary::new();
                for (key, val) in dictionary {
                    dict.insert(key.as_str(), Self::deserialize_value(val)?);
                }
                let mut stream = if let Some(reference) = lazy {
                    crate::types::PdfStream::new_lazy(dict, reference.clone())
                } else if *decoded {
                    crate::types::PdfStream::from_data(
                        dict,
                        crate::types::StreamData::Decoded(data.clone()),
                    )
                } else {
                    crate::types::PdfStream::new(dict, data.clone())
                };
                stream.lossless.original_bytes = original_bytes.clone();
                stream.lossless.declared_length = *declared_length;
                stream.lossless.observed_length = observed_length.unwrap_or(stream.data.len());
                stream.lossless.parse_errors = parse_errors.clone();
                stream.lossless.recovery_actions = recovery_actions.clone();
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
    /// Serializes a document after charging its AST and retained byte payloads
    /// to the supplied resource budget.
    pub fn from_document_with_budget(
        document: &PdfDocument,
        budget: &ResourceBudget,
    ) -> Result<Self, ResourceBudgetError> {
        let _ = SerializableGraph::from_ast_with_budget(&document.ast, budget)?;
        if let Some(bytes) = &document.original_bytes {
            budget.consume_input(bytes.len() as u64)?;
        }
        check_value_budget(&PdfValue::Dictionary(document.trailer.clone()), budget)?;
        for _ in &document.xref.entries {
            budget.consume_object()?;
        }
        for stream in &document.xref.streams {
            budget.consume_object()?;
            check_value_budget(&PdfValue::Dictionary(stream.dict.clone()), budget)?;
        }
        for revision in &document.revisions {
            budget.consume_object()?;
            check_value_budget(&PdfValue::Dictionary(revision.trailer.clone()), budget)?;
        }
        Ok(Self::from_document(document))
    }

    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }

    pub fn to_json_with_budget(&self, budget: &ResourceBudget) -> Result<String, String> {
        check_serializable_document_budget(self, budget).map_err(|error| error.to_string())?;
        let output = self.to_json().map_err(|error| error.to_string())?;
        budget
            .consume_decoded(output.len() as u64)
            .map_err(|error| error.to_string())?;
        Ok(output)
    }

    pub fn to_cbor(&self) -> serde_cbor::Result<Vec<u8>> {
        serde_cbor::to_vec(self)
    }

    pub fn to_cbor_with_budget(&self, budget: &ResourceBudget) -> Result<Vec<u8>, String> {
        check_serializable_document_budget(self, budget).map_err(|error| error.to_string())?;
        let output = self.to_cbor().map_err(|error| error.to_string())?;
        budget
            .consume_decoded(output.len() as u64)
            .map_err(|error| error.to_string())?;
        Ok(output)
    }

    pub fn from_json(json: &str) -> serde_json::Result<Self> {
        serde_json::from_str(json)
    }

    pub fn from_json_with_budget(json: &str, budget: &ResourceBudget) -> Result<Self, String> {
        budget.check().map_err(|error| error.to_string())?;
        if json.len() as u64 > budget.max_input_bytes {
            return Err(ResourceBudgetError::InputBytes.to_string());
        }
        let document = Self::from_json(json).map_err(|error| error.to_string())?;
        check_serializable_document_budget(&document, budget).map_err(|error| error.to_string())?;
        Ok(document)
    }

    pub fn from_cbor(data: &[u8]) -> serde_cbor::Result<Self> {
        serde_cbor::from_slice(data)
    }

    pub fn from_cbor_with_budget(data: &[u8], budget: &ResourceBudget) -> Result<Self, String> {
        budget.check().map_err(|error| error.to_string())?;
        if data.len() as u64 > budget.max_input_bytes {
            return Err(ResourceBudgetError::InputBytes.to_string());
        }
        let document = Self::from_cbor(data).map_err(|error| error.to_string())?;
        check_serializable_document_budget(&document, budget).map_err(|error| error.to_string())?;
        Ok(document)
    }

    pub fn deserialize_ast(&self) -> Result<PdfAstGraph, String> {
        GraphDeserializer::deserialize(self.ast.clone())
    }

    /// Deserializes a document after charging retained bytes, nodes, edges,
    /// xref streams, and revisions to the supplied resource budget.
    pub fn deserialize_document_with_budget(
        &self,
        budget: &ResourceBudget,
    ) -> Result<PdfDocument, String> {
        check_serializable_document_budget(self, budget).map_err(|error| error.to_string())?;
        self.deserialize_document()
    }

    pub fn deserialize_document(&self) -> Result<PdfDocument, String> {
        let ast = self.deserialize_ast()?;
        let version = PdfVersion::from_string(&self.version)
            .ok_or_else(|| format!("Invalid PDF version: {}", self.version))?;
        let node_id = |serial_id: Option<usize>| {
            serial_id.and_then(|id| {
                self.ast
                    .nodes
                    .iter()
                    .find(|node| node.id == id)
                    .map(|node| NodeId(node.original_id.unwrap_or(node.id)))
            })
        };
        let trailer = match GraphDeserializer::deserialize_value(&self.trailer)? {
            PdfValue::Dictionary(dict) => dict,
            value => {
                return Err(format!(
                    "Document trailer must be a dictionary, got {}",
                    value.type_name()
                ))
            }
        };

        let mut xref = CrossReferenceTable {
            entries: HashMap::new(),
            streams: Vec::new(),
            prev_offset: self.xref_prev_offset,
            hybrid_mode: self.xref_hybrid_mode,
        };
        for (key, entry) in &self.xref_entries {
            xref.entries
                .insert(parse_object_id(key)?, deserialize_xref_entry(entry)?);
        }
        for stream in &self.xref_streams {
            let dict = match GraphDeserializer::deserialize_value(&stream.dict)? {
                PdfValue::Dictionary(dict) => dict,
                value => {
                    return Err(format!(
                        "XRef stream dictionary must be a dictionary, got {}",
                        value.type_name()
                    ))
                }
            };
            xref.streams.push(XRefStream {
                object_id: object_id_from_tuple(stream.object_id),
                dict,
                entries: stream.entries.clone(),
            });
        }

        let revisions = self
            .revisions
            .iter()
            .map(|revision| {
                let trailer = match GraphDeserializer::deserialize_value(&revision.trailer)? {
                    PdfValue::Dictionary(dict) => dict,
                    value => {
                        return Err(format!(
                            "Revision trailer must be a dictionary, got {}",
                            value.type_name()
                        ))
                    }
                };
                Ok(DocumentRevision {
                    revision_number: revision.revision_number,
                    xref_offset: revision.xref_offset,
                    trailer,
                    modified_objects: revision
                        .modified_objects
                        .iter()
                        .copied()
                        .map(object_id_from_tuple)
                        .collect(),
                    added_objects: revision
                        .added_objects
                        .iter()
                        .copied()
                        .map(object_id_from_tuple)
                        .collect(),
                    deleted_objects: revision
                        .deleted_objects
                        .iter()
                        .copied()
                        .map(object_id_from_tuple)
                        .collect(),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;

        let forensic = self
            .forensic
            .as_ref()
            .map(deserialize_forensic)
            .transpose()?;
        let linearization = self.linearization.as_ref().map(|value| LinearizationInfo {
            version: value.version,
            file_length: value.file_length,
            hint_stream_offset: value.hint_stream_offset,
            hint_stream_length: value.hint_stream_length,
            object_count: value.object_count,
            first_page_object_number: value.first_page_object_number,
            first_page_end_offset: value.first_page_end_offset,
            main_xref_table_entries: value.main_xref_table_entries,
        });

        let metadata = DocumentMetadata {
            file_size: self.metadata.file_size,
            linearized: self.metadata.linearized,
            encrypted: self.metadata.encrypted,
            has_forms: self.metadata.has_forms,
            has_xfa: self.metadata.has_xfa,
            xfa_packets: self.metadata.xfa_packets,
            has_xfa_scripts: self.metadata.has_xfa_scripts,
            xfa_script_nodes: self.metadata.xfa_script_nodes,
            has_hybrid_forms: self.metadata.has_hybrid_forms,
            form_field_count: self.metadata.form_field_count,
            has_javascript: self.metadata.has_javascript,
            has_embedded_files: self.metadata.has_embedded_files,
            has_signatures: self.metadata.has_signatures,
            has_richmedia: self.metadata.has_richmedia,
            richmedia_annotations: self.metadata.richmedia_annotations,
            richmedia_assets: self.metadata.richmedia_assets,
            richmedia_scripts: self.metadata.richmedia_scripts,
            has_3d: self.metadata.has_3d,
            threed_annotations: self.metadata.threed_annotations,
            threed_u3d: self.metadata.threed_u3d,
            threed_prc: self.metadata.threed_prc,
            has_audio: self.metadata.has_audio,
            audio_annotations: self.metadata.audio_annotations,
            has_video: self.metadata.has_video,
            video_annotations: self.metadata.video_annotations,
            has_dss: self.metadata.has_dss,
            dss_vri_count: self.metadata.dss_vri_count,
            dss_certs: self.metadata.dss_certs,
            dss_ocsp: self.metadata.dss_ocsp,
            dss_crl: self.metadata.dss_crl,
            dss_timestamps: self.metadata.dss_timestamps,
            page_count: self.metadata.page_count,
            compliance: self.metadata.compliance.clone(),
            producer: self.metadata.producer.clone(),
            creator: self.metadata.creator.clone(),
            creation_date: self.metadata.creation_date.clone(),
            modification_date: self.metadata.modification_date.clone(),
            title: self.metadata.title.clone(),
            author: self.metadata.author.clone(),
            subject: self.metadata.subject.clone(),
        };

        let mut document = PdfDocument::new(version);
        document.original_bytes = self.original_bytes.clone();
        document.ast = ast;
        document.catalog = node_id(self.catalog);
        document.info = node_id(self.info);
        if self.catalog.is_some() && document.catalog.is_none() {
            return Err("Document catalog references an unknown AST node".to_string());
        }
        if self.info.is_some() && document.info.is_none() {
            return Err("Document info references an unknown AST node".to_string());
        }
        document.trailer = trailer;
        document.xref = xref;
        document.metadata = metadata;
        document.linearization = linearization;
        document.revisions = revisions;
        document.diagnostics = self.diagnostics.clone();
        document.forensic = forensic;
        Ok(document)
    }

    pub fn from_document(document: &PdfDocument) -> Self {
        let ast_serializable = SerializableGraph::from_ast(&document.ast);

        // Convert XRef entries
        let mut xref_entries = HashMap::new();
        for (obj_id, entry) in &document.xref.entries {
            let key = format!("{}_{}", obj_id.number, obj_id.generation);
            let serializable_entry = serialize_xref_entry(*entry);
            xref_entries.insert(key, serializable_entry);
        }

        // Convert original node IDs to serial IDs.
        let serial_id_for = |node_id: Option<NodeId>| {
            node_id.and_then(|node_id| {
                ast_serializable
                    .nodes
                    .iter()
                    .find(|node| node.original_id.unwrap_or(node.id) == node_id.0)
                    .map(|node| node.id)
            })
        };
        let catalog_serial_id = serial_id_for(document.catalog);
        let info_serial_id = serial_id_for(document.info);

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
            xref_prev_offset: document.xref.prev_offset,
            xref_hybrid_mode: document.xref.hybrid_mode,
            xref_streams: document
                .xref
                .streams
                .iter()
                .map(|stream| SerializableXRefStream {
                    object_id: serial_object_id(stream.object_id),
                    dict: GraphSerializer::serialize_value(&PdfValue::Dictionary(
                        stream.dict.clone(),
                    )),
                    entries: stream.entries.clone(),
                })
                .collect(),
            original_bytes: document.original_bytes.clone(),
            revisions: document
                .revisions
                .iter()
                .map(|revision| SerializableRevision {
                    revision_number: revision.revision_number,
                    xref_offset: revision.xref_offset,
                    trailer: GraphSerializer::serialize_value(&PdfValue::Dictionary(
                        revision.trailer.clone(),
                    )),
                    modified_objects: revision
                        .modified_objects
                        .iter()
                        .map(|id| (id.number, id.generation))
                        .collect(),
                    added_objects: revision
                        .added_objects
                        .iter()
                        .map(|id| (id.number, id.generation))
                        .collect(),
                    deleted_objects: revision
                        .deleted_objects
                        .iter()
                        .map(|id| (id.number, id.generation))
                        .collect(),
                })
                .collect(),
            diagnostics: document.diagnostics.clone(),
            forensic: document
                .forensic
                .as_ref()
                .map(SerializableForensicSnapshot::from),
            linearization: document.linearization.as_ref().map(|value| {
                SerializableLinearizationInfo {
                    version: value.version,
                    file_length: value.file_length,
                    hint_stream_offset: value.hint_stream_offset,
                    hint_stream_length: value.hint_stream_length,
                    object_count: value.object_count,
                    first_page_object_number: value.first_page_object_number,
                    first_page_end_offset: value.first_page_end_offset,
                    main_xref_table_entries: value.main_xref_table_entries,
                }
            }),
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
                title: document.metadata.title.clone(),
                author: document.metadata.author.clone(),
                subject: document.metadata.subject.clone(),
                compliance: document.metadata.compliance.clone(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{
        DocumentRevision, ForensicSnapshot, NodeType, PdfAstGraph, PdfDocument, PdfVersion,
        XRefEntry,
    };
    use crate::types::{ObjectId, PdfDictionary, PdfValue};

    #[test]
    fn budgeted_serialization_checks_structure_input_and_output() {
        let mut ast = PdfAstGraph::new();
        let root_id = ast.create_node(NodeType::Root, PdfValue::Null);
        ast.set_root(root_id);
        let serialized = SerializableGraph::from_ast(&ast);

        let node_budget = ResourceBudget::new(1024, 1024, 1024, 100, 10, 0, 10, 10);
        assert!(serialized
            .to_json_with_budget(&node_budget)
            .expect_err("serialization must consume node budget")
            .contains("Nodes"));

        let output_budget = ResourceBudget::new(1024, 0, 1024, 100, 10, 10, 10, 10);
        assert!(serialized
            .to_cbor_with_budget(&output_budget)
            .expect_err("serialization output must consume decoded budget")
            .contains("DecodedBytes"));

        let json = serialized.to_json().unwrap();
        let input_budget = ResourceBudget::new(0, 1024, 1024, 100, 10, 10, 10, 10);
        assert!(
            SerializableGraph::from_json_with_budget(&json, &input_budget)
                .expect_err("serialized input must respect the budget")
                .contains("InputBytes")
        );
    }

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
    fn preserves_semantic_object_identity_across_round_trip() {
        let mut ast = PdfAstGraph::new();
        let node_id = ast.create_node(NodeType::Page, PdfValue::Null);
        let object_id = ObjectId::new(7, 0);
        assert!(ast.register_object_node(object_id, node_id));

        let serialized = SerializableGraph::from_ast(&ast);
        let node = serialized
            .nodes
            .iter()
            .find(|node| node.original_id == Some(node_id.index()))
            .expect("semantic node should be serialized");
        assert_eq!(node.object_id, Some((7, 0)));

        let restored = GraphDeserializer::deserialize(serialized).expect("graph should restore");
        assert_eq!(
            restored.get_node_by_object(object_id).map(|node| node.id),
            Some(node_id)
        );
    }

    #[test]
    fn rejects_object_nodes_without_identity() {
        let graph = SerializableGraph {
            nodes: vec![SerializableNode {
                original_id: None,
                id: 0,
                node_type: "Object".to_string(),
                value: SerializableValue::Null,
                object_id: None,
                offset: None,
                size: None,
                errors: Vec::new(),
                warnings: Vec::new(),
                properties: HashMap::new(),
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

        for version in ["1", "1.2.0", "2.0.0"] {
            graph.metadata.serialization_version = version.to_string();
            let error = GraphDeserializer::deserialize(graph.clone()).unwrap_err();
            assert!(error.contains("Unsupported AST serialization version"));
        }
    }

    #[test]
    fn migrates_ast_1_0_graphs_without_losing_object_identity() {
        let mut ast = PdfAstGraph::new();
        let object_id = crate::types::ObjectId::new(42, 7);
        let node_id = ast.create_node(NodeType::Object(object_id), PdfValue::Null);
        ast.set_root(node_id);
        let mut graph = SerializableGraph::from_ast(&ast);
        graph.metadata.serialization_version = "1.0".to_string();

        let restored = GraphDeserializer::deserialize(graph).unwrap();
        assert!(restored.get_node_by_object(object_id).is_some());
    }

    #[test]
    fn preserves_non_contiguous_node_ids_across_round_trip() {
        let mut ast = PdfAstGraph::new();
        let root_id = NodeId::new(41);
        let child_id = NodeId::new(9001);
        ast.add_node(AstNode::new(
            root_id,
            NodeType::Root,
            PdfValue::Dictionary(PdfDictionary::new()),
        ));
        ast.add_node(AstNode::new(child_id, NodeType::Metadata, PdfValue::Null));
        ast.set_root(root_id);
        ast.add_edge(root_id, child_id, EdgeType::Child);

        let restored = GraphDeserializer::deserialize(SerializableGraph::from_ast(&ast)).unwrap();

        assert!(restored.get_node(root_id).is_some());
        assert!(restored.get_node(child_id).is_some());
        assert_eq!(restored.get_children(root_id), vec![child_id]);
        assert_eq!(restored.get_root(), Some(root_id));
    }

    #[test]
    fn preserves_node_source_metadata_across_round_trip() {
        let mut ast = PdfAstGraph::new();
        let root_id = ast.create_node(NodeType::Root, PdfValue::Null);
        ast.set_root(root_id);
        ast.get_node_mut(root_id).unwrap().metadata.offset = Some(123);
        ast.get_node_mut(root_id).unwrap().metadata.size = Some(45);
        ast.get_node_mut(root_id)
            .unwrap()
            .metadata
            .errors
            .push(crate::ast::ParseError {
                code: crate::ast::ErrorCode::MalformedStructure,
                message: "recovered dictionary".to_string(),
                offset: Some(124),
                recoverable: true,
            });
        ast.get_node_mut(root_id)
            .unwrap()
            .metadata
            .warnings
            .push("missing end marker".to_string());
        ast.get_node_mut(root_id)
            .unwrap()
            .metadata
            .properties
            .insert("decode_state".to_string(), "recovered".to_string());

        let serialized = SerializableGraph::from_ast(&ast);
        assert_eq!(serialized.nodes[0].offset, Some(123));
        assert_eq!(serialized.nodes[0].size, Some(45));
        assert_eq!(serialized.nodes[0].errors.len(), 1);
        assert_eq!(serialized.nodes[0].warnings, vec!["missing end marker"]);
        assert_eq!(
            serialized.nodes[0].properties.get("decode_state"),
            Some(&"recovered".to_string())
        );

        let restored = GraphDeserializer::deserialize(serialized).unwrap();
        let restored_node = restored.get_node(restored.root.unwrap()).unwrap();
        assert_eq!(restored_node.metadata.offset, Some(123));
        assert_eq!(restored_node.metadata.size, Some(45));
        assert_eq!(restored_node.metadata.errors.len(), 1);
        assert_eq!(
            restored_node.metadata.errors[0].code,
            crate::ast::ErrorCode::MalformedStructure
        );
        assert_eq!(restored_node.metadata.warnings, vec!["missing end marker"]);
        assert_eq!(
            restored_node.metadata.properties.get("decode_state"),
            Some(&"recovered".to_string())
        );
    }

    #[test]
    fn preserves_stream_decode_state_across_round_trip() {
        let mut ast = PdfAstGraph::new();
        let mut dictionary = PdfDictionary::new();
        dictionary.insert("Length", PdfValue::Integer(3));
        let stream_id = ast.create_node(
            NodeType::ContentStream,
            PdfValue::Stream({
                let mut stream = crate::types::PdfStream::from_data(
                    dictionary,
                    crate::types::StreamData::Decoded(b"abc".to_vec()),
                );
                stream.lossless.original_bytes = Some(b"raw".to_vec());
                stream.lossless.observed_length = 3;
                stream.lossless.parse_errors = vec!["decode failed".to_string()];
                stream.lossless.recovery_actions = vec!["stream_decode_skipped".to_string()];
                stream
            }),
        );
        ast.set_root(stream_id);

        let serialized = SerializableGraph::from_ast(&ast);
        let SerializableValue::Stream { decoded, .. } = &serialized.nodes[0].value else {
            panic!("expected serialized stream");
        };
        assert!(*decoded);

        let restored = GraphDeserializer::deserialize(serialized).unwrap();
        let stream = restored
            .get_node(restored.root.unwrap())
            .and_then(|node| node.value.as_stream())
            .expect("restored stream");
        assert!(matches!(
            stream.data,
            crate::types::StreamData::Decoded(ref data) if data == b"abc"
        ));
        assert_eq!(stream.original_data(), Some(b"raw".as_slice()));
        assert_eq!(stream.lossless.observed_length, 3);
        assert_eq!(stream.lossless.parse_errors, vec!["decode failed"]);
        assert_eq!(
            stream.lossless.recovery_actions,
            vec!["stream_decode_skipped"]
        );
    }

    #[test]
    fn rejects_ast_1_0_object_nodes_without_identity_during_migration() {
        let mut ast = PdfAstGraph::new();
        let root_id = ast.create_node(NodeType::Root, PdfValue::Null);
        ast.set_root(root_id);
        let mut graph = SerializableGraph::from_ast(&ast);
        graph.nodes[0].node_type = "Object".to_string();
        graph.nodes[0].object_id = None;
        graph.metadata.serialization_version = "1.0".to_string();

        let error = GraphDeserializer::deserialize(graph).unwrap_err();
        assert!(error.contains("Cannot migrate AST 1.0 object node"));
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
        let mut document = PdfDocument::new(version);
        document.original_bytes = Some(b"%PDF-1.7\n".to_vec());
        document.diagnostics.push(crate::ast::ParseDiagnostic {
            object_id: Some(ObjectId::new(7, 0)),
            offset: Some(123),
            error_code: "xref_missing_object".to_string(),
            recovery_action: "xref recovery".to_string(),
            confidence: 0.5,
            bytes_consumed: 42,
            message: "recovered missing object".to_string(),
        });
        document.forensic = Some(ForensicSnapshot {
            declared_xref: HashMap::from([(
                ObjectId::new(1, 0),
                XRefEntry::InUse {
                    offset: 12,
                    generation: 0,
                },
            )]),
            recovered_xref: HashMap::from([(
                ObjectId::new(2, 0),
                XRefEntry::Free {
                    next_free_object: 0,
                    generation: 65535,
                },
            )]),
            duplicate_objects: vec![ObjectId::new(3, 0)],
            overwritten_objects: vec![ObjectId::new(4, 0)],
            residual_ranges: vec![(100, 120)],
        });
        document.revisions.push(DocumentRevision {
            revision_number: 1,
            xref_offset: 123,
            trailer: PdfDictionary::new(),
            modified_objects: vec![ObjectId::new(1, 0)],
            added_objects: vec![ObjectId::new(2, 0)],
            deleted_objects: Vec::new(),
        });

        let json = to_json(&document).unwrap();
        assert!(json.contains("1.7"));
        assert!(json.contains("ast"));
        assert!(json.contains("metadata"));
        assert!(json.contains("schema_version"));
        assert!(json.contains("original_bytes"));

        let deserialized = SerializableDocument::from_json(&json).unwrap();
        assert_eq!(deserialized.original_bytes, Some(b"%PDF-1.7\n".to_vec()));
        assert_eq!(deserialized.revisions.len(), 1);
        assert_eq!(deserialized.revisions[0].xref_offset, 123);
        assert_eq!(deserialized.revisions[0].added_objects, vec![(2, 0)]);
        assert_eq!(deserialized.diagnostics.len(), 1);
        assert_eq!(deserialized.diagnostics[0].recovery_action, "xref recovery");
        assert_eq!(deserialized.diagnostics[0].bytes_consumed, 42);
        let forensic = deserialized.forensic.as_ref().unwrap();
        assert_eq!(forensic.declared_xref.len(), 1);
        assert_eq!(forensic.recovered_xref.len(), 1);
        assert_eq!(forensic.duplicate_objects, vec![(3, 0)]);
        assert_eq!(forensic.residual_ranges, vec![(100, 120)]);
        deserialized.deserialize_ast().unwrap();

        let cbor = SerializableDocument::from_document(&document)
            .to_cbor()
            .unwrap();
        assert_eq!(
            SerializableDocument::from_cbor(&cbor)
                .unwrap()
                .revisions
                .len(),
            1
        );
    }

    #[test]
    fn preserves_full_document_state_across_round_trip() {
        let mut document = PdfDocument::new(PdfVersion::new(2, 0));
        let catalog_id = NodeId::new(41);
        let info_id = NodeId::new(9001);
        document.ast.add_node(AstNode::new(
            catalog_id,
            NodeType::Catalog,
            PdfValue::Dictionary(PdfDictionary::new()),
        ));
        document.ast.add_node(AstNode::new(
            info_id,
            NodeType::Metadata,
            PdfValue::Dictionary(PdfDictionary::new()),
        ));
        document.set_catalog(catalog_id);
        document.set_info(info_id);
        document.trailer.insert("Size", PdfValue::Integer(12));
        document.add_xref_entry(
            ObjectId::new(1, 0),
            XRefEntry::InUse {
                offset: 100,
                generation: 0,
            },
        );
        document.add_xref_entry(
            ObjectId::new(2, 1),
            XRefEntry::Free {
                next_free_object: 0,
                generation: 1,
            },
        );
        document.add_xref_entry(
            ObjectId::new(3, 0),
            XRefEntry::Compressed {
                stream_object: 7,
                index: 2,
            },
        );
        document.xref.prev_offset = Some(88);
        document.xref.hybrid_mode = true;
        let mut xref_stream_dict = PdfDictionary::new();
        xref_stream_dict.insert("Type", PdfValue::Name(crate::types::PdfName::new("XRef")));
        document.add_xref_stream(crate::ast::XRefStream {
            object_id: ObjectId::new(7, 0),
            dict: xref_stream_dict,
            entries: vec![XRefEntry::Compressed {
                stream_object: 7,
                index: 2,
            }],
        });
        document.linearization = Some(LinearizationInfo {
            version: 1.0,
            file_length: 1000,
            hint_stream_offset: 200,
            hint_stream_length: Some(20),
            object_count: 12,
            first_page_object_number: 4,
            first_page_end_offset: 500,
            main_xref_table_entries: 8,
        });
        document.metadata.title = Some("Round trip".to_string());
        document.metadata.author = Some("pdf-core".to_string());
        document.metadata.compliance = vec![crate::ast::ComplianceProfile::PdfA1b];

        let restored = SerializableDocument::from_document(&document)
            .deserialize_document()
            .unwrap();

        assert_eq!(restored.version, PdfVersion::new(2, 0));
        assert_eq!(restored.catalog, Some(catalog_id));
        assert_eq!(restored.info, Some(info_id));
        assert_eq!(restored.trailer.get("Size"), Some(&PdfValue::Integer(12)));
        assert_eq!(restored.xref.entries, document.xref.entries);
        assert_eq!(restored.xref.prev_offset, Some(88));
        assert!(restored.xref.hybrid_mode);
        assert_eq!(restored.xref.streams.len(), 1);
        assert_eq!(
            restored.xref.streams[0].entries,
            document.xref.streams[0].entries
        );
        assert_eq!(restored.linearization.as_ref().unwrap().file_length, 1000);
        assert_eq!(restored.metadata.title.as_deref(), Some("Round trip"));
        assert_eq!(restored.metadata.author.as_deref(), Some("pdf-core"));
        assert_eq!(restored.metadata.compliance, document.metadata.compliance);
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

    #[test]
    fn budgeted_graph_serialization_rejects_node_limits() {
        let mut ast = PdfAstGraph::new();
        let root_id = ast.create_node(NodeType::Root, PdfValue::Null);
        ast.set_root(root_id);
        let budget = ResourceBudget::new(1024, 1024, 1024, 100, 10, 0, 10, 10);

        let error = SerializableGraph::from_ast_with_budget(&ast, &budget)
            .expect_err("node budget must apply to serialization");
        assert_eq!(error, ResourceBudgetError::Nodes);
    }

    #[test]
    fn budgeted_graph_serialization_charges_stream_bytes() {
        let mut ast = PdfAstGraph::new();
        let stream_id = ast.create_node(
            NodeType::ContentStream,
            PdfValue::Stream(crate::types::PdfStream::new(
                PdfDictionary::new(),
                b"abc".to_vec(),
            )),
        );
        ast.set_root(stream_id);
        let budget = ResourceBudget::new(5, 1024, 1024, 100, 10, 10, 10, 10);

        let error = SerializableGraph::from_ast_with_budget(&ast, &budget)
            .expect_err("stream payload must apply to serialization budget");
        assert_eq!(error, ResourceBudgetError::InputBytes);
    }

    #[test]
    fn budgeted_graph_deserialization_charges_stream_bytes() {
        let mut ast = PdfAstGraph::new();
        let stream_id = ast.create_node(
            NodeType::ContentStream,
            PdfValue::Stream(crate::types::PdfStream::new(
                PdfDictionary::new(),
                b"abc".to_vec(),
            )),
        );
        ast.set_root(stream_id);
        let serialized = SerializableGraph::from_ast(&ast);
        let budget = ResourceBudget::new(2, 1024, 1024, 100, 10, 10, 10, 10);

        let error = serialized
            .deserialize_with_budget(&budget)
            .expect_err("stream payload must apply to deserialization budget");
        assert!(error.contains("InputBytes"));
    }

    #[test]
    fn budgeted_document_round_trip_charges_xref_entries() {
        let mut document = PdfDocument::new(PdfVersion::new(1, 4));
        document.xref.entries.insert(
            ObjectId::new(1, 0),
            XRefEntry::InUse {
                offset: 10,
                generation: 0,
            },
        );
        let serialized = SerializableDocument::from_document(&document);
        let budget = ResourceBudget::new(1024, 1024, 1024, 100, 0, 10, 10, 10);

        let error = serialized
            .deserialize_document_with_budget(&budget)
            .expect_err("xref entries must apply to document deserialization budget");
        assert!(error.contains("Objects"));
    }
}
