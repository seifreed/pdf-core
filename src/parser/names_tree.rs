use crate::ast::document::{NameTree, NameTreeNode};
use crate::ast::{AstNode, NodeId, NodeType, PdfAstGraph};
use crate::parser::reference_resolver::ObjectNodeMap;
use crate::performance::ResourceBudget;
use crate::types::{PdfArray, PdfDictionary, PdfValue};
use std::collections::{BTreeMap, HashSet};

pub struct NameTreeParser<'a> {
    ast: &'a mut PdfAstGraph,
    resolver: &'a ObjectNodeMap,
    budget: ResourceBudget,
}

impl<'a> NameTreeParser<'a> {
    pub fn new(ast: &'a mut PdfAstGraph, resolver: &'a ObjectNodeMap) -> Self {
        Self::new_with_budget(ast, resolver, &ResourceBudget::default())
    }

    pub fn new_with_budget(
        ast: &'a mut PdfAstGraph,
        resolver: &'a ObjectNodeMap,
        budget: &ResourceBudget,
    ) -> Self {
        NameTreeParser {
            ast,
            resolver,
            budget: budget.clone(),
        }
    }

    pub fn parse_names_dictionary(&mut self, names_dict: &PdfDictionary) -> NameTree {
        NameTree {
            dests: self.parse_tree_entry(names_dict, "Dests"),
            embedded_files: self.parse_tree_entry(names_dict, "EmbeddedFiles"),
            javascript: self.parse_tree_entry(names_dict, "JavaScript"),
            pages: self.parse_tree_entry(names_dict, "Pages"),
            templates: self.parse_tree_entry(names_dict, "Templates"),
            alternate_presentations: self.parse_tree_entry(names_dict, "AlternatePresentations"),
            renditions: self.parse_tree_entry(names_dict, "Renditions"),
        }
    }

    fn parse_tree_entry(&mut self, dict: &PdfDictionary, key: &str) -> Option<NameTreeNode> {
        match dict.get(key) {
            Some(PdfValue::Dictionary(tree_dict)) => Some(self.parse_name_tree_node(tree_dict)),
            Some(PdfValue::Reference(obj_id)) => {
                if let Some(node_id) = self.resolver.get_node_id(&obj_id.id()) {
                    if let Some(node) = self.ast.get_node(node_id) {
                        if let Some(tree_dict) = node.as_dict() {
                            let tree_dict = tree_dict.clone();
                            return Some(self.parse_name_tree_node(&tree_dict));
                        }
                    }
                }
                None
            }
            _ => None,
        }
    }

    fn parse_name_tree_node(&mut self, node_dict: &PdfDictionary) -> NameTreeNode {
        let mut tree_node = NameTreeNode {
            names: Vec::new(),
            kids: Vec::new(),
            limits: None,
        };

        // Parse limits (range of names in this subtree)
        if let Some(PdfValue::Array(limits)) = node_dict.get("Limits") {
            if limits.len() >= 2 {
                let min = self.extract_string(&limits[0]);
                let max = self.extract_string(&limits[1]);
                if let (Some(min_str), Some(max_str)) = (min, max) {
                    tree_node.limits = Some((min_str, max_str));
                }
            }
        }

        // Parse names array (leaf node)
        if let Some(PdfValue::Array(names)) = node_dict.get("Names") {
            tree_node.names = self.parse_names_array(names);
        }

        // Parse kids array (intermediate node)
        if let Some(PdfValue::Array(kids)) = node_dict.get("Kids") {
            for kid_ref in kids.iter() {
                if let PdfValue::Reference(obj_id) = kid_ref {
                    if let Some(kid_node_id) = self.resolver.get_node_id(&obj_id.id()) {
                        tree_node.kids.push(kid_node_id);

                        // Mark kid as NameTree node
                        if let Some(kid_node) = self.ast.get_node_mut(kid_node_id) {
                            kid_node.node_type = NodeType::NameTree;
                        }
                    }
                }
            }
        }

        tree_node
    }

    fn parse_names_array(&mut self, names: &PdfArray) -> Vec<(String, NodeId)> {
        let mut result = Vec::new();
        let mut i = 0;

        while i + 1 < names.len() {
            if let Some(name) = self.extract_string(&names[i]) {
                let value_node_id = self.process_name_value(&names[i + 1], &name);
                result.push((name, value_node_id));
            }
            i += 2;
        }

        result
    }

