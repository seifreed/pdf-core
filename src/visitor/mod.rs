use crate::ast::{AstNode, NodeId, NodeType, PdfAstGraph};
use crate::performance::{ResourceBudget, ResourceBudgetError};
use crate::types::PdfDictionary;
use std::collections::HashSet;

/// Dispatches the visitor call based on node type.
/// Handles the common pattern of calling type-specific visitor methods
/// when a dictionary is available, falling back to visit_node otherwise.
fn dispatch_visitor<V: Visitor>(visitor: &mut V, node: &AstNode) -> VisitorAction {
    let dict = node.as_dict();
    match (&node.node_type, dict) {
        (NodeType::Catalog, Some(d)) => visitor.visit_catalog(node, d),
        (NodeType::Page, Some(d)) => visitor.visit_page(node, d),
        (NodeType::Font, Some(d)) => visitor.visit_font(node, d),
        (NodeType::Image, Some(d)) => visitor.visit_image(node, d),
        (NodeType::Annotation, Some(d)) => visitor.visit_annotation(node, d),
        (NodeType::Action, Some(d)) => visitor.visit_action(node, d),
        (NodeType::EmbeddedFile, Some(d)) => visitor.visit_embedded_file(node, d),
        (NodeType::Signature, Some(d)) => visitor.visit_signature(node, d),
        (NodeType::StructTreeRoot, Some(d)) => visitor.visit_struct_tree_root(node, d),
        (NodeType::StructElem, Some(d)) => visitor.visit_struct_elem(node, d),
        (NodeType::CMap, Some(d)) => visitor.visit_cmap(node, d),
        (NodeType::ToUnicode, Some(d)) => visitor.visit_tounicode(node, d),
        _ => visitor.visit_node(node),
    }
}

pub trait Visitor {
    fn visit_node(&mut self, _node: &AstNode) -> VisitorAction {
        VisitorAction::Continue
    }

    fn visit_catalog(&mut self, node: &AstNode, dict: &PdfDictionary) -> VisitorAction {
        let _ = dict;
        self.visit_node(node)
    }

    fn visit_page(&mut self, node: &AstNode, dict: &PdfDictionary) -> VisitorAction {
        let _ = dict;
        self.visit_node(node)
    }

    fn visit_font(&mut self, node: &AstNode, dict: &PdfDictionary) -> VisitorAction {
        let _ = dict;
        self.visit_node(node)
    }

    fn visit_image(&mut self, node: &AstNode, dict: &PdfDictionary) -> VisitorAction {
        let _ = dict;
        self.visit_node(node)
    }

    fn visit_annotation(&mut self, node: &AstNode, dict: &PdfDictionary) -> VisitorAction {
        let _ = dict;
        self.visit_node(node)
    }

    fn visit_action(&mut self, node: &AstNode, dict: &PdfDictionary) -> VisitorAction {
        let _ = dict;
        self.visit_node(node)
    }

    fn visit_embedded_file(&mut self, node: &AstNode, dict: &PdfDictionary) -> VisitorAction {
        let _ = dict;
        self.visit_node(node)
    }

    fn visit_signature(&mut self, node: &AstNode, dict: &PdfDictionary) -> VisitorAction {
        let _ = dict;
        self.visit_node(node)
    }

    fn visit_struct_tree_root(&mut self, node: &AstNode, dict: &PdfDictionary) -> VisitorAction {
        let _ = dict;
        self.visit_node(node)
    }

    fn visit_struct_elem(&mut self, node: &AstNode, dict: &PdfDictionary) -> VisitorAction {
        let _ = dict;
        self.visit_node(node)
    }

    fn visit_cmap(&mut self, node: &AstNode, dict: &PdfDictionary) -> VisitorAction {
        let _ = dict;
        self.visit_node(node)
    }

