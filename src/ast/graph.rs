use crate::ast::{AstNode, NodeId, NodeType};
use crate::events::AstEventListener;
use crate::performance::{ResourceBudget, ResourceBudgetError};
use crate::types::{ObjectId, PdfReference, PdfValue};
use crate::visitor::{AstWalker as VisitorAstWalker, Visitor};
use petgraph::graph::NodeIndex;
use petgraph::visit::{Bfs, Dfs, EdgeRef};
use petgraph::Graph;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone)]
pub struct PdfAstGraph {
    graph: Graph<AstNode, EdgeType>,
    node_map: HashMap<NodeId, NodeIndex>,
    object_map: HashMap<ObjectId, NodeId>,
    next_node_id: usize,
    pub root: Option<NodeId>,
    deterministic_ids: bool,
    content_hash: HashMap<String, NodeId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EdgeType {
    Child,
    Reference,
    Parent,
    Resource,
    Annotation,
    Content,
}

#[derive(Debug, Clone)]
pub struct EdgeInfo {
    pub from: NodeId,
    pub to: NodeId,
    pub edge_type: EdgeType,
}

impl PdfAstGraph {
    /// Creates a new empty PDF AST graph with sequential node IDs.
    ///
    /// # Returns
    /// An empty `PdfAstGraph` ready for node insertion
    pub fn new() -> Self {
        PdfAstGraph {
            graph: Graph::new(),
            node_map: HashMap::new(),
            object_map: HashMap::new(),
            next_node_id: 0,
            root: None,
            deterministic_ids: false,
            content_hash: HashMap::new(),
        }
    }

    /// Walks nodes in depth-first order starting from the root node.
    pub fn walk_nodes<V: Visitor>(&self, visitor: &mut V) {
        let mut walker = VisitorAstWalker::new(self);
        walker.walk(visitor);
    }

    /// Walks nodes with a lightweight callback.
    pub fn walk_nodes_with<F>(&self, mut f: F)
    where
        F: FnMut(&AstNode),
    {
        self.walk_nodes(&mut CallbackVisitor { callback: &mut f });
    }

    pub fn walk_nodes_with_budget<V: Visitor>(
        &self,
        visitor: &mut V,
        budget: &ResourceBudget,
    ) -> Result<(), ResourceBudgetError> {
        let mut walker = VisitorAstWalker::new(self);
        walker.walk_with_budget(visitor, budget)
    }

    /// Creates a new PDF AST graph with content-based deterministic node IDs.
    ///
    /// Deterministic IDs are derived from node content, ensuring identical
    /// content produces identical node IDs across multiple parses. This is
    /// useful for diffing, testing, and reproducible builds.
    ///
    /// # Returns
    /// An empty `PdfAstGraph` configured for deterministic node ID generation
    pub fn new_with_deterministic_ids() -> Self {
        PdfAstGraph {
            graph: Graph::new(),
            node_map: HashMap::new(),
            object_map: HashMap::new(),
            next_node_id: 0,
            root: None,
            deterministic_ids: true,
            content_hash: HashMap::new(),
        }
    }

    /// Enables or disables deterministic node ID generation.
    ///
    /// # Arguments
    /// * `deterministic` - If true, node IDs are derived from content; if false, sequential IDs are used
    ///
    /// # Note
    /// Disabling deterministic mode clears the internal content hash cache
    pub fn set_deterministic_ids(&mut self, deterministic: bool) {
        self.deterministic_ids = deterministic;
        if !deterministic {
            self.content_hash.clear();
        }
    }

    /// Creates a new node in the graph with the specified type and value.
    ///
    /// # Arguments
    /// * `node_type` - The semantic type of the node (Page, Catalog, Stream, etc.)
    /// * `value` - The PDF value stored in this node
    ///
    /// # Returns
    /// The unique `NodeId` assigned to the newly created node
    pub fn create_node(&mut self, node_type: NodeType, value: PdfValue) -> NodeId {
        let node_id = if self.deterministic_ids {
            self.generate_deterministic_id(&node_type, &value)
        } else {
            let id = NodeId(self.next_node_id);
            self.next_node_id += 1;
            id
        };

        let node = AstNode::new(node_id, node_type.clone(), value);
        let index = self.graph.add_node(node);
        self.node_map.insert(node_id, index);

        if let NodeType::Object(obj_id) = node_type {
            self.object_map.insert(obj_id, node_id);
        }

        node_id
    }