    fn process_name_value(&mut self, value: &PdfValue, name: &str) -> NodeId {
        match value {
            PdfValue::Reference(obj_id) => {
                if let Some(node_id) = self.resolver.get_node_id(&obj_id.id()) {
                    // Update node type based on context
                    if let Some(node) = self.ast.get_node_mut(node_id) {
                        // Inline the update logic to avoid borrowing conflict
                        if name.starts_with("JavaScript") || name.contains(".js") {
                            if node.node_type == NodeType::Unknown {
                                node.node_type = NodeType::EmbeddedJS;
                            }
                        } else if (name.ends_with(".pdf") || name.contains("embed"))
                            && node.node_type == NodeType::Unknown
                        {
                            node.node_type = NodeType::EmbeddedFile;
                        }
                    }
                    node_id
                } else {
                    // Create placeholder node for unresolved reference
                    self.create_placeholder_node(value.clone(), name)
                }
            }
            PdfValue::Dictionary(dict) => {
                // Create inline dictionary node
                let node_type = self.determine_node_type_from_dict(dict, name);
                let node_id = self.ast.next_node_id();
                let node = AstNode::new(node_id, node_type, PdfValue::Dictionary(dict.clone()));
                self.ast.add_node(node)
            }
            PdfValue::Array(arr) => {
                // Create destination array node
                let node_id = self.ast.next_node_id();
                let node = AstNode::new(node_id, NodeType::Unknown, PdfValue::Array(arr.clone()));
                self.ast.add_node(node)
            }
            _ => {
                // Create node for other value types
                self.create_placeholder_node(value.clone(), name)
            }
        }
    }

    #[allow(dead_code)]
    fn update_node_type_from_context(&self, node: &mut AstNode, name: &str) {
        // Update node type based on name context
        if name.starts_with("JavaScript") || name.contains(".js") {
            if node.node_type == NodeType::Unknown {
                node.node_type = NodeType::EmbeddedJS;
            }
        } else if (name.ends_with(".pdf") || name.contains("embed"))
            && node.node_type == NodeType::Unknown
        {
            node.node_type = NodeType::EmbeddedFile;
        }

        // Add name as metadata
        node.metadata
            .set_property("named_reference".to_string(), name.to_string());
    }

    fn determine_node_type_from_dict(&self, dict: &PdfDictionary, name: &str) -> NodeType {
        // Check for type entry
        if let Some(PdfValue::Name(type_name)) = dict.get("Type") {
            match type_name.without_slash() {
                "Action" => {
                    if let Some(PdfValue::Name(s)) = dict.get("S") {
                        match s.without_slash() {
                            "JavaScript" => NodeType::JavaScriptAction,
                            "GoTo" => NodeType::GoToAction,
                            "URI" => NodeType::URIAction,
                            "Launch" => NodeType::LaunchAction,
                            "SubmitForm" => NodeType::SubmitFormAction,
                            _ => NodeType::Action,
                        }
                    } else {
                        NodeType::Action
                    }
                }
                "Filespec" => NodeType::EmbeddedFile,
                _ => NodeType::Unknown,
            }
        } else if name.starts_with("JavaScript") {
            NodeType::EmbeddedJS
        } else {
            NodeType::Unknown
        }
    }

    fn create_placeholder_node(&mut self, value: PdfValue, name: &str) -> NodeId {
        let node_id = self.ast.next_node_id();
        let mut node = AstNode::new(node_id, NodeType::Unknown, value);
        node.metadata
            .set_property("named_reference".to_string(), name.to_string());
        self.ast.add_node(node)
    }

    fn extract_string(&self, value: &PdfValue) -> Option<String> {
        match value {
            PdfValue::String(s) => Some(s.to_string_lossy()),
            PdfValue::Name(n) => Some(n.without_slash().to_string()),
            _ => None,
        }
    }

    pub fn collect_all_names(&mut self, tree_node: &NameTreeNode) -> BTreeMap<String, NodeId> {
        let mut all_names = BTreeMap::new();
        self.collect_names_recursive(tree_node, &mut all_names, &mut HashSet::new(), 0);
        all_names
    }

