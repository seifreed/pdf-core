use crate::ast::{AstNode, NodeId, NodeType, PdfAstGraph};
use crate::parser::reference_resolver::ObjectNodeMap;
use crate::performance::{ResourceBudget, ResourceBudgetError};
use crate::types::{PdfDictionary, PdfValue};

/// Parser for ExtGState (Extended Graphics State) parameters
pub struct ExtGStateParser<'a> {
    ast: &'a mut PdfAstGraph,
    resolver: &'a ObjectNodeMap,
    budget: ResourceBudget,
    budget_error: Option<ResourceBudgetError>,
}

impl<'a> ExtGStateParser<'a> {
    pub fn new(ast: &'a mut PdfAstGraph, resolver: &'a ObjectNodeMap) -> Self {
        Self::new_with_budget(ast, resolver, &ResourceBudget::default())
    }

    pub fn new_with_budget(
        ast: &'a mut PdfAstGraph,
        resolver: &'a ObjectNodeMap,
        budget: &ResourceBudget,
    ) -> Self {
        ExtGStateParser {
            ast,
            resolver,
            budget: budget.clone(),
            budget_error: None,
        }
    }

    pub fn parse_extgstate(&mut self, gs_dict: &PdfDictionary, gs_id: NodeId) {
        self.budget_error = None;
        self.parse_extgstate_inner(gs_dict, gs_id);
    }

