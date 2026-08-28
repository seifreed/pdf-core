use crate::ast::{AstNode, NodeId, NodeType, PdfAstGraph};
use crate::filters::decode_stream_with_budget;
use crate::metadata::icc::parse_icc_profile;
use crate::parser::reference_resolver::ObjectNodeMap;
use crate::performance::{ResourceBudget, ResourceBudgetError};
use crate::types::{PdfDictionary, PdfValue};

/// Parser for OutputIntents from catalog
pub struct OutputIntentsParser<'a> {
    ast: &'a mut PdfAstGraph,
    resolver: &'a ObjectNodeMap,
    budget: ResourceBudget,
}

impl<'a> OutputIntentsParser<'a> {
    pub fn new(ast: &'a mut PdfAstGraph, resolver: &'a ObjectNodeMap) -> Self {
        Self::new_with_budget(ast, resolver, &ResourceBudget::default())
    }

    pub fn new_with_budget(
        ast: &'a mut PdfAstGraph,
        resolver: &'a ObjectNodeMap,
        budget: &ResourceBudget,
    ) -> Self {
        OutputIntentsParser {
            ast,
            resolver,
            budget: budget.clone(),
        }
    }

    pub fn parse_output_intents(&mut self, catalog: &PdfDictionary) -> Vec<NodeId> {
        self.parse_output_intents_with_budget(catalog)
            .unwrap_or_default()
    }

    pub fn parse_output_intents_with_budget(
        &mut self,
        catalog: &PdfDictionary,
    ) -> Result<Vec<NodeId>, ResourceBudgetError> {
        let mut intent_nodes = Vec::new();

        if let Some(PdfValue::Array(intents)) = catalog.get("OutputIntents") {
            for intent_value in intents {
                match intent_value {
                    PdfValue::Dictionary(intent_dict) => {
                        intent_nodes.push(self.parse_output_intent(intent_dict)?);
                    }
                    PdfValue::Reference(obj_id) => {
                        if let Some(intent_id) = self.resolver.get_node_id(&obj_id.id()) {
                            // Update node type
                            if let Some(node) = self.ast.get_node_mut(intent_id) {
                                node.node_type = NodeType::OutputIntent;
                            }

                            // Parse the referenced dictionary
                            if let Some(node) = self.ast.get_node(intent_id) {
                                if let Some(dict) = node.as_dict() {
                                    self.enrich_output_intent(dict.clone(), intent_id);
                                }
                            }

                            intent_nodes.push(intent_id);
                        }
                    }
                    _ => {}
                }
            }
        }

        Ok(intent_nodes)
    }

    fn parse_output_intent(
        &mut self,
        intent_dict: &PdfDictionary,
    ) -> Result<NodeId, ResourceBudgetError> {
        self.budget.consume_node()?;
        // Create OutputIntent node
        let mut node = AstNode::new(
            self.ast.next_node_id(),
            NodeType::OutputIntent,
            PdfValue::Dictionary(intent_dict.clone()),
        );

        // Extract metadata

        // Subtype (e.g., GTS_PDFX, GTS_PDFA1, ISO_PDFE1)
        if let Some(PdfValue::Name(s)) = intent_dict.get("S") {
            node.metadata
                .set_property("subtype".to_string(), s.without_slash().to_string());
        }

        // OutputCondition
        if let Some(PdfValue::String(oc)) = intent_dict.get("OutputCondition") {
            node.metadata
                .set_property("output_condition".to_string(), oc.to_string_lossy());
        }

        // OutputConditionIdentifier
        if let Some(PdfValue::String(oci)) = intent_dict.get("OutputConditionIdentifier") {
            node.metadata.set_property(
                "output_condition_identifier".to_string(),
                oci.to_string_lossy(),
            );
        }

        // RegistryName
        if let Some(PdfValue::String(reg)) = intent_dict.get("RegistryName") {
            node.metadata
                .set_property("registry_name".to_string(), reg.to_string_lossy());
        }

        // Info
        if let Some(PdfValue::String(info)) = intent_dict.get("Info") {
            node.metadata
                .set_property("info".to_string(), info.to_string_lossy());
        }

        let intent_id = self.ast.add_node(node);

        // Link to ICC profile if present
        if let Some(PdfValue::Reference(icc_ref)) = intent_dict.get("DestOutputProfile") {
            if let Some(icc_id) = self.resolver.get_node_id(&icc_ref.id()) {
                self.add_edge(intent_id, icc_id, crate::ast::EdgeType::Reference);

                // Update ICC profile node
                if let Some(icc_node) = self.ast.get_node_mut(icc_id) {
                    icc_node.node_type = NodeType::ICCBased;
                    icc_node
                        .metadata
                        .set_property("usage".to_string(), "OutputIntent".to_string());
                }

                let stream = self
                    .ast
                    .get_node(icc_id)
                    .and_then(|node| node.as_stream().cloned());
                if let Some(stream) = stream {
                    self.attach_icc_profile_node(icc_id, &stream);
                }
            }
        }

        Ok(intent_id)
    }

