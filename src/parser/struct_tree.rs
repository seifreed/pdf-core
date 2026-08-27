use crate::ast::document::{ParentTree, ParentTreeEntry, StructureTree};
use crate::ast::{AstNode, EdgeType, NodeId, NodeType, PdfAstGraph};
use crate::parser::reference_resolver::ObjectNodeMap;
use crate::performance::{ResourceBudget, ResourceBudgetError};
use crate::types::{ObjectId, PdfArray, PdfDictionary, PdfValue};
use std::collections::{HashMap, HashSet};

/// Parser for Tagged PDF structure tree
pub struct StructTreeParser<'a> {
    ast: &'a mut PdfAstGraph,
    resolver: &'a ObjectNodeMap,
    mcid_map: HashMap<(NodeId, i32), NodeId>, // (page_id, MCID) -> StructElem
    budget: ResourceBudget,
    active_elements: HashSet<NodeId>,
}

impl<'a> StructTreeParser<'a> {
    pub fn new(ast: &'a mut PdfAstGraph, resolver: &'a ObjectNodeMap) -> Self {
        Self::new_with_budget(ast, resolver, &ResourceBudget::default())
    }

    pub fn new_with_budget(
        ast: &'a mut PdfAstGraph,
        resolver: &'a ObjectNodeMap,
        budget: &ResourceBudget,
    ) -> Self {
        StructTreeParser {
            ast,
            resolver,
            mcid_map: HashMap::new(),
            budget: budget.clone(),
            active_elements: HashSet::new(),
        }
    }

    pub fn parse_struct_tree_root(&mut self, root_dict: &PdfDictionary) -> Option<StructureTree> {
        self.budget.consume_node().ok()?;
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
        self.parse_struct_elements_at_depth(root_dict, root_id, 0);

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

        if let Some(rm) = dict
            .get("RoleMap")
            .and_then(|value| self.resolve_dict(value))
        {
            for (key, value) in rm.iter() {
                if let Some(mapped) = self.resolve_name(value) {
                    role_map.insert(key.to_string(), mapped);
                }
            }
        }

        role_map
    }

    fn resolve_string(&self, value: &PdfValue) -> Option<String> {
        match value {
            PdfValue::String(string) => Some(string.to_string_lossy()),
            PdfValue::Reference(reference) => self
                .resolver
                .get_node_id(&reference.id())
                .and_then(|node_id| self.ast.get_node(node_id))
                .and_then(|node| node.value.as_string())
                .map(|string| string.to_string_lossy()),
            _ => None,
        }
    }

    fn resolve_array(&self, value: &PdfValue) -> Option<PdfArray> {
        match value {
            PdfValue::Array(array) => Some(array.clone()),
            PdfValue::Reference(reference) => self
                .resolver
                .get_node_id(&reference.id())
                .and_then(|node_id| self.ast.get_node(node_id))
                .and_then(|node| node.value.as_array())
                .cloned(),
            _ => None,
        }
    }

    fn resolve_dict(&self, value: &PdfValue) -> Option<PdfDictionary> {
        match value {
            PdfValue::Dictionary(dict) => Some(dict.clone()),
            PdfValue::Reference(reference) => self
                .resolver
                .get_node_id(&reference.id())
                .and_then(|node_id| self.ast.get_node(node_id))
                .and_then(|node| node.value.as_dict())
                .cloned(),
            _ => None,
        }
    }

    fn resolve_name(&self, value: &PdfValue) -> Option<String> {
        match value {
            PdfValue::Name(name) => Some(name.without_slash().to_string()),
            PdfValue::Reference(reference) => self
                .resolver
                .get_node_id(&reference.id())
                .and_then(|node_id| self.ast.get_node(node_id))
                .and_then(|node| node.value.as_name())
                .map(|name| name.without_slash().to_string()),
            _ => None,
        }
    }

