use crate::ast::{NodeId, NodeType, PdfAstGraph};
use crate::types::{
    ObjectId, PdfArray, PdfDictionary, PdfName, PdfReference, PdfStream, PdfString, PdfValue,
    StreamData,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

type MigrationFunction = Box<dyn Fn(&mut StableAstSchema) -> Result<(), String>>;

pub const SCHEMA_VERSION: &str = "1.1.0";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl SchemaVersion {
    pub fn current() -> Self {
        Self {
            major: 1,
            minor: 1,
            patch: 0,
        }
    }

    pub fn from_string(version: &str) -> Result<Self, String> {
        let parts: Vec<&str> = version.split('.').collect();
        if parts.len() != 3 {
            return Err("Invalid version format".to_string());
        }

        Ok(Self {
            major: parts[0].parse().map_err(|_| "Invalid major version")?,
            minor: parts[1].parse().map_err(|_| "Invalid minor version")?,
            patch: parts[2].parse().map_err(|_| "Invalid patch version")?,
        })
    }

    pub fn is_compatible_with(&self, other: &SchemaVersion) -> bool {
        self.major == other.major
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StableNode {
    pub id: String,
    pub node_type: String,
    pub value_type: String,
    pub value: Value,
    pub metadata: HashMap<String, Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StableEdge {
    pub source: String,
    pub target: String,
    pub edge_type: String,
    pub metadata: HashMap<String, Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StableAstSchema {
    pub version: SchemaVersion,
    pub document_metadata: HashMap<String, Value>,
    pub nodes: Vec<StableNode>,
    pub edges: Vec<StableEdge>,
    pub deterministic_ids: bool,
}

impl StableAstSchema {
    pub fn to_graph(&self) -> Result<PdfAstGraph, String> {
        let mut graph = PdfAstGraph::new();
        graph.set_deterministic_ids(false);

        let mut id_map: HashMap<String, NodeId> = HashMap::new();

        for node in &self.nodes {
            let node_type = parse_node_type(node)?;
            let value = stable_value_to_pdf(&node.value_type, &node.value)?;
            let node_id = graph.create_node(node_type, value);
            id_map.insert(node.id.clone(), node_id);
        }

        for edge in &self.edges {
            let from = id_map
                .get(&edge.source)
                .ok_or_else(|| format!("Unknown edge source: {}", edge.source))?;
            let to = id_map
                .get(&edge.target)
                .ok_or_else(|| format!("Unknown edge target: {}", edge.target))?;
            let edge_type = parse_edge_type(&edge.edge_type)?;
            graph.add_edge(*from, *to, edge_type);
        }

        if let Some(root_node) = self
            .nodes
            .iter()
            .find(|node| node.node_type == "Root")
            .and_then(|node| id_map.get(&node.id).copied())
        {
            graph.set_root(root_node);
        } else if let Some(catalog_node) = self
            .nodes
            .iter()
            .find(|node| node.node_type == "Catalog")
            .and_then(|node| id_map.get(&node.id).copied())
        {
            graph.set_root(catalog_node);
        }

        Ok(graph)
    }
}

fn parse_node_type(node: &StableNode) -> Result<NodeType, String> {
    let type_name = node
        .node_type
        .split('(')
        .next()
        .unwrap_or(node.node_type.as_str())
        .trim();
    match type_name {
        "Root" => Ok(NodeType::Root),
        "Catalog" => Ok(NodeType::Catalog),
        "Pages" => Ok(NodeType::Pages),
        "Page" => Ok(NodeType::Page),
        "Resource" => Ok(NodeType::Resource),
        "Font" => Ok(NodeType::Font),
        "Image" => Ok(NodeType::Image),
        "ContentStream" => Ok(NodeType::ContentStream),
        "Annotation" => Ok(NodeType::Annotation),
        "Action" => Ok(NodeType::Action),
        "Metadata" => Ok(NodeType::Metadata),
        "EmbeddedFile" => Ok(NodeType::EmbeddedFile),
        "Signature" => Ok(NodeType::Signature),
        "Stream" => Ok(NodeType::Stream),
        "FilteredStream" => Ok(NodeType::FilteredStream),
        "DecodedStream" => Ok(NodeType::DecodedStream),
        "XObject" => Ok(NodeType::XObject),
        "FormXObject" => Ok(NodeType::FormXObject),
        "ImageXObject" => Ok(NodeType::ImageXObject),
        "Type1Font" => Ok(NodeType::Type1Font),
        "TrueTypeFont" => Ok(NodeType::TrueTypeFont),
        "Type3Font" => Ok(NodeType::Type3Font),
        "CIDFont" => Ok(NodeType::CIDFont),
        "JavaScriptAction" => Ok(NodeType::JavaScriptAction),
        "GoToAction" => Ok(NodeType::GoToAction),
        "URIAction" => Ok(NodeType::URIAction),
        "LaunchAction" => Ok(NodeType::LaunchAction),
        "SubmitFormAction" => Ok(NodeType::SubmitFormAction),
        "AcroForm" => Ok(NodeType::AcroForm),
        "Field" => Ok(NodeType::Field),
        "Encrypt" => Ok(NodeType::Encrypt),
        "Permission" => Ok(NodeType::Permission),
        "ContentOperator" => Ok(NodeType::ContentOperator),
        "TextOperator" => Ok(NodeType::TextOperator),
        "GraphicsOperator" => Ok(NodeType::GraphicsOperator),
        "EmbeddedJS" => Ok(NodeType::EmbeddedJS),
        "SuspiciousAction" => Ok(NodeType::SuspiciousAction),
        "ExternalReference" => Ok(NodeType::ExternalReference),
        "EncodedContent" => Ok(NodeType::EncodedContent),
        "Outline" => Ok(NodeType::Outline),
        "OutlineItem" => Ok(NodeType::OutlineItem),
        "NameTree" => Ok(NodeType::NameTree),
        "StructTreeRoot" => Ok(NodeType::StructTreeRoot),
        "StructElem" => Ok(NodeType::StructElem),
        "ColorSpace" => Ok(NodeType::ColorSpace),
        "ICCBased" => Ok(NodeType::ICCBased),
        "Separation" => Ok(NodeType::Separation),
        "DeviceN" => Ok(NodeType::DeviceN),
        "Indexed" => Ok(NodeType::Indexed),
        "Pattern" => Ok(NodeType::Pattern),
        "Shading" => Ok(NodeType::Shading),
        "ExtGState" => Ok(NodeType::ExtGState),
        "Function" => Ok(NodeType::Function),
        "CMap" => Ok(NodeType::CMap),
        "ToUnicode" => Ok(NodeType::ToUnicode),
        "Encoding" => Ok(NodeType::Encoding),
        "OCG" => Ok(NodeType::OCG),
        "OCProperties" => Ok(NodeType::OCProperties),
        "OCMD" => Ok(NodeType::OCMD),
        "RichMedia" => Ok(NodeType::RichMedia),
        "Rendition" => Ok(NodeType::Rendition),
        "Screen" => Ok(NodeType::Screen),
        "Sound" => Ok(NodeType::Sound),
        "Movie" => Ok(NodeType::Movie),
        "ThreeD" => Ok(NodeType::ThreeD),
        "U3D" => Ok(NodeType::U3D),
        "PRC" => Ok(NodeType::PRC),
        "Object" => {
            let object_id = extract_object_id(node)
                .ok_or_else(|| "Object node missing object_id".to_string())?;
            Ok(NodeType::Object(object_id))
        }
        "Unknown" => Ok(NodeType::Unknown),
        other => Err(format!("Unknown node type: {}", other)),
    }
}

fn extract_object_id(node: &StableNode) -> Option<ObjectId> {
    if let Some(object_id) = &node.object_id {
        if let Some((num, gen)) = parse_object_id(object_id) {
            return Some(ObjectId::new(num, gen));
        }
    }
    if let (Some(num), Some(gen)) = (
        node.metadata.get("object_number").and_then(|v| v.as_u64()),
        node.metadata.get("generation").and_then(|v| v.as_u64()),
    ) {
        return Some(ObjectId::new(num as u32, gen as u16));
    }
    parse_object_id(&node.node_type).map(|(num, gen)| ObjectId::new(num, gen))
}

fn parse_object_id(text: &str) -> Option<(u32, u16)> {
    let mut numbers = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        if ch.is_ascii_digit() {
            current.push(ch);
        } else if !current.is_empty() {
            if let Ok(num) = current.parse::<u32>() {
                numbers.push(num);
            }
            current.clear();
        }
    }
    if !current.is_empty() {
        if let Ok(num) = current.parse::<u32>() {
            numbers.push(num);
        }
    }
    let number = numbers.first().copied()?;
    let generation = numbers.get(1).copied().unwrap_or(0) as u16;
    Some((number, generation))
}

fn parse_edge_type(edge_type: &str) -> Result<crate::ast::graph::EdgeType, String> {
    match edge_type {
        "Child" => Ok(crate::ast::graph::EdgeType::Child),
        "Reference" => Ok(crate::ast::graph::EdgeType::Reference),
        "Parent" => Ok(crate::ast::graph::EdgeType::Parent),
        "Resource" => Ok(crate::ast::graph::EdgeType::Resource),
        "Annotation" => Ok(crate::ast::graph::EdgeType::Annotation),
        "Content" => Ok(crate::ast::graph::EdgeType::Content),
        other => Err(format!("Unknown edge type: {}", other)),
    }
}

fn stable_value_to_pdf(value_type: &str, value: &Value) -> Result<PdfValue, String> {
    match value_type {
        "null" => Ok(PdfValue::Null),
        "boolean" => value
            .as_bool()
            .map(PdfValue::Boolean)
            .ok_or_else(|| "Expected boolean".to_string()),
        "integer" => value
            .as_i64()
            .map(PdfValue::Integer)
            .ok_or_else(|| "Expected integer".to_string()),
        "real" => value
            .as_f64()
            .map(PdfValue::Real)
            .ok_or_else(|| "Expected real".to_string()),
        "string" => value
            .as_str()
            .map(|s| PdfValue::String(PdfString::new_literal(s.as_bytes())))
            .ok_or_else(|| "Expected string".to_string()),
        "name" => value
            .as_str()
            .map(|s| PdfValue::Name(PdfName::new(s)))
            .ok_or_else(|| "Expected name".to_string()),
        "array" => match value.as_array() {
            Some(items) => {
                let mut array = PdfArray::new();
                for item in items {
                    array.push(json_to_pdf_value(item)?);
                }
                Ok(PdfValue::Array(array))
            }
            None => Err("Expected array".to_string()),
        },
        "dictionary" => match value.as_object() {
            Some(map) => {
                let mut dict = PdfDictionary::new();
                for (key, val) in map {
                    dict.insert(key.as_str(), json_to_pdf_value(val)?);
                }
                Ok(PdfValue::Dictionary(dict))
            }
            None => Err("Expected dictionary".to_string()),
        },
        "stream" => parse_stream_value(value),
        "reference" => parse_reference_value(value),
        other => Err(format!("Unknown value type: {}", other)),
    }
}

fn json_to_pdf_value(value: &Value) -> Result<PdfValue, String> {
    if let Some(obj) = value.as_object() {
        if let Some(type_value) = obj.get("type").and_then(|v| v.as_str()) {
            match type_value {
                "stream" => return parse_stream_value(value),
                "reference" => return parse_reference_value(value),
                _ => {}
            }
        }
        let mut dict = PdfDictionary::new();
        for (key, val) in obj {
            dict.insert(key.as_str(), json_to_pdf_value(val)?);
        }
        return Ok(PdfValue::Dictionary(dict));
    }

    match value {
        Value::Null => Ok(PdfValue::Null),
        Value::Bool(b) => Ok(PdfValue::Boolean(*b)),
        Value::Number(num) => {
            if let Some(i) = num.as_i64() {
                Ok(PdfValue::Integer(i))
            } else if let Some(f) = num.as_f64() {
                Ok(PdfValue::Real(f))
            } else {
                Err("Unsupported number type".to_string())
            }
        }
        Value::String(s) => Ok(PdfValue::String(PdfString::new_literal(s.as_bytes()))),
        Value::Array(items) => {
            let mut array = PdfArray::new();
            for item in items {
                array.push(json_to_pdf_value(item)?);
            }
            Ok(PdfValue::Array(array))
        }
        Value::Object(_) => Err("Unexpected object value".to_string()),
    }
}

fn parse_stream_value(value: &Value) -> Result<PdfValue, String> {
    let obj = value
        .as_object()
        .ok_or_else(|| "Stream value must be object".to_string())?;
    let dict_value = obj
        .get("dict")
        .ok_or_else(|| "Stream value missing dict".to_string())?;
    let mut dict = match dict_value.as_object() {
        Some(map) => {
            let mut d = PdfDictionary::new();
            for (key, val) in map {
                d.insert(key.as_str(), json_to_pdf_value(val)?);
            }
            d
        }
        None => return Err("Stream dict must be object".to_string()),
    };

    if let Some(length) = obj.get("length").and_then(|v| v.as_i64()) {
        if !dict.contains_key("Length") {
            dict.insert("Length", PdfValue::Integer(length));
        }
    }

    let stream = PdfStream {
        dict,
        data: StreamData::Raw(Vec::new()),
    };
    Ok(PdfValue::Stream(stream))
}

fn parse_reference_value(value: &Value) -> Result<PdfValue, String> {
    let obj = value
        .as_object()
        .ok_or_else(|| "Reference value must be object".to_string())?;
    let object_str = obj
        .get("object")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Reference missing object string".to_string())?;
    let (number, generation) =
        parse_object_id(object_str).ok_or_else(|| "Invalid reference object id".to_string())?;
    Ok(PdfValue::Reference(PdfReference::new(number, generation)))
}

pub struct SchemaExporter {
    deterministic_ids: bool,
    id_map: HashMap<NodeId, String>,
}

impl SchemaExporter {
    pub fn new(deterministic_ids: bool) -> Self {
        Self {
            deterministic_ids,
            id_map: HashMap::new(),
        }
    }

    pub fn export(&mut self, graph: &PdfAstGraph) -> StableAstSchema {
        let nodes = self.export_nodes(graph);
        let edges = self.export_edges(graph);

        StableAstSchema {
            version: SchemaVersion::current(),
            document_metadata: self.extract_document_metadata(graph),
            nodes,
            edges,
            deterministic_ids: self.deterministic_ids,
        }
    }

    fn generate_deterministic_id(
        &self,
        node_id: NodeId,
        node_type: &NodeType,
        graph: &PdfAstGraph,
    ) -> String {
        if self.deterministic_ids {
            match node_type {
                NodeType::Root => "root".to_string(),
                NodeType::Catalog => "catalog".to_string(),
                NodeType::Pages => format!("pages_{}", self.hash_path(node_id, graph)),
                NodeType::Page => {
                    let page_num = self.get_page_number(node_id, graph);
                    format!("page_{}", page_num)
                }
                NodeType::Font => {
                    let font_name = self.get_font_name(node_id, graph);
                    format!("font_{}", font_name)
                }
                NodeType::Image => format!("image_{}", self.hash_path(node_id, graph)),
                NodeType::Annotation => format!("annot_{}", self.hash_path(node_id, graph)),
                NodeType::Form => format!("form_{}", self.hash_path(node_id, graph)),
                NodeType::Outline => format!("outline_{}", self.hash_path(node_id, graph)),
                NodeType::StructElem => format!("struct_{}", self.hash_path(node_id, graph)),
                _ => format!("node_{}", self.hash_path(node_id, graph)),
            }
        } else {
            format!("node_{}", node_id.index())
        }
    }

    fn hash_path(&self, node_id: NodeId, graph: &PdfAstGraph) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        let path = graph.get_path_to_root(node_id);
        for id in path {
            id.index().hash(&mut hasher);
        }
        format!("{:x}", hasher.finish())
    }

    fn get_page_number(&self, node_id: NodeId, graph: &PdfAstGraph) -> usize {
        graph.get_page_number(node_id).unwrap_or(0)
    }

    fn get_font_name(&self, node_id: NodeId, graph: &PdfAstGraph) -> String {
        if let Some(node) = graph.get_node(node_id) {
            if let PdfValue::Dictionary(dict) = &node.value {
                if let Some(PdfValue::Name(name)) = dict.get("BaseFont") {
                    return name.without_slash().to_string();
                }
            }
        }
        "unknown".to_string()
    }

    fn export_nodes(&mut self, graph: &PdfAstGraph) -> Vec<StableNode> {
        let mut nodes = Vec::new();

        for node_id in graph.node_indices() {
            if let Some(node) = graph.get_node(node_id) {
                let stable_id = self.generate_deterministic_id(node_id, &node.node_type, graph);
                self.id_map.insert(node_id, stable_id.clone());

                let value_json = self.pdf_value_to_json(&node.value);

                let mut metadata = HashMap::new();
                if let Some(obj_id) = graph.get_object_id(node_id) {
                    metadata.insert("object_number".to_string(), json!(obj_id.number));
                    metadata.insert("generation".to_string(), json!(obj_id.generation));
                }

                nodes.push(StableNode {
                    id: stable_id,
                    node_type: format!("{:?}", node.node_type),
                    value_type: self.get_value_type(&node.value),
                    value: value_json,
                    metadata,
                    object_id: graph
                        .get_object_id(node_id)
                        .map(|id| format!("{} {} obj", id.number, id.generation)),
                });
            }
        }

        nodes
    }

    fn export_edges(&self, graph: &PdfAstGraph) -> Vec<StableEdge> {
        let mut edges = Vec::new();

        for edge in graph.raw_edges() {
            let source_id = self
                .id_map
                .get(&edge.from)
                .cloned()
                .unwrap_or_else(|| format!("unknown_{}", edge.from.index()));
            let target_id = self
                .id_map
                .get(&edge.to)
                .cloned()
                .unwrap_or_else(|| format!("unknown_{}", edge.to.index()));

            edges.push(StableEdge {
                source: source_id,
                target: target_id,
                edge_type: format!("{:?}", edge.edge_type),
                metadata: HashMap::new(),
            });
        }

        edges
    }

    #[allow(clippy::only_used_in_recursion)]
    fn pdf_value_to_json(&self, value: &PdfValue) -> Value {
        match value {
            PdfValue::Null => json!(null),
            PdfValue::Boolean(b) => json!(b),
            PdfValue::Integer(i) => json!(i),
            PdfValue::Real(r) => json!(r),
            PdfValue::String(s) => json!(s.to_string_lossy()),
            PdfValue::Name(n) => json!(n.as_str()),
            PdfValue::Array(arr) => {
                json!(arr
                    .iter()
                    .map(|v| self.pdf_value_to_json(v))
                    .collect::<Vec<_>>())
            }
            PdfValue::Dictionary(dict) => {
                let mut map = serde_json::Map::new();
                for (key, val) in dict.iter() {
                    map.insert(key.to_string(), self.pdf_value_to_json(val));
                }
                json!(map)
            }
            PdfValue::Stream(stream) => {
                json!({
                    "type": "stream",
                    "dict": self.pdf_value_to_json(&PdfValue::Dictionary(stream.dict.clone())),
                    "length": stream.data.len()
                })
            }
            PdfValue::Reference(r) => {
                json!({
                    "type": "reference",
                    "object": format!("{} {} R", r.number(), r.generation())
                })
            }
        }
    }

    fn get_value_type(&self, value: &PdfValue) -> String {
        match value {
            PdfValue::Null => "null",
            PdfValue::Boolean(_) => "boolean",
            PdfValue::Integer(_) => "integer",
            PdfValue::Real(_) => "real",
            PdfValue::String(_) => "string",
            PdfValue::Name(_) => "name",
            PdfValue::Array(_) => "array",
            PdfValue::Dictionary(_) => "dictionary",
            PdfValue::Stream(_) => "stream",
            PdfValue::Reference(_) => "reference",
        }
        .to_string()
    }

    fn extract_document_metadata(&self, graph: &PdfAstGraph) -> HashMap<String, Value> {
        let mut metadata = HashMap::new();

        if let Some(root_id) = graph.get_root() {
            if let Some(catalog_id) = graph.get_children(root_id).into_iter().find(|&id| {
                graph
                    .get_node(id)
                    .map(|n| matches!(n.node_type, NodeType::Catalog))
                    .unwrap_or(false)
            }) {
                if let Some(catalog_node) = graph.get_node(catalog_id) {
                    if let PdfValue::Dictionary(dict) = &catalog_node.value {
                        if let Some(PdfValue::Dictionary(info)) = dict.get("Info") {
                            for (key, val) in info.iter() {
                                metadata.insert(key.to_string(), self.pdf_value_to_json(val));
                            }
                        }

                        if let Some(version) = dict.get("Version") {
                            metadata
                                .insert("pdf_version".to_string(), self.pdf_value_to_json(version));
                        }
                    }
                }
            }
        }

        metadata.insert("node_count".to_string(), json!(graph.node_count()));
        metadata.insert("edge_count".to_string(), json!(graph.edge_count()));
        metadata.insert("schema_version".to_string(), json!(SCHEMA_VERSION));

        metadata
    }
}

pub struct SchemaMigration {
    from_version: SchemaVersion,
    to_version: SchemaVersion,
    migration_fn: MigrationFunction,
}

pub struct SchemaMigrator {
    migrations: Vec<SchemaMigration>,
}

impl Default for SchemaMigrator {
    fn default() -> Self {
        Self::new()
    }
}

impl SchemaMigrator {
    pub fn new() -> Self {
        let mut migrator = Self {
            migrations: Vec::new(),
        };

        migrator.register_builtin_migrations();
        migrator
    }

    fn register_builtin_migrations(&mut self) {
        // Example migration from 0.9.0 to 1.0.0
        self.register_migration(
            SchemaVersion {
                major: 0,
                minor: 9,
                patch: 0,
            },
            SchemaVersion {
                major: 1,
                minor: 0,
                patch: 0,
            },
            Box::new(|schema| {
                // Add deterministic_ids field if missing
                if !schema.nodes.is_empty() {
                    for node in &mut schema.nodes {
                        if node.id.starts_with("node_") && !node.id.contains("_0x") {
                            // Old format, needs update
                            node.id = format!("{}_migrated", node.id);
                        }
                    }
                }
                Ok(())
            }),
        );
    }

    pub fn register_migration(
        &mut self,
        from: SchemaVersion,
        to: SchemaVersion,
        migration_fn: MigrationFunction,
    ) {
        self.migrations.push(SchemaMigration {
            from_version: from,
            to_version: to,
            migration_fn,
        });
    }

    pub fn migrate(
        &self,
        mut schema: StableAstSchema,
        target_version: SchemaVersion,
    ) -> Result<StableAstSchema, String> {
        while !schema.version.is_compatible_with(&target_version) {
            let migration = self.find_migration(&schema.version)?;
            (migration.migration_fn)(&mut schema)?;
            schema.version = migration.to_version.clone();
        }

        Ok(schema)
    }

    fn find_migration(&self, from: &SchemaVersion) -> Result<&SchemaMigration, String> {
        self.migrations
            .iter()
            .find(|m| {
                m.from_version.major == from.major
                    && m.from_version.minor == from.minor
                    && m.from_version.patch == from.patch
            })
            .ok_or_else(|| {
                format!(
                    "No migration found from version {}.{}.{}",
                    from.major, from.minor, from.patch
                )
            })
    }
}

pub fn generate_json_schema() -> Value {
    json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "title": "PDF-AST Schema",
        "version": SCHEMA_VERSION,
        "type": "object",
        "required": ["version", "nodes", "edges"],
        "properties": {
            "version": {
                "type": "object",
                "properties": {
                    "major": { "type": "integer" },
                    "minor": { "type": "integer" },
                    "patch": { "type": "integer" }
                },
                "required": ["major", "minor", "patch"]
            },
            "document_metadata": {
                "type": "object",
                "additionalProperties": true
            },
            "nodes": {
                "type": "array",
                "items": {
                    "type": "object",
                    "required": ["id", "node_type", "value_type", "value"],
                    "properties": {
                        "id": { "type": "string" },
                        "node_type": {
                            "type": "string",
                            "enum": ["Root", "Catalog", "Pages", "Page", "Font", "Image",
                                    "Annotation", "Form", "Outline", "StructElem", "ContentStream",
                                    "Resources", "MediaBox", "Metadata", "Info", "Unknown"]
                        },
                        "value_type": {
                            "type": "string",
                            "enum": ["null", "boolean", "integer", "real", "string",
                                    "name", "array", "dictionary", "stream", "reference"]
                        },
                        "value": {},
                        "metadata": { "type": "object" },
                        "object_id": { "type": "string" }
                    }
                }
            },
            "edges": {
                "type": "array",
                "items": {
                    "type": "object",
                    "required": ["source", "target", "edge_type"],
                    "properties": {
                        "source": { "type": "string" },
                        "target": { "type": "string" },
                        "edge_type": {
                            "type": "string",
                            "enum": ["Child", "Parent", "Reference", "Resource",
                                    "Annotation", "Field", "StructuralParent", "ContentItem"]
                        },
                        "metadata": { "type": "object" }
                    }
                }
            },
            "deterministic_ids": { "type": "boolean" }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::PdfDictionary;

    #[test]
    fn test_schema_version() {
        let v1 = SchemaVersion::current();
        let v2 = SchemaVersion::from_string("1.0.0").unwrap();
        assert!(v1.is_compatible_with(&v2));

        let v3 = SchemaVersion::from_string("2.0.0").unwrap();
        assert!(!v1.is_compatible_with(&v3));
    }

    #[test]
    fn test_deterministic_id_generation() {
        let mut graph = PdfAstGraph::new();
        let root = graph.create_node(NodeType::Root, PdfValue::Null);
        graph.set_root(root);

        let mut exporter = SchemaExporter::new(true);
        let schema = exporter.export(&graph);

        assert!(schema.deterministic_ids);
        assert_eq!(schema.nodes[0].id, "root");
    }

    #[test]
    fn test_schema_round_trip_to_graph() {
        let mut graph = PdfAstGraph::new();
        let catalog = graph.create_node(
            NodeType::Catalog,
            PdfValue::Dictionary(PdfDictionary::new()),
        );
        graph.set_root(catalog);

        let mut exporter = SchemaExporter::new(true);
        let schema = exporter.export(&graph);
        let imported = schema.to_graph().expect("Should rebuild graph");

        assert_eq!(imported.node_count(), graph.node_count());
        assert!(imported.get_root().is_some());
    }
}