    fn enrich_output_intent(&mut self, intent_dict: PdfDictionary, intent_id: NodeId) {
        if let Some(node) = self.ast.get_node_mut(intent_id) {
            // Extract all the metadata fields
            if let Some(PdfValue::Name(s)) = intent_dict.get("S") {
                node.metadata
                    .set_property("subtype".to_string(), s.without_slash().to_string());

                // Determine compliance profile
                let profile = match s.without_slash() {
                    "GTS_PDFX" => "PDF/X",
                    "GTS_PDFA1" => "PDF/A-1",
                    "GTS_PDFA2" => "PDF/A-2",
                    "GTS_PDFA3" => "PDF/A-3",
                    "ISO_PDFE1" => "PDF/E-1",
                    "ISO_PDFUA1" => "PDF/UA-1",
                    "ISO_PDFVT" => "PDF/VT",
                    _ => "Unknown",
                };

                node.metadata
                    .set_property("compliance_profile".to_string(), profile.to_string());
            }

            // Extract condition details
            if let Some(PdfValue::String(oc)) = intent_dict.get("OutputCondition") {
                node.metadata
                    .set_property("output_condition".to_string(), oc.to_string_lossy());
            }

            if let Some(PdfValue::String(oci)) = intent_dict.get("OutputConditionIdentifier") {
                let identifier = oci.to_string_lossy();
                node.metadata.set_property(
                    "output_condition_identifier".to_string(),
                    identifier.clone(),
                );

                // Parse common identifiers
                if identifier.contains("FOGRA") {
                    node.metadata
                        .set_property("color_profile_type".to_string(), "FOGRA".to_string());
                } else if identifier.contains("CGATS") {
                    node.metadata
                        .set_property("color_profile_type".to_string(), "CGATS".to_string());
                } else if identifier.contains("sRGB") {
                    node.metadata
                        .set_property("color_profile_type".to_string(), "sRGB".to_string());
                }
            }
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

    pub fn validate_output_intent(&self, intent_id: NodeId) -> Vec<String> {
        let mut issues = Vec::new();

        if let Some(node) = self.ast.get_node(intent_id) {
            let props = &node.metadata.properties;

            // Check required fields based on subtype
            if let Some(subtype) = props.get("subtype") {
                match subtype.as_str() {
                    "GTS_PDFX" => {
                        if !props.contains_key("output_condition_identifier") {
                            issues.push(
                                "PDF/X OutputIntent missing OutputConditionIdentifier".to_string(),
                            );
                        }
                        if !props.contains_key("registry_name") {
                            issues.push("PDF/X OutputIntent missing RegistryName".to_string());
                        }
                    }
                    "GTS_PDFA1" | "GTS_PDFA2" | "GTS_PDFA3"
                        if !props.contains_key("output_condition_identifier") =>
                    {
                        issues.push(
                            "PDF/A OutputIntent missing OutputConditionIdentifier".to_string(),
                        );
                    }
                    _ => {}
                }
            }

            // Check for ICC profile
            let has_icc = self
                .ast
                .get_all_edges()
                .iter()
                .any(|e| e.from == intent_id && e.edge_type == crate::ast::EdgeType::Reference);

            if !has_icc && !node.metadata.properties.contains_key("registry_name") {
                issues.push(
                    "OutputIntent missing both DestOutputProfile and RegistryName".to_string(),
                );
            }
        }

        issues
    }

    fn add_edge(&mut self, from: NodeId, to: NodeId, edge_type: crate::ast::EdgeType) {
        if self.budget.consume_edge().is_ok() {
            self.ast.add_edge(from, to, edge_type);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::PdfArray;

    #[test]
    fn output_intents_parser_respects_node_budget() {
        let mut ast = PdfAstGraph::new();
        let resolver = ObjectNodeMap::new();
        let budget = ResourceBudget::new(1024, 1024, 1024, 10, 10, 1, 10, 8);
        let mut parser = OutputIntentsParser::new_with_budget(&mut ast, &resolver, &budget);
        let intent = || PdfValue::Dictionary(PdfDictionary::new());
        let mut catalog = PdfDictionary::new();
        catalog.insert(
            "OutputIntents",
            PdfValue::Array(PdfArray::from(vec![intent(), intent()])),
        );

        assert!(matches!(
            parser.parse_output_intents_with_budget(&catalog),
            Err(ResourceBudgetError::Nodes)
        ));
        assert_eq!(ast.node_count(), 1);
    }
}