    fn collect_names_recursive(
        &mut self,
        tree_node: &NameTreeNode,
        all_names: &mut BTreeMap<String, NodeId>,
        active: &mut HashSet<NodeId>,
        depth: usize,
    ) {
        // Add direct names
        for (name, node_id) in &tree_node.names {
            all_names.insert(name.clone(), *node_id);
        }

        if depth >= self.budget.max_depth {
            return;
        }

        for kid_id in tree_node.kids.clone() {
            if !active.insert(kid_id) {
                continue;
            }
            if let Some(kid_dict) = self
                .ast
                .get_node(kid_id)
                .and_then(|kid_node| kid_node.as_dict())
                .cloned()
            {
                let kid_tree = self.parse_name_tree_node(&kid_dict);
                self.collect_names_recursive(&kid_tree, all_names, active, depth + 1);
            }
            active.remove(&kid_id);
        }
    }

    pub fn find_name(&mut self, tree_node: &NameTreeNode, target: &str) -> Option<NodeId> {
        self.find_name_recursive(tree_node, target, &mut HashSet::new(), 0)
    }

    fn find_name_recursive(
        &mut self,
        tree_node: &NameTreeNode,
        target: &str,
        active: &mut HashSet<NodeId>,
        depth: usize,
    ) -> Option<NodeId> {
        // Check limits first for early termination
        if let Some((min, max)) = &tree_node.limits {
            if target < min.as_str() || target > max.as_str() {
                return None;
            }
        }

        // Search in direct names
        for (name, node_id) in &tree_node.names {
            if name == target {
                return Some(*node_id);
            }
        }

        if depth >= self.budget.max_depth {
            return None;
        }

        for kid_id in tree_node.kids.clone() {
            if !active.insert(kid_id) {
                continue;
            }
            let result = self
                .ast
                .get_node(kid_id)
                .and_then(|kid_node| kid_node.as_dict())
                .cloned()
                .and_then(|kid_dict| {
                    let kid_tree = self.parse_name_tree_node(&kid_dict);
                    self.find_name_recursive(&kid_tree, target, active, depth + 1)
                });
            active.remove(&kid_id);
            if result.is_some() {
                return result;
            }
        }

        None
    }