    /// Associates an indirect object identity with an already-created node.
    ///
    /// Resolvers often assign a semantic node type instead of
    /// `NodeType::Object`, so the identity must be registered separately.
    pub fn register_object_node(&mut self, object_id: ObjectId, node_id: NodeId) -> bool {
        if self.node_map.contains_key(&node_id) {
            self.object_map.insert(object_id, node_id);
            true
        } else {
            false
        }
    }

    /// Creates a node and emits an event to the provided listener.
    pub fn create_node_with_listener(
        &mut self,
        node_type: NodeType,
        value: PdfValue,
        listener: &mut dyn AstEventListener,
    ) -> NodeId {
        let node_id = self.create_node(node_type.clone(), value);
        listener.on_node_added(node_id, &node_type);
        node_id
    }

    fn generate_deterministic_id(&mut self, node_type: &NodeType, value: &PdfValue) -> NodeId {
        let content_hash = self.compute_content_hash(node_type, value);

        // Check if we already have a node with this content
        if let Some(&existing_id) = self.content_hash.get(&content_hash) {
            return existing_id;
        }

        // Create a deterministic ID based on content
        let mut hasher = DefaultHasher::new();
        content_hash.hash(&mut hasher);
        let hash_value = hasher.finish() as usize;

        // Ensure uniqueness by checking existing IDs
        let mut id_value = hash_value;
        let mut node_id = NodeId(id_value);

        while self.node_map.contains_key(&node_id) {
            id_value = id_value.wrapping_add(1);
            node_id = NodeId(id_value);
        }

        self.content_hash.insert(content_hash, node_id);

        // Update next_node_id to maintain consistency
        if id_value >= self.next_node_id {
            self.next_node_id = id_value + 1;
        }

        node_id
    }

    fn compute_content_hash(&self, node_type: &NodeType, value: &PdfValue) -> String {
        let mut hasher = DefaultHasher::new();

        // Hash node type
        format!("{:?}", node_type).hash(&mut hasher);

        // Hash value content
        self.hash_pdf_value(value, &mut hasher);

        format!("{:x}", hasher.finish())
    }

    #[allow(clippy::only_used_in_recursion)]
    fn hash_pdf_value(&self, value: &PdfValue, hasher: &mut DefaultHasher) {
        match value {
            PdfValue::Null => "null".hash(hasher),
            PdfValue::Boolean(b) => b.hash(hasher),
            PdfValue::Integer(i) => i.hash(hasher),
            PdfValue::Real(r) => r.to_bits().hash(hasher),
            PdfValue::String(s) => s.as_bytes().hash(hasher),
            PdfValue::Name(n) => n.as_str().hash(hasher),
            PdfValue::Array(arr) => {
                arr.len().hash(hasher);
                for item in arr {
                    self.hash_pdf_value(item, hasher);
                }
            }
            PdfValue::Dictionary(dict) => {
                dict.len().hash(hasher);
                // Sort keys for deterministic hashing
                let mut keys: Vec<_> = dict.keys().collect();
                keys.sort();
                for key in keys {
                    key.hash(hasher);
                    if let Some(val) = dict.get(key.as_str()) {
                        self.hash_pdf_value(val, hasher);
                    }
                }
            }
            PdfValue::Stream(stream) => {
                self.hash_pdf_value(&PdfValue::Dictionary(stream.dict.clone()), hasher);
                stream.data.len().hash(hasher);
                if stream.data.len() < 1024 {
                    // Hash small stream data
                    let data_hash = stream.data.hash();
                    data_hash.hash(hasher);
                } else {
                    // For large streams, use content-based hashing approach
                    let data_hash = stream.data.hash();
                    data_hash.hash(hasher);
                }
            }
            PdfValue::Reference(r) => {
                r.number().hash(hasher);
                r.generation().hash(hasher);
            }
        }
    }