    fn parse_class_map(&mut self, dict: &PdfDictionary) -> HashMap<String, NodeId> {
        let mut class_map = HashMap::new();

        if let Some(cm) = dict
            .get("ClassMap")
            .and_then(|value| self.resolve_dict(value))
        {
            for (key, value) in cm.iter() {
                match value {
                    PdfValue::Reference(obj_id) => {
                        if let Some(node_id) = self.resolver.get_node_id(&obj_id.id()) {
                            class_map.insert(key.to_string(), node_id);
                        }
                    }
                    PdfValue::Dictionary(d) => {
                        if self.budget.consume_node().is_err() {
                            continue;
                        }
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
        if let Some(nums_array) = tree_dict
            .get("Nums")
            .and_then(|value| self.resolve_array(value))
        {
            self.parse_parent_tree_nums(&nums_array, &mut parent_tree);
        }

        // Parse intermediate nodes recursively
        if let Some(kids_array) = tree_dict
            .get("Kids")
            .and_then(|value| self.resolve_array(value))
        {
            for kid in &kids_array {
                if let PdfValue::Reference(obj_id) = kid {
                    self.parse_parent_tree_intermediate(&obj_id.id(), &mut parent_tree);
                } else if let PdfValue::Dictionary(kid_dict) = kid {
                    self.parse_parent_tree_intermediate_dict(kid_dict, &mut parent_tree);
                }
            }
        }

        // Parse Limits to understand number range
        if let Some(limits) = tree_dict
            .get("Limits")
            .and_then(|value| self.resolve_array(value))
        {
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
        if let Some(nums_array) = dict.get("Nums").and_then(|value| self.resolve_array(value)) {
            self.parse_parent_tree_nums(&nums_array, parent_tree);
        }
    }

    fn parse_id_tree(
        &mut self,
        _dict: &PdfDictionary,
    ) -> Option<crate::ast::document::NameTreeNode> {
        // Similar to NameTree parsing
        None // Simplified for now
    }

    fn parse_struct_elements_at_depth(
        &mut self,
        dict: &PdfDictionary,
        parent_id: NodeId,
        depth: usize,
    ) {
        if depth >= self.budget.max_depth {
            return;
        }

        // Parse K (kids)
        match dict.get("K") {
            Some(PdfValue::Reference(obj_id)) => {
                if let Some(elem_id) = self.resolver.get_node_id(&obj_id.id()) {
                    self.parse_struct_elem(elem_id, parent_id, depth + 1);
                }
            }
            Some(PdfValue::Array(kids)) => {
                for kid in kids {
                    match kid {
                        PdfValue::Reference(obj_id) => {
                            if let Some(elem_id) = self.resolver.get_node_id(&obj_id.id()) {
                                self.parse_struct_elem(elem_id, parent_id, depth + 1);
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
                if self.budget.consume_node().is_err() {
                    return;
                }
                // Inline structure element
                let elem_node = AstNode::new(
                    self.ast.next_node_id(),
                    NodeType::StructElem,
                    PdfValue::Dictionary(elem_dict.clone()),
                );
                let elem_id = self.ast.add_node(elem_node);
                self.add_edge(parent_id, elem_id, EdgeType::Child);
                self.parse_struct_elem(elem_id, parent_id, depth + 1);
            }
            _ => {}
        }
    }

    fn parse_struct_elem(&mut self, elem_id: NodeId, parent_id: NodeId, depth: usize) {
        if depth >= self.budget.max_depth || !self.active_elements.insert(elem_id) {
            return;
        }

        // Update node type
        if let Some(node) = self.ast.get_node_mut(elem_id) {
            node.node_type = NodeType::StructElem;
        }

        // Add edge from parent
        self.add_edge(parent_id, elem_id, EdgeType::Child);

        // Get element dictionary
        let elem_dict = match self.ast.get_node(elem_id).and_then(|n| n.as_dict()) {
            Some(d) => d.clone(),
            None => {
                self.active_elements.remove(&elem_id);
                return;
            }
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
        if let Some(lang) = elem_dict
            .get("Lang")
            .and_then(|value| self.resolve_string(value))
        {
            if let Some(node) = self.ast.get_node_mut(elem_id) {
                node.metadata.set_property("language".to_string(), lang);
            }
        }

        // Extract Alt text
        if let Some(alt) = elem_dict
            .get("Alt")
            .and_then(|value| self.resolve_string(value))
        {
            if let Some(node) = self.ast.get_node_mut(elem_id) {
                node.metadata.set_property("alt_text".to_string(), alt);
            }
        }

        // Extract ActualText
        if let Some(actual) = elem_dict
            .get("ActualText")
            .and_then(|value| self.resolve_string(value))
        {
            if let Some(node) = self.ast.get_node_mut(elem_id) {
                node.metadata
                    .set_property("actual_text".to_string(), actual);
            }
        }

        // Parse Pg (page reference)
        if let Some(PdfValue::Reference(page_ref)) = elem_dict.get("Pg") {
            if let Some(page_id) = self.resolver.get_node_id(&page_ref.id()) {
                self.add_edge(elem_id, page_id, EdgeType::Reference);
            }
        }

        // Parse K (kids) recursively
        self.parse_struct_elements_at_depth(&elem_dict, elem_id, depth);
        self.active_elements.remove(&elem_id);
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
                        self.add_edge(parent_id, obj_id, EdgeType::Reference);
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
        if self.budget.consume_node().is_err() {
            return;
        }
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
        self.add_edge(parent_id, mcr_id, EdgeType::Child);
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
                    let text = String::from_utf8_lossy(text);
                    if self.budget.consume_object().is_ok()
                        && self.budget.consume_decoded(text.len() as u64).is_ok()
                    {
                        mcid_content.push(text.into_owned());
                    }
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
        if self.budget.consume_node().is_err() {
            return;
        }
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
        self.add_edge(struct_elem_id, content_id, EdgeType::Content);

        // Link content to page
        self.add_edge(page_id, content_id, EdgeType::Content);
    }

    fn add_edge(&mut self, from: NodeId, to: NodeId, edge_type: EdgeType) {
        if self.budget.consume_edge().is_ok() {
            self.ast.add_edge(from, to, edge_type);
        }
    }

    fn store_mcid_content(&mut self, page_id: NodeId, mcid: i32, content: &[String]) {
        // Store the actual text content associated with this MCID
        if let Some(struct_elem_id) = self.mcid_map.get(&(page_id, mcid)) {
            if let Some(node) = self.ast.get_node_mut(*struct_elem_id) {
                let output_bytes = content.iter().map(String::len).sum::<usize>()
                    + content.len().saturating_sub(1);
                if self.budget.consume_decoded(output_bytes as u64).is_err() {
                    return;
                }
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
        let _ = self.get_text_for_structure_into(struct_elem_id, &mut texts);
        texts
    }

    pub fn get_text_for_structure_with_budget(
        &self,
        struct_elem_id: NodeId,
    ) -> Result<Vec<String>, ResourceBudgetError> {
        let mut texts = Vec::new();
        self.get_text_for_structure_into(struct_elem_id, &mut texts)?;
        Ok(texts)
    }

    fn get_text_for_structure_into(
        &self,
        struct_elem_id: NodeId,
        texts: &mut Vec<String>,
    ) -> Result<(), ResourceBudgetError> {
        if let Some(node) = self.ast.get_node(struct_elem_id) {
            // Collect all MCID content
            for (key, value) in &node.metadata.properties {
                if key.starts_with("mcid_") && key.ends_with("_content") {
                    self.budget.consume_object()?;
                    self.budget.consume_decoded(value.len() as u64)?;
                    texts.push(value.clone());
                }
            }

            // Check for ActualText
            if let Some(actual) = node.metadata.get_property("actual_text") {
                self.budget.consume_object()?;
                self.budget.consume_decoded(actual.len() as u64)?;
                texts.push(actual.clone());
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ObjectId, PdfName, PdfReference};

    #[test]
    fn struct_tree_parser_terminates_on_cyclic_kids() {
        let mut ast = PdfAstGraph::new();
        let mut element_dict = PdfDictionary::new();
        element_dict.insert("S", PdfValue::Name(PdfName::new("P")));
        element_dict.insert("K", PdfValue::Reference(PdfReference::new(1, 0)));
        let element_id = ast.create_node(
            NodeType::Object(ObjectId::new(1, 0)),
            PdfValue::Dictionary(element_dict),
        );

        let mut root_dict = PdfDictionary::new();
        root_dict.insert("ParentTree", PdfValue::Dictionary(PdfDictionary::new()));
        root_dict.insert("K", PdfValue::Reference(PdfReference::new(1, 0)));
        let mut resolver = ObjectNodeMap::new();
        resolver.insert(ObjectId::new(1, 0), element_id);
        let budget = ResourceBudget::new(1024, 1024, 1024, 10, 10, 10, 10, 8);
        let mut parser = StructTreeParser::new_with_budget(&mut ast, &resolver, &budget);

        let tree = parser
            .parse_struct_tree_root(&root_dict)
            .expect("structure tree should parse");
        assert_eq!(tree.root, NodeId(1));
        drop(parser);
        assert_eq!(ast.node_count(), 2);
    }

    #[test]
    fn struct_tree_parser_respects_edge_budget() {
        let mut ast = PdfAstGraph::new();
        let mut root_dict = PdfDictionary::new();
        root_dict.insert("ParentTree", PdfValue::Dictionary(PdfDictionary::new()));
        root_dict.insert(
            "K",
            PdfValue::Array(PdfArray::from(vec![
                PdfValue::Integer(1),
                PdfValue::Integer(2),
                PdfValue::Integer(3),
            ])),
        );
        let resolver = ObjectNodeMap::new();
        let budget = ResourceBudget::new(1024, 1024, 1024, 10, 10, 10, 1, 8);
        let mut parser = StructTreeParser::new_with_budget(&mut ast, &resolver, &budget);

        parser
            .parse_struct_tree_root(&root_dict)
            .expect("structure tree should parse");
        assert_eq!(ast.edge_count(), 1);
    }

    #[test]
    fn struct_tree_parser_resolves_indirect_accessibility_strings() {
        let mut ast = PdfAstGraph::new();
        let element_id = ast.create_node(
            NodeType::Object(ObjectId::new(1, 0)),
            PdfValue::Dictionary({
                let mut dict = PdfDictionary::new();
                dict.insert("S", PdfValue::Name(PdfName::new("Figure")));
                dict.insert("Lang", PdfValue::Reference(PdfReference::new(2, 0)));
                dict.insert("Alt", PdfValue::Reference(PdfReference::new(3, 0)));
                dict.insert("ActualText", PdfValue::Reference(PdfReference::new(4, 0)));
                dict
            }),
        );
        let language_id = ast.create_node(
            NodeType::Object(ObjectId::new(2, 0)),
            PdfValue::String(crate::types::PdfString::new_literal(b"en-US")),
        );
        let alt_id = ast.create_node(
            NodeType::Object(ObjectId::new(3, 0)),
            PdfValue::String(crate::types::PdfString::new_literal(b"figure")),
        );
        let actual_id = ast.create_node(
            NodeType::Object(ObjectId::new(4, 0)),
            PdfValue::String(crate::types::PdfString::new_literal(b"actual")),
        );
        let role_map_id = ast.create_node(
            NodeType::Object(ObjectId::new(5, 0)),
            PdfValue::Dictionary({
                let mut dict = PdfDictionary::new();
                dict.insert("Figure", PdfValue::Reference(PdfReference::new(8, 0)));
                dict
            }),
        );
        let role_name_id = ast.create_node(
            NodeType::Object(ObjectId::new(8, 0)),
            PdfValue::Name(PdfName::new("Figure")),
        );
        let class_id = ast.create_node(
            NodeType::Object(ObjectId::new(7, 0)),
            PdfValue::Dictionary(PdfDictionary::new()),
        );
        let class_map_id = ast.create_node(
            NodeType::Object(ObjectId::new(6, 0)),
            PdfValue::Dictionary({
                let mut dict = PdfDictionary::new();
                dict.insert("Layout", PdfValue::Reference(PdfReference::new(7, 0)));
                dict
            }),
        );
        let mut resolver = ObjectNodeMap::new();
        resolver.insert(ObjectId::new(1, 0), element_id);
        resolver.insert(ObjectId::new(2, 0), language_id);
        resolver.insert(ObjectId::new(3, 0), alt_id);
        resolver.insert(ObjectId::new(4, 0), actual_id);
        resolver.insert(ObjectId::new(5, 0), role_map_id);
        resolver.insert(ObjectId::new(8, 0), role_name_id);
        resolver.insert(ObjectId::new(6, 0), class_map_id);
        resolver.insert(ObjectId::new(7, 0), class_id);

        let mut root_dict = PdfDictionary::new();
        root_dict.insert("ParentTree", PdfValue::Dictionary(PdfDictionary::new()));
        root_dict.insert("K", PdfValue::Reference(PdfReference::new(1, 0)));
        root_dict.insert("RoleMap", PdfValue::Reference(PdfReference::new(5, 0)));
        root_dict.insert("ClassMap", PdfValue::Reference(PdfReference::new(6, 0)));
        let mut parser = StructTreeParser::new(&mut ast, &resolver);
        let tree = parser
            .parse_struct_tree_root(&root_dict)
            .expect("structure tree should parse");

        let element = ast.get_node(element_id).expect("element should exist");
        assert_eq!(
            element
                .metadata
                .get_property("language")
                .map(String::as_str),
            Some("en-US")
        );
        assert_eq!(
            element
                .metadata
                .get_property("alt_text")
                .map(String::as_str),
            Some("figure")
        );
        assert_eq!(
            element
                .metadata
                .get_property("actual_text")
                .map(String::as_str),
            Some("actual")
        );
        assert_eq!(
            tree.role_map.get("/Figure").map(String::as_str),
            Some("Figure")
        );
        assert_eq!(tree.class_map.get("/Layout"), Some(&class_id));
    }

    #[test]
    fn struct_tree_parser_resolves_indirect_parent_tree_arrays() {
        let mut ast = PdfAstGraph::new();
        let element_id = ast.create_node(
            NodeType::Object(ObjectId::new(1, 0)),
            PdfValue::Dictionary({
                let mut dict = PdfDictionary::new();
                dict.insert("S", PdfValue::Name(PdfName::new("P")));
                dict
            }),
        );
        let nums_id = ast.create_node(
            NodeType::Object(ObjectId::new(3, 0)),
            PdfValue::Array(PdfArray::from(vec![
                PdfValue::Integer(7),
                PdfValue::Reference(PdfReference::new(1, 0)),
            ])),
        );
        let limits_id = ast.create_node(
            NodeType::Object(ObjectId::new(4, 0)),
            PdfValue::Array(PdfArray::from(vec![
                PdfValue::Integer(7),
                PdfValue::Integer(7),
            ])),
        );
        let parent_tree_id = ast.create_node(
            NodeType::Object(ObjectId::new(2, 0)),
            PdfValue::Dictionary({
                let mut dict = PdfDictionary::new();
                dict.insert("Nums", PdfValue::Reference(PdfReference::new(3, 0)));
                dict.insert("Limits", PdfValue::Reference(PdfReference::new(4, 0)));
                dict
            }),
        );
        let mut resolver = ObjectNodeMap::new();
        resolver.insert(ObjectId::new(1, 0), element_id);
        resolver.insert(ObjectId::new(2, 0), parent_tree_id);
        resolver.insert(ObjectId::new(3, 0), nums_id);
        resolver.insert(ObjectId::new(4, 0), limits_id);

        let mut root_dict = PdfDictionary::new();
        root_dict.insert("ParentTree", PdfValue::Reference(PdfReference::new(2, 0)));
        root_dict.insert("K", PdfValue::Reference(PdfReference::new(1, 0)));
        let mut parser = StructTreeParser::new(&mut ast, &resolver);
        let tree = parser
            .parse_struct_tree_root(&root_dict)
            .expect("structure tree should parse");

        assert!(matches!(
            tree.parent_tree.get_parents(7),
            Some(ParentTreeEntry::Single(id)) if *id == element_id
        ));
        assert_eq!(tree.parent_tree.limits, Some((7, 7)));
    }

    #[test]
    fn struct_tree_text_query_reports_budget_exhaustion() {
        let mut ast = PdfAstGraph::new();
        let mut node = AstNode::new(NodeId(0), NodeType::Unknown, PdfValue::Null);
        node.metadata
            .set_property("actual_text".to_string(), "text".to_string());
        ast.add_node(node);
        let resolver = ObjectNodeMap::new();
        let budget = ResourceBudget::new(1024, 1024, 1024, 100, 0, 10, 10, 10);
        let parser = StructTreeParser::new_with_budget(&mut ast, &resolver, &budget);

        assert_eq!(
            parser
                .get_text_for_structure_with_budget(NodeId(0))
                .expect_err("structure text must respect the object budget"),
            ResourceBudgetError::Objects
        );
    }
}
