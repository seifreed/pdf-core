use super::*;
use crate::ast::{AstNode, NodeId, PdfAstGraph};
use crate::types::PdfValue;

/// Transform operation that can be applied to AST nodes
#[derive(Debug, Clone)]
pub enum TransformOperation {
    /// Replace a node with a new node
    ReplaceNode { target: NodeId, new_node: AstNode },
    /// Insert a new node as a child
    InsertChild {
        parent: NodeId,
        child: AstNode,
        position: Option<usize>,
    },
    /// Remove a node
    RemoveNode {
        target: NodeId,
        preserve_children: bool,
    },
    /// Move a node to a new parent
    MoveNode {
        target: NodeId,
        new_parent: NodeId,
        position: Option<usize>,
    },
    /// Update node value
    UpdateValue { target: NodeId, new_value: PdfValue },
    /// Batch operation containing multiple operations
    Batch(Vec<TransformOperation>),
}

impl TransformOperation {
    /// Apply this operation to the graph
    pub fn apply(&self, graph: &mut PdfAstGraph) -> AstResult<()> {
        match self {
            TransformOperation::ReplaceNode { target, new_node } => {
                graph.replace_node(*target, new_node.clone())?;
            }
            TransformOperation::InsertChild {
                parent,
                child,
                position,
            } => {
                if graph.get_node(*parent).is_none() {
                    return Err(AstError::NodeNotFound(format!("Parent node {:?}", parent)));
                }
                let child_id = graph.create_node(child.node_type.clone(), child.value.clone());
                if let Some(node) = graph.get_node_mut(child_id) {
                    node.metadata = child.metadata.clone();
                }
                if !graph.add_edge(*parent, child_id, crate::ast::EdgeType::Child) {
                    graph.remove_node(child_id);
                    return Err(AstError::NodeNotFound(format!("Parent node {:?}", parent)));
                }
                if let Some(position) = position {
                    let mut children = graph.get_children(*parent);
                    children.retain(|id| *id != child_id);
                    let position = (*position).min(children.len());
                    children.insert(position, child_id);
                    if !graph.reorder_children(*parent, &children) {
                        return Err(AstError::InvalidStructure(
                            "Unable to reorder inserted child".to_string(),
                        ));
                    }
                }
            }
            TransformOperation::RemoveNode {
                target,
                preserve_children,
            } => {
                if *preserve_children {
                    // Move children to parent before removing
                    let children = graph.get_children(*target);
                    if let Some(parent_id) = graph.get_parent(*target) {
                        for child_id in children {
                            graph.remove_edge(*target, child_id);
                            graph.add_edge(parent_id, child_id, crate::ast::EdgeType::Child);
                        }
                    }
                }
                graph.remove_node(*target);
            }
            TransformOperation::MoveNode {
                target,
                new_parent,
                position,
            } => {
                if graph.get_node(*target).is_none() {
                    return Err(AstError::NodeNotFound(format!("Node {:?}", target)));
                }
                if graph.get_node(*new_parent).is_none() {
                    return Err(AstError::NodeNotFound(format!(
                        "Parent node {:?}",
                        new_parent
                    )));
                }
                if target == new_parent {
                    return Err(AstError::InvalidStructure(
                        "Cannot move a node under itself".to_string(),
                    ));
                }
                // Remove from current parent
                if let Some(old_parent) = graph.get_parent(*target) {
                    graph.remove_edge(old_parent, *target);
                }

                // Add to new parent
                if !graph.add_edge(*new_parent, *target, crate::ast::EdgeType::Child) {
                    return Err(AstError::NodeNotFound(format!(
                        "Parent node {:?}",
                        new_parent
                    )));
                }
                if let Some(position) = position {
                    let mut children = graph.get_children(*new_parent);
                    children.retain(|id| *id != *target);
                    let position = (*position).min(children.len());
                    children.insert(position, *target);
                    if !graph.reorder_children(*new_parent, &children) {
                        return Err(AstError::InvalidStructure(
                            "Unable to reorder moved child".to_string(),
                        ));
                    }
                }
            }
            TransformOperation::UpdateValue { target, new_value } => {
                if let Some(node) = graph.get_node_mut(*target) {
                    node.value = new_value.clone();
                } else {
                    return Err(AstError::NodeNotFound(format!("Node {:?}", target)));
                }
            }
            TransformOperation::Batch(operations) => {
                for operation in operations {
                    operation.apply(graph)?;
                }
            }
        }
        Ok(())
    }