    /// Adds a pre-constructed node to the graph.
    ///
    /// # Arguments
    /// * `node` - An `AstNode` with its ID, type, and value already set
    ///
    /// # Returns
    /// The `NodeId` of the added node
    pub fn add_node(&mut self, node: AstNode) -> NodeId {
        let node_id = node.id;
        let index = self.graph.add_node(node.clone());
        self.node_map.insert(node_id, index);

        if let NodeType::Object(obj_id) = &node.node_type {
            self.object_map.insert(*obj_id, node_id);
        }

        if self.next_node_id <= node_id.0 {
            self.next_node_id = node_id.0 + 1;
        }

        node_id
    }

    /// Retrieves an immutable reference to a node by its ID.
    ///
    /// # Arguments
    /// * `node_id` - The unique identifier of the node
    ///
    /// # Returns
    /// `Some(&AstNode)` if the node exists, `None` otherwise
    pub fn get_node(&self, node_id: NodeId) -> Option<&AstNode> {
        self.node_map
            .get(&node_id)
            .and_then(|&index| self.graph.node_weight(index))
    }

    /// Retrieves a mutable reference to a node by its ID.
    ///
    /// # Arguments
    /// * `node_id` - The unique identifier of the node
    ///
    /// # Returns
    /// `Some(&mut AstNode)` if the node exists, `None` otherwise
    pub fn get_node_mut(&mut self, node_id: NodeId) -> Option<&mut AstNode> {
        self.node_map
            .get(&node_id)
            .and_then(|&index| self.graph.node_weight_mut(index))
    }

    /// Retrieves a node by its PDF object ID.
    ///
    /// # Arguments
    /// * `obj_id` - The PDF object identifier (e.g., "1 0 obj")
    ///
    /// # Returns
    /// `Some(&AstNode)` if a node with this object ID exists, `None` otherwise
    pub fn get_node_by_object(&self, obj_id: ObjectId) -> Option<&AstNode> {
        self.object_map
            .get(&obj_id)
            .and_then(|&node_id| self.get_node(node_id))
    }

    /// Returns all nodes in the graph as a vector of references.
    ///
    /// # Returns
    /// A vector containing immutable references to all nodes in the graph
    pub fn get_all_nodes(&self) -> Vec<&AstNode> {
        self.get_all_nodes_with_budget(&ResourceBudget::default())
            .unwrap_or_default()
    }

    pub fn get_all_nodes_with_budget(
        &self,
        budget: &ResourceBudget,
    ) -> Result<Vec<&AstNode>, ResourceBudgetError> {
        let mut nodes = Vec::new();
        for node in self.graph.node_weights() {
            budget.consume_node()?;
            nodes.push(node);
        }
        Ok(nodes)
    }

    /// Returns all edges in the graph with their source, target, and type information.
    ///
    /// # Returns
    /// A vector of `EdgeInfo` structs describing all edges in the graph
    pub fn get_all_edges(&self) -> Vec<EdgeInfo> {
        self.get_all_edges_with_budget(&ResourceBudget::default())
            .unwrap_or_default()
    }

    pub fn get_all_edges_with_budget(
        &self,
        budget: &ResourceBudget,
    ) -> Result<Vec<EdgeInfo>, ResourceBudgetError> {
        let mut edges = Vec::new();
        let mut node_id_reverse_map = HashMap::new();
        for (&node_id, &index) in &self.node_map {
            budget.consume_node()?;
            node_id_reverse_map.insert(index, node_id);
        }

        for edge_ref in self.graph.edge_references() {
            if let (Some(&from_id), Some(&to_id)) = (
                node_id_reverse_map.get(&edge_ref.source()),
                node_id_reverse_map.get(&edge_ref.target()),
            ) {
                budget.consume_edge()?;
                edges.push(EdgeInfo {
                    from: from_id,
                    to: to_id,
                    edge_type: *edge_ref.weight(),
                });
            }
        }

        Ok(edges)
    }

