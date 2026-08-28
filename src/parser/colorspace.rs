use crate::ast::{AstNode, NodeId, NodeType, PdfAstGraph};
use crate::filters::decode_stream_with_budget;
use crate::metadata::icc::parse_icc_profile;
use crate::parser::reference_resolver::ObjectNodeMap;
use crate::performance::{PerformanceLimits, ResourceBudgetError};
use crate::types::primitive::PdfName;
use crate::types::{PdfArray, PdfDictionary, PdfValue};

pub struct ColorSpaceParser<'a> {
    ast: &'a mut PdfAstGraph,
    resolver: &'a ObjectNodeMap,
    limits: PerformanceLimits,
    budget_error: Option<ResourceBudgetError>,
}

impl<'a> ColorSpaceParser<'a> {
    pub fn new(ast: &'a mut PdfAstGraph, resolver: &'a ObjectNodeMap) -> Self {
        Self::new_with_limits(ast, resolver, &PerformanceLimits::default())
    }

    pub fn new_with_limits(
        ast: &'a mut PdfAstGraph,
        resolver: &'a ObjectNodeMap,
        limits: &PerformanceLimits,
    ) -> Self {
        ColorSpaceParser {
            ast,
            resolver,
            limits: limits.clone(),
            budget_error: None,
        }
    }

    pub fn parse_colorspace(&mut self, value: &PdfValue) -> Option<NodeId> {
        self.parse_colorspace_with_budget(value).ok().flatten()
    }

    pub fn parse_colorspace_with_budget(
        &mut self,
        value: &PdfValue,
    ) -> Result<Option<NodeId>, ResourceBudgetError> {
        self.budget_error = None;
        let result = self.parse_colorspace_inner(value);
        match self.budget_error.take() {
            Some(error) => Err(error),
            None => Ok(result),
        }
    }

