use crate::ast::document::{ParentTree, ParentTreeEntry, StructureTree};
use crate::ast::{AstNode, EdgeType, NodeId, NodeType, PdfAstGraph};
use crate::parser::reference_resolver::ObjectNodeMap;
use crate::types::{ObjectId, PdfArray, PdfDictionary, PdfValue};
use std::collections::HashMap;

/// Parser for Tagged PDF structure tree
pub struct StructTreeParser<'a> {
    ast: &'a mut PdfAstGraph,
    resolver: &'a ObjectNodeMap,
    mcid_map: HashMap<(NodeId, i32), NodeId>, // (page_id, MCID) -> StructElem
}

impl<'a> StructTreeParser<'a> {
    pub fn new(ast: &'a mut PdfAstGraph, resolver: &'a ObjectNodeMap) -> Self {
        StructTreeParser {
            ast,
            resolver,
            mcid_map: HashMap::new(),
        }
    }

    pub fn parse_struct_tree_root(&mut self, root_dict: &PdfDictionary) -> Option<StructureTree> {
        // Create root node
        let root_node = AstNode::new(
            self.ast.next_node_id(),
            NodeType::StructTreeRoot,
            PdfValue::Dictionary(root_dict.clone()),
        );
        let root_id = self.ast.add_node(root_node);

        // Parse RoleMap
        let role_map = self.parse_role_map(root_dict);

        // Parse ClassMap
        let class_map = self.parse_class_map(root_dict);

        // Parse ParentTree
        let parent_tree = self.parse_parent_tree(root_dict)?;

        // Parse IDTree
        let id_tree = self.parse_id_tree(root_dict);

        // Parse K (kids) - the actual structure elements
        self.parse_struct_elements(root_dict, root_id);

        Some(StructureTree {
            root: root_id,
            role_map,
            class_map,
            parent_tree,
            id_tree,
        })
    }

    fn parse_role_map(&self, dict: &PdfDictionary) -> HashMap<String, String> {
        let mut role_map = HashMap::new();

        if let Some(PdfValue::Dictionary(rm)) = dict.get("RoleMap") {
            for (key, value) in rm.iter() {
                if let PdfValue::Name(mapped) = value {
                    role_map.insert(key.to_string(), mapped.without_slash().to_string());
                }
            }
        }

        role_map
    }

    fn parse_class_map(&mut self, dict: &PdfDictionary) -> HashMap<String, NodeId> {
        let mut class_map = HashMap::new();

        if let Some(PdfValue::Dictionary(cm)) = dict.get("ClassMap") {
            for (key, value) in cm.iter() {
                match value {
                    PdfValue::Reference(obj_id) => {
                        if let Some(node_id) = self.resolver.get_node_id(&obj_id.id()) {
                            class_map.insert(key.to_string(), node_id);
                        }
                    }
                    PdfValue::Dictionary(d) => {
                        // Create inline class node
                        let class_node = AstNode::new(
                            self.ast.next_node_id(),
                            NodeType::Unknown,
                            PdfValue::Dictionary(d.clone()),
                        );
                        let class_id = self.ast.add_node(class_node);
                        class_map.insert(key.to_string(), class_id);
                    }
                    _ => {}
                }
            }
        }

        class_map
    }

    fn parse_parent_tree(&mut self, dict: &PdfDictionary) -> Option<ParentTree> {
        let parent_tree_dict = match dict.get("ParentTree") {
            Some(PdfValue::Dictionary(d)) => d.clone(),
            Some(PdfValue::Reference(obj_id)) => {
                let node_id = self.resolver.get_node_id(&obj_id.id())?;
                let node = self.ast.get_node(node_id)?;
                if let PdfValue::Dictionary(d) = &node.value {
                    d.clone()
                } else {
                    return None;
                }
            }
            _ => return None,
        };

        self.parse_complete_parent_tree(&parent_tree_dict)
    }

    fn parse_complete_parent_tree(&mut self, tree_dict: &PdfDictionary) -> Option<ParentTree> {
        let mut parent_tree = ParentTree::new();

        // Parse Nums array for leaf entries
        if let Some(PdfValue::Array(nums_array)) = tree_dict.get("Nums") {
            self.parse_parent_tree_nums(nums_array, &mut parent_tree);
        }

        // Parse intermediate nodes recursively
        if let Some(PdfValue::Array(kids_array)) = tree_dict.get("Kids") {
            for kid in kids_array.iter() {
                if let PdfValue::Reference(obj_id) = kid {
                    self.parse_parent_tree_intermediate(&obj_id.id(), &mut parent_tree);
                } else if let PdfValue::Dictionary(kid_dict) = kid {
                    self.parse_parent_tree_intermediate_dict(kid_dict, &mut parent_tree);
                }
            }
        }

        // Parse Limits to understand number range
        if let Some(PdfValue::Array(limits)) = tree_dict.get("Limits") {
            if limits.len() >= 2 {
                if let (Some(PdfValue::Integer(min)), Some(PdfValue::Integer(max))) =
                    (limits.get(0), limits.get(1))
                {
                    parent_tree.set_limits(*min, *max);
                }
            }
        }

        Some(parent_tree)
    }

