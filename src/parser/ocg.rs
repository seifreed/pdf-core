use crate::ast::document::{
    BaseState, ListMode, OCDisplayDict, OCOrderItem, OptionalContentConfig,
    OptionalContentProperties,
};
use crate::ast::{AstNode, NodeId, NodeType, PdfAstGraph};
use crate::parser::reference_resolver::ObjectNodeMap;
use crate::performance::{ResourceBudget, ResourceBudgetError};
use crate::types::{PdfDictionary, PdfValue};
use std::collections::HashSet;

pub struct OCGParser<'a> {
    ast: &'a mut PdfAstGraph,
    resolver: &'a ObjectNodeMap,
    budget: ResourceBudget,
    budget_error: Option<ResourceBudgetError>,
}

impl<'a> OCGParser<'a> {
    pub fn new(ast: &'a mut PdfAstGraph, resolver: &'a ObjectNodeMap) -> Self {
        Self::new_with_budget(ast, resolver, &ResourceBudget::default())
    }

    pub fn new_with_budget(
        ast: &'a mut PdfAstGraph,
        resolver: &'a ObjectNodeMap,
        budget: &ResourceBudget,
    ) -> Self {
        OCGParser {
            ast,
            resolver,
            budget: budget.clone(),
            budget_error: None,
        }
    }

    pub fn parse_ocproperties(
        &mut self,
        ocprops_dict: &PdfDictionary,
    ) -> Option<OptionalContentConfig> {
        self.parse_ocproperties_inner(ocprops_dict)
    }

    pub fn parse_ocproperties_with_budget(
        &mut self,
        ocprops_dict: &PdfDictionary,
    ) -> Result<Option<OptionalContentConfig>, ResourceBudgetError> {
        self.budget_error = None;
        let result = self.parse_ocproperties_inner(ocprops_dict);
        match self.budget_error.take() {
            Some(error) => Err(error),
            None => Ok(result),
        }
    }

    fn parse_ocproperties_inner(
        &mut self,
        ocprops_dict: &PdfDictionary,
    ) -> Option<OptionalContentConfig> {
        // Get all OCGs
        let ocgs = self.parse_ocg_array(ocprops_dict.get("OCGs")?)?;

        // Get default configuration
        let default_config = self.parse_occonfig(ocprops_dict.get("D")?)?;

        // Get additional configurations
        let mut configs = Vec::new();
        if let Some(PdfValue::Array(configs_array)) = ocprops_dict.get("Configs") {
            for config_ref in configs_array {
                if let Some(config_id) = self.resolve_reference(config_ref) {
                    configs.push(config_id);
                }
            }
        }

        // Parse properties from default config
        let properties = self.parse_occonfig_properties(ocprops_dict.get("D")?)?;

        Some(OptionalContentConfig {
            ocgs,
            default_config,
            configs,
            properties,
        })
    }

    fn parse_ocg_array(&mut self, value: &PdfValue) -> Option<Vec<NodeId>> {
        match value {
            PdfValue::Array(arr) => {
                let mut ocg_nodes = Vec::new();

                for ocg_ref in arr {
                    if let Some(ocg_id) = self.resolve_reference(ocg_ref) {
                        // Extract metadata from the dict first
                        let (ocg_name, intent_str, has_usage) =
                            if let Some(node) = self.ast.get_node(ocg_id) {
                                if let Some(dict) = node.as_dict() {
                                    let name = dict.get("Name").and_then(|v| match v {
                                        PdfValue::String(s) => Some(s.to_string_lossy()),
                                        _ => None,
                                    });

                                    let intent_str =
                                        dict.get("Intent").map(|intent| self.format_intent(intent));

                                    let has_usage = dict.contains_key("Usage");

                                    (name, intent_str, has_usage)
                                } else {
                                    (None, None, false)
                                }
                            } else {
                                (None, None, false)
                            };

                        // Update node type and metadata
                        if let Some(node) = self.ast.get_node_mut(ocg_id) {
                            node.node_type = NodeType::OCG;

                            if let Some(name) = ocg_name {
                                node.metadata.set_property("ocg_name".to_string(), name);
                            }

                            if let Some(intent) = intent_str {
                                node.metadata.set_property("intent".to_string(), intent);
                            }

                            if has_usage {
                                node.metadata
                                    .set_property("has_usage".to_string(), "true".to_string());
                            }
                        }

                        ocg_nodes.push(ocg_id);
                    }
                }

                Some(ocg_nodes)
            }
            _ => None,
        }
    }