    fn parse_colorspace_inner(&mut self, value: &PdfValue) -> Option<NodeId> {
        match value {
            PdfValue::Name(name) => {
                // Device color space
                self.create_device_colorspace(name.without_slash())
            }
            PdfValue::Array(arr) if !arr.is_empty() => {
                // Complex color space
                self.parse_colorspace_array(arr)
            }
            PdfValue::Reference(obj_id) => {
                // Indirect reference
                if let Some(node_id) = self.resolver.get_node_id(&obj_id.object_id()) {
                    if let Some(node) = self.ast.get_node_mut(node_id) {
                        node.node_type = NodeType::ColorSpace;
                    }
                    Some(node_id)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn create_device_colorspace(&mut self, name: &str) -> Option<NodeId> {
        let node_type = match name {
            "DeviceGray" | "DeviceRGB" | "DeviceCMYK" => NodeType::ColorSpace,
            _ => return None,
        };

        let node = AstNode::new(
            self.ast.next_node_id(),
            node_type,
            PdfValue::Name(PdfName::new(name.to_string())),
        );

        self.add_node(node)
    }

    fn parse_colorspace_array(&mut self, arr: &PdfArray) -> Option<NodeId> {
        let cs_type = match arr.get(0)? {
            PdfValue::Name(n) => n.without_slash(),
            _ => return None,
        };

        match cs_type {
            "ICCBased" => self.parse_icc_based(arr),
            "CalGray" => self.parse_cal_gray(arr),
            "CalRGB" => self.parse_cal_rgb(arr),
            "Lab" => self.parse_lab(arr),
            "Separation" => self.parse_separation(arr),
            "DeviceN" => self.parse_device_n(arr),
            "Indexed" => self.parse_indexed(arr),
            "Pattern" => self.parse_pattern(arr),
            _ => None,
        }
    }

    fn parse_icc_based(&mut self, arr: &PdfArray) -> Option<NodeId> {
        if arr.len() < 2 {
            return None;
        }

        // Create ICCBased node
        let icc_node = AstNode::new(
            self.ast.next_node_id(),
            NodeType::ICCBased,
            PdfValue::Array(arr.clone()),
        );

        let icc_id = self.add_node(icc_node)?;

        // Link to ICC profile stream
        match arr.get(1)? {
            PdfValue::Reference(obj_id) => {
                if let Some(stream_id) = self.resolver.get_node_id(&obj_id.object_id()) {
                    self.add_edge(icc_id, stream_id, crate::ast::EdgeType::Reference);

                    let stream = self
                        .ast
                        .get_node(stream_id)
                        .and_then(|node| node.as_stream().cloned());

                    if let Some(stream) = stream {
                        let dict = stream.dict.clone();
                        self.extract_icc_metadata(icc_id, &dict);
                        self.attach_icc_profile_node(icc_id, &stream);
                    }
                }
            }
            PdfValue::Stream(stream) => {
                self.extract_icc_metadata(icc_id, &stream.dict);
                self.attach_icc_profile_node(icc_id, stream);
            }
            _ => {}
        }

        Some(icc_id)
    }

    fn extract_icc_metadata(&mut self, node_id: NodeId, dict: &PdfDictionary) {
        // Extract data first before taking mutable reference
        let components = dict.get("N").and_then(|v| match v {
            PdfValue::Integer(n) => Some(n.to_string()),
            _ => None,
        });

        let range_str = dict.get("Range").and_then(|v| match v {
            PdfValue::Array(range) => Some(format!("{:?}", range)),
            _ => None,
        });

        let alt_id = dict
            .get("Alternate")
            .and_then(|alt| self.parse_colorspace_inner(alt));

        // Now update the node
        if let Some(node) = self.ast.get_node_mut(node_id) {
            if let Some(comp_str) = components {
                node.metadata
                    .set_property("components".to_string(), comp_str);
            }

            if let Some(range) = range_str {
                node.metadata.set_property("range".to_string(), range);
            }
        }

        // Add edge after releasing mutable reference
        if let Some(alt_id) = alt_id {
            self.add_edge(node_id, alt_id, crate::ast::EdgeType::Reference);
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
        let decoded = decode_stream_with_budget(raw, &filters, &self.limits.budget)
            .ok()
            .unwrap_or_default();

        let info = match parse_icc_profile(&decoded) {
            Some(info) => info,
            None => return,
        };

        let node_id = self.ast.next_node_id();
        if self.limits.budget.consume_node().is_err() {
            return;
        }
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

    fn parse_cal_gray(&mut self, arr: &PdfArray) -> Option<NodeId> {
        if arr.len() < 2 {
            return None;
        }

        let node = AstNode::new(
            self.ast.next_node_id(),
            NodeType::ColorSpace,
            PdfValue::Array(arr.clone()),
        );

        let node_id = self.add_node(node)?;

        // Extract CalGray parameters
        if let Some(PdfValue::Dictionary(params)) = arr.get(1) {
            // Extract all string values first to avoid borrowing conflicts
            let wp_str = params.get("WhitePoint").and_then(|v| {
                if let PdfValue::Array(wp) = v {
                    Some(self.array_to_string(wp))
                } else {
                    None
                }
            });
            let bp_str = params.get("BlackPoint").and_then(|v| {
                if let PdfValue::Array(bp) = v {
                    Some(self.array_to_string(bp))
                } else {
                    None
                }
            });
            let gamma_str = params.get("Gamma").map(|gamma| self.value_to_string(gamma));

            // Now update the node
            if let Some(node) = self.ast.get_node_mut(node_id) {
                node.metadata
                    .set_property("colorspace_type".to_string(), "CalGray".to_string());

                if let Some(wp_str) = wp_str {
                    node.metadata
                        .set_property("white_point".to_string(), wp_str);
                }

                if let Some(bp_str) = bp_str {
                    node.metadata
                        .set_property("black_point".to_string(), bp_str);
                }

                if let Some(gamma_str) = gamma_str {
                    node.metadata.set_property("gamma".to_string(), gamma_str);
                }
            }
        }

        Some(node_id)
    }

    fn parse_cal_rgb(&mut self, arr: &PdfArray) -> Option<NodeId> {
        if arr.len() < 2 {
            return None;
        }

        let node = AstNode::new(
            self.ast.next_node_id(),
            NodeType::ColorSpace,
            PdfValue::Array(arr.clone()),
        );

        let node_id = self.add_node(node)?;

        // Extract CalRGB parameters
        if let Some(PdfValue::Dictionary(params)) = arr.get(1) {
            // Extract all string values first to avoid borrowing conflicts
            let wp_str = params.get("WhitePoint").and_then(|v| {
                if let PdfValue::Array(wp) = v {
                    Some(self.array_to_string(wp))
                } else {
                    None
                }
            });
            let bp_str = params.get("BlackPoint").and_then(|v| {
                if let PdfValue::Array(bp) = v {
                    Some(self.array_to_string(bp))
                } else {
                    None
                }
            });
            let gamma_str = params.get("Gamma").and_then(|v| {
                if let PdfValue::Array(gamma) = v {
                    Some(self.array_to_string(gamma))
                } else {
                    None
                }
            });
            let matrix_str = params.get("Matrix").and_then(|v| {
                if let PdfValue::Array(matrix) = v {
                    Some(self.array_to_string(matrix))
                } else {
                    None
                }
            });

            // Now update the node
            if let Some(node) = self.ast.get_node_mut(node_id) {
                node.metadata
                    .set_property("colorspace_type".to_string(), "CalRGB".to_string());

                if let Some(wp_str) = wp_str {
                    node.metadata
                        .set_property("white_point".to_string(), wp_str);
                }

                if let Some(bp_str) = bp_str {
                    node.metadata
                        .set_property("black_point".to_string(), bp_str);
                }

                if let Some(gamma_str) = gamma_str {
                    node.metadata.set_property("gamma".to_string(), gamma_str);
                }

                if let Some(matrix_str) = matrix_str {
                    node.metadata.set_property("matrix".to_string(), matrix_str);
                }
            }
        }

        Some(node_id)
    }

    fn parse_lab(&mut self, arr: &PdfArray) -> Option<NodeId> {
        if arr.len() < 2 {
            return None;
        }

        let node = AstNode::new(
            self.ast.next_node_id(),
            NodeType::ColorSpace,
            PdfValue::Array(arr.clone()),
        );

        let node_id = self.add_node(node)?;

        // Extract Lab parameters
        if let Some(PdfValue::Dictionary(params)) = arr.get(1) {
            // Extract values first
            let wp_str = if let Some(PdfValue::Array(wp)) = params.get("WhitePoint") {
                Some(self.array_to_string(wp))
            } else {
                None
            };

            let bp_str = if let Some(PdfValue::Array(bp)) = params.get("BlackPoint") {
                Some(self.array_to_string(bp))
            } else {
                None
            };

            let range_str = if let Some(PdfValue::Array(range)) = params.get("Range") {
                Some(self.array_to_string(range))
            } else {
                None
            };

            // Now get mutable reference and set properties
            if let Some(node) = self.ast.get_node_mut(node_id) {
                node.metadata
                    .set_property("colorspace_type".to_string(), "Lab".to_string());

                if let Some(wp_str) = wp_str {
                    node.metadata
                        .set_property("white_point".to_string(), wp_str);
                }

                if let Some(bp_str) = bp_str {
                    node.metadata
                        .set_property("black_point".to_string(), bp_str);
                }

                if let Some(range_str) = range_str {
                    node.metadata.set_property("range".to_string(), range_str);
                }
            }
        }

        Some(node_id)
    }

    fn parse_separation(&mut self, arr: &PdfArray) -> Option<NodeId> {
        if arr.len() < 4 {
            return None;
        }

        let mut node = AstNode::new(
            self.ast.next_node_id(),
            NodeType::Separation,
            PdfValue::Array(arr.clone()),
        );

        // Extract colorant name
        if let Some(PdfValue::Name(name)) = arr.get(1) {
            node.metadata
                .set_property("colorant".to_string(), name.without_slash().to_string());
        }

        let sep_id = self.add_node(node)?;

        // Link to alternate color space
        if let Some(alt_id) = arr
            .get(2)
            .and_then(|value| self.parse_colorspace_inner(value))
        {
            self.add_edge(sep_id, alt_id, crate::ast::EdgeType::Reference);
        }

        // Link to tint transform function
        match arr.get(3)? {
            PdfValue::Reference(obj_id) => {
                if let Some(func_id) = self.resolver.get_node_id(&obj_id.object_id()) {
                    self.add_edge(sep_id, func_id, crate::ast::EdgeType::Reference);

                    if let Some(func_node) = self.ast.get_node_mut(func_id) {
                        func_node.node_type = NodeType::Function;
                    }
                }
            }
            PdfValue::Dictionary(func_dict) => {
                let func_node = AstNode::new(
                    self.ast.next_node_id(),
                    NodeType::Function,
                    PdfValue::Dictionary(func_dict.clone()),
                );
                let func_id = self.add_node(func_node)?;
                self.add_edge(sep_id, func_id, crate::ast::EdgeType::Reference);
            }
            _ => {}
        }

        Some(sep_id)
    }

    fn parse_device_n(&mut self, arr: &PdfArray) -> Option<NodeId> {
        if arr.len() < 4 {
            return None;
        }

        let mut node = AstNode::new(
            self.ast.next_node_id(),
            NodeType::DeviceN,
            PdfValue::Array(arr.clone()),
        );

        // Extract colorant names
        if let Some(PdfValue::Array(names)) = arr.get(1) {
            let names_str = names
                .iter()
                .filter_map(|v| match v {
                    PdfValue::Name(n) => Some(n.without_slash()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join(",");
            node.metadata
                .set_property("colorants".to_string(), names_str);
        }

        let devn_id = self.add_node(node)?;

        // Link to alternate color space
        if let Some(alt_id) = arr
            .get(2)
            .and_then(|value| self.parse_colorspace_inner(value))
        {
            self.add_edge(devn_id, alt_id, crate::ast::EdgeType::Reference);
        }

        // Link to tint transform function
        match arr.get(3)? {
            PdfValue::Reference(obj_id) => {
                if let Some(func_id) = self.resolver.get_node_id(&obj_id.object_id()) {
                    self.add_edge(devn_id, func_id, crate::ast::EdgeType::Reference);

                    if let Some(func_node) = self.ast.get_node_mut(func_id) {
                        func_node.node_type = NodeType::Function;
                    }
                }
            }
            PdfValue::Dictionary(func_dict) => {
                let func_node = AstNode::new(
                    self.ast.next_node_id(),
                    NodeType::Function,
                    PdfValue::Dictionary(func_dict.clone()),
                );
                let func_id = self.add_node(func_node)?;
                self.add_edge(devn_id, func_id, crate::ast::EdgeType::Reference);
            }
            _ => {}
        }

        // Process attributes (spot colors, process colors, etc.)
        if arr.len() > 4 {
            if let Some(PdfValue::Dictionary(attrs)) = arr.get(4) {
                self.process_device_n_attributes(devn_id, attrs);
            }
        }

        Some(devn_id)
    }

    fn process_device_n_attributes(&mut self, node_id: NodeId, attrs: &PdfDictionary) {
        if let Some(node) = self.ast.get_node_mut(node_id) {
            // Subtype (NChannel or DeviceN)
            if let Some(PdfValue::Name(subtype)) = attrs.get("Subtype") {
                node.metadata
                    .set_property("subtype".to_string(), subtype.without_slash().to_string());
            }

            // Process dictionary
            if let Some(PdfValue::Dictionary(process)) = attrs.get("Process") {
                if let Some(PdfValue::Name(cs)) = process.get("ColorSpace") {
                    node.metadata.set_property(
                        "process_colorspace".to_string(),
                        cs.without_slash().to_string(),
                    );
                }
            }

            // Colorants dictionary
            if let Some(PdfValue::Dictionary(colorants)) = attrs.get("Colorants") {
                let count = colorants.len();
                node.metadata
                    .set_property("colorants_count".to_string(), count.to_string());
            }
        }
    }

    fn parse_indexed(&mut self, arr: &PdfArray) -> Option<NodeId> {
        if arr.len() < 4 {
            return None;
        }

        let mut node = AstNode::new(
            self.ast.next_node_id(),
            NodeType::Indexed,
            PdfValue::Array(arr.clone()),
        );

        // Extract max index
        if let Some(PdfValue::Integer(max)) = arr.get(2) {
            node.metadata
                .set_property("max_index".to_string(), max.to_string());
        }

        let indexed_id = self.add_node(node)?;

        // Link to base color space
        if let Some(base_id) = arr
            .get(1)
            .and_then(|value| self.parse_colorspace_inner(value))
        {
            self.add_edge(indexed_id, base_id, crate::ast::EdgeType::Reference);
        }

        // Process lookup table
        match arr.get(3)? {
            PdfValue::String(s) => {
                if let Some(node) = self.ast.get_node_mut(indexed_id) {
                    node.metadata
                        .set_property("lookup_size".to_string(), s.as_bytes().len().to_string());
                }
            }
            PdfValue::Stream(_) => {
                if let Some(node) = self.ast.get_node_mut(indexed_id) {
                    node.metadata
                        .set_property("lookup_type".to_string(), "stream".to_string());
                }
            }
            PdfValue::Reference(obj_id) => {
                if let Some(lookup_id) = self.resolver.get_node_id(&obj_id.object_id()) {
                    self.add_edge(indexed_id, lookup_id, crate::ast::EdgeType::Reference);
                }
            }
            _ => {}
        }

        Some(indexed_id)
    }

    fn parse_pattern(&mut self, arr: &PdfArray) -> Option<NodeId> {
        let mut node = AstNode::new(
            self.ast.next_node_id(),
            NodeType::Pattern,
            PdfValue::Array(arr.clone()),
        );

        // Pattern can have an optional underlying color space
        if arr.len() > 1 {
            node.metadata
                .set_property("has_underlying".to_string(), "true".to_string());
        }

        let pattern_id = self.add_node(node)?;

        // Link to underlying color space if present
        if arr.len() > 1 {
            if let Some(underlying_id) = arr
                .get(1)
                .and_then(|value| self.parse_colorspace_inner(value))
            {
                self.add_edge(pattern_id, underlying_id, crate::ast::EdgeType::Reference);
            }
        }

        Some(pattern_id)
    }

    fn add_node(&mut self, node: AstNode) -> Option<NodeId> {
        if let Err(error) = self.limits.budget.consume_node() {
            self.budget_error = Some(error);
            return None;
        }
        Some(self.ast.add_node(node))
    }

    fn add_edge(&mut self, from: NodeId, to: NodeId, edge_type: crate::ast::EdgeType) -> bool {
        if let Err(error) = self.limits.budget.consume_edge() {
            self.budget_error = Some(error);
            return false;
        }
        self.ast.add_edge(from, to, edge_type)
    }

    fn array_to_string(&self, arr: &PdfArray) -> String {
        arr.iter()
            .map(|v| self.value_to_string(v))
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn value_to_string(&self, value: &PdfValue) -> String {
        match value {
            PdfValue::Integer(i) => i.to_string(),
            PdfValue::Real(r) => format!("{:.4}", r),
            PdfValue::Name(n) => n.without_slash().to_string(),
            PdfValue::String(s) => s.to_string_lossy(),
            PdfValue::Array(a) => format!("[{}]", self.array_to_string(a)),
            _ => "?".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::performance::ResourceBudget;

    #[test]
    fn malformed_color_space_arrays_are_rejected_without_panicking() {
        let mut ast = PdfAstGraph::new();
        let resolver = ObjectNodeMap::new();
        let limits = PerformanceLimits::default();
        let malformed = [
            PdfValue::Array(PdfArray::from(vec![PdfValue::Name(PdfName::new(
                "ICCBased",
            ))])),
            PdfValue::Array(PdfArray::from(vec![PdfValue::Name(PdfName::new(
                "Separation",
            ))])),
            PdfValue::Array(PdfArray::from(vec![PdfValue::Name(PdfName::new(
                "DeviceN",
            ))])),
            PdfValue::Array(PdfArray::from(vec![PdfValue::Name(PdfName::new(
                "Indexed",
            ))])),
        ];

        let mut parser = ColorSpaceParser::new_with_limits(&mut ast, &resolver, &limits);
        for value in malformed {
            assert!(parser.parse_colorspace(&value).is_none());
        }
    }

    #[test]
    fn reports_color_space_budget_exhaustion() {
        let mut ast = PdfAstGraph::new();
        let resolver = ObjectNodeMap::new();
        let limits = PerformanceLimits {
            budget: ResourceBudget::new(1024, 1024, 1024, 10, 10, 0, 10, 8),
            ..PerformanceLimits::default()
        };
        let mut parser = ColorSpaceParser::new_with_limits(&mut ast, &resolver, &limits);

        let error = parser
            .parse_colorspace_with_budget(&PdfValue::Name(PdfName::new("DeviceRGB")))
            .expect_err("color space node budget must propagate");
        assert_eq!(error, ResourceBudgetError::Nodes);
    }
}