    fn parse_parent_tree_nums(&mut self, nums_array: &PdfArray, parent_tree: &mut ParentTree) {
        // Nums array alternates between integers and values
        let mut i = 0;
        while i + 1 < nums_array.len() {
            if let (Some(PdfValue::Integer(page_obj_num)), Some(parent_value)) =
                (nums_array.get(i), nums_array.get(i + 1))
            {
                match parent_value {
                    PdfValue::Reference(parent_obj_id) => {
                        if let Some(parent_node_id) = self.resolver.get_node_id(&parent_obj_id.id())
                        {
                            parent_tree.add_parent_entry(
                                *page_obj_num as u32,
                                ParentTreeEntry::Single(parent_node_id),
                            );
                        }
                    }
                    PdfValue::Array(parent_array) => {
                        let mut parents = Vec::new();
                        for parent_ref in parent_array.iter() {
                            if let PdfValue::Reference(obj_id) = parent_ref {
                                if let Some(node_id) = self.resolver.get_node_id(&obj_id.id()) {
                                    parents.push(node_id);
                                }
                            }
                        }
                        if !parents.is_empty() {
                            parent_tree.add_parent_entry(
                                *page_obj_num as u32,
                                ParentTreeEntry::Multiple(parents),
                            );
                        }
                    }
                    _ => {}
                }
            }
            i += 2;
        }
    }

    fn parse_parent_tree_intermediate(&mut self, obj_id: &ObjectId, parent_tree: &mut ParentTree) {
        if let Some(node_id) = self.resolver.get_node_id(obj_id) {
            if let Some(node) = self.ast.get_node(node_id) {
                if let PdfValue::Dictionary(dict) = &node.value {
                    let dict_clone = dict.clone();
                    self.parse_parent_tree_intermediate_dict(&dict_clone, parent_tree);
                }
            }
        }
    }

    fn parse_parent_tree_intermediate_dict(
        &mut self,
        dict: &PdfDictionary,
        parent_tree: &mut ParentTree,
    ) {
        // Parse Nums array directly instead of recursive call
        if let Some(PdfValue::Array(nums_array)) = dict.get("Nums") {
            self.parse_parent_tree_nums(nums_array, parent_tree);
        }
    }

    fn parse_id_tree(
        &mut self,
        _dict: &PdfDictionary,
    ) -> Option<crate::ast::document::NameTreeNode> {
        // Similar to NameTree parsing
        None // Simplified for now
    }

    fn parse_struct_elements(&mut self, dict: &PdfDictionary, parent_id: NodeId) {
        // Parse K (kids)
        match dict.get("K") {
            Some(PdfValue::Reference(obj_id)) => {
                if let Some(elem_id) = self.resolver.get_node_id(&obj_id.id()) {
                    self.parse_struct_elem(elem_id, parent_id);
                }
            }
            Some(PdfValue::Array(kids)) => {
                for kid in kids {
                    match kid {
                        PdfValue::Reference(obj_id) => {
                            if let Some(elem_id) = self.resolver.get_node_id(&obj_id.id()) {
                                self.parse_struct_elem(elem_id, parent_id);
                            }
                        }
                        PdfValue::Integer(mcid) => {
                            // Direct MCID reference
                            self.create_mcr_node(*mcid as i32, parent_id);
                        }
                        PdfValue::Dictionary(mcr_dict) => {
                            // MCR or OBJR dictionary
                            self.parse_content_reference(mcr_dict, parent_id);
                        }
                        _ => {}
                    }
                }
            }
            Some(PdfValue::Dictionary(elem_dict)) => {
                // Inline structure element
                let elem_node = AstNode::new(
                    self.ast.next_node_id(),
                    NodeType::StructElem,
                    PdfValue::Dictionary(elem_dict.clone()),
                );
                let elem_id = self.ast.add_node(elem_node);
                self.ast.add_edge(parent_id, elem_id, EdgeType::Child);
                self.parse_struct_elem(elem_id, parent_id);
            }
            _ => {}
        }
    }