    /// Returns the total number of nodes in the graph.
    ///
    /// # Returns
    /// The count of nodes currently in the graph
    pub fn node_count(&self) -> usize {
        self.graph.node_count()
    }

    /// Returns the total number of edges in the graph.
    ///
    /// # Returns
    /// The count of edges currently in the graph
    pub fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }

    /// Detects whether the graph contains any cycles.
    ///
    /// A well-formed PDF should not have cycles in its object reference graph,
    /// so this method can help detect malformed or potentially malicious documents.
    ///
    /// # Returns
    /// `true` if at least one cycle is detected, `false` otherwise
    pub fn is_cyclic(&self) -> bool {
        use petgraph::visit::depth_first_search;
        use petgraph::visit::DfsEvent;

        let mut is_cyclic = false;
        depth_first_search(
            &self.graph,
            self.graph.node_indices(),
            |event| match event {
                DfsEvent::BackEdge(..) => {
                    is_cyclic = true;
                    petgraph::visit::Control::Break(())
                }
                _ => petgraph::visit::Control::Continue,
            },
        );
        is_cyclic
    }

    /// Sets the root node of the graph (typically the Catalog).
    ///
    /// # Arguments
    /// * `root_id` - The `NodeId` to designate as the document root
    pub fn set_root(&mut self, root_id: NodeId) {
        self.root = Some(root_id);
    }

    /// Finds all nodes matching a specific node type.
    ///
    /// # Arguments
    /// * `node_type` - The node type to search for (e.g., `NodeType::Page`, `NodeType::JavaScriptAction`)
    ///
    /// # Returns
    /// A vector of `NodeId`s for all nodes matching the specified type
    pub fn find_nodes_by_type(&self, node_type: NodeType) -> Vec<NodeId> {
        self.find_nodes_by_type_with_budget(node_type, &ResourceBudget::default())
            .unwrap_or_default()
    }

    pub fn find_nodes_by_type_with_budget(
        &self,
        node_type: NodeType,
        budget: &ResourceBudget,
    ) -> Result<Vec<NodeId>, ResourceBudgetError> {
        let mut nodes = Vec::new();
        for node in self.graph.node_weights() {
            if std::mem::discriminant(&node.node_type) == std::mem::discriminant(&node_type) {
                budget.consume_node()?;
                nodes.push(node.id);
            }
        }
        Ok(nodes)
    }

    /// Adds a directed edge between two nodes in the graph.
    ///
    /// # Arguments
    /// * `from` - The source node ID
    /// * `to` - The target node ID
    /// * `edge_type` - The semantic type of the relationship (Child, Reference, Parent, etc.)
    ///
    /// # Returns
    /// `true` if the edge was successfully added, `false` if either node doesn't exist
    pub fn add_edge(&mut self, from: NodeId, to: NodeId, edge_type: EdgeType) -> bool {
        if let (Some(&from_idx), Some(&to_idx)) = (self.node_map.get(&from), self.node_map.get(&to))
        {
            self.graph.add_edge(from_idx, to_idx, edge_type);

            match edge_type {
                EdgeType::Child => {
                    if let Some(from_node) = self.get_node_mut(from) {
                        from_node.add_child(to);
                    }
                }
                EdgeType::Reference => {
                    if let Some(from_node) = self.get_node_mut(from) {
                        from_node.add_reference(to);
                    }
                }
                _ => {}
            }
            true
        } else {
            false
        }
    }

    /// Adds an edge and emits an event to the provided listener.
    pub fn add_edge_with_listener(
        &mut self,
        from: NodeId,
        to: NodeId,
        edge_type: EdgeType,
        listener: &mut dyn AstEventListener,
    ) -> bool {
        let added = self.add_edge(from, to, edge_type);
        if added {
            listener.on_edge_added(from, to, edge_type);
        }
        added
    }