    fn parse_occonfig(&mut self, value: &PdfValue) -> Option<NodeId> {
        match value {
            PdfValue::Dictionary(dict) => {
                if let Err(error) = self.budget.consume_node() {
                    self.budget_error = Some(error);
                    return None;
                }
                // Create inline configuration node
                let config_node = AstNode::new(
                    self.ast.next_node_id(),
                    NodeType::OCProperties,
                    PdfValue::Dictionary(dict.clone()),
                );
                Some(self.ast.add_node(config_node))
            }
            PdfValue::Reference(obj_id) => self.resolver.get_node_id(&obj_id.id()),
            _ => None,
        }
    }

    fn parse_occonfig_properties(&mut self, value: &PdfValue) -> Option<OptionalContentProperties> {
        let dict = match value {
            PdfValue::Dictionary(d) => d.clone(),
            PdfValue::Reference(obj_id) => {
                let node_id = self.resolver.get_node_id(&obj_id.id())?;
                let node = self.ast.get_node(node_id)?;
                node.as_dict()?.clone()
            }
            _ => return None,
        };

        Some(OptionalContentProperties {
            d: self.parse_display_dict(&dict),
            base_state: self.parse_base_state(&dict),
            on: self.parse_ocg_refs(dict.get("ON")),
            off: self.parse_ocg_refs(dict.get("OFF")),
            order: self.parse_order(dict.get("Order")),
            list_mode: self.parse_list_mode(&dict),
            rb_groups: self.parse_rb_groups(dict.get("RBGroups")),
            locked: self.parse_ocg_refs(dict.get("Locked")),
        })
    }

    fn parse_display_dict(&self, dict: &PdfDictionary) -> OCDisplayDict {
        OCDisplayDict {
            name: dict.get("Name").and_then(|v| match v {
                PdfValue::String(s) => Some(s.to_string_lossy()),
                _ => None,
            }),
            creator: dict.get("Creator").and_then(|v| match v {
                PdfValue::String(s) => Some(s.to_string_lossy()),
                _ => None,
            }),
        }
    }

    fn parse_base_state(&self, dict: &PdfDictionary) -> BaseState {
        match dict.get("BaseState") {
            Some(PdfValue::Name(name)) => match name.without_slash() {
                "ON" => BaseState::On,
                "OFF" => BaseState::Off,
                _ => BaseState::Unchanged,
            },
            _ => BaseState::On,
        }
    }

    fn parse_ocg_refs(&mut self, value: Option<&PdfValue>) -> Vec<NodeId> {
        let mut refs = Vec::new();

        if let Some(PdfValue::Array(arr)) = value {
            for item in arr {
                if let Some(node_id) = self.resolve_reference(item) {
                    refs.push(node_id);
                }
            }
        }

        refs
    }

    fn parse_order(&mut self, value: Option<&PdfValue>) -> Vec<OCOrderItem> {
        let mut order = Vec::new();

        if let Some(PdfValue::Array(arr)) = value {
            for item in arr {
                if let Some(order_item) = self.parse_order_item(item) {
                    order.push(order_item);
                }
            }
        }

        order
    }

    fn parse_order_item(&mut self, value: &PdfValue) -> Option<OCOrderItem> {
        self.parse_order_item_at_depth(value, 0)
    }

    fn parse_order_item_at_depth(&mut self, value: &PdfValue, depth: usize) -> Option<OCOrderItem> {
        if depth >= self.budget.max_depth {
            return None;
        }

        match value {
            PdfValue::Reference(_) => self.resolve_reference(value).map(OCOrderItem::Group),
            PdfValue::String(s) => Some(OCOrderItem::Label(s.to_string_lossy())),
            PdfValue::Array(arr) => {
                let mut items = Vec::new();
                for sub_item in arr {
                    if let Some(item) = self.parse_order_item_at_depth(sub_item, depth + 1) {
                        items.push(item);
                    }
                }
                Some(OCOrderItem::Array(items))
            }
            _ => None,
        }
    }

    fn parse_list_mode(&self, dict: &PdfDictionary) -> ListMode {
        match dict.get("ListMode") {
            Some(PdfValue::Name(name)) if name.without_slash() == "VisiblePages" => {
                ListMode::VisiblePages
            }
            _ => ListMode::AllPages,
        }
    }

    fn parse_rb_groups(&mut self, value: Option<&PdfValue>) -> Vec<Vec<NodeId>> {
        let mut groups = Vec::new();

        if let Some(PdfValue::Array(arr)) = value {
            for group_value in arr {
                if let PdfValue::Array(group_arr) = group_value {
                    let mut group = Vec::new();
                    for item in group_arr {
                        if let Some(node_id) = self.resolve_reference(item) {
                            group.push(node_id);
                        }
                    }
                    if !group.is_empty() {
                        groups.push(group);
                    }
                }
            }
        }

        groups
    }