    fn parse_struct_elem(&mut self, elem_id: NodeId, parent_id: NodeId) {
        // Update node type
        if let Some(node) = self.ast.get_node_mut(elem_id) {
            node.node_type = NodeType::StructElem;
        }

        // Add edge from parent
        self.ast.add_edge(parent_id, elem_id, EdgeType::Child);

        // Get element dictionary
        let elem_dict = match self.ast.get_node(elem_id).and_then(|n| n.as_dict()) {
            Some(d) => d.clone(),
            None => return,
        };

        // Extract structure type
        if let Some(PdfValue::Name(s_type)) = elem_dict.get("S") {
            if let Some(node) = self.ast.get_node_mut(elem_id) {
                node.metadata.set_property(
                    "struct_type".to_string(),
                    s_type.without_slash().to_string(),
                );
            }
        }

        // Extract language
        if let Some(PdfValue::String(lang)) = elem_dict.get("Lang") {
            if let Some(node) = self.ast.get_node_mut(elem_id) {
                node.metadata
                    .set_property("language".to_string(), lang.to_string_lossy());
            }
        }

        // Extract Alt text
        if let Some(PdfValue::String(alt)) = elem_dict.get("Alt") {
            if let Some(node) = self.ast.get_node_mut(elem_id) {
                node.metadata
                    .set_property("alt_text".to_string(), alt.to_string_lossy());
            }
        }

        // Extract ActualText
        if let Some(PdfValue::String(actual)) = elem_dict.get("ActualText") {
            if let Some(node) = self.ast.get_node_mut(elem_id) {
                node.metadata
                    .set_property("actual_text".to_string(), actual.to_string_lossy());
            }
        }

        // Parse Pg (page reference)
        if let Some(PdfValue::Reference(page_ref)) = elem_dict.get("Pg") {
            if let Some(page_id) = self.resolver.get_node_id(&page_ref.id()) {
                self.ast.add_edge(elem_id, page_id, EdgeType::Reference);
            }
        }

        // Parse K (kids) recursively
        self.parse_struct_elements(&elem_dict, elem_id);
    }

    fn parse_content_reference(&mut self, mcr_dict: &PdfDictionary, parent_id: NodeId) {
        // Determine type
        let ref_type = mcr_dict
            .get("Type")
            .and_then(|v| match v {
                PdfValue::Name(n) => Some(n.without_slash()),
                _ => None,
            })
            .unwrap_or("MCR");

        match ref_type {
            "MCR" => {
                // Marked Content Reference
                if let Some(PdfValue::Integer(mcid)) = mcr_dict.get("MCID") {
                    self.create_mcr_node(*mcid as i32, parent_id);
                }
            }
            "OBJR" => {
                // Object Reference
                if let Some(PdfValue::Reference(obj_ref)) = mcr_dict.get("Obj") {
                    if let Some(obj_id) = self.resolver.get_node_id(&obj_ref.id()) {
                        self.ast.add_edge(parent_id, obj_id, EdgeType::Reference);
                    }
                }
            }
            _ => {}
        }

        // Store page reference if present
        if let Some(PdfValue::Reference(page_ref)) = mcr_dict.get("Pg") {
            if let Some(page_id) = self.resolver.get_node_id(&page_ref.id()) {
                if let Some(PdfValue::Integer(mcid)) = mcr_dict.get("MCID") {
                    // Map MCID to structure element
                    self.mcid_map.insert((page_id, *mcid as i32), parent_id);
                }
            }
        }
    }

    fn create_mcr_node(&mut self, mcid: i32, parent_id: NodeId) {
        // Create MCR node
        let mut mcr_node = AstNode::new(
            self.ast.next_node_id(),
            NodeType::Unknown,
            PdfValue::Integer(mcid as i64),
        );

        mcr_node
            .metadata
            .set_property("mcr_type".to_string(), "MCID".to_string());
        mcr_node
            .metadata
            .set_property("mcid".to_string(), mcid.to_string());

        let mcr_id = self.ast.add_node(mcr_node);
        self.ast.add_edge(parent_id, mcr_id, EdgeType::Child);
    }

    #[allow(dead_code)]
    fn build_mcid_mappings(&mut self, _page_index: u32, _struct_elem_id: NodeId) {
        // This would need to parse the structure element to find MCIDs
        // For now, simplified
    }

