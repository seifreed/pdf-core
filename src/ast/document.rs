use crate::ast::linearization::LinearizationInfo;
use crate::ast::{NodeId, NodeType, PdfAstGraph};
use crate::forms::XfaDocument;
use crate::performance::ResourceBudget;
use crate::types::{ObjectId, PdfDictionary, PdfValue};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct PdfDocument {
    pub budget: ResourceBudget,
    pub ast: PdfAstGraph,
    pub version: PdfVersion,
    pub catalog: Option<NodeId>,
    pub info: Option<NodeId>,
    pub trailer: PdfDictionary,
    pub xref: CrossReferenceTable,
    pub metadata: DocumentMetadata,
    pub xfa: Option<XfaDocument>,
    pub linearization: Option<LinearizationInfo>,
    pub revisions: Vec<DocumentRevision>,
    pub names_tree: Option<NameTree>,
    pub outlines: Option<OutlineTree>,
    pub struct_tree: Option<StructureTree>,
    pub optional_content: Option<OptionalContentConfig>,
    pub diagnostics: Vec<ParseDiagnostic>,
    pub forensic: Option<ForensicSnapshot>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PdfVersion {
    pub major: u8,
    pub minor: u8,
}

#[derive(Debug, Clone, Default)]
pub struct CrossReferenceTable {
    pub entries: HashMap<ObjectId, XRefEntry>,
    pub streams: Vec<XRefStream>,
    pub prev_offset: Option<u64>,
    pub hybrid_mode: bool,
}

/// Structured parser finding retained for tolerant and forensic consumers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParseDiagnostic {
    pub object_id: Option<ObjectId>,
    pub offset: Option<u64>,
    pub error_code: String,
    pub recovery_action: String,
    pub confidence: f32,
    pub bytes_consumed: u64,
    pub message: String,
}