    /// Returns the root node ID if one has been set.
    ///
    /// # Returns
    /// `Some(NodeId)` if a root exists, `None` otherwise
    pub fn get_root(&self) -> Option<NodeId> {
        self.root
    }

    /// Returns all child nodes of the specified node.
    ///
    /// # Arguments
    /// * `node_id` - The parent node ID
    ///
    /// # Returns
    /// A vector of `NodeId`s for all nodes connected via `EdgeType::Child` edges
    pub fn get_children(&self, node_id: NodeId) -> Vec<NodeId> {
        self.get_children_with_budget(node_id, &ResourceBudget::default())
            .unwrap_or_default()
    }

    pub fn get_children_with_budget(
        &self,
        node_id: NodeId,
        budget: &ResourceBudget,
    ) -> Result<Vec<NodeId>, ResourceBudgetError> {
        let mut children = Vec::new();
        if let Some(&index) = self.node_map.get(&node_id) {
            for edge in self.graph.edges(index) {
                if *edge.weight() == EdgeType::Child {
                    if let Some(node) = self.graph.node_weight(edge.target()) {
                        budget.consume_node()?;
                        children.push(node.id);
                    }
                }
            }
        }
        Ok(children)
    }

    /// Returns all nodes referenced by the specified node.
    ///
    /// # Arguments
    /// * `node_id` - The referencing node ID
    ///
    /// # Returns
    /// A vector of `NodeId`s for all nodes connected via `EdgeType::Reference` edges
    pub fn get_references(&self, node_id: NodeId) -> Vec<NodeId> {
        self.get_references_with_budget(node_id, &ResourceBudget::default())
            .unwrap_or_default()
    }

    pub fn get_references_with_budget(
        &self,
        node_id: NodeId,
        budget: &ResourceBudget,
    ) -> Result<Vec<NodeId>, ResourceBudgetError> {
        let mut references = Vec::new();
        if let Some(&index) = self.node_map.get(&node_id) {
            for edge in self.graph.edges(index) {
                if *edge.weight() == EdgeType::Reference {
                    if let Some(node) = self.graph.node_weight(edge.target()) {
                        budget.consume_node()?;
                        references.push(node.id);
                    }
                }
            }
        }
        Ok(references)
    }

    /// Resolves a PDF indirect reference to its corresponding AST node.
    ///
    /// # Arguments
    /// * `reference` - A PDF indirect reference (e.g., "5 0 R")
    ///
    /// # Returns
    /// `Some(&AstNode)` if the referenced object exists, `None` otherwise
    pub fn resolve_reference(&self, reference: &PdfReference) -> Option<&AstNode> {
        self.get_node_by_object(reference.id())
    }

    /// Performs a breadth-first traversal of the graph starting from the root node.
    ///
    /// # Arguments
    /// * `visitor` - A closure called for each node during traversal
    ///
    /// # Note
    /// Does nothing if no root node has been set
    pub fn bfs_from_root<F>(&self, mut visitor: F)
    where
        F: FnMut(&AstNode),
    {
        let _ = self.bfs_from_root_with_budget(&mut visitor, &ResourceBudget::default());
    }

    pub fn bfs_from_root_with_budget<F>(
        &self,
        visitor: &mut F,
        budget: &ResourceBudget,
    ) -> Result<(), ResourceBudgetError>
    where
        F: FnMut(&AstNode),
    {
        if let Some(root_id) = self.root {
            if let Some(&root_index) = self.node_map.get(&root_id) {
                let mut bfs = Bfs::new(&self.graph, root_index);
                while let Some(nx) = bfs.next(&self.graph) {
                    if let Some(node) = self.graph.node_weight(nx) {
                        budget.consume_node()?;
                        visitor(node);
                    }
                }
            }
        }
        Ok(())
    }

    /// Performs a depth-first traversal of the graph starting from the root node.
    ///
    /// # Arguments
    /// * `visitor` - A closure called for each node during traversal
    ///
    /// # Note
    /// Does nothing if no root node has been set
    pub fn dfs_from_root<F>(&self, mut visitor: F)
    where
        F: FnMut(&AstNode),
    {
        let _ = self.dfs_from_root_with_budget(&mut visitor, &ResourceBudget::default());
    }

