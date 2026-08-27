use crate::ast::document::{Destination, DestinationType, OutlineItem, OutlineTree};
use crate::ast::{NodeId, NodeType, PdfAstGraph};
use crate::parser::reference_resolver::ObjectNodeMap;
use crate::performance::{ResourceBudget, ResourceBudgetError};
use crate::types::{PdfArray, PdfDictionary, PdfValue};
use std::collections::{HashMap, HashSet};

pub struct OutlineParser<'a> {
    ast: &'a mut PdfAstGraph,
    resolver: &'a ObjectNodeMap,
    budget: ResourceBudget,
}

impl<'a> OutlineParser<'a> {
    pub fn new(ast: &'a mut PdfAstGraph, resolver: &'a ObjectNodeMap) -> Self {
        Self::new_with_budget(ast, resolver, &ResourceBudget::default())
    }

    pub fn new_with_budget(
        ast: &'a mut PdfAstGraph,
        resolver: &'a ObjectNodeMap,
        budget: &ResourceBudget,
    ) -> Self {
        OutlineParser {
            ast,
            resolver,
            budget: budget.clone(),
        }
    }

    pub fn resolve_page_numbers(&self, tree: &mut OutlineTree, page_list: &[NodeId]) {
        for item in tree.items.values_mut() {
            if let Some(Destination::Page(page_id) | Destination::Explicit { page: page_id, .. }) =
                &item.dest
            {
                // Find page index in document's page list
                if let Some(index) = page_list.iter().position(|p| p == page_id) {
                    // Store as metadata
                    item.count = index as i32; // Reuse count field or add new field
                }
            }
        }
    }

    pub fn parse_outline_tree(&mut self, outlines_dict: &PdfDictionary) -> Option<OutlineTree> {
        // Get first and last outline items
        let first_id = self.resolve_outline_reference(outlines_dict.get("First"))?;

        // Mark root as outline node
        if let Some(node) = self.ast.get_node_mut(first_id) {
            node.node_type = NodeType::Outline;
        }

        let mut tree = OutlineTree {
            root: first_id,
            items: HashMap::new(),
        };

        // Parse all outline items starting from first
        self.parse_outline_items(first_id, None, &mut tree);

        Some(tree)
    }

    fn parse_outline_items(
        &mut self,
        start_id: NodeId,
        parent_id: Option<NodeId>,
        tree: &mut OutlineTree,
    ) {
        let mut current_id = Some(start_id);

        while let Some(item_id) = current_id {
            if tree.items.contains_key(&item_id) {
                // Avoid infinite loops
                break;
            }

            let next_id = if let Some(node) = self.ast.get_node(item_id) {
                if let Some(item_dict) = node.as_dict() {
                    let item_dict = item_dict.clone();
                    let outline_item = self.parse_outline_item(&item_dict, item_id, parent_id);
                    let next = outline_item.next;

                    // Insert before descending so /First cycles are visible to recursive calls.
                    tree.items.insert(item_id, outline_item);

                    // Mark as outline item
                    if let Some(node) = self.ast.get_node_mut(item_id) {
                        node.node_type = NodeType::OutlineItem;
                    }

                    // Process children if present
                    if let Some(first_child) = tree.items.get(&item_id).and_then(|item| item.first)
                    {
                        self.parse_outline_items(first_child, Some(item_id), tree);
                    }

                    next
                } else {
                    None
                }
            } else {
                None
            };

            current_id = next_id;
        }
    }

    fn parse_outline_item(
        &mut self,
        dict: &PdfDictionary,
        _item_id: NodeId,
        parent_id: Option<NodeId>,
    ) -> OutlineItem {
        OutlineItem {
            title: self.extract_title(dict),
            dest: self.parse_destination(dict),
            action: self.resolve_outline_reference(dict.get("A")),
            parent: parent_id,
            prev: self.resolve_outline_reference(dict.get("Prev")),
            next: self.resolve_outline_reference(dict.get("Next")),
            first: self.resolve_outline_reference(dict.get("First")),
            last: self.resolve_outline_reference(dict.get("Last")),
            count: self.extract_count(dict),
            flags: self.extract_flags(dict),
            color: self.extract_color(dict),
        }
    }

