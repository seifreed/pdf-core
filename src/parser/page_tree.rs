use crate::ast::{AstNode, NodeId, NodeType, PdfAstGraph};
use crate::filters::decode_stream_with_budget;
use crate::metadata::icc::parse_icc_profile;
use crate::parser::reference_resolver::ObjectNodeMap;
use crate::performance::ResourceBudget;
use crate::types::{PdfDictionary, PdfValue};
use std::collections::{HashMap, HashSet};

pub struct PageTreeParser<'a> {
    ast: &'a mut PdfAstGraph,
    resolver: &'a ObjectNodeMap,
    resources_cache: HashMap<NodeId, PdfDictionary>,
    budget: ResourceBudget,
    visited: HashSet<NodeId>,
}

impl<'a> PageTreeParser<'a> {
    pub fn new(ast: &'a mut PdfAstGraph, resolver: &'a ObjectNodeMap) -> Self {
        Self::new_with_budget(ast, resolver, &ResourceBudget::default())
    }

    pub fn new_with_budget(
        ast: &'a mut PdfAstGraph,
        resolver: &'a ObjectNodeMap,
        budget: &ResourceBudget,
    ) -> Self {
        PageTreeParser {
            ast,
            resolver,
            resources_cache: HashMap::new(),
            budget: budget.clone(),
            visited: HashSet::new(),
        }
    }

    pub fn parse_page_tree(
        &mut self,
        pages_dict: &PdfDictionary,
        parent_id: NodeId,
    ) -> Vec<NodeId> {
        self.parse_page_tree_at_depth(pages_dict, parent_id, 0)
    }