    pub fn dfs_from_root_with_budget<F>(
        &self,
        visitor: &mut F,
        budget: &ResourceBudget,
    ) -> Result<(), ResourceBudgetError>
    where
        F: FnMut(&AstNode),
    {
        if let Some(root_id) = self.root {
            if let Some(&root_index) = self.node_map.get(&root_id) {
                let mut dfs = Dfs::new(&self.graph, root_index);
                while let Some(nx) = dfs.next(&self.graph) {
                    if let Some(node) = self.graph.node_weight(nx) {
                        budget.consume_node()?;
                        visitor(node);
                    }
                }
            }
        }
        Ok(())
    }

    /// Finds all nodes marked as containing errors during parsing.
    ///
    /// # Returns
    /// A vector of `NodeId`s for nodes that encountered parse errors
    pub fn find_error_nodes(&self) -> Vec<NodeId> {
        self.find_error_nodes_with_budget(&ResourceBudget::default())
            .unwrap_or_default()
    }

    pub fn find_error_nodes_with_budget(
        &self,
        budget: &ResourceBudget,
    ) -> Result<Vec<NodeId>, ResourceBudgetError> {
        let mut nodes = Vec::new();
        for node in self.graph.node_weights().filter(|node| node.is_error()) {
            budget.consume_node()?;
            nodes.push(node.id);
        }
        Ok(nodes)
    }

    pub fn get_graph(&self) -> &Graph<AstNode, EdgeType> {
        &self.graph
    }

    // Missing methods that are used in the codebase
    /// Checks whether a node with the given ID exists in the graph.
    ///
    /// # Arguments
    /// * `node_id` - The node ID to check
    ///
    /// # Returns
    /// `true` if the node exists, `false` otherwise
    pub fn contains_node(&self, node_id: NodeId) -> bool {
        self.node_map.contains_key(&node_id)
    }

    /// Removes a node and all its edges from the graph.
    ///
    /// # Arguments
    /// * `node_id` - The node ID to remove
    ///
    /// # Returns
    /// `true` if the node was removed, `false` if it didn't exist
    pub fn remove_node(&mut self, node_id: NodeId) -> bool {
        if let Some(index) = self.node_map.remove(&node_id) {
            self.graph.remove_node(index);
            true
        } else {
            false
        }
    }

    pub fn remove_edge(&mut self, from: NodeId, to: NodeId) -> bool {
        if let (Some(&from_idx), Some(&to_idx)) = (self.node_map.get(&from), self.node_map.get(&to))
        {
            if let Some(edge_idx) = self.graph.find_edge(from_idx, to_idx) {
                self.graph.remove_edge(edge_idx);
                return true;
            }
        }
        false
    }

    /// Returns the parent node of the specified node.
    ///
    /// # Arguments
    /// * `node_id` - The child node ID
    ///
    /// # Returns
    /// `Some(NodeId)` of the parent if connected via `EdgeType::Child`, `None` otherwise
    pub fn get_parent(&self, node_id: NodeId) -> Option<NodeId> {
        if let Some(&index) = self.node_map.get(&node_id) {
            for edge in self
                .graph
                .edges_directed(index, petgraph::Direction::Incoming)
            {
                if *edge.weight() == EdgeType::Child {
                    if let Some(parent_node) = self.graph.node_weight(edge.source()) {
                        return Some(parent_node.id);
                    }
                }
            }
        }
        None
    }

    pub fn get_nodes_by_type(&self, node_type: NodeType) -> Vec<NodeId> {
        self.find_nodes_by_type(node_type)
    }

    pub fn get_object_id(&self, node_id: NodeId) -> Option<ObjectId> {
        if let Some(node) = self.get_node(node_id) {
            if let NodeType::Object(obj_id) = &node.node_type {
                return Some(*obj_id);
            }
        }
        None
    }