    fn extract_title(&self, dict: &PdfDictionary) -> String {
        dict.get("Title")
            .and_then(|v| match v {
                PdfValue::String(s) => Some(s.to_string_lossy()),
                _ => None,
            })
            .unwrap_or_else(|| "Untitled".to_string())
    }

    fn extract_count(&self, dict: &PdfDictionary) -> i32 {
        dict.get("Count")
            .and_then(|v| match v {
                PdfValue::Integer(i) => Some(*i as i32),
                _ => None,
            })
            .unwrap_or(0)
    }

    fn extract_flags(&self, dict: &PdfDictionary) -> u32 {
        dict.get("F")
            .and_then(|v| match v {
                PdfValue::Integer(i) => Some(*i as u32),
                _ => None,
            })
            .unwrap_or(0)
    }

    fn extract_color(&self, dict: &PdfDictionary) -> Option<[f32; 3]> {
        dict.get("C").and_then(|v| match v {
            PdfValue::Array(arr) if arr.len() >= 3 => {
                let r = self.extract_float(&arr[0])?;
                let g = self.extract_float(&arr[1])?;
                let b = self.extract_float(&arr[2])?;
                Some([r, g, b])
            }
            _ => None,
        })
    }

    fn extract_float(&self, value: &PdfValue) -> Option<f32> {
        match value {
            PdfValue::Integer(i) => Some(*i as f32),
            PdfValue::Real(r) => Some(*r as f32),
            _ => None,
        }
    }

    fn parse_destination(&mut self, dict: &PdfDictionary) -> Option<Destination> {
        // Check for direct destination
        if let Some(dest_value) = dict.get("Dest") {
            return self.parse_destination_value(dest_value);
        }

        // Check for action with destination
        if let Some(PdfValue::Dictionary(action_dict)) = dict.get("A") {
            if let Some(PdfValue::Name(action_type)) = action_dict.get("S") {
                match action_type.without_slash() {
                    "GoTo" => {
                        if let Some(dest) = action_dict.get("D") {
                            return self.parse_destination_value(dest);
                        }
                    }
                    "GoToR" => {
                        // Remote go-to action
                        let file = action_dict
                            .get("F")
                            .and_then(|v| match v {
                                PdfValue::String(s) => Some(s.to_string_lossy()),
                                _ => None,
                            })
                            .unwrap_or_default();

                        let dest = action_dict
                            .get("D")
                            .and_then(|v| match v {
                                PdfValue::String(s) => Some(s.to_string_lossy()),
                                PdfValue::Name(n) => Some(n.without_slash().to_string()),
                                _ => None,
                            })
                            .unwrap_or_default();

                        return Some(Destination::Remote(file, dest));
                    }
                    _ => {}
                }
            }
        }

        None
    }