    fn parse_page_tree_at_depth(
        &mut self,
        pages_dict: &PdfDictionary,
        parent_id: NodeId,
        depth: usize,
    ) -> Vec<NodeId> {
        let mut page_nodes = Vec::new();
        if depth >= self.budget.max_depth {
            return page_nodes;
        }

        // Get parent resources for inheritance
        let parent_resources = self.extract_resources(pages_dict);

        // Process Kids array
        if let Some(PdfValue::Array(kids)) = pages_dict.get("Kids") {
            for kid_ref in kids.iter() {
                if self.budget.check().is_err() {
                    break;
                }
                if let PdfValue::Reference(obj_id) = kid_ref {
                    if let Some(kid_node_id) = self.resolver.get_node_id(&obj_id.id()) {
                        if !self.visited.insert(kid_node_id) {
                            continue;
                        }
                        if let Some(kid_node) = self.ast.get_node(kid_node_id) {
                            if let Some(kid_dict) = kid_node.as_dict() {
                                let kid_dict = kid_dict.clone();
                                let page_type =
                                    kid_dict.get_type().map(|t| t.without_slash()).unwrap_or("");

                                match page_type {
                                    "Pages" => {
                                        // Recursively parse pages tree node
                                        let child_resources = self.merge_resources(
                                            &parent_resources,
                                            &self.extract_resources(&kid_dict),
                                        );
                                        self.resources_cache.insert(kid_node_id, child_resources);

                                        let child_pages = self.parse_page_tree_at_depth(
                                            &kid_dict,
                                            kid_node_id,
                                            depth + 1,
                                        );
                                        page_nodes.extend(child_pages);

                                        // Link pages node to parent
                                        self.add_edge(
                                            parent_id,
                                            kid_node_id,
                                            crate::ast::EdgeType::Child,
                                        );
                                    }
                                    "Page" => {
                                        // Process individual page
                                        let page_id = self.process_page(
                                            &kid_dict,
                                            kid_node_id,
                                            &parent_resources,
                                        );
                                        page_nodes.push(page_id);

                                        // Link page to parent
                                        self.add_edge(
                                            parent_id,
                                            page_id,
                                            crate::ast::EdgeType::Child,
                                        );
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
            }
        }

        page_nodes
    }

    fn process_page(
        &mut self,
        page_dict: &PdfDictionary,
        node_id: NodeId,
        parent_resources: &PdfDictionary,
    ) -> NodeId {
        // Merge page resources with inherited resources
        let page_resources = self.extract_resources(page_dict);
        let merged_resources = self.merge_resources(parent_resources, &page_resources);

        // Store merged resources for this page
        self.resources_cache
            .insert(node_id, merged_resources.clone());

        // Update node with complete resources
        if let Some(node) = self.ast.get_node_mut(node_id) {
            node.node_type = NodeType::Page;

            // Add merged resources to metadata
            node.metadata.set_property(
                "has_inherited_resources".to_string(),
                (!parent_resources.is_empty()).to_string(),
            );
        }

        // Process page contents
        if let Some(PdfValue::Reference(contents_ref)) = page_dict.get("Contents") {
            if let Some(contents_id) = self.resolver.get_node_id(&contents_ref.id()) {
                self.add_edge(node_id, contents_id, crate::ast::EdgeType::Content);
            }
        } else if let Some(PdfValue::Array(contents_array)) = page_dict.get("Contents") {
            for content_ref in contents_array.iter() {
                if let PdfValue::Reference(obj_id) = content_ref {
                    if let Some(content_id) = self.resolver.get_node_id(&obj_id.id()) {
                        self.add_edge(node_id, content_id, crate::ast::EdgeType::Content);
                    }
                }
            }
        }

        // Process annotations
        if let Some(PdfValue::Array(annots)) = page_dict.get("Annots") {
            for annot_ref in annots.iter() {
                if let PdfValue::Reference(obj_id) = annot_ref {
                    if let Some(annot_id) = self.resolver.get_node_id(&obj_id.id()) {
                        self.add_edge(node_id, annot_id, crate::ast::EdgeType::Annotation);

                        // Update annotation node type
                        if let Some(annot_node) = self.ast.get_node_mut(annot_id) {
                            annot_node.node_type = NodeType::Annotation;
                        }
                    }
                }
            }
        }

        // Process page-level resources (fonts, XObjects, etc.)
        self.process_page_resources(node_id, &merged_resources);

        node_id
    }

    fn extract_resources(&self, dict: &PdfDictionary) -> PdfDictionary {
        if let Some(PdfValue::Dictionary(resources)) = dict.get("Resources") {
            resources.clone()
        } else if let Some(PdfValue::Reference(res_ref)) = dict.get("Resources") {
            // Resolve indirect reference to resources dictionary
            if let Some(res_node_id) = self.resolver.get_node_id(&res_ref.id()) {
                if let Some(res_node) = self.ast.get_node(res_node_id) {
                    if let Some(res_dict) = res_node.as_dict() {
                        return res_dict.clone();
                    }
                }
            }
            PdfDictionary::new()
        } else {
            PdfDictionary::new()
        }
    }

    fn merge_resources(&self, parent: &PdfDictionary, child: &PdfDictionary) -> PdfDictionary {
        let mut merged = parent.clone();

        // Resource categories that should be merged
        let categories = [
            "Font",
            "XObject",
            "ExtGState",
            "ColorSpace",
            "Pattern",
            "Shading",
            "Properties",
            "ProcSet",
        ];

        for category in &categories {
            let parent_cat = parent
                .get(category)
                .and_then(|v| v.as_dict())
                .cloned()
                .unwrap_or_else(PdfDictionary::new);

            let child_cat = child
                .get(category)
                .and_then(|v| v.as_dict())
                .cloned()
                .unwrap_or_else(PdfDictionary::new);

            if !child_cat.is_empty() {
                let mut merged_cat = parent_cat;
                // Child resources override parent resources with same name
                for (key, value) in child_cat.iter() {
                    merged_cat.insert(key.clone(), value.clone());
                }
                merged.insert(category.to_string(), PdfValue::Dictionary(merged_cat));
            } else if !parent_cat.is_empty() {
                merged.insert(category.to_string(), PdfValue::Dictionary(parent_cat));
            }
        }

        merged
    }

    fn process_page_resources(&mut self, page_id: NodeId, resources: &PdfDictionary) {
        // Process fonts
        if let Some(PdfValue::Dictionary(fonts)) = resources.get("Font") {
            for (font_name, font_ref) in fonts.iter() {
                if let PdfValue::Reference(obj_id) = font_ref {
                    if let Some(font_id) = self.resolver.get_node_id(&obj_id.id()) {
                        self.add_edge(page_id, font_id, crate::ast::EdgeType::Resource);

                        // Update font node
                        let font_type = if let Some(font_node) = self.ast.get_node(font_id) {
                            self.determine_font_type(font_node.as_dict())
                        } else {
                            NodeType::Font
                        };

                        if let Some(font_node) = self.ast.get_node_mut(font_id) {
                            font_node.node_type = font_type;
                            font_node
                                .metadata
                                .set_property("resource_name".to_string(), font_name.to_string());
                        }
                    }
                }
            }
        }

        // Process XObjects (images, forms)
        if let Some(PdfValue::Dictionary(xobjects)) = resources.get("XObject") {
            for (xobj_name, xobj_ref) in xobjects.iter() {
                if let PdfValue::Reference(obj_id) = xobj_ref {
                    if let Some(xobj_id) = self.resolver.get_node_id(&obj_id.id()) {
                        self.add_edge(page_id, xobj_id, crate::ast::EdgeType::Resource);

                        // Update XObject node
                        let xobj_type = if let Some(xobj_node) = self.ast.get_node(xobj_id) {
                            self.determine_xobject_type(xobj_node.as_dict())
                        } else {
                            NodeType::XObject
                        };

                        if let Some(xobj_node) = self.ast.get_node_mut(xobj_id) {
                            xobj_node.node_type = xobj_type;
                            xobj_node
                                .metadata
                                .set_property("resource_name".to_string(), xobj_name.to_string());
                        }
                    }
                }
            }
        }

        // Process ExtGState (graphics state)
        if let Some(PdfValue::Dictionary(gstates)) = resources.get("ExtGState") {
            for (gs_name, gs_ref) in gstates.iter() {
                if let PdfValue::Reference(obj_id) = gs_ref {
                    if let Some(gs_id) = self.resolver.get_node_id(&obj_id.id()) {
                        self.add_edge(page_id, gs_id, crate::ast::EdgeType::Resource);

                        if let Some(gs_node) = self.ast.get_node_mut(gs_id) {
                            gs_node.node_type = NodeType::ExtGState;
                            gs_node
                                .metadata
                                .set_property("resource_name".to_string(), gs_name.to_string());
                        }
                    }
                }
            }
        }

        // Process ColorSpace
        if let Some(PdfValue::Dictionary(colorspaces)) = resources.get("ColorSpace") {
            for (cs_name, cs_value) in colorspaces.iter() {
                self.process_colorspace(page_id, cs_name.as_str(), cs_value);
            }
        }

        // Process Pattern
        if let Some(PdfValue::Dictionary(patterns)) = resources.get("Pattern") {
            for (pattern_name, pattern_ref) in patterns.iter() {
                if let PdfValue::Reference(obj_id) = pattern_ref {
                    if let Some(pattern_id) = self.resolver.get_node_id(&obj_id.id()) {
                        self.add_edge(page_id, pattern_id, crate::ast::EdgeType::Resource);

                        if let Some(pattern_node) = self.ast.get_node_mut(pattern_id) {
                            pattern_node.node_type = NodeType::Pattern;
                            pattern_node.metadata.set_property(
                                "resource_name".to_string(),
                                pattern_name.to_string(),
                            );
                        }
                    }
                }
            }
        }

        // Process Shading
        if let Some(PdfValue::Dictionary(shadings)) = resources.get("Shading") {
            for (shading_name, shading_ref) in shadings.iter() {
                if let PdfValue::Reference(obj_id) = shading_ref {
                    if let Some(shading_id) = self.resolver.get_node_id(&obj_id.id()) {
                        self.add_edge(page_id, shading_id, crate::ast::EdgeType::Resource);

                        if let Some(shading_node) = self.ast.get_node_mut(shading_id) {
                            shading_node.node_type = NodeType::Shading;
                            shading_node.metadata.set_property(
                                "resource_name".to_string(),
                                shading_name.to_string(),
                            );
                        }
                    }
                }
            }
        }
    }

    fn determine_font_type(&self, font_dict: Option<&PdfDictionary>) -> NodeType {
        if let Some(dict) = font_dict {
            if let Some(PdfValue::Name(subtype)) = dict.get("Subtype") {
                match subtype.without_slash() {
                    "Type1" => NodeType::Type1Font,
                    "TrueType" => NodeType::TrueTypeFont,
                    "Type3" => NodeType::Type3Font,
                    "Type0" => NodeType::CIDFont,
                    _ => NodeType::Font,
                }
            } else {
                NodeType::Font
            }
        } else {
            NodeType::Font
        }
    }

    fn determine_xobject_type(&self, xobj_dict: Option<&PdfDictionary>) -> NodeType {
        if let Some(dict) = xobj_dict {
            if let Some(PdfValue::Name(subtype)) = dict.get("Subtype") {
                match subtype.without_slash() {
                    "Image" => NodeType::ImageXObject,
                    "Form" => NodeType::FormXObject,
                    "PS" => NodeType::XObject,
                    _ => NodeType::XObject,
                }
            } else {
                NodeType::XObject
            }
        } else {
            NodeType::XObject
        }
    }

    fn process_colorspace(&mut self, page_id: NodeId, name: &str, value: &PdfValue) {
        match value {
            PdfValue::Name(cs_name) => {
                // Simple color space name
                if self.budget.consume_node().is_err() {
                    return;
                }
                let node_id = self.ast.next_node_id();
                let cs_node_id = self.ast.add_node(AstNode::new(
                    node_id,
                    NodeType::ColorSpace,
                    PdfValue::Name(cs_name.clone()),
                ));

                self.add_edge(page_id, cs_node_id, crate::ast::EdgeType::Resource);

                if let Some(cs_node) = self.ast.get_node_mut(cs_node_id) {
                    cs_node
                        .metadata
                        .set_property("resource_name".to_string(), name.to_string());
                }
            }
            PdfValue::Array(cs_array) if !cs_array.is_empty() => {
                // Complex color space definition
                if let Some(PdfValue::Name(cs_type)) = cs_array.get(0) {
                    let node_type = match cs_type.without_slash() {
                        "ICCBased" => NodeType::ICCBased,
                        "Separation" => NodeType::Separation,
                        "DeviceN" => NodeType::DeviceN,
                        "Indexed" => NodeType::Indexed,
                        "Pattern" => NodeType::Pattern,
                        _ => NodeType::ColorSpace,
                    };

                    let node_id = self.ast.next_node_id();
                    let is_icc_based = node_type == NodeType::ICCBased;
                    if self.budget.consume_node().is_err() {
                        return;
                    }
                    let cs_node_id =
                        self.ast
                            .add_node(AstNode::new(node_id, node_type, value.clone()));

                    self.add_edge(page_id, cs_node_id, crate::ast::EdgeType::Resource);

                    if let Some(cs_node) = self.ast.get_node_mut(cs_node_id) {
                        cs_node
                            .metadata
                            .set_property("resource_name".to_string(), name.to_string());
                    }

                    // Process ICC profile stream if ICCBased
                    if is_icc_based && cs_array.len() > 1 {
                        if let Some(PdfValue::Reference(icc_ref)) = cs_array.get(1) {
                            if let Some(icc_id) = self.resolver.get_node_id(&icc_ref.id()) {
                                self.add_edge(cs_node_id, icc_id, crate::ast::EdgeType::Reference);
                                if let Some(stream) = self
                                    .ast
                                    .get_node(icc_id)
                                    .and_then(|node| node.as_stream().cloned())
                                {
                                    self.attach_icc_profile_node(cs_node_id, &stream);
                                }
                            }
                        } else if let Some(PdfValue::Stream(stream)) = cs_array.get(1) {
                            self.attach_icc_profile_node(cs_node_id, stream);
                        }
                    }
                }
            }
            PdfValue::Reference(obj_id) => {
                // Indirect reference to color space
                if let Some(cs_id) = self.resolver.get_node_id(&obj_id.id()) {
                    self.add_edge(page_id, cs_id, crate::ast::EdgeType::Resource);

                    if let Some(cs_node) = self.ast.get_node_mut(cs_id) {
                        cs_node.node_type = NodeType::ColorSpace;
                        cs_node
                            .metadata
                            .set_property("resource_name".to_string(), name.to_string());
                    }
                }
            }
            _ => {}
        }
    }

    fn attach_icc_profile_node(&mut self, icc_id: NodeId, stream: &crate::types::PdfStream) {
        let raw = match stream.raw_data() {
            Some(data) => data,
            None => return,
        };

        let filters = match stream.get_filters_with_params_checked() {
            Ok(filters) => filters,
            Err(_) => return,
        };
        let decoded = match decode_stream_with_budget(raw, &filters, &self.budget) {
            Ok(decoded) => decoded,
            Err(_) => return,
        };

        let info = match parse_icc_profile(&decoded) {
            Some(info) => info,
            None => return,
        };

        if self.budget.consume_node().is_err() {
            return;
        }
        let node_id = self.ast.next_node_id();
        let profile_id =
            self.ast
                .add_node(AstNode::new(node_id, NodeType::Metadata, PdfValue::Null));
        self.add_edge(icc_id, profile_id, crate::ast::EdgeType::Child);

        if let Some(node) = self.ast.get_node_mut(profile_id) {
            node.metadata
                .set_property("metadata_kind".to_string(), "icc_profile".to_string());
            node.metadata
                .set_property("icc_size".to_string(), info.size.to_string());
            node.metadata
                .set_property("icc_cmm_type".to_string(), info.cmm_type);
            node.metadata
                .set_property("icc_version".to_string(), info.version);
            node.metadata
                .set_property("icc_device_class".to_string(), info.device_class);
            node.metadata
                .set_property("icc_color_space".to_string(), info.color_space);
            node.metadata.set_property("icc_pcs".to_string(), info.pcs);
            node.metadata
                .set_property("icc_signature".to_string(), info.signature);
        }
    }

    fn add_edge(&mut self, from: NodeId, to: NodeId, edge_type: crate::ast::EdgeType) {
        if self.budget.consume_edge().is_ok() {
            self.ast.add_edge(from, to, edge_type);
        }
    }

    pub fn get_page_resources(&self, page_id: NodeId) -> Option<&PdfDictionary> {
        self.resources_cache.get(&page_id)
    }

    pub fn get_inherited_resources(
        &self,
        page_id: NodeId,
    ) -> HashMap<String, Vec<(String, NodeId)>> {
        let mut resources = HashMap::new();

        if let Some(page_res) = self.resources_cache.get(&page_id) {
            // Collect all resource references by category
            let categories = [
                "Font",
                "XObject",
                "ExtGState",
                "ColorSpace",
                "Pattern",
                "Shading",
            ];

            for category in &categories {
                if let Some(PdfValue::Dictionary(cat_dict)) = page_res.get(category) {
                    let mut cat_resources = Vec::new();

                    for (res_name, res_value) in cat_dict.iter() {
                        if let PdfValue::Reference(obj_id) = res_value {
                            if let Some(res_node_id) = self.resolver.get_node_id(&obj_id.id()) {
                                cat_resources.push((res_name.to_string(), res_node_id));
                            }
                        }
                    }

                    if !cat_resources.is_empty() {
                        resources.insert(category.to_string(), cat_resources);
                    }
                }
            }
        }

        resources
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::AstNode;
    use crate::types::{ObjectId, PdfReference};

    #[test]
    fn page_tree_parser_terminates_on_cycles() {
        let mut ast = PdfAstGraph::new();
        let root_id = ast.add_node(AstNode::new(NodeId(0), NodeType::Catalog, PdfValue::Null));
        let mut pages_dict = PdfDictionary::new();
        pages_dict.insert(
            "Type",
            PdfValue::Name(crate::types::primitive::PdfName::new("Pages")),
        );
        pages_dict.insert(
            "Kids",
            PdfValue::Array(vec![PdfValue::Reference(PdfReference::new(1, 0))].into()),
        );
        let pages_id = ast.add_node(AstNode::new(
            NodeId(1),
            NodeType::Pages,
            PdfValue::Dictionary(pages_dict.clone()),
        ));
        let mut resolver = ObjectNodeMap::new();
        resolver.insert(ObjectId::new(1, 0), pages_id);
        let mut parser = PageTreeParser::new(&mut ast, &resolver);

        assert!(parser.parse_page_tree(&pages_dict, root_id).is_empty());
    }

    #[test]
    fn page_tree_parser_respects_resource_budgets() {
        let mut ast = PdfAstGraph::new();
        let root_id = ast.add_node(AstNode::new(NodeId(0), NodeType::Catalog, PdfValue::Null));
        let page_id = ast.add_node(AstNode::new(
            NodeId(1),
            NodeType::Page,
            PdfValue::Dictionary(PdfDictionary::new()),
        ));
        let mut resources = PdfDictionary::new();
        resources.insert(
            "ColorSpace",
            PdfValue::Dictionary({
                let mut colorspaces = PdfDictionary::new();
                colorspaces.insert(
                    "CS1",
                    PdfValue::Name(crate::types::primitive::PdfName::new("DeviceRGB")),
                );
                colorspaces
            }),
        );
        let mut pages_dict = PdfDictionary::new();
        pages_dict.insert(
            "Kids",
            PdfValue::Array(vec![PdfValue::Reference(PdfReference::new(1, 0))].into()),
        );
        let mut page_dict = PdfDictionary::new();
        page_dict.insert(
            "Type",
            PdfValue::Name(crate::types::primitive::PdfName::new("Page")),
        );
        page_dict.insert("Resources", PdfValue::Dictionary(resources));
        if let Some(node) = ast.get_node_mut(page_id) {
            node.value = PdfValue::Dictionary(page_dict);
        }
        let mut resolver = ObjectNodeMap::new();
        resolver.insert(ObjectId::new(1, 0), page_id);
        let budget = ResourceBudget::new(1024, 1024, 1024, 10, 10, 0, 0, 8);
        let mut parser = PageTreeParser::new_with_budget(&mut ast, &resolver, &budget);

        parser.parse_page_tree(&pages_dict, root_id);
        assert_eq!(ast.node_count(), 2);
        assert_eq!(ast.edge_count(), 0);
    }
}