    pub fn node_indices(&self) -> Vec<NodeId> {
        self.node_indices_with_budget(&ResourceBudget::default())
            .unwrap_or_default()
    }

    pub fn node_indices_with_budget(
        &self,
        budget: &ResourceBudget,
    ) -> Result<Vec<NodeId>, ResourceBudgetError> {
        let mut nodes = Vec::new();
        for node_id in self.node_map.keys().copied() {
            budget.consume_node()?;
            nodes.push(node_id);
        }
        Ok(nodes)
    }

    pub fn get_path_to_root(&self, node_id: NodeId) -> Vec<NodeId> {
        self.get_path_to_root_with_budget(node_id, &ResourceBudget::default())
            .unwrap_or_default()
    }

    pub fn get_path_to_root_with_budget(
        &self,
        node_id: NodeId,
        budget: &ResourceBudget,
    ) -> Result<Vec<NodeId>, ResourceBudgetError> {
        let mut path = Vec::new();
        let mut current = node_id;
        let mut visited = HashSet::new();

        while visited.insert(current) {
            budget.consume_node()?;
            path.push(current);
            if let Some(parent) = self.get_parent(current) {
                current = parent;
            } else {
                break;
            }
        }
        path.reverse();
        Ok(path)
    }

    pub fn get_page_number(&self, node_id: NodeId) -> Option<usize> {
        self.get_page_number_with_budget(node_id, &ResourceBudget::default())
            .ok()
            .flatten()
    }

    pub fn get_page_number_with_budget(
        &self,
        node_id: NodeId,
        budget: &ResourceBudget,
    ) -> Result<Option<usize>, ResourceBudgetError> {
        // Find the page node or its parent page
        let mut current = node_id;
        let mut page_node = None;
        let mut visited = HashSet::new();

        // Walk up the tree to find a Page node
        while visited.insert(current) {
            budget.consume_node()?;
            if let Some(node) = self.get_node(current) {
                if matches!(node.node_type, NodeType::Page) {
                    page_node = Some(current);
                    break;
                }
            }

            if let Some(parent) = self.get_parent(current) {
                current = parent;
            } else {
                break;
            }
        }

        // If we found a page node, count its position among siblings
        if let Some(page_id) = page_node {
            // Find the Pages parent
            if let Some(parent_id) = self.get_parent(page_id) {
                let siblings = self.get_children_with_budget(parent_id, budget)?;
                for (index, sibling) in siblings.iter().enumerate() {
                    if *sibling == page_id {
                        return Ok(Some(index + 1)); // Page numbers are 1-indexed
                    }
                }
            }
        }

        Ok(None)
    }

    pub fn raw_edges(&self) -> Vec<EdgeInfo> {
        self.raw_edges_with_budget(&ResourceBudget::default())
            .unwrap_or_default()
    }

    pub fn raw_edges_with_budget(
        &self,
        budget: &ResourceBudget,
    ) -> Result<Vec<EdgeInfo>, ResourceBudgetError> {
        self.get_all_edges_with_budget(budget)
    }

    /// Calculates the maximum depth of the graph from the root node.
    ///
    /// # Returns
    /// The longest path from root to any leaf node, or 0 if no root is set
    pub fn get_max_depth(&self) -> usize {
        self.get_max_depth_with_budget(&ResourceBudget::default())
            .unwrap_or_default()
    }

    pub fn get_max_depth_with_budget(
        &self,
        budget: &ResourceBudget,
    ) -> Result<usize, ResourceBudgetError> {
        self.root
            .map(|root_id| {
                self.calculate_depth_with_budget(root_id, 0, &mut HashSet::new(), budget)
            })
            .transpose()
            .map(|depth| depth.unwrap_or(0))
    }