    fn visit_tounicode(&mut self, node: &AstNode, dict: &PdfDictionary) -> VisitorAction {
        let _ = dict;
        self.visit_node(node)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisitorAction {
    Continue,
    SkipChildren,
    Stop,
}

pub struct AstWalker<'a> {
    graph: &'a PdfAstGraph,
    visited: HashSet<NodeId>,
}

pub struct DepthAwareWalker<'a> {
    graph: &'a PdfAstGraph,
    visited: HashSet<NodeId>,
    max_depth: Option<usize>,
}

impl<'a> AstWalker<'a> {
    pub fn new(graph: &'a PdfAstGraph) -> Self {
        AstWalker {
            graph,
            visited: HashSet::new(),
        }
    }

    pub fn walk<V: Visitor>(&mut self, visitor: &mut V) {
        if let Some(root_id) = self.graph.get_root() {
            self.walk_node(root_id, visitor);
        }
    }

    pub fn walk_with_budget<V: Visitor>(
        &mut self,
        visitor: &mut V,
        budget: &ResourceBudget,
    ) -> Result<(), ResourceBudgetError> {
        if let Some(root_id) = self.graph.get_root() {
            self.walk_node_with_budget(root_id, visitor, 0, budget)?;
        }
        Ok(())
    }

    fn walk_node<V: Visitor>(&mut self, node_id: NodeId, visitor: &mut V) -> VisitorAction {
        self.walk_node_with_depth(node_id, visitor, 0)
    }

    fn walk_node_with_depth<V: Visitor>(
        &mut self,
        node_id: NodeId,
        visitor: &mut V,
        _depth: usize,
    ) -> VisitorAction {
        if self.visited.contains(&node_id) {
            return VisitorAction::SkipChildren;
        }
        self.visited.insert(node_id);

        let node = match self.graph.get_node(node_id) {
            Some(n) => n,
            None => return VisitorAction::Continue,
        };

        match dispatch_visitor(visitor, node) {
            VisitorAction::Stop => return VisitorAction::Stop,
            VisitorAction::SkipChildren => return VisitorAction::Continue,
            VisitorAction::Continue => {}
        }

        for child_id in node.children.clone() {
            if self.walk_node(child_id, visitor) == VisitorAction::Stop {
                return VisitorAction::Stop;
            }
        }

        VisitorAction::Continue
    }

    fn walk_node_with_budget<V: Visitor>(
        &mut self,
        node_id: NodeId,
        visitor: &mut V,
        depth: usize,
        budget: &ResourceBudget,
    ) -> Result<VisitorAction, ResourceBudgetError> {
        if depth > budget.max_depth {
            return Err(ResourceBudgetError::Depth);
        }
        if self.visited.contains(&node_id) {
            return Ok(VisitorAction::SkipChildren);
        }
        self.visited.insert(node_id);
        budget.consume_node()?;

        let node = match self.graph.get_node(node_id) {
            Some(n) => n,
            None => return Ok(VisitorAction::Continue),
        };

        match dispatch_visitor(visitor, node) {
            VisitorAction::Stop => return Ok(VisitorAction::Stop),
            VisitorAction::SkipChildren => return Ok(VisitorAction::Continue),
            VisitorAction::Continue => {}
        }

        for child_id in node.children.clone() {
            budget.consume_edge()?;
            if self.walk_node_with_budget(child_id, visitor, depth + 1, budget)?
                == VisitorAction::Stop
            {
                return Ok(VisitorAction::Stop);
            }
        }
        Ok(VisitorAction::Continue)
    }
}

pub struct QueryBuilder {
    node_types: Vec<NodeType>,
    has_errors: Option<bool>,
    has_warnings: Option<bool>,
    max_depth: Option<usize>,
}

impl QueryBuilder {
    pub fn new() -> Self {
        QueryBuilder {
            node_types: Vec::new(),
            has_errors: None,
            has_warnings: None,
            max_depth: None,
        }
    }

    pub fn with_type(mut self, node_type: NodeType) -> Self {
        self.node_types.push(node_type);
        self
    }