    fn parse_destination_value(&mut self, value: &PdfValue) -> Option<Destination> {
        match value {
            PdfValue::String(s) => {
                // Named destination
                Some(Destination::Named(s.to_string_lossy()))
            }
            PdfValue::Name(s) => {
                // Named destination
                Some(Destination::Named(s.without_slash().to_string()))
            }
            PdfValue::Array(arr) if !arr.is_empty() => {
                // Explicit destination array
                self.parse_explicit_destination(arr)
            }
            PdfValue::Reference(obj_id) => {
                // Indirect reference to destination
                if let Some(dest_node_id) = self.resolver.get_node_id(&obj_id.id()) {
                    if let Some(dest_node) = self.ast.get_node(dest_node_id) {
                        let value = dest_node.value.clone();
                        match &value {
                            PdfValue::Array(arr) => self.parse_explicit_destination(arr),
                            PdfValue::String(s) => Some(Destination::Named(s.to_string_lossy())),
                            PdfValue::Name(s) => {
                                Some(Destination::Named(s.without_slash().to_string()))
                            }
                            _ => Some(Destination::Page(dest_node_id)),
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn parse_explicit_destination(&mut self, arr: &PdfArray) -> Option<Destination> {
        if arr.is_empty() {
            return None;
        }

        // First element is page reference
        let page_id = match &arr[0] {
            PdfValue::Reference(obj_id) => self.resolver.get_node_id(&obj_id.id())?,
            _ => return None,
        };

        // Second element is destination type
        if arr.len() < 2 {
            return Some(Destination::Page(page_id));
        }

        let (dest_type, coords) = match &arr[1] {
            PdfValue::Name(name) => {
                let typ = match name.without_slash() {
                    "XYZ" => DestinationType::XYZ,
                    "Fit" => DestinationType::Fit,
                    "FitH" => DestinationType::FitH,
                    "FitV" => DestinationType::FitV,
                    "FitR" => DestinationType::FitR,
                    "FitB" => DestinationType::FitB,
                    "FitBH" => DestinationType::FitBH,
                    "FitBV" => DestinationType::FitBV,
                    _ => return Some(Destination::Page(page_id)),
                };

                // Extract coordinates based on type
                let coords = self.extract_destination_coords(&typ, arr);
                (typ, coords)
            }
            _ => return Some(Destination::Page(page_id)),
        };

        Some(Destination::Explicit {
            page: page_id,
            typ: dest_type,
            coords,
        })
    }

    fn extract_destination_coords(&self, typ: &DestinationType, arr: &PdfArray) -> Vec<f32> {
        let mut coords = Vec::new();

        let start_idx = 2; // Skip page ref and type name
        let count = match typ {
            DestinationType::XYZ => 3, // left, top, zoom
            DestinationType::Fit => 0,
            DestinationType::FitH | DestinationType::FitBH => 1, // top
            DestinationType::FitV | DestinationType::FitBV => 1, // left
            DestinationType::FitR => 4,                          // left, bottom, right, top
            DestinationType::FitB => 0,
        };

        for i in start_idx..arr.len().min(start_idx + count) {
            if let Some(f) = self.extract_float(&arr[i]) {
                coords.push(f);
            } else if matches!(&arr[i], PdfValue::Null) {
                coords.push(0.0); // null means "unchanged"
            }
        }

        coords
    }

    fn resolve_outline_reference(&self, value: Option<&PdfValue>) -> Option<NodeId> {
        match value? {
            PdfValue::Reference(obj_id) => self.resolver.get_node_id(&obj_id.id()),
            _ => None,
        }
    }

    pub fn count_visible_items(&self, tree: &OutlineTree) -> usize {
        let mut count = 0;
        for item in tree.items.values() {
            if item.count >= 0 {
                count += 1;
            }
        }
        count
    }

    pub fn get_outline_hierarchy(&self, tree: &OutlineTree) -> Vec<OutlineNode> {
        let mut hierarchy = Vec::new();
        let _ = self.get_outline_hierarchy_into(tree, &mut hierarchy);
        hierarchy
    }

    pub fn get_outline_hierarchy_with_budget(
        &self,
        tree: &OutlineTree,
    ) -> Result<Vec<OutlineNode>, ResourceBudgetError> {
        let mut hierarchy = Vec::new();
        self.get_outline_hierarchy_into(tree, &mut hierarchy)?;
        Ok(hierarchy)
    }

    fn get_outline_hierarchy_into(
        &self,
        tree: &OutlineTree,
        hierarchy: &mut Vec<OutlineNode>,
    ) -> Result<(), ResourceBudgetError> {
        // Find root items (those without parent)
        for (id, item) in &tree.items {
            if item.parent.is_none() {
                hierarchy.push(self.build_outline_node(*id, item, tree)?);
            }
        }

        Ok(())
    }

    fn build_outline_node(
        &self,
        id: NodeId,
        item: &OutlineItem,
        tree: &OutlineTree,
    ) -> Result<OutlineNode, ResourceBudgetError> {
        self.build_outline_node_at_depth(id, item, tree, 0, &mut HashSet::new())
    }

    fn build_outline_node_at_depth(
        &self,
        id: NodeId,
        item: &OutlineItem,
        tree: &OutlineTree,
        level: usize,
        active: &mut HashSet<NodeId>,
    ) -> Result<OutlineNode, ResourceBudgetError> {
        self.budget.consume_object()?;
        self.budget
            .consume_decoded(outline_output_bytes(item) as u64)?;
        let mut node = OutlineNode {
            id,
            title: item.title.clone(),
            level,
            destination: item.dest.clone(),
            action: item.action,
            is_open: item.count > 0,
            color: item.color,
            children: Vec::new(),
        };

        if level >= self.budget.max_depth || !active.insert(id) {
            return Ok(node);
        }

        // Add children
        let mut child_id = item.first;
        let mut siblings = HashSet::new();
        while let Some(cid) = child_id {
            if !siblings.insert(cid) {
                break;
            }
            if let Some(child_item) = tree.items.get(&cid) {
                node.children.push(self.build_outline_node_at_depth(
                    cid,
                    child_item,
                    tree,
                    level + 1,
                    active,
                )?);
                child_id = child_item.next;
            } else {
                break;
            }
        }
        active.remove(&id);

        Ok(node)
    }
}

fn outline_output_bytes(item: &OutlineItem) -> usize {
    let destination_bytes = match &item.dest {
        Some(Destination::Named(name)) => name.len(),
        Some(Destination::Remote(file, name)) => file.len() + name.len(),
        Some(Destination::Explicit { coords, .. }) => coords.len() * std::mem::size_of::<f32>(),
        _ => 0,
    };
    item.title.len() + destination_bytes
}

#[derive(Debug, Clone)]
pub struct OutlineNode {
    pub id: NodeId,
    pub title: String,
    pub level: usize,
    pub destination: Option<Destination>,
    pub action: Option<NodeId>,
    pub is_open: bool,
    pub color: Option<[f32; 3]>,
    pub children: Vec<OutlineNode>,
}

impl OutlineNode {
    pub fn flatten(&self) -> Vec<FlatOutlineEntry> {
        let mut entries = Vec::new();
        let _ = self.flatten_recursive(&mut entries, &ResourceBudget::default());
        entries
    }

    pub fn flatten_with_budget(
        &self,
        budget: &ResourceBudget,
    ) -> Result<Vec<FlatOutlineEntry>, ResourceBudgetError> {
        let mut entries = Vec::new();
        self.flatten_recursive(&mut entries, budget)?;
        Ok(entries)
    }

    fn flatten_recursive(
        &self,
        entries: &mut Vec<FlatOutlineEntry>,
        budget: &ResourceBudget,
    ) -> Result<(), ResourceBudgetError> {
        budget.consume_object()?;
        budget.consume_decoded(self.title.len() as u64)?;
        entries.push(FlatOutlineEntry {
            id: self.id,
            title: self.title.clone(),
            level: self.level,
            destination: self.destination.clone(),
            page_number: self.extract_page_number(),
        });

        for child in &self.children {
            child.flatten_recursive(entries, budget)?;
        }

        Ok(())
    }

    fn extract_page_number(&self) -> Option<usize> {
        match &self.destination {
            Some(Destination::Page(page_id)) => {
                // Get actual page index from AST
                self.get_page_index(*page_id)
            }
            Some(Destination::Explicit { page, .. }) => {
                // Get actual page index from AST
                self.get_page_index(*page)
            }
            _ => None,
        }
    }

    fn get_page_index(&self, page_id: NodeId) -> Option<usize> {
        // This would need access to the document's page list
        // For now, store page_id as metadata to be resolved later
        Some(page_id.0)
    }
}

#[derive(Debug, Clone)]
pub struct FlatOutlineEntry {
    pub id: NodeId,
    pub title: String,
    pub level: usize,
    pub destination: Option<Destination>,
    pub page_number: Option<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outline_hierarchy_terminates_on_child_and_sibling_cycles() {
        let empty_item = |title: &str| OutlineItem {
            title: title.to_string(),
            dest: None,
            action: None,
            parent: None,
            prev: None,
            next: None,
            first: None,
            last: None,
            count: 0,
            flags: 0,
            color: None,
        };
        let mut root = empty_item("root");
        root.first = Some(NodeId(1));
        let mut child = empty_item("child");
        child.parent = Some(NodeId(0));
        child.first = Some(NodeId(0));
        child.next = Some(NodeId(1));

        let tree = OutlineTree {
            root: NodeId(0),
            items: HashMap::from([(NodeId(0), root), (NodeId(1), child)]),
        };
        let mut ast = PdfAstGraph::new();
        let resolver = ObjectNodeMap::new();
        let budget = ResourceBudget::new(1024, 1024, 1024, 10, 10, 10, 10, 8);
        let parser = OutlineParser::new_with_budget(&mut ast, &resolver, &budget);

        let hierarchy = parser.get_outline_hierarchy(&tree);
        assert_eq!(hierarchy.len(), 1);
        assert_eq!(hierarchy[0].children.len(), 1);
        assert_eq!(hierarchy[0].children[0].children.len(), 1);
        assert!(hierarchy[0].children[0].children[0].children.is_empty());
    }

    #[test]
    fn outline_materialization_reports_budget_exhaustion() {
        let tree = OutlineTree {
            root: NodeId(0),
            items: HashMap::from([(
                NodeId(0),
                OutlineItem {
                    title: "root".to_string(),
                    dest: None,
                    action: None,
                    parent: None,
                    prev: None,
                    next: None,
                    first: None,
                    last: None,
                    count: 0,
                    flags: 0,
                    color: None,
                },
            )]),
        };
        let mut ast = PdfAstGraph::new();
        let resolver = ObjectNodeMap::new();
        let budget = ResourceBudget::new(1024, 1024, 1024, 100, 0, 10, 10, 10);
        let parser = OutlineParser::new_with_budget(&mut ast, &resolver, &budget);

        assert_eq!(
            parser
                .get_outline_hierarchy_with_budget(&tree)
                .expect_err("outline hierarchy must respect the object budget"),
            ResourceBudgetError::Objects
        );

        let node = OutlineNode {
            id: NodeId(0),
            title: "root".to_string(),
            level: 0,
            destination: None,
            action: None,
            is_open: false,
            color: None,
            children: Vec::new(),
        };
        assert_eq!(
            node.flatten_with_budget(&budget)
                .expect_err("outline flattening must respect the object budget"),
            ResourceBudgetError::Objects
        );
    }

    #[test]
    fn parse_outline_tree_terminates_on_first_cycle() {
        let object_id = crate::types::ObjectId::new(1, 0);
        let node_id = NodeId(0);
        let mut item_dict = PdfDictionary::new();
        item_dict.insert("Title", PdfValue::String("cyclic".into()));
        item_dict.insert("First", PdfValue::Reference(object_id.into()));

        let mut ast = PdfAstGraph::new();
        ast.add_node(crate::ast::AstNode::new(
            node_id,
            NodeType::Unknown,
            PdfValue::Dictionary(item_dict),
        ));
        let mut resolver = ObjectNodeMap::new();
        resolver.insert(object_id, node_id);

        let mut outlines_dict = PdfDictionary::new();
        outlines_dict.insert("First", PdfValue::Reference(object_id.into()));
        let budget = ResourceBudget::new(1024, 1024, 1024, 10, 10, 10, 10, 8);
        let mut parser = OutlineParser::new_with_budget(&mut ast, &resolver, &budget);

        let tree = parser
            .parse_outline_tree(&outlines_dict)
            .expect("outline root should resolve");
        assert_eq!(tree.items.len(), 1);
        assert_eq!(tree.root, node_id);
    }
}