    pub fn link_content_to_structure(
        &mut self,
        page_id: NodeId,
        operators: &[crate::parser::content_stream::ContentOperator],
    ) {
        let mut current_mcid: Option<i32> = None;
        let mut mcid_content: Vec<String> = Vec::new();

        for op in operators {
            match op {
                crate::parser::content_stream::ContentOperator::BeginMarkedContent(tag) => {
                    // Check if this is a structure tag
                    if self.is_structure_tag(tag) {
                        // Start collecting content for this marked content
                        mcid_content.clear();
                    }
                }
                crate::parser::content_stream::ContentOperator::BeginMarkedContentWithProps(
                    _tag,
                    crate::parser::content_stream::MarkedContentProps::Dictionary(dict),
                ) => {
                    // Extract MCID from properties
                    if let Some(PdfValue::Integer(mcid)) = dict.get("MCID") {
                        current_mcid = Some(*mcid as i32);

                        // Link to structure element
                        if let Some(struct_elem) = self.mcid_map.get(&(page_id, *mcid as i32)) {
                            // Create edge from content to structure
                            self.create_mcid_content_node(page_id, *mcid as i32, *struct_elem);
                        }
                    }
                }
                crate::parser::content_stream::ContentOperator::BeginMarkedContentWithProps(
                    _tag,
                    _props,
                ) => {
                    // Handle non-dictionary properties (no MCID extraction possible)
                }
                crate::parser::content_stream::ContentOperator::EndMarkedContent => {
                    if let Some(mcid) = current_mcid {
                        // Store collected content
                        self.store_mcid_content(page_id, mcid, &mcid_content);
                        current_mcid = None;
                        mcid_content.clear();
                    }
                }
                crate::parser::content_stream::ContentOperator::ShowText(text)
                    if current_mcid.is_some() =>
                {
                    mcid_content.push(String::from_utf8_lossy(text).to_string());
                }
                _ => {}
            }
        }
    }

    fn is_structure_tag(&self, tag: &str) -> bool {
        // Standard structure types
        matches!(
            tag,
            "Document"
                | "Part"
                | "Art"
                | "Sect"
                | "Div"
                | "BlockQuote"
                | "Caption"
                | "TOC"
                | "TOCI"
                | "Index"
                | "NonStruct"
                | "Private"
                | "P"
                | "H"
                | "H1"
                | "H2"
                | "H3"
                | "H4"
                | "H5"
                | "H6"
                | "L"
                | "LI"
                | "Lbl"
                | "LBody"
                | "Table"
                | "TR"
                | "TH"
                | "TD"
                | "THead"
                | "TBody"
                | "TFoot"
                | "Span"
                | "Quote"
                | "Note"
                | "Reference"
                | "BibEntry"
                | "Code"
                | "Link"
                | "Annot"
                | "Ruby"
                | "Warichu"
                | "WT"
                | "WP"
                | "Figure"
                | "Formula"
                | "Form"
        )
    }

    fn create_mcid_content_node(&mut self, page_id: NodeId, mcid: i32, struct_elem_id: NodeId) {
        // Create a node representing the content with this MCID
        let mut content_node =
            AstNode::new(self.ast.next_node_id(), NodeType::Unknown, PdfValue::Null);

        content_node
            .metadata
            .set_property("content_type".to_string(), "MCID".to_string());
        content_node
            .metadata
            .set_property("mcid".to_string(), mcid.to_string());
        content_node
            .metadata
            .set_property("page".to_string(), format!("{:?}", page_id));

        let content_id = self.ast.add_node(content_node);

        // Link content to structure element
        self.ast
            .add_edge(struct_elem_id, content_id, EdgeType::Content);

        // Link content to page
        self.ast.add_edge(page_id, content_id, EdgeType::Content);
    }

    fn store_mcid_content(&mut self, page_id: NodeId, mcid: i32, content: &[String]) {
        // Store the actual text content associated with this MCID
        if let Some(struct_elem_id) = self.mcid_map.get(&(page_id, mcid)) {
            if let Some(node) = self.ast.get_node_mut(*struct_elem_id) {
                let text = content.join(" ");
                node.metadata
                    .set_property(format!("mcid_{}_content", mcid), text);
            }
        }
    }

    pub fn get_structure_for_mcid(&self, page_id: NodeId, mcid: i32) -> Option<NodeId> {
        self.mcid_map.get(&(page_id, mcid)).copied()
    }

    pub fn get_text_for_structure(&self, struct_elem_id: NodeId) -> Vec<String> {
        let mut texts = Vec::new();

        if let Some(node) = self.ast.get_node(struct_elem_id) {
            // Collect all MCID content
            for (key, value) in &node.metadata.properties {
                if key.starts_with("mcid_") && key.ends_with("_content") {
                    texts.push(value.clone());
                }
            }

            // Check for ActualText
            if let Some(actual) = node.metadata.get_property("actual_text") {
                texts.push(actual.clone());
            }
        }

        texts
    }
}