    pub fn with_errors(mut self, has_errors: bool) -> Self {
        self.has_errors = Some(has_errors);
        self
    }

    pub fn with_warnings(mut self, has_warnings: bool) -> Self {
        self.has_warnings = Some(has_warnings);
        self
    }

    pub fn with_max_depth(mut self, depth: usize) -> Self {
        self.max_depth = Some(depth);
        self
    }

    pub fn execute(&self, graph: &PdfAstGraph) -> Vec<NodeId> {
        let mut results = Vec::new();
        let mut collector = QueryCollector {
            query: self,
            results: &mut results,
        };

        let mut walker = DepthAwareWalker::new(graph, self.max_depth);
        walker.walk(&mut collector);

        results
    }

    pub fn execute_with_budget(
        &self,
        graph: &PdfAstGraph,
        budget: &ResourceBudget,
    ) -> Result<Vec<NodeId>, ResourceBudgetError> {
        let mut results = Vec::new();
        let mut collector = QueryCollector {
            query: self,
            results: &mut results,
        };
        let mut walker = DepthAwareWalker::new(graph, self.max_depth);
        walker.walk_with_budget(&mut collector, budget)?;
        Ok(results)
    }
}

struct QueryCollector<'a> {
    query: &'a QueryBuilder,
    results: &'a mut Vec<NodeId>,
}

impl<'a> Visitor for QueryCollector<'a> {
    fn visit_node(&mut self, node: &AstNode) -> VisitorAction {
        let mut matches = true;

        if !self.query.node_types.is_empty() {
            matches &= self
                .query
                .node_types
                .iter()
                .any(|t| std::mem::discriminant(t) == std::mem::discriminant(&node.node_type));
        }

        if let Some(has_errors) = self.query.has_errors {
            matches &= node.is_error() == has_errors;
        }

        if let Some(has_warnings) = self.query.has_warnings {
            matches &= node.has_warnings() == has_warnings;
        }

        if matches {
            self.results.push(node.id);
        }

        VisitorAction::Continue
    }
}