    fn resolve_reference(&self, value: &PdfValue) -> Option<NodeId> {
        match value {
            PdfValue::Reference(obj_id) => self.resolver.get_node_id(&obj_id.id()),
            _ => None,
        }
    }

    fn format_intent(&self, value: &PdfValue) -> String {
        match value {
            PdfValue::Name(n) => n.without_slash().to_string(),
            PdfValue::Array(arr) => arr
                .iter()
                .filter_map(|v| match v {
                    PdfValue::Name(n) => Some(n.without_slash()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join(","),
            _ => "View".to_string(),
        }
    }

    pub fn parse_ocmd(&mut self, ocmd_dict: &PdfDictionary) -> Option<NodeId> {
        self.parse_ocmd_with_budget(ocmd_dict).ok().flatten()
    }

    pub fn parse_ocmd_with_budget(
        &mut self,
        ocmd_dict: &PdfDictionary,
    ) -> Result<Option<NodeId>, ResourceBudgetError> {
        self.budget_error = None;
        let result = self.parse_ocmd_inner(ocmd_dict);
        match self.budget_error.take() {
            Some(error) => Err(error),
            None => Ok(result),
        }
    }

    fn parse_ocmd_inner(&mut self, ocmd_dict: &PdfDictionary) -> Option<NodeId> {
        if let Err(error) = self.budget.consume_node() {
            self.budget_error = Some(error);
            return None;
        }
        // Create OCMD node
        let mut node = AstNode::new(
            self.ast.next_node_id(),
            NodeType::OCMD,
            PdfValue::Dictionary(ocmd_dict.clone()),
        );

        // Extract OCMD type
        if let Some(PdfValue::Name(typ)) = ocmd_dict.get("Type") {
            if typ.without_slash() != "OCMD" {
                return None;
            }
        }

        // Extract policy
        if let Some(PdfValue::Name(policy)) = ocmd_dict.get("P") {
            node.metadata
                .set_property("policy".to_string(), policy.without_slash().to_string());
        }

        // Extract visibility expression
        if let Some(ve) = ocmd_dict.get("VE") {
            let ve_str = self.format_visibility_expression(ve);
            node.metadata
                .set_property("visibility_expression".to_string(), ve_str);
        }

        let ocmd_id = self.ast.add_node(node);

        // Link to OCGs
        if let Some(PdfValue::Array(ocgs)) = ocmd_dict.get("OCGs") {
            for ocg_ref in ocgs {
                if let Some(ocg_id) = self.resolve_reference(ocg_ref) {
                    self.ast
                        .add_edge(ocmd_id, ocg_id, crate::ast::EdgeType::Reference);
                }
            }
        }

        Some(ocmd_id)
    }

    fn format_visibility_expression(&self, value: &PdfValue) -> String {
        self.format_visibility_expression_at_depth(value, 0)
    }

    fn format_visibility_expression_at_depth(&self, value: &PdfValue, depth: usize) -> String {
        if depth >= self.budget.max_depth {
            return "<depth-limit>".to_string();
        }

        match value {
            PdfValue::Array(arr) if !arr.is_empty() => {
                if let PdfValue::Name(op) = &arr[0] {
                    let operator = op.without_slash();
                    let operands: Vec<String> = arr
                        .iter()
                        .skip(1)
                        .map(|v| self.format_visibility_expression_at_depth(v, depth + 1))
                        .collect();
                    format!("{}({})", operator, operands.join(", "))
                } else {
                    "array".to_string()
                }
            }
            PdfValue::Reference(obj_id) => {
                format!("ref:{} {}", obj_id.number(), obj_id.generation())
            }
            PdfValue::Name(n) => n.without_slash().to_string(),
            _ => "?".to_string(),
        }
    }

    pub fn get_visible_ocgs(
        &self,
        config: &OptionalContentConfig,
        context: &OCContext,
    ) -> HashSet<NodeId> {
        let mut visible = HashSet::new();

        // Start with base state
        match config.properties.base_state {
            BaseState::On => {
                visible.extend(&config.ocgs);
            }
            BaseState::Off => {
                // Start with none visible
            }
            BaseState::Unchanged => {
                // Use previous state (context-dependent)
                visible.extend(&context.previous_visible);
            }
        }

        // Apply ON array
        for ocg_id in &config.properties.on {
            visible.insert(*ocg_id);
        }

        // Apply OFF array
        for ocg_id in &config.properties.off {
            visible.remove(ocg_id);
        }

        // Apply radio button constraints
        for rb_group in &config.properties.rb_groups {
            let mut group_has_visible = false;
            for ocg_id in rb_group {
                if visible.contains(ocg_id) {
                    if group_has_visible {
                        // Only one can be visible in radio group
                        visible.remove(ocg_id);
                    } else {
                        group_has_visible = true;
                    }
                }
            }
        }

        visible
    }

    pub fn evaluate_ocmd(&self, ocmd: &PdfDictionary, visible_ocgs: &HashSet<NodeId>) -> bool {
        // Get policy (default is AnyOn)
        let policy = ocmd
            .get("P")
            .and_then(|v| match v {
                PdfValue::Name(n) => Some(n.without_slash()),
                _ => None,
            })
            .unwrap_or("AnyOn");

        // Get OCGs referenced by this OCMD
        let mut ocg_ids = Vec::new();
        if let Some(PdfValue::Array(ocgs)) = ocmd.get("OCGs") {
            for ocg_ref in ocgs {
                if let Some(ocg_id) = self.resolve_reference(ocg_ref) {
                    ocg_ids.push(ocg_id);
                }
            }
        }

        // Evaluate based on policy
        match policy {
            "AllOn" => ocg_ids.iter().all(|id| visible_ocgs.contains(id)),
            "AnyOn" => ocg_ids.iter().any(|id| visible_ocgs.contains(id)),
            "AllOff" => ocg_ids.iter().all(|id| !visible_ocgs.contains(id)),
            "AnyOff" => ocg_ids.iter().any(|id| !visible_ocgs.contains(id)),
            _ => true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct OCContext {
    pub previous_visible: HashSet<NodeId>,
    pub current_page: Option<NodeId>,
    pub print_state: bool,
    pub export_state: bool,
    pub view_state: bool,
}

impl Default for OCContext {
    fn default() -> Self {
        Self::new()
    }
}

impl OCContext {
    pub fn new() -> Self {
        OCContext {
            previous_visible: HashSet::new(),
            current_page: None,
            print_state: false,
            export_state: false,
            view_state: true,
        }
    }

    pub fn for_viewing() -> Self {
        OCContext {
            previous_visible: HashSet::new(),
            current_page: None,
            print_state: false,
            export_state: false,
            view_state: true,
        }
    }

    pub fn for_printing() -> Self {
        OCContext {
            previous_visible: HashSet::new(),
            current_page: None,
            print_state: true,
            export_state: false,
            view_state: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::primitive::PdfName;

    #[test]
    fn visibility_expression_respects_budget_depth() {
        let mut expression = PdfValue::Name(PdfName::new("OCG"));
        for _ in 0..4 {
            expression =
                PdfValue::Array(vec![PdfValue::Name(PdfName::new("And")), expression].into());
        }

        let mut ocmd = PdfDictionary::new();
        ocmd.insert("Type", PdfValue::Name(PdfName::new("OCMD")));
        ocmd.insert("VE", expression);

        let mut ast = PdfAstGraph::new();
        let resolver = ObjectNodeMap::new();
        let budget = ResourceBudget::new(1024, 1024, 1024, 10, 10, 10, 10, 2);
        let mut parser = OCGParser::new_with_budget(&mut ast, &resolver, &budget);
        let node_id = parser.parse_ocmd(&ocmd).expect("OCMD should be parsed");

        let mut order = PdfValue::String("label".into());
        for _ in 0..4 {
            order = PdfValue::Array(vec![order].into());
        }
        let item = parser.parse_order_item(&order).expect("order should parse");
        let mut nesting = 0;
        let mut current = &item;
        while let OCOrderItem::Array(items) = current {
            nesting += 1;
            let Some(first) = items.first() else { break };
            current = first;
        }
        assert_eq!(nesting, 2);
        drop(parser);

        let expression = ast
            .get_node(node_id)
            .and_then(|node| node.metadata.properties.get("visibility_expression"))
            .expect("visibility expression metadata");
        assert!(expression.contains("<depth-limit>"));
    }

    #[test]
    fn ocmd_parser_reports_node_budget_exhaustion() {
        let mut ocmd = PdfDictionary::new();
        ocmd.insert("Type", PdfValue::Name(PdfName::new("OCMD")));
        let mut ast = PdfAstGraph::new();
        let resolver = ObjectNodeMap::new();
        let budget = ResourceBudget::new(1024, 1024, 1024, 10, 10, 0, 10, 8);
        let mut parser = OCGParser::new_with_budget(&mut ast, &resolver, &budget);

        assert_eq!(
            parser
                .parse_ocmd_with_budget(&ocmd)
                .expect_err("OCMD node budget must propagate"),
            ResourceBudgetError::Nodes
        );
    }
}