    pub fn parse_extgstate_with_budget(
        &mut self,
        gs_dict: &PdfDictionary,
        gs_id: NodeId,
    ) -> Result<(), ResourceBudgetError> {
        self.budget_error = None;
        self.parse_extgstate_inner(gs_dict, gs_id);
        match self.budget_error.take() {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    fn parse_extgstate_inner(&mut self, gs_dict: &PdfDictionary, gs_id: NodeId) {
        // Extract values first to avoid borrow checker issues
        let line_width = gs_dict
            .get("LW")
            .and_then(|value| self.get_number(value))
            .map(|value| value.to_string());

        let line_cap = gs_dict
            .get("LC")
            .and_then(|value| self.get_number(value))
            .map(|value| value.to_string());

        let line_join = gs_dict
            .get("LJ")
            .and_then(|value| self.get_number(value))
            .map(|value| value.to_string());

        let miter_limit = gs_dict
            .get("ML")
            .and_then(|ml| self.get_number(ml))
            .map(|val| val.to_string());

        let rendering_intent = gs_dict.get("RI").and_then(|v| match v {
            PdfValue::Name(ri) => Some(ri.without_slash().to_string()),
            _ => None,
        });

        let overprint = gs_dict.get("OP").and_then(|v| match v {
            PdfValue::Boolean(op) => Some(op.to_string()),
            _ => None,
        });

        let overprint_fill = gs_dict.get("op").and_then(|v| match v {
            PdfValue::Boolean(op) => Some(op.to_string()),
            _ => None,
        });

        let overprint_mode = gs_dict
            .get("OPM")
            .and_then(|value| self.get_number(value))
            .map(|value| value.to_string());

        let flatness = gs_dict
            .get("FL")
            .and_then(|fl| self.get_number(fl))
            .map(|val| val.to_string());

        let smoothness = gs_dict
            .get("SM")
            .and_then(|sm| self.get_number(sm))
            .map(|val| val.to_string());

        let text_knockout = gs_dict.get("TK").and_then(|v| match v {
            PdfValue::Boolean(tk) => Some(tk.to_string()),
            _ => None,
        });

        // Now update node with extracted values
        if let Some(node) = self.ast.get_node_mut(gs_id) {
            node.node_type = NodeType::ExtGState;

            if let Some(lw) = line_width {
                node.metadata.set_property("line_width".to_string(), lw);
            }

            if let Some(lc) = line_cap {
                node.metadata.set_property("line_cap".to_string(), lc);
            }

            if let Some(lj) = line_join {
                node.metadata.set_property("line_join".to_string(), lj);
            }

            if let Some(ml) = miter_limit {
                node.metadata.set_property("miter_limit".to_string(), ml);
            }

            if let Some(ri) = rendering_intent {
                node.metadata
                    .set_property("rendering_intent".to_string(), ri);
            }

            if let Some(op) = overprint {
                node.metadata.set_property("overprint".to_string(), op);
            }

            if let Some(op) = overprint_fill {
                node.metadata.set_property("overprint_fill".to_string(), op);
            }

            if let Some(opm) = overprint_mode {
                node.metadata
                    .set_property("overprint_mode".to_string(), opm);
            }

            if let Some(fl) = flatness {
                node.metadata.set_property("flatness".to_string(), fl);
            }

            if let Some(sm) = smoothness {
                node.metadata.set_property("smoothness".to_string(), sm);
            }

            if let Some(tk) = text_knockout {
                node.metadata.set_property("text_knockout".to_string(), tk);
            }
        }

        // Alpha values
        self.parse_alpha_values(gs_dict, gs_id);

        // Blend mode
        self.parse_blend_mode(gs_dict, gs_id);

        // Soft mask
        self.parse_soft_mask(gs_dict, gs_id);

        // Transfer function
        self.parse_transfer_function(gs_dict, gs_id);

        // Font
        self.parse_font_reference(gs_dict, gs_id);

        // Halftone
        self.parse_halftone(gs_dict, gs_id);

        // Black generation/Undercolor removal
        self.parse_color_rendering(gs_dict, gs_id);
    }

    fn parse_alpha_values(&mut self, gs_dict: &PdfDictionary, gs_id: NodeId) {
        // Constant alpha for stroking
        if let Some(ca) = gs_dict.get("CA") {
            if let Some(val) = self.get_number(ca) {
                if let Some(node) = self.ast.get_node_mut(gs_id) {
                    node.metadata
                        .set_property("alpha_stroke".to_string(), val.to_string());
                }
            }
        }

        // Constant alpha for non-stroking
        if let Some(ca) = gs_dict.get("ca") {
            if let Some(val) = self.get_number(ca) {
                if let Some(node) = self.ast.get_node_mut(gs_id) {
                    node.metadata
                        .set_property("alpha_fill".to_string(), val.to_string());
                }
            }
        }

        // Alpha is shape
        if let Some(PdfValue::Boolean(ais)) = gs_dict.get("AIS") {
            if let Some(node) = self.ast.get_node_mut(gs_id) {
                node.metadata
                    .set_property("alpha_is_shape".to_string(), ais.to_string());
            }
        }
    }

    fn parse_blend_mode(&mut self, gs_dict: &PdfDictionary, gs_id: NodeId) {
        if let Some(bm) = gs_dict.get("BM") {
            let mode = match bm {
                PdfValue::Name(n) => n.without_slash().to_string(),
                PdfValue::Array(arr) if !arr.is_empty() => {
                    // Array of blend modes - use first
                    if let PdfValue::Name(n) = &arr[0] {
                        n.without_slash().to_string()
                    } else {
                        "Normal".to_string()
                    }
                }
                _ => "Normal".to_string(),
            };

            if let Some(node) = self.ast.get_node_mut(gs_id) {
                node.metadata.set_property("blend_mode".to_string(), mode);
            }
        }
    }

    fn parse_soft_mask(&mut self, gs_dict: &PdfDictionary, gs_id: NodeId) {
        match gs_dict.get("SMask") {
            Some(PdfValue::Name(n)) if n.without_slash() == "None" => {
                if let Some(node) = self.ast.get_node_mut(gs_id) {
                    node.metadata
                        .set_property("soft_mask".to_string(), "None".to_string());
                }
            }
            Some(PdfValue::Dictionary(smask_dict)) => {
                // Create soft mask node
                if let Err(error) = self.budget.consume_node() {
                    self.budget_error = Some(error);
                    return;
                }
                let smask_node = AstNode::new(
                    self.ast.next_node_id(),
                    NodeType::Unknown,
                    PdfValue::Dictionary(smask_dict.clone()),
                );
                let smask_id = self.ast.add_node(smask_node);

                // Link to ExtGState
                self.add_edge(gs_id, smask_id, crate::ast::EdgeType::Reference);

                // Parse soft mask parameters
                self.parse_soft_mask_dict(smask_dict, smask_id);

                if let Some(node) = self.ast.get_node_mut(gs_id) {
                    node.metadata
                        .set_property("soft_mask".to_string(), "Present".to_string());
                }
            }
            Some(PdfValue::Reference(obj_id)) => {
                if let Some(smask_id) = self.resolver.get_node_id(&obj_id.object_id()) {
                    self.add_edge(gs_id, smask_id, crate::ast::EdgeType::Reference);

                    if let Some(node) = self.ast.get_node_mut(gs_id) {
                        node.metadata
                            .set_property("soft_mask".to_string(), "Reference".to_string());
                    }
                }
            }
            _ => {}
        }
    }

    fn parse_soft_mask_dict(&mut self, smask_dict: &PdfDictionary, smask_id: NodeId) {
        // Extract values first
        let subtype = if let Some(PdfValue::Name(s)) = smask_dict.get("S") {
            Some(s.without_slash().to_string())
        } else {
            None
        };

        let group_id = if let Some(PdfValue::Reference(g_ref)) = smask_dict.get("G") {
            self.resolver.get_node_id(&g_ref.object_id())
        } else {
            None
        };

        let bc_str = if let Some(PdfValue::Array(bc)) = smask_dict.get("BC") {
            Some(
                bc.iter()
                    .filter_map(|v| self.get_number(v))
                    .map(|n| n.to_string())
                    .collect::<Vec<_>>()
                    .join(" "),
            )
        } else {
            None
        };

        // Now get mutable node and set properties
        if let Some(node) = self.ast.get_node_mut(smask_id) {
            if let Some(subtype) = subtype {
                node.metadata.set_property("subtype".to_string(), subtype);
            }

            if let Some(bc_str) = bc_str {
                node.metadata
                    .set_property("background_color".to_string(), bc_str);
            }
        }

        // Add edge after releasing the mutable borrow
        if let Some(group_id) = group_id {
            self.add_edge(smask_id, group_id, crate::ast::EdgeType::Reference);
        }

        // Handle transfer function
        if let Some(tr) = smask_dict.get("TR") {
            match tr {
                PdfValue::Name(n) if n.without_slash() == "Identity" => {
                    if let Some(node) = self.ast.get_node_mut(smask_id) {
                        node.metadata
                            .set_property("transfer_function".to_string(), "Identity".to_string());
                    }
                }
                PdfValue::Reference(tr_ref) => {
                    if let Some(tr_id) = self.resolver.get_node_id(&tr_ref.object_id()) {
                        self.add_edge(smask_id, tr_id, crate::ast::EdgeType::Reference);
                        if let Some(node) = self.ast.get_node_mut(smask_id) {
                            node.metadata.set_property(
                                "transfer_function".to_string(),
                                "Function".to_string(),
                            );
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn parse_transfer_function(&mut self, gs_dict: &PdfDictionary, gs_id: NodeId) {
        // TR - Transfer function
        if let Some(tr) = gs_dict.get("TR") {
            match tr {
                PdfValue::Name(n) if n.without_slash() == "Identity" => {
                    if let Some(node) = self.ast.get_node_mut(gs_id) {
                        node.metadata
                            .set_property("transfer".to_string(), "Identity".to_string());
                    }
                }
                PdfValue::Reference(tr_ref) => {
                    if let Some(tr_id) = self.resolver.get_node_id(&tr_ref.object_id()) {
                        self.add_edge(gs_id, tr_id, crate::ast::EdgeType::Reference);

                        if let Some(node) = self.ast.get_node_mut(tr_id) {
                            node.node_type = NodeType::Function;
                        }
                    }
                }
                PdfValue::Array(funcs) => {
                    // Array of transfer functions (one per component)
                    for (i, func) in funcs.iter().enumerate() {
                        if let PdfValue::Reference(func_ref) = func {
                            if let Some(func_id) = self.resolver.get_node_id(&func_ref.id()) {
                                self.add_edge(gs_id, func_id, crate::ast::EdgeType::Reference);

                                if let Some(node) = self.ast.get_node_mut(func_id) {
                                    node.node_type = NodeType::Function;
                                    node.metadata
                                        .set_property("component".to_string(), i.to_string());
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        // TR2 - Transfer function (PDF 1.3)
        if let Some(tr2) = gs_dict.get("TR2") {
            self.parse_transfer_function_v2(tr2, gs_id);
        }
    }

    fn parse_transfer_function_v2(&mut self, tr2: &PdfValue, gs_id: NodeId) {
        match tr2 {
            PdfValue::Name(n) => {
                let val = if n.without_slash() == "Default" {
                    "Default"
                } else {
                    "Identity"
                };
                if let Some(node) = self.ast.get_node_mut(gs_id) {
                    node.metadata
                        .set_property("transfer2".to_string(), val.to_string());
                }
            }
            PdfValue::Reference(tr_ref) => {
                if let Some(tr_id) = self.resolver.get_node_id(&tr_ref.object_id()) {
                    self.add_edge(gs_id, tr_id, crate::ast::EdgeType::Reference);
                }
            }
            _ => {}
        }
    }

    fn parse_font_reference(&mut self, gs_dict: &PdfDictionary, gs_id: NodeId) {
        if let Some(PdfValue::Array(font_arr)) = gs_dict.get("Font") {
            if font_arr.len() >= 2 {
                // First element is font reference
                if let PdfValue::Reference(font_ref) = &font_arr[0] {
                    if let Some(font_id) = self.resolver.get_node_id(&font_ref.id()) {
                        self.add_edge(gs_id, font_id, crate::ast::EdgeType::Reference);

                        if let Some(node) = self.ast.get_node_mut(font_id) {
                            node.node_type = NodeType::Font;
                        }
                    }
                }

                // Second element is font size
                if let Some(size) = self.get_number(&font_arr[1]) {
                    if let Some(node) = self.ast.get_node_mut(gs_id) {
                        node.metadata
                            .set_property("font_size".to_string(), size.to_string());
                    }
                }
            }
        }
    }

    fn parse_halftone(&mut self, gs_dict: &PdfDictionary, gs_id: NodeId) {
        match gs_dict.get("HT") {
            Some(PdfValue::Name(n)) if n.without_slash() == "Default" => {
                if let Some(node) = self.ast.get_node_mut(gs_id) {
                    node.metadata
                        .set_property("halftone".to_string(), "Default".to_string());
                }
            }
            Some(PdfValue::Reference(ht_ref)) => {
                if let Some(ht_id) = self.resolver.get_node_id(&ht_ref.id()) {
                    self.add_edge(gs_id, ht_id, crate::ast::EdgeType::Reference);

                    if let Some(node) = self.ast.get_node_mut(gs_id) {
                        node.metadata
                            .set_property("halftone".to_string(), "Custom".to_string());
                    }
                }
            }
            Some(PdfValue::Dictionary(ht_dict)) => {
                // Inline halftone dictionary
                if let Err(error) = self.budget.consume_node() {
                    self.budget_error = Some(error);
                    return;
                }
                let ht_node = AstNode::new(
                    self.ast.next_node_id(),
                    NodeType::Unknown,
                    PdfValue::Dictionary(ht_dict.clone()),
                );
                let ht_id = self.ast.add_node(ht_node);
                self.add_edge(gs_id, ht_id, crate::ast::EdgeType::Reference);

                if let Some(node) = self.ast.get_node_mut(gs_id) {
                    node.metadata
                        .set_property("halftone".to_string(), "Inline".to_string());
                }
            }
            _ => {}
        }
    }

    fn parse_color_rendering(&mut self, gs_dict: &PdfDictionary, gs_id: NodeId) {
        // Black generation
        if let Some(bg) = gs_dict.get("BG") {
            self.parse_color_function(bg, gs_id, "black_generation");
        }

        // Black generation (PDF 1.3)
        if let Some(bg2) = gs_dict.get("BG2") {
            self.parse_color_function(bg2, gs_id, "black_generation2");
        }

        // Undercolor removal
        if let Some(ucr) = gs_dict.get("UCR") {
            self.parse_color_function(ucr, gs_id, "undercolor_removal");
        }

        // Undercolor removal (PDF 1.3)
        if let Some(ucr2) = gs_dict.get("UCR2") {
            self.parse_color_function(ucr2, gs_id, "undercolor_removal2");
        }
    }

    fn parse_color_function(&mut self, value: &PdfValue, gs_id: NodeId, property: &str) {
        match value {
            PdfValue::Name(n) if n.without_slash() == "Default" => {
                if let Some(node) = self.ast.get_node_mut(gs_id) {
                    node.metadata
                        .set_property(property.to_string(), "Default".to_string());
                }
            }
            PdfValue::Reference(func_ref) => {
                if let Some(func_id) = self.resolver.get_node_id(&func_ref.id()) {
                    self.add_edge(gs_id, func_id, crate::ast::EdgeType::Reference);

                    if let Some(node) = self.ast.get_node_mut(func_id) {
                        node.node_type = NodeType::Function;
                        node.metadata
                            .set_property("function_role".to_string(), property.to_string());
                    }
                }
            }
            _ => {}
        }
    }

    fn get_number(&self, value: &PdfValue) -> Option<f64> {
        match value {
            PdfValue::Integer(i) => Some(*i as f64),
            PdfValue::Real(r) => Some(*r),
            PdfValue::Reference(reference) => self
                .resolver
                .get_node_id(&reference.id())
                .and_then(|node_id| self.ast.get_node(node_id))
                .and_then(|node| match &node.value {
                    PdfValue::Integer(integer) => Some(*integer as f64),
                    PdfValue::Real(real) => Some(*real),
                    _ => None,
                }),
            _ => None,
        }
    }

    fn add_edge(&mut self, from: NodeId, to: NodeId, edge_type: crate::ast::EdgeType) {
        if let Err(error) = self.budget.consume_edge() {
            self.budget_error = Some(error);
        } else {
            self.ast.add_edge(from, to, edge_type);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::AstNode;

    #[test]
    fn extgstate_parser_respects_node_and_edge_budgets() {
        let mut ast = PdfAstGraph::new();
        let gs_id = ast.add_node(AstNode::new(
            NodeId(0),
            NodeType::ExtGState,
            PdfValue::Dictionary(PdfDictionary::new()),
        ));
        let resolver = ObjectNodeMap::new();
        let budget = ResourceBudget::new(1024, 1024, 1024, 10, 10, 1, 1, 8);
        let mut parser = ExtGStateParser::new_with_budget(&mut ast, &resolver, &budget);
        let mut gs_dict = PdfDictionary::new();
        gs_dict.insert("SMask", PdfValue::Dictionary(PdfDictionary::new()));
        gs_dict.insert("HT", PdfValue::Dictionary(PdfDictionary::new()));

        parser.parse_extgstate(&gs_dict, gs_id);
        assert_eq!(ast.node_count(), 2);
        assert_eq!(ast.edge_count(), 1);
    }

    #[test]
    fn extgstate_parser_reports_node_budget_exhaustion() {
        let mut ast = PdfAstGraph::new();
        let resolver = ObjectNodeMap::new();
        let budget = ResourceBudget::new(1024, 1024, 1024, 10, 10, 0, 10, 8);
        let mut parser = ExtGStateParser::new_with_budget(&mut ast, &resolver, &budget);
        let mut gs_dict = PdfDictionary::new();
        gs_dict.insert("SMask", PdfValue::Dictionary(PdfDictionary::new()));

        assert_eq!(
            parser
                .parse_extgstate_with_budget(&gs_dict, NodeId(0))
                .expect_err("ExtGState node budget must propagate"),
            ResourceBudgetError::Nodes
        );
    }

    #[test]
    fn extgstate_parser_resolves_indirect_numeric_parameters() {
        let mut ast = PdfAstGraph::new();
        let gs_id = ast.add_node(AstNode::new(
            NodeId(0),
            NodeType::ExtGState,
            PdfValue::Dictionary(PdfDictionary::new()),
        ));
        let width_id = ast.add_node(AstNode::new(
            NodeId(1),
            NodeType::Unknown,
            PdfValue::Real(2.5),
        ));
        let cap_id = ast.add_node(AstNode::new(
            NodeId(2),
            NodeType::Unknown,
            PdfValue::Integer(1),
        ));
        let mode_id = ast.add_node(AstNode::new(
            NodeId(3),
            NodeType::Unknown,
            PdfValue::Integer(0),
        ));
        let mut resolver = ObjectNodeMap::new();
        resolver.insert(crate::types::ObjectId::new(1, 0), width_id);
        resolver.insert(crate::types::ObjectId::new(2, 0), cap_id);
        resolver.insert(crate::types::ObjectId::new(3, 0), mode_id);
        let mut gs_dict = PdfDictionary::new();
        gs_dict.insert(
            "LW",
            PdfValue::Reference(crate::types::PdfReference::new(1, 0)),
        );
        gs_dict.insert(
            "LC",
            PdfValue::Reference(crate::types::PdfReference::new(2, 0)),
        );
        gs_dict.insert(
            "OPM",
            PdfValue::Reference(crate::types::PdfReference::new(3, 0)),
        );

        let mut parser = ExtGStateParser::new(&mut ast, &resolver);
        parser.parse_extgstate(&gs_dict, gs_id);
        let node = ast.get_node(gs_id).expect("graphics state should exist");
        assert_eq!(
            node.metadata.get_property("line_width").map(String::as_str),
            Some("2.5")
        );
        assert_eq!(
            node.metadata.get_property("line_cap").map(String::as_str),
            Some("1")
        );
        assert_eq!(
            node.metadata
                .get_property("overprint_mode")
                .map(String::as_str),
            Some("0")
        );
    }
}