impl Default for QueryBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> DepthAwareWalker<'a> {
    pub fn new(graph: &'a PdfAstGraph, max_depth: Option<usize>) -> Self {
        DepthAwareWalker {
            graph,
            visited: HashSet::new(),
            max_depth,
        }
    }

    pub fn walk<V: Visitor>(&mut self, visitor: &mut V) {
        if let Some(root_id) = self.graph.get_root() {
            self.walk_node_with_depth(root_id, visitor, 0);
        }
    }

    fn walk_with_budget<V: Visitor>(
        &mut self,
        visitor: &mut V,
        budget: &ResourceBudget,
    ) -> Result<(), ResourceBudgetError> {
        if let Some(root_id) = self.graph.get_root() {
            self.walk_node_with_depth_and_budget(root_id, visitor, 0, budget)?;
        }
        Ok(())
    }

    fn walk_node_with_depth<V: Visitor>(
        &mut self,
        node_id: NodeId,
        visitor: &mut V,
        depth: usize,
    ) -> VisitorAction {
        // Check depth limit
        if let Some(max_depth) = self.max_depth {
            if depth > max_depth {
                return VisitorAction::SkipChildren;
            }
        }

        if self.visited.contains(&node_id) {
            return VisitorAction::SkipChildren;
        }
        self.visited.insert(node_id);

        let node = match self.graph.get_node(node_id) {
            Some(n) => n,
            None => return VisitorAction::Continue,
        };

        match dispatch_visitor(visitor, node) {
            VisitorAction::Stop => return VisitorAction::Stop,
            VisitorAction::SkipChildren => return VisitorAction::Continue,
            VisitorAction::Continue => {}
        }

        for child_id in &node.children {
            if self.walk_node_with_depth(*child_id, visitor, depth + 1) == VisitorAction::Stop {
                return VisitorAction::Stop;
            }
        }

        VisitorAction::Continue
    }

    fn walk_node_with_depth_and_budget<V: Visitor>(
        &mut self,
        node_id: NodeId,
        visitor: &mut V,
        depth: usize,
        budget: &ResourceBudget,
    ) -> Result<VisitorAction, ResourceBudgetError> {
        if let Some(max_depth) = self.max_depth {
            if depth > max_depth {
                return Ok(VisitorAction::SkipChildren);
            }
        }
        if depth > budget.max_depth {
            return Err(ResourceBudgetError::Depth);
        }
        if self.visited.contains(&node_id) {
            return Ok(VisitorAction::SkipChildren);
        }
        self.visited.insert(node_id);
        budget.consume_node()?;

        let node = match self.graph.get_node(node_id) {
            Some(n) => n,
            None => return Ok(VisitorAction::Continue),
        };
        match dispatch_visitor(visitor, node) {
            VisitorAction::Stop => return Ok(VisitorAction::Stop),
            VisitorAction::SkipChildren => return Ok(VisitorAction::Continue),
            VisitorAction::Continue => {}
        }
        for child_id in &node.children {
            budget.consume_edge()?;
            if self.walk_node_with_depth_and_budget(*child_id, visitor, depth + 1, budget)?
                == VisitorAction::Stop
            {
                return Ok(VisitorAction::Stop);
            }
        }
        Ok(VisitorAction::Continue)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct CountingVisitor;

    impl Visitor for CountingVisitor {}

    #[test]
    fn budgeted_visitor_walk_rejects_excess_nodes() {
        let mut graph = PdfAstGraph::new();
        let root = graph.create_node(NodeType::Root, crate::types::PdfValue::Null);
        let child = graph.create_node(NodeType::Page, crate::types::PdfValue::Null);
        graph.add_edge(root, child, crate::ast::EdgeType::Child);
        graph.set_root(root);
        let budget = ResourceBudget::new(1024, 1024, 1024, 100, 10, 1, 10, 10);

        let mut visitor = CountingVisitor;
        let error = AstWalker::new(&graph)
            .walk_with_budget(&mut visitor, &budget)
            .expect_err("visitor traversal must consume node budget");
        assert_eq!(error, ResourceBudgetError::Nodes);
    }

    #[test]
    fn budgeted_query_walk_accepts_graph_within_limits() {
        let mut graph = PdfAstGraph::new();
        let root = graph.create_node(NodeType::Root, crate::types::PdfValue::Null);
        graph.set_root(root);
        let budget = ResourceBudget::new(1024, 1024, 1024, 100, 10, 1, 10, 10);

        let result = QueryBuilder::new()
            .execute_with_budget(&graph, &budget)
            .expect("query should fit budget");
        assert_eq!(result, vec![root]);
    }

    #[test]
    fn budgeted_visitor_walk_rejects_edges_and_depth() {
        let mut graph = PdfAstGraph::new();
        let root = graph.create_node(NodeType::Root, crate::types::PdfValue::Null);
        let child = graph.create_node(NodeType::Page, crate::types::PdfValue::Null);
        graph.add_edge(root, child, crate::ast::EdgeType::Child);
        graph.set_root(root);

        let mut visitor = CountingVisitor;
        let edge_budget = ResourceBudget::new(1024, 1024, 1024, 100, 10, 10, 0, 10);
        assert_eq!(
            AstWalker::new(&graph)
                .walk_with_budget(&mut visitor, &edge_budget)
                .expect_err("visitor traversal must consume edge budget"),
            ResourceBudgetError::Edges
        );

        let depth_budget = ResourceBudget::new(1024, 1024, 1024, 100, 10, 10, 10, 0);
        assert_eq!(
            AstWalker::new(&graph)
                .walk_with_budget(&mut visitor, &depth_budget)
                .expect_err("visitor traversal must enforce depth budget"),
            ResourceBudgetError::Depth
        );
    }
}