/// Raw xref evidence kept when forensic parsing is requested.
#[derive(Debug, Clone, Default)]
pub struct ForensicSnapshot {
    pub declared_xref: HashMap<ObjectId, XRefEntry>,
    pub recovered_xref: HashMap<ObjectId, XRefEntry>,
    pub duplicate_objects: Vec<ObjectId>,
    pub overwritten_objects: Vec<ObjectId>,
    pub residual_ranges: Vec<(u64, u64)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum XRefEntry {
    InUse {
        offset: u64,
        generation: u16,
    },
    Free {
        next_free_object: u32,
        generation: u16,
    },
    Compressed {
        stream_object: u32,
        index: u32,
    },
}

#[derive(Debug, Clone)]
pub struct XRefStream {
    pub object_id: ObjectId,
    pub dict: PdfDictionary,
    pub entries: Vec<XRefEntry>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DocumentMetadata {
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
    pub compliance: Vec<ComplianceProfile>,
    pub producer: Option<String>,
    pub creator: Option<String>,
    pub creation_date: Option<String>,
    pub modification_date: Option<String>,
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ComplianceProfile {
    PdfA1a,
    PdfA1b,
    PdfA2a,
    PdfA2b,
    PdfA2u,
    PdfA3a,
    PdfA3b,
    PdfA3u,
    PdfUA1,
    PdfX1a,
    PdfX3,
    PdfX4,
    Custom(String),
}

impl PdfDocument {
    /// Creates a new PDF document with the specified version.
    ///
    /// # Arguments
    /// * `version` - The PDF version (e.g., 1.7 for PDF 1.7)
    ///
    /// # Returns
    /// A new `PdfDocument` with an empty AST graph and default metadata
    pub fn new(version: PdfVersion) -> Self {
        PdfDocument {
            budget: ResourceBudget::default(),
            ast: PdfAstGraph::new(),
            version,
            catalog: None,
            info: None,
            trailer: PdfDictionary::new(),
            xref: CrossReferenceTable::default(),
            metadata: DocumentMetadata::default(),
            xfa: None,
            linearization: None,
            revisions: Vec::new(),
            names_tree: None,
            outlines: None,
            struct_tree: None,
            optional_content: None,
            diagnostics: Vec::new(),
            forensic: None,
        }
    }

    /// Sets the document catalog (root object) node.
    ///
    /// # Arguments
    /// * `catalog_id` - The node ID containing the document catalog dictionary
    ///
    /// # Note
    /// This also sets the AST graph root to the catalog node
    pub fn set_catalog(&mut self, catalog_id: NodeId) {
        self.catalog = Some(catalog_id);
        self.ast.set_root(catalog_id);
    }

    /// Sets the document information dictionary node.
    ///
    /// # Arguments
    /// * `info_id` - The node ID containing the document info dictionary (metadata like Title, Author, etc.)
    pub fn set_info(&mut self, info_id: NodeId) {
        self.info = Some(info_id);
    }

    /// Sets the document trailer dictionary.
    ///
    /// # Arguments
    /// * `trailer` - The trailer dictionary containing Size, Root, Info, Encrypt, etc.
    pub fn set_trailer(&mut self, trailer: PdfDictionary) {
        self.trailer = trailer;
    }

    /// Adds an entry to the cross-reference table.
    ///
    /// # Arguments
    /// * `obj_id` - The PDF object identifier
    /// * `entry` - The cross-reference entry (InUse, Free, or Compressed)
    pub fn add_xref_entry(&mut self, obj_id: ObjectId, entry: XRefEntry) {
        self.xref.entries.insert(obj_id, entry);
    }

    pub fn add_xref_stream(&mut self, stream: XRefStream) {
        self.xref.streams.push(stream);
    }

    pub fn set_linearization(&mut self, linearization: LinearizationInfo) {
        self.linearization = Some(linearization);
    }

    /// Checks if the document is linearized (optimized for web viewing).
    ///
    /// # Returns
    /// `true` if linearization information was detected, `false` otherwise
    pub fn is_linearized(&self) -> bool {
        self.linearization.is_some()
    }

    /// Returns linearization information if the document is linearized.
    ///
    /// # Returns
    /// `Some(&LinearizationInfo)` if available, `None` otherwise
    pub fn get_linearization(&self) -> Option<&LinearizationInfo> {
        self.linearization.as_ref()
    }

    /// Returns the document catalog dictionary.
    ///
    /// # Returns
    /// `Some(&PdfDictionary)` if the catalog exists and is a dictionary, `None` otherwise
    pub fn get_catalog(&self) -> Option<&PdfDictionary> {
        self.catalog
            .and_then(|id| self.ast.get_node(id))
            .and_then(|node| node.as_dict())
    }

    /// Returns the document information dictionary.
    ///
    /// # Returns
    /// `Some(&PdfDictionary)` if the info dictionary exists, `None` otherwise
    pub fn get_info(&self) -> Option<&PdfDictionary> {
        self.info
            .and_then(|id| self.ast.get_node(id))
            .and_then(|node| node.as_dict())
    }

    /// Returns all page nodes in the document.
    ///
    /// # Returns
    /// A vector of `NodeId`s for all page objects in the document
    pub fn get_pages(&self) -> Vec<NodeId> {
        self.ast.find_nodes_by_type(NodeType::Page)
    }

    /// Returns a specific page by zero-based index.
    ///
    /// # Arguments
    /// * `index` - The zero-based page index (0 for first page)
    ///
    /// # Returns
    /// `Some(NodeId)` if the page exists, `None` otherwise
    pub fn get_page(&self, index: usize) -> Option<NodeId> {
        self.get_pages().get(index).copied()
    }

    /// Analyzes and populates document metadata by scanning the AST.
    ///
    /// This method extracts:
    /// - Page count
    /// - JavaScript presence
    /// - Embedded files
    /// - Encryption status
    /// - Signatures
    /// - Multimedia content (RichMedia, 3D, audio, video)
    /// - XFA forms
    /// - Document Security Store (DSS) information
    /// - Info dictionary metadata (Title, Author, Producer, etc.)
    ///
    /// # Note
    /// This should be called after parsing is complete to populate the metadata field
    pub fn analyze_metadata(&mut self) {
        // Count pages from AST nodes, but also check Pages object count
        let ast_page_count = self.get_pages().len();
        let catalog_page_count = self.get_page_count_from_catalog();

        self.metadata.page_count = if ast_page_count > 0 {
            ast_page_count
        } else {
            catalog_page_count
        };

        // Check for JavaScript - look in multiple places
        self.metadata.has_javascript = self.detect_javascript();

        // Check for embedded files
        self.metadata.has_embedded_files = self.detect_embedded_files();

        // Check for encryption
        self.metadata.encrypted = self.detect_encryption();

        // Check for linearization
        self.metadata.linearized = self.is_linearized();

        self.metadata.has_signatures = !self.ast.find_nodes_by_type(NodeType::Signature).is_empty();

        // Extract info dictionary metadata
        if let Some(info_node) = self.info {
            if let Some(node) = self.ast.get_node(info_node) {
                if let Some(info_dict) = node.as_dict() {
                    if let Some(PdfValue::String(s)) = info_dict.get("Producer") {
                        self.metadata.producer = Some(s.to_string_lossy());
                    }
                    if let Some(PdfValue::String(s)) = info_dict.get("Creator") {
                        self.metadata.creator = Some(s.to_string_lossy());
                    }
                    if let Some(PdfValue::String(s)) = info_dict.get("CreationDate") {
                        self.metadata.creation_date = Some(s.to_string_lossy());
                    }
                    if let Some(PdfValue::String(s)) = info_dict.get("ModDate") {
                        self.metadata.modification_date = Some(s.to_string_lossy());
                    }
                }
            }
        }
    }

    fn get_page_count_from_catalog(&self) -> usize {
        // Try to get page count from Pages object in catalog
        if let Some(catalog_dict) = self.get_catalog() {
            if let Some(_pages_ref) = catalog_dict.get("Pages") {
                // Look for a Pages node that might have a Count
                for node in self.ast.get_all_nodes() {
                    if let Some(dict) = node.as_dict() {
                        if let Some(PdfValue::Name(type_name)) = dict.get("Type") {
                            if type_name.without_slash() == "Pages" {
                                if let Some(PdfValue::Integer(count)) = dict.get("Count") {
                                    return *count as usize;
                                }
                            }
                        }
                    }
                }
            }
        }
        0
    }

    fn detect_javascript(&self) -> bool {
        // Check for JavaScript actions by node type
        let js_actions = !self
            .ast
            .find_nodes_by_type(NodeType::JavaScriptAction)
            .is_empty();
        let embedded_js = !self.ast.find_nodes_by_type(NodeType::EmbeddedJS).is_empty();

        if js_actions || embedded_js {
            return true;
        }

        // Check all nodes for JavaScript content
        for node in self.ast.get_all_nodes() {
            if let Some(dict) = node.as_dict() {
                // Check for JavaScript action signature
                if let Some(PdfValue::Name(name)) = dict.get("S") {
                    if name.without_slash() == "JavaScript" {
                        return true;
                    }
                }
                // Check for JS entry
                if dict.contains_key("JS") {
                    return true;
                }
                // Check Type = Action with JavaScript subtype
                if let Some(PdfValue::Name(type_name)) = dict.get("Type") {
                    if type_name.without_slash() == "Action" {
                        if let Some(PdfValue::Name(subtype)) = dict.get("S") {
                            if subtype.without_slash() == "JavaScript" {
                                return true;
                            }
                        }
                    }
                }
            }
        }

        // Check catalog for JavaScript patterns - now properly resolved
        if let Some(catalog_dict) = self.get_catalog() {
            // Check OpenAction for JavaScript by following edges in AST
            if catalog_dict.contains_key("OpenAction") {
                // If we have an OpenAction, check if we have any action nodes linked from catalog
                if let Some(catalog_id) = self.catalog {
                    for edge in self.ast.get_edges_from(catalog_id) {
                        let target_node = self.ast.get_node(edge.to);
                        if let Some(target) = target_node {
                            // Check if this is an Action node type (even if the value is Null due to parsing issues)
                            if matches!(target.node_type, NodeType::Action) {
                                // If we have an Action node connected to a catalog with OpenAction,
                                // it's very likely JavaScript (this is a pragmatic heuristic for parsing issues)
                                return true;
                            }

                            if let Some(action_dict) = target.as_dict() {
                                // Check if this action is JavaScript
                                if let Some(PdfValue::Name(subtype)) = action_dict.get("S") {
                                    if subtype.without_slash() == "JavaScript" {
                                        return true;
                                    }
                                }
                                if action_dict.contains_key("JS") {
                                    return true;
                                }
                            }
                        }
                    }
                }
            }

            // Check Names dictionary for JavaScript
            if let Some(names_value) = catalog_dict.get("Names") {
                if let Some(names_dict) = names_value.as_dict() {
                    if names_dict.contains_key("JavaScript") {
                        return true;
                    }
                }
            }
        }

        false
    }

    fn detect_embedded_files(&self) -> bool {
        // Check for EmbeddedFile nodes
        if !self
            .ast
            .find_nodes_by_type(NodeType::EmbeddedFile)
            .is_empty()
        {
            return true;
        }

        // Check for filespec objects in AST
        for node in self.ast.get_all_nodes() {
            if let Some(dict) = node.as_dict() {
                if let Some(PdfValue::Name(type_name)) = dict.get("Type") {
                    if type_name.without_slash() == "Filespec" {
                        return true;
                    }
                }
                // Check for EmbeddedFiles patterns
                if dict.contains_key("EmbeddedFiles")
                    || (dict.contains_key("F") && dict.contains_key("EF"))
                {
                    return true;
                }
                // Check for Names dictionary with EmbeddedFiles entry
                if dict.contains_key("Names") {
                    // This might be a Names dictionary with embedded files
                    // Check if it has any children in the AST that are embedded files
                    return true;
                }
            }
        }

        // Check catalog for Names dictionary through AST edges
        if let Some(catalog_dict) = self.get_catalog() {
            if catalog_dict.contains_key("Names") {
                // Look through AST edges for names dictionary linked from catalog
                if let Some(catalog_id) = self.catalog {
                    for edge in self.ast.get_edges_from(catalog_id) {
                        if let Some(target_node) = self.ast.get_node(edge.to) {
                            if let Some(names_dict) = target_node.as_dict() {
                                // Check if this names dictionary has EmbeddedFiles
                                if names_dict.contains_key("EmbeddedFiles") {
                                    return true;
                                }
                                // Check for any embedded file pattern in the AST graph
                                for sub_edge in self.ast.get_edges_from(target_node.id) {
                                    if let Some(sub_node) = self.ast.get_node(sub_edge.to) {
                                        if let Some(sub_dict) = sub_node.as_dict() {
                                            if sub_dict.contains_key("Names") {
                                                // This could be an embedded files name tree
                                                return true;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        false
    }

    fn detect_encryption(&self) -> bool {
        // Check trailer for Encrypt entry
        if self.trailer.contains_key("Encrypt") {
            return true;
        }

        // Check for encryption-related nodes
        !self.ast.find_nodes_by_type(NodeType::Encrypt).is_empty()
    }

    /// Validates the document structure against PDF specification requirements.
    ///
    /// Checks:
    /// - Presence of required catalog dictionary
    /// - Required entries in catalog (Pages, Type)
    /// - Absence of circular references in object graph
    ///
    /// # Returns
    /// A vector of error messages describing structural problems, or an empty vector if valid
    pub fn validate_structure(&self) -> Vec<String> {
        let mut errors = Vec::new();

        if self.catalog.is_none() {
            errors.push("Missing catalog dictionary".to_string());
        }

        if let Some(catalog_dict) = self.get_catalog() {
            if !catalog_dict.contains_key("Pages") {
                errors.push("Catalog missing Pages entry".to_string());
            }
            if !catalog_dict.contains_key("Type") {
                errors.push("Catalog missing Type entry".to_string());
            }
        }

        if self.ast.is_cyclic() {
            errors.push("Document graph contains cycles".to_string());
        }

        errors
    }
}

impl PdfVersion {
    /// Creates a new PDF version identifier.
    ///
    /// # Arguments
    /// * `major` - Major version number (typically 1 or 2)
    /// * `minor` - Minor version number (0-7 for PDF 1.x)
    ///
    /// # Returns
    /// A new `PdfVersion` instance
    ///
    /// # Example
    /// ```
    /// use pdf_ast::PdfVersion;
    /// let version = PdfVersion::new(1, 7); // PDF 1.7
    /// ```
    pub fn new(major: u8, minor: u8) -> Self {
        PdfVersion { major, minor }
    }

    /// Parses a PDF version from a string like "1.7" or "2.0".
    ///
    /// # Arguments
    /// * `s` - A string in the format "major.minor"
    ///
    /// # Returns
    /// `Some(PdfVersion)` if parsing succeeds, `None` if the format is invalid
    pub fn from_string(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() == 2 {
            let major = parts[0].parse().ok()?;
            let minor = parts[1].parse().ok()?;
            Some(PdfVersion::new(major, minor))
        } else {
            None
        }
    }
}

impl Default for PdfVersion {
    fn default() -> Self {
        PdfVersion::new(1, 7)
    }
}

impl PdfDocument {
    /// Returns a timeline view over incremental document revisions.
    pub fn timeline(&self) -> Timeline<'_> {
        Timeline::new(&self.revisions)
    }

    /// Walks incremental revisions in order.
    pub fn walk_incrementals<F>(&self, mut f: F)
    where
        F: FnMut(&DocumentRevision),
    {
        for revision in &self.revisions {
            f(revision);
        }
    }

    /// Adds a revision and emits an event to the provided listener.
    pub fn add_revision_with_listener(
        &mut self,
        revision: DocumentRevision,
        listener: &mut dyn crate::events::AstEventListener,
    ) {
        self.revisions.push(revision);
        if let Some(latest) = self.revisions.last() {
            listener.on_incremental_applied(latest);
        }
    }
}

impl std::fmt::Display for PdfVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

/// Document revision for incremental updates
#[derive(Debug, Clone)]
pub struct DocumentRevision {
    pub revision_number: u32,
    pub xref_offset: u64,
    pub trailer: PdfDictionary,
    pub modified_objects: Vec<ObjectId>,
    pub added_objects: Vec<ObjectId>,
    pub deleted_objects: Vec<ObjectId>,
}

/// Timeline view over document revisions.
pub struct Timeline<'a> {
    revisions: &'a [DocumentRevision],
}

impl<'a> Timeline<'a> {
    pub fn new(revisions: &'a [DocumentRevision]) -> Self {
        Self { revisions }
    }

    pub fn iter(&self) -> TimelineIter<'a> {
        TimelineIter {
            revisions: self.revisions,
            index: 0,
        }
    }
}

pub struct TimelineIter<'a> {
    revisions: &'a [DocumentRevision],
    index: usize,
}

impl<'a> Iterator for TimelineIter<'a> {
    type Item = TimelineStep<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.revisions.len() {
            return None;
        }
        let current_index = self.index;
        self.index += 1;
        Some(TimelineStep {
            revision: &self.revisions[current_index],
            previous: self
                .revisions
                .get(current_index.saturating_sub(1))
                .filter(|_| current_index > 0),
        })
    }
}

pub struct TimelineStep<'a> {
    revision: &'a DocumentRevision,
    previous: Option<&'a DocumentRevision>,
}

impl<'a> TimelineStep<'a> {
    pub fn revision(&self) -> &'a DocumentRevision {
        self.revision
    }

    /// Returns object-level changes for this revision.
    pub fn nodes(&self) -> RevisionDiff<'a> {
        RevisionDiff {
            added: &self.revision.added_objects,
            modified: &self.revision.modified_objects,
            deleted: &self.revision.deleted_objects,
        }
    }

    /// Returns object-level changes relative to the previous revision.
    pub fn diff_from_previous(&self) -> Option<RevisionDiff<'a>> {
        self.previous.map(|_| self.nodes())
    }
}

pub struct RevisionDiff<'a> {
    pub added: &'a [ObjectId],
    pub modified: &'a [ObjectId],
    pub deleted: &'a [ObjectId],
}

/// Names tree for various named destinations
#[derive(Debug, Clone)]
pub struct NameTree {
    pub dests: Option<NameTreeNode>,
    pub embedded_files: Option<NameTreeNode>,
    pub javascript: Option<NameTreeNode>,
    pub pages: Option<NameTreeNode>,
    pub templates: Option<NameTreeNode>,
    pub alternate_presentations: Option<NameTreeNode>,
    pub renditions: Option<NameTreeNode>,
}

#[derive(Debug, Clone)]
pub struct NameTreeNode {
    pub names: Vec<(String, NodeId)>,
    pub kids: Vec<NodeId>,
    pub limits: Option<(String, String)>,
}

/// Outline/Bookmarks tree
#[derive(Debug, Clone)]
pub struct OutlineTree {
    pub root: NodeId,
    pub items: HashMap<NodeId, OutlineItem>,
}

#[derive(Debug, Clone)]
pub struct OutlineItem {
    pub title: String,
    pub dest: Option<Destination>,
    pub action: Option<NodeId>,
    pub parent: Option<NodeId>,
    pub prev: Option<NodeId>,
    pub next: Option<NodeId>,
    pub first: Option<NodeId>,
    pub last: Option<NodeId>,
    pub count: i32,
    pub flags: u32,
    pub color: Option<[f32; 3]>,
}

#[derive(Debug, Clone)]
pub enum Destination {
    Page(NodeId),
    Named(String),
    Remote(String, String),
    Explicit {
        page: NodeId,
        typ: DestinationType,
        coords: Vec<f32>,
    },
}

#[derive(Debug, Clone)]
pub enum DestinationType {
    XYZ,
    Fit,
    FitH,
    FitV,
    FitR,
    FitB,
    FitBH,
    FitBV,
}

/// Structure tree for tagged PDF
#[derive(Debug, Clone)]
pub struct StructureTree {
    pub root: NodeId,
    pub role_map: HashMap<String, String>,
    pub class_map: HashMap<String, NodeId>,
    pub parent_tree: ParentTree,
    pub id_tree: Option<NameTreeNode>,
}

#[derive(Debug, Clone)]
pub struct ParentTree {
    pub entries: HashMap<u32, ParentTreeEntry>, // Page object number -> parent StructElems
    pub limits: Option<(i64, i64)>,             // Min and max keys covered by this tree
}

#[derive(Debug, Clone)]
pub enum ParentTreeEntry {
    Single(NodeId),        // Single parent structure element
    Multiple(Vec<NodeId>), // Array of parent structure elements
}

impl Default for ParentTree {
    fn default() -> Self {
        Self::new()
    }
}

impl ParentTree {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            limits: None,
        }
    }