    pub fn parse_javascript_names(&mut self, tree_node: &NameTreeNode) -> Vec<(String, String)> {
        let mut js_entries = Vec::new();

        for (name, node_id) in &tree_node.names {
            if let Some(node) = self.ast.get_node(*node_id) {
                match &node.value {
                    PdfValue::Stream(stream) => {
                        // JavaScript code in stream
                        if let Ok(js_code) = stream.decode_with_budget(&self.budget) {
                            if let Ok(js_str) = String::from_utf8(js_code) {
                                js_entries.push((name.clone(), js_str));
                            }
                        }
                    }
                    PdfValue::Dictionary(dict) => {
                        // Check for JS entry in dictionary
                        if let Some(PdfValue::String(js)) = dict.get("JS") {
                            js_entries.push((name.clone(), js.to_string_lossy()));
                        } else if let Some(PdfValue::Stream(js_stream)) = dict.get("JS") {
                            if let Ok(js_code) = js_stream.decode_with_budget(&self.budget) {
                                if let Ok(js_str) = String::from_utf8(js_code) {
                                    js_entries.push((name.clone(), js_str));
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        js_entries
    }

    pub fn parse_embedded_files(&mut self, tree_node: &NameTreeNode) -> Vec<EmbeddedFileInfo> {
        let mut files = Vec::new();

        for (name, node_id) in &tree_node.names {
            if let Some(node) = self.ast.get_node(*node_id) {
                if let Some(filespec_dict) = node.as_dict() {
                    let mut file_info = EmbeddedFileInfo {
                        name: name.clone(),
                        description: None,
                        mime_type: None,
                        size: None,
                        creation_date: None,
                        modification_date: None,
                        checksum: None,
                        stream_node_id: None,
                    };

                    // Extract description
                    if let Some(PdfValue::String(desc)) = filespec_dict.get("Desc") {
                        file_info.description = Some(desc.to_string_lossy());
                    }

                    // Extract embedded file stream
                    if let Some(PdfValue::Dictionary(ef_dict)) = filespec_dict.get("EF") {
                        if let Some(PdfValue::Reference(stream_ref)) = ef_dict.get("F") {
                            if let Some(stream_node_id) =
                                self.resolver.get_node_id(&stream_ref.id())
                            {
                                file_info.stream_node_id = Some(stream_node_id);

                                // Extract params from stream dictionary
                                if let Some(stream_node) = self.ast.get_node(stream_node_id) {
                                    if let Some(stream) = stream_node.as_stream() {
                                        if let Some(params) =
                                            stream.dict.get("Params").and_then(|v| v.as_dict())
                                        {
                                            if let Some(PdfValue::Integer(size)) =
                                                params.get("Size")
                                            {
                                                file_info.size = Some(*size as u64);
                                            }

                                            if let Some(PdfValue::String(date)) =
                                                params.get("CreationDate")
                                            {
                                                file_info.creation_date =
                                                    Some(date.to_string_lossy());
                                            }

                                            if let Some(PdfValue::String(date)) =
                                                params.get("ModDate")
                                            {
                                                file_info.modification_date =
                                                    Some(date.to_string_lossy());
                                            }

                                            if let Some(PdfValue::String(checksum)) =
                                                params.get("CheckSum")
                                            {
                                                file_info.checksum = Some(
                                                    checksum
                                                        .as_bytes()
                                                        .iter()
                                                        .map(|b| format!("{:02x}", b))
                                                        .collect::<String>(),
                                                );
                                            }
                                        }

                                        // Extract MIME type from subtype
                                        if let Some(PdfValue::Name(subtype)) =
                                            stream.dict.get("Subtype")
                                        {
                                            file_info.mime_type =
                                                Some(subtype.without_slash().to_string());
                                        }
                                    }
                                }
                            }
                        }
                    }

                    files.push(file_info);
                }
            }
        }

        files
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{AstNode, NodeId};
    use crate::types::PdfReference;

    #[test]
    fn javascript_names_use_the_shared_decode_budget() {
        let mut ast = PdfAstGraph::new();
        let node_id = ast.add_node(AstNode::new(
            NodeId(0),
            NodeType::Unknown,
            PdfValue::Stream(crate::types::PdfStream::new(
                PdfDictionary::new(),
                b"alert".to_vec(),
            )),
        ));
        let tree = NameTreeNode {
            names: vec![("script".to_string(), node_id)],
            kids: Vec::new(),
            limits: None,
        };
        let resolver = ObjectNodeMap::new();
        let budget = ResourceBudget::new(1024, 4, 4, 10, 1, 1, 1, 1);
        let mut parser = NameTreeParser::new_with_budget(&mut ast, &resolver, &budget);

        assert!(parser.parse_javascript_names(&tree).is_empty());
    }

    #[test]
    fn name_tree_queries_terminate_on_cycles() {
        let mut ast = PdfAstGraph::new();
        let node_id = ast.add_node(AstNode::new(
            NodeId(0),
            NodeType::NameTree,
            PdfValue::Dictionary({
                let mut dict = PdfDictionary::new();
                dict.insert(
                    "Kids",
                    PdfValue::Array(vec![PdfValue::Reference(PdfReference::new(1, 0))].into()),
                );
                dict
            }),
        ));
        let mut resolver = ObjectNodeMap::new();
        resolver.insert(crate::types::ObjectId::new(1, 0), node_id);
        let mut parser = NameTreeParser::new(&mut ast, &resolver);
        let tree = NameTreeNode {
            names: Vec::new(),
            kids: vec![node_id],
            limits: None,
        };

        assert!(parser.collect_all_names(&tree).is_empty());
        assert_eq!(parser.find_name(&tree, "missing"), None);
    }
}

#[derive(Debug, Clone)]
pub struct EmbeddedFileInfo {
    pub name: String,
    pub description: Option<String>,
    pub mime_type: Option<String>,
    pub size: Option<u64>,
    pub creation_date: Option<String>,
    pub modification_date: Option<String>,
    pub checksum: Option<String>,
    pub stream_node_id: Option<NodeId>,
}