    fn calculate_depth_with_budget(
        &self,
        node_id: NodeId,
        current_depth: usize,
        active: &mut HashSet<NodeId>,
        budget: &ResourceBudget,
    ) -> Result<usize, ResourceBudgetError> {
        budget.consume_node()?;
        if !active.insert(node_id) {
            return Ok(current_depth);
        }

        let children = self.get_children_with_budget(node_id, budget)?;
        let depth = if children.is_empty() {
            current_depth
        } else {
            let mut max_depth = current_depth;
            for child in children {
                max_depth = max_depth.max(self.calculate_depth_with_budget(
                    child,
                    current_depth + 1,
                    active,
                    budget,
                )?);
            }
            max_depth
        };
        active.remove(&node_id);
        Ok(depth)
    }

    pub fn next_node_id(&mut self) -> NodeId {
        let id = NodeId(self.next_node_id);
        self.next_node_id += 1;
        id
    }

    pub fn get_edges_from(&self, node_id: NodeId) -> Vec<EdgeInfo> {
        self.get_edges_from_with_budget(node_id, &ResourceBudget::default())
            .unwrap_or_default()
    }

    pub fn get_edges_from_with_budget(
        &self,
        node_id: NodeId,
        budget: &ResourceBudget,
    ) -> Result<Vec<EdgeInfo>, ResourceBudgetError> {
        let mut edges = Vec::new();
        if let Some(&index) = self.node_map.get(&node_id) {
            let mut node_id_reverse_map = HashMap::new();
            for (&id, &index) in &self.node_map {
                budget.consume_node()?;
                node_id_reverse_map.insert(index, id);
            }

            for edge in self.graph.edges(index) {
                if let Some(&target_id) = node_id_reverse_map.get(&edge.target()) {
                    budget.consume_edge()?;
                    edges.push(EdgeInfo {
                        from: node_id,
                        to: target_id,
                        edge_type: *edge.weight(),
                    });
                }
            }
        }
        Ok(edges)
    }
}

struct CallbackVisitor<'a, F> {
    callback: &'a mut F,
}

impl<'a, F> Visitor for CallbackVisitor<'a, F>
where
    F: FnMut(&AstNode),
{
    fn visit_node(&mut self, node: &AstNode) -> crate::visitor::VisitorAction {
        (self.callback)(node);
        crate::visitor::VisitorAction::Continue
    }
}

impl Default for PdfAstGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_queries_terminate_on_child_cycles() {
        let mut graph = PdfAstGraph::new();
        let first = graph.add_node(AstNode::new(NodeId(0), NodeType::Unknown, PdfValue::Null));
        let second = graph.add_node(AstNode::new(NodeId(1), NodeType::Page, PdfValue::Null));
        graph.add_edge(first, second, EdgeType::Child);
        graph.add_edge(second, first, EdgeType::Child);
        graph.set_root(first);

        assert_eq!(graph.get_max_depth(), 2);
        assert_eq!(graph.get_path_to_root(second), vec![first, second]);
        assert_eq!(graph.get_page_number(first), Some(1));
    }

    #[test]
    fn graph_collection_queries_report_budget_exhaustion() {
        let mut graph = PdfAstGraph::new();
        graph.add_node(AstNode::new(NodeId(0), NodeType::Unknown, PdfValue::Null));
        let budget = ResourceBudget::new(1024, 1024, 1024, 100, 10, 0, 10, 10);

        assert_eq!(
            graph
                .get_all_nodes_with_budget(&budget)
                .expect_err("graph node collection must respect the node budget"),
            ResourceBudgetError::Nodes
        );
    }

    #[test]
    fn graph_depth_queries_report_budget_exhaustion() {
        let mut graph = PdfAstGraph::new();
        let root = graph.add_node(AstNode::new(NodeId(0), NodeType::Page, PdfValue::Null));
        graph.set_root(root);
        let budget = ResourceBudget::new(1024, 1024, 1024, 100, 10, 0, 10, 10);

        assert_eq!(
            graph
                .get_max_depth_with_budget(&budget)
                .expect_err("depth traversal must respect the node budget"),
            ResourceBudgetError::Nodes
        );
        assert_eq!(
            graph
                .get_page_number_with_budget(root, &budget)
                .expect_err("page traversal must respect the node budget"),
            ResourceBudgetError::Nodes
        );
    }
}