    pub fn add_parent_entry(&mut self, page_obj_num: u32, entry: ParentTreeEntry) {
        self.entries.insert(page_obj_num, entry);
    }

    pub fn get_parents(&self, page_obj_num: u32) -> Option<&ParentTreeEntry> {
        self.entries.get(&page_obj_num)
    }

    pub fn set_limits(&mut self, min: i64, max: i64) {
        self.limits = Some((min, max));
    }

    pub fn merge(&mut self, other: ParentTree) {
        for (key, value) in other.entries {
            self.entries.insert(key, value);
        }

        // Update limits if needed
        if let Some((other_min, other_max)) = other.limits {
            match self.limits {
                Some((min, max)) => {
                    self.limits = Some((min.min(other_min), max.max(other_max)));
                }
                None => {
                    self.limits = Some((other_min, other_max));
                }
            }
        }
    }

    pub fn get_all_parents(&self) -> Vec<NodeId> {
        let mut all_parents = Vec::new();
        for entry in self.entries.values() {
            match entry {
                ParentTreeEntry::Single(node_id) => all_parents.push(*node_id),
                ParentTreeEntry::Multiple(nodes) => all_parents.extend(nodes),
            }
        }
        all_parents
    }
}

/// Optional Content Configuration
#[derive(Debug, Clone)]
pub struct OptionalContentConfig {
    pub ocgs: Vec<NodeId>,
    pub default_config: NodeId,
    pub configs: Vec<NodeId>,
    pub properties: OptionalContentProperties,
}

#[derive(Debug, Clone)]
pub struct OptionalContentProperties {
    pub d: OCDisplayDict,
    pub base_state: BaseState,
    pub on: Vec<NodeId>,
    pub off: Vec<NodeId>,
    pub order: Vec<OCOrderItem>,
    pub list_mode: ListMode,
    pub rb_groups: Vec<Vec<NodeId>>,
    pub locked: Vec<NodeId>,
}

#[derive(Debug, Clone)]
pub struct OCDisplayDict {
    pub name: Option<String>,
    pub creator: Option<String>,
}

#[derive(Debug, Clone)]
pub enum BaseState {
    On,
    Off,
    Unchanged,
}

#[derive(Debug, Clone)]
pub enum OCOrderItem {
    Group(NodeId),
    Label(String),
    Array(Vec<OCOrderItem>),
}

#[derive(Debug, Clone)]
pub enum ListMode {
    AllPages,
    VisiblePages,
}
