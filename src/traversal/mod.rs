use crate::ast::{AstNode, PdfAstGraph, PdfDocument};
use crate::performance::{ResourceBudget, ResourceBudgetError};
use crate::visitor::{AstWalker as VisitorAstWalker, Visitor};

/// Trait for walking AST nodes in a document graph.
pub trait AstWalker {
    /// Walk nodes in depth-first order from the graph root.
    fn walk_nodes<V: Visitor>(&self, visitor: &mut V);

    /// Walk nodes with a lightweight callback.
    fn walk_nodes_with<F>(&self, f: F)
    where
        F: FnMut(&AstNode);

    fn walk_nodes_with_budget<V: Visitor>(
        &self,
        visitor: &mut V,
        budget: &ResourceBudget,
    ) -> Result<(), ResourceBudgetError>;
}

/// Trait for walking the graph structure (nodes + edges).
pub trait GraphWalker {
    /// Walk nodes by iterating over all nodes in the graph.
    fn walk_all_nodes<F>(&self, f: F)
    where
        F: FnMut(&AstNode);

    /// Walk edges by iterating over all edges in the graph.
    fn walk_edges<F>(&self, f: F)
    where
        F: FnMut(&crate::ast::EdgeInfo);

    fn walk_all_nodes_with_budget<F>(
        &self,
        f: F,
        budget: &ResourceBudget,
    ) -> Result<(), ResourceBudgetError>
    where
        F: FnMut(&AstNode);

    fn walk_edges_with_budget<F>(
        &self,
        f: F,
        budget: &ResourceBudget,
    ) -> Result<(), ResourceBudgetError>
    where
        F: FnMut(&crate::ast::EdgeInfo);
}

/// Trait for iterating incremental timeline steps.
pub trait TimelineWalker {
    /// Walk document revisions in order.
    fn walk_revisions<F>(&self, f: F)
    where
        F: FnMut(&crate::ast::DocumentRevision);

    fn walk_revisions_with_budget<F>(
        &self,
        f: F,
        budget: &ResourceBudget,
    ) -> Result<(), ResourceBudgetError>
    where
        F: FnMut(&crate::ast::DocumentRevision);
}

impl AstWalker for PdfAstGraph {
    fn walk_nodes<V: Visitor>(&self, visitor: &mut V) {
        let mut walker = VisitorAstWalker::new(self);
        walker.walk(visitor);
    }

    fn walk_nodes_with<F>(&self, mut f: F)
    where
        F: FnMut(&AstNode),
    {
        self.walk_nodes(&mut CallbackVisitor { callback: &mut f });
    }

    fn walk_nodes_with_budget<V: Visitor>(
        &self,
        visitor: &mut V,
        budget: &ResourceBudget,
    ) -> Result<(), ResourceBudgetError> {
        let mut walker = VisitorAstWalker::new(self);
        walker.walk_with_budget(visitor, budget)
    }
}

impl GraphWalker for PdfAstGraph {
    fn walk_all_nodes<F>(&self, mut f: F)
    where
        F: FnMut(&AstNode),
    {
        for node in self.get_all_nodes() {
            f(node);
        }
    }

    fn walk_edges<F>(&self, mut f: F)
    where
        F: FnMut(&crate::ast::EdgeInfo),
    {
        for edge in self.get_all_edges() {
            f(&edge);
        }
    }

    fn walk_all_nodes_with_budget<F>(
        &self,
        mut f: F,
        budget: &ResourceBudget,
    ) -> Result<(), ResourceBudgetError>
    where
        F: FnMut(&AstNode),
    {
        for node in self.get_all_nodes_with_budget(budget)? {
            f(node);
        }
        Ok(())
    }

    fn walk_edges_with_budget<F>(
        &self,
        mut f: F,
        budget: &ResourceBudget,
    ) -> Result<(), ResourceBudgetError>
    where
        F: FnMut(&crate::ast::EdgeInfo),
    {
        for edge in self.get_all_edges_with_budget(budget)? {
            f(&edge);
        }
        Ok(())
    }
}

impl TimelineWalker for PdfDocument {
    fn walk_revisions<F>(&self, mut f: F)
    where
        F: FnMut(&crate::ast::DocumentRevision),
    {
        for revision in &self.revisions {
            f(revision);
        }
    }

    fn walk_revisions_with_budget<F>(
        &self,
        mut f: F,
        budget: &ResourceBudget,
    ) -> Result<(), ResourceBudgetError>
    where
        F: FnMut(&crate::ast::DocumentRevision),
    {
        for revision in &self.revisions {
            budget.consume_object()?;
            f(revision);
        }
        Ok(())
    }
}

struct CallbackVisitor<'a, F> {
    callback: &'a mut F,
}

impl<'a, F> Visitor for CallbackVisitor<'a, F>
where
    F: FnMut(&AstNode),
{
    fn visit_node(&mut self, node: &AstNode) -> crate::visitor::VisitorAction {
        (self.callback)(node);
        crate::visitor::VisitorAction::Continue
    }
}

#[cfg(test)]
mod tests {
    use super::GraphWalker;
    use crate::ast::{EdgeType, NodeType, PdfAstGraph};
    use crate::performance::{ResourceBudget, ResourceBudgetError};
    use crate::types::PdfValue;

    fn graph_with_edge() -> PdfAstGraph {
        let mut graph = PdfAstGraph::new();
        let root = graph.create_node(NodeType::Root, PdfValue::Null);
        let child = graph.create_node(NodeType::Page, PdfValue::Null);
        graph.set_root(root);
        graph.add_edge(root, child, EdgeType::Child);
        graph
    }

    #[test]
    fn node_walk_uses_the_supplied_budget() {
        let graph = graph_with_edge();
        let budget = ResourceBudget::new(1024, 1024, 1024, 100, 10, 1, 10, 10);
        let error = graph
            .walk_all_nodes_with_budget(|_| {}, &budget)
            .expect_err("node traversal must stop at the supplied node limit");
        assert_eq!(error, ResourceBudgetError::Nodes);
    }

    #[test]
    fn edge_walk_uses_the_supplied_budget() {
        let graph = graph_with_edge();
        let budget = ResourceBudget::new(1024, 1024, 1024, 100, 10, 10, 0, 10);
        let error = graph
            .walk_edges_with_budget(|_| {}, &budget)
            .expect_err("edge traversal must stop at the supplied edge limit");
        assert_eq!(error, ResourceBudgetError::Edges);
    }
}