    /// Create a replace operation
    pub fn replace(target: NodeId, new_node: AstNode) -> Self {
        TransformOperation::ReplaceNode { target, new_node }
    }

    /// Create an insert operation
    pub fn insert(parent: NodeId, child: AstNode) -> Self {
        TransformOperation::InsertChild {
            parent,
            child,
            position: None,
        }
    }

    /// Create an insert operation at specific position
    pub fn insert_at(parent: NodeId, child: AstNode, position: usize) -> Self {
        TransformOperation::InsertChild {
            parent,
            child,
            position: Some(position),
        }
    }

    /// Create a remove operation
    pub fn remove(target: NodeId) -> Self {
        TransformOperation::RemoveNode {
            target,
            preserve_children: false,
        }
    }

    /// Create a remove operation that preserves children
    pub fn remove_preserve_children(target: NodeId) -> Self {
        TransformOperation::RemoveNode {
            target,
            preserve_children: true,
        }
    }

    /// Create a move operation
    pub fn move_node(target: NodeId, new_parent: NodeId) -> Self {
        TransformOperation::MoveNode {
            target,
            new_parent,
            position: None,
        }
    }

    /// Create a move operation to specific position
    pub fn move_to_position(target: NodeId, new_parent: NodeId, position: usize) -> Self {
        TransformOperation::MoveNode {
            target,
            new_parent,
            position: Some(position),
        }
    }

    /// Create an update value operation
    pub fn update_value(target: NodeId, new_value: PdfValue) -> Self {
        TransformOperation::UpdateValue { target, new_value }
    }

    /// Create a batch operation
    pub fn batch(operations: Vec<TransformOperation>) -> Self {
        TransformOperation::Batch(operations)
    }
}

#[cfg(test)]
mod tests {
    use super::TransformOperation;
    use crate::ast::{NodeType, PdfAstGraph};
    use crate::types::PdfValue;

    #[test]
    fn insert_at_preserves_requested_child_order() {
        let mut graph = PdfAstGraph::new();
        let parent = graph.create_node(NodeType::Pages, PdfValue::Null);
        let first = graph.create_node(NodeType::Page, PdfValue::Integer(1));
        let second = graph.create_node(NodeType::Page, PdfValue::Integer(2));
        graph.add_edge(parent, first, crate::ast::EdgeType::Child);
        graph.add_edge(parent, second, crate::ast::EdgeType::Child);

        TransformOperation::insert_at(
            parent,
            crate::ast::AstNode::new(crate::ast::NodeId(99), NodeType::Page, PdfValue::Integer(0)),
            0,
        )
        .apply(&mut graph)
        .expect("insert should succeed");

        let values: Vec<_> = graph
            .get_children(parent)
            .iter()
            .map(|id| graph.get_node(*id).expect("child exists").value.clone())
            .collect();
        assert_eq!(
            values,
            vec![
                PdfValue::Integer(0),
                PdfValue::Integer(1),
                PdfValue::Integer(2)
            ]
        );
    }

    #[test]
    fn move_to_position_updates_both_parent_lists() {
        let mut graph = PdfAstGraph::new();
        let source = graph.create_node(NodeType::Pages, PdfValue::Null);
        let target = graph.create_node(NodeType::Pages, PdfValue::Null);
        let first = graph.create_node(NodeType::Page, PdfValue::Integer(1));
        let moved = graph.create_node(NodeType::Page, PdfValue::Integer(2));
        let existing = graph.create_node(NodeType::Page, PdfValue::Integer(3));
        graph.add_edge(source, first, crate::ast::EdgeType::Child);
        graph.add_edge(source, moved, crate::ast::EdgeType::Child);
        graph.add_edge(target, existing, crate::ast::EdgeType::Child);

        TransformOperation::move_to_position(moved, target, 0)
            .apply(&mut graph)
            .expect("move should succeed");

        assert_eq!(graph.get_children(source), vec![first]);
        assert_eq!(graph.get_children(target), vec![moved, existing]);
    }
}
