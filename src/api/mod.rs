use crate::ast::{NodeId, NodeType, PdfAstGraph};
use crate::types::PdfValue;
use regex::Regex;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub enum QuerySelector {
    NodeType(NodeType),
    NodeTypeName(String),
    ObjectId(u32, u16),
    Path(Vec<QuerySelector>),
    Child(Box<QuerySelector>, Box<QuerySelector>),
    Descendant(Box<QuerySelector>, Box<QuerySelector>),
    Parent(Box<QuerySelector>),
    Ancestor(Box<QuerySelector>),
    Sibling(Box<QuerySelector>),
    HasProperty(String),
    PropertyEquals(String, String),
    PropertyMatches(String, Regex),
    And(Vec<QuerySelector>),
    Or(Vec<QuerySelector>),
    Not(Box<QuerySelector>),
    First,
    Last,
    Index(usize),
    Range(usize, usize),
}

#[allow(dead_code)]
pub struct QueryEngine<'a> {
    graph: &'a PdfAstGraph,
    cache: HashMap<String, Vec<NodeId>>,
}

impl<'a> QueryEngine<'a> {
    pub fn new(graph: &'a PdfAstGraph) -> Self {
        Self {
            graph,
            cache: HashMap::new(),
        }
    }

    pub fn query(&mut self, selector: &QuerySelector) -> Vec<NodeId> {
        self.evaluate_selector(selector, None)
    }

    pub fn query_from(&mut self, selector: &QuerySelector, context: NodeId) -> Vec<NodeId> {
        self.evaluate_selector(selector, Some(context))
    }

    fn evaluate_selector(
        &mut self,
        selector: &QuerySelector,
        context: Option<NodeId>,
    ) -> Vec<NodeId> {
        match selector {
            QuerySelector::NodeType(node_type) => self.find_by_type(node_type.clone()),
            QuerySelector::NodeTypeName(name) => self.find_by_type_name(name),
            QuerySelector::ObjectId(num, gen) => self.find_by_object_id(*num, *gen),
            QuerySelector::Path(selectors) => self.evaluate_path(selectors, context),
            QuerySelector::Child(parent_sel, child_sel) => {
                let parents = self.evaluate_selector(parent_sel, context);
                let mut results = Vec::new();
                for parent in parents {
                    let children = self.graph.get_children(parent);
                    for child in children {
                        let matches = self.evaluate_selector(child_sel, Some(child));
                        if matches.contains(&child) {
                            results.push(child);
                        }
                    }
                }
                results
            }
            QuerySelector::Descendant(ancestor_sel, descendant_sel) => {
                let ancestors = self.evaluate_selector(ancestor_sel, context);
                let mut results = Vec::new();
                for ancestor in ancestors {
                    let descendants = self.get_all_descendants(ancestor);
                    for desc in descendants {
                        let matches = self.evaluate_selector(descendant_sel, Some(desc));
                        if matches.contains(&desc) {
                            results.push(desc);
                        }
                    }
                }
                results
            }
            QuerySelector::Parent(child_sel) => {
                let children = self.evaluate_selector(child_sel, context);
                let mut results = Vec::new();
                for child in children {
                    if let Some(parent) = self.graph.get_parent(child) {
                        results.push(parent);
                    }
                }
                results
                    .into_iter()
                    .collect::<HashSet<_>>()
                    .into_iter()
                    .collect()
            }
            QuerySelector::Ancestor(descendant_sel) => {
                let descendants = self.evaluate_selector(descendant_sel, context);
                let mut results = HashSet::new();
                for desc in descendants {
                    let mut current = desc;
                    while let Some(parent) = self.graph.get_parent(current) {
                        results.insert(parent);
                        current = parent;
                    }
                }
                results.into_iter().collect()
            }
            QuerySelector::Sibling(sel) => {
                let nodes = self.evaluate_selector(sel, context);
                let mut results = HashSet::new();
                for node in nodes {
                    if let Some(parent) = self.graph.get_parent(node) {
                        for sibling in self.graph.get_children(parent) {
                            if sibling != node {
                                results.insert(sibling);
                            }
                        }
                    }
                }
                results.into_iter().collect()
            }
            QuerySelector::HasProperty(prop) => self.find_with_property(prop),
            QuerySelector::PropertyEquals(prop, value) => {
                self.find_with_property_value(prop, value)
            }
            QuerySelector::PropertyMatches(prop, regex) => {
                self.find_with_property_regex(prop, regex)
            }
            QuerySelector::And(selectors) => {
                if selectors.is_empty() {
                    return Vec::new();
                }
                let mut result_set: Option<HashSet<NodeId>> = None;
                for sel in selectors {
                    let matches: HashSet<NodeId> =
                        self.evaluate_selector(sel, context).into_iter().collect();
                    result_set = Some(match result_set {
                        None => matches,
                        Some(set) => set.intersection(&matches).cloned().collect(),
                    });
                }
                result_set.unwrap_or_default().into_iter().collect()
            }
            QuerySelector::Or(selectors) => {
                let mut result_set = HashSet::new();
                for sel in selectors {
                    result_set.extend(self.evaluate_selector(sel, context));
                }
                result_set.into_iter().collect()
            }
            QuerySelector::Not(sel) => {
                let excluded: HashSet<NodeId> =
                    self.evaluate_selector(sel, context).into_iter().collect();
                let all_nodes: HashSet<NodeId> = self.graph.node_indices().into_iter().collect();
                all_nodes.difference(&excluded).cloned().collect()
            }
            QuerySelector::First => {
                if let Some(ctx) = context {
                    self.graph.get_children(ctx).into_iter().take(1).collect()
                } else {
                    Vec::new()
                }
            }
            QuerySelector::Last => {
                if let Some(ctx) = context {
                    let children = self.graph.get_children(ctx);
                    if let Some(last) = children.last() {
                        vec![*last]
                    } else {
                        Vec::new()
                    }
                } else {
                    Vec::new()
                }
            }
            QuerySelector::Index(idx) => {
                if let Some(ctx) = context {
                    self.graph
                        .get_children(ctx)
                        .into_iter()
                        .nth(*idx)
                        .map(|n| vec![n])
                        .unwrap_or_default()
                } else {
                    Vec::new()
                }
            }
            QuerySelector::Range(start, end) => {
                if let Some(ctx) = context {
                    if end <= start {
                        return Vec::new();
                    }
                    self.graph
                        .get_children(ctx)
                        .into_iter()
                        .skip(*start)
                        .take(end - start)
                        .collect()
                } else {
                    Vec::new()
                }
            }
        }
    }

    fn evaluate_path(
        &mut self,
        selectors: &[QuerySelector],
        context: Option<NodeId>,
    ) -> Vec<NodeId> {
        let mut current = if let Some(ctx) = context {
            vec![ctx]
        } else if let Some(root) = self.graph.get_root() {
            vec![root]
        } else {
            return Vec::new();
        };

        for selector in selectors {
            let mut next = Vec::new();
            for node in current {
                next.extend(self.evaluate_selector(selector, Some(node)));
            }
            current = next;
        }

        current
    }

    fn find_by_type(&self, node_type: NodeType) -> Vec<NodeId> {
        self.graph
            .node_indices()
            .into_iter()
            .filter(|&id| {
                self.graph
                    .get_node(id)
                    .map(|n| n.node_type == node_type)
                    .unwrap_or(false)
            })
            .collect()
    }

    fn find_by_type_name(&self, name: &str) -> Vec<NodeId> {
        let node_type = match name {
            "root" => NodeType::Root,
            "catalog" => NodeType::Catalog,
            "pages" => NodeType::Pages,
            "page" => NodeType::Page,
            "font" => NodeType::Font,
            "image" => NodeType::Image,
            "annotation" => NodeType::Annotation,
            "form" => NodeType::Form,
            "outline" => NodeType::Outline,
            "struct" => NodeType::StructElem,
            _ => return Vec::new(),
        };
        self.find_by_type(node_type)
    }

    fn find_by_object_id(&self, num: u32, gen: u16) -> Vec<NodeId> {
        self.graph
            .node_indices()
            .into_iter()
            .filter(|&id| {
                self.graph
                    .get_object_id(id)
                    .map(|obj_id| obj_id.number == num && obj_id.generation == gen)
                    .unwrap_or(false)
            })
            .collect()
    }

    fn find_with_property(&self, prop: &str) -> Vec<NodeId> {
        self.graph
            .node_indices()
            .into_iter()
            .filter(|&id| {
                if let Some(node) = self.graph.get_node(id) {
                    if let PdfValue::Dictionary(dict) = &node.value {
                        return dict.contains_key(prop);
                    }
                }
                false
            })
            .collect()
    }

    fn find_with_property_value(&self, prop: &str, value: &str) -> Vec<NodeId> {
        self.graph
            .node_indices()
            .into_iter()
            .filter(|&id| {
                if let Some(node) = self.graph.get_node(id) {
                    if let PdfValue::Dictionary(dict) = &node.value {
                        if let Some(val) = dict.get(prop) {
                            return self.value_matches(val, value);
                        }
                    }
                }
                false
            })
            .collect()
    }

    fn find_with_property_regex(&self, prop: &str, regex: &Regex) -> Vec<NodeId> {
        self.graph
            .node_indices()
            .into_iter()
            .filter(|&id| {
                if let Some(node) = self.graph.get_node(id) {
                    if let PdfValue::Dictionary(dict) = &node.value {
                        if let Some(val) = dict.get(prop) {
                            let val_str = self.value_to_string(val);
                            return regex.is_match(&val_str);
                        }
                    }
                }
                false
            })
            .collect()
    }

    fn value_matches(&self, value: &PdfValue, target: &str) -> bool {
        match value {
            PdfValue::Name(n) => n.without_slash() == target,
            PdfValue::String(s) => s.to_string_lossy() == target,
            PdfValue::Integer(i) => i.to_string() == target,
            PdfValue::Real(r) => r.to_string() == target,
            PdfValue::Boolean(b) => b.to_string() == target,
            _ => false,
        }
    }

    fn value_to_string(&self, value: &PdfValue) -> String {
        match value {
            PdfValue::Name(n) => n.without_slash().to_string(),
            PdfValue::String(s) => s.to_string_lossy(),
            PdfValue::Integer(i) => i.to_string(),
            PdfValue::Real(r) => r.to_string(),
            PdfValue::Boolean(b) => b.to_string(),
            _ => String::new(),
        }
    }

    fn get_all_descendants(&self, node: NodeId) -> Vec<NodeId> {
        let mut descendants = Vec::new();
        let mut to_visit = vec![node];
        let mut visited = HashSet::new();

        while let Some(current) = to_visit.pop() {
            if visited.insert(current) {
                let children = self.graph.get_children(current);
                descendants.extend(&children);
                to_visit.extend(children);
            }
        }

        descendants
    }
}

pub struct QueryParser;

impl QueryParser {
    pub fn parse(query: &str) -> Result<QuerySelector, String> {
        let query = query.trim();

        if query.is_empty() {
            return Err("Empty query".to_string());
        }

        // Simple parser for CSS-like selectors
        if query.contains(" > ") {
            let parts: Vec<&str> = query.split(" > ").collect();
            if parts.len() == 2 {
                let parent = Self::parse_simple(parts[0])?;
                let child = Self::parse_simple(parts[1])?;
                return Ok(QuerySelector::Child(Box::new(parent), Box::new(child)));
            }
        }

        if query.contains(" ") {
            let parts: Vec<&str> = query.split_whitespace().collect();
            if parts.len() == 2 {
                let ancestor = Self::parse_simple(parts[0])?;
                let descendant = Self::parse_simple(parts[1])?;
                return Ok(QuerySelector::Descendant(
                    Box::new(ancestor),
                    Box::new(descendant),
                ));
            }
        }

        if query.contains(',') {
            let parts: Vec<&str> = query.split(',').map(|s| s.trim()).collect();
            let selectors: Result<Vec<_>, _> = parts.into_iter().map(Self::parse_simple).collect();
            return Ok(QuerySelector::Or(selectors?));
        }

        if query.starts_with(':') {
            return Self::parse_pseudo(query);
        }

        if query.starts_with('[') && query.ends_with(']') {
            return Self::parse_attribute(&query[1..query.len() - 1]);
        }

        Self::parse_simple(query)
    }

    fn parse_simple(query: &str) -> Result<QuerySelector, String> {
        if let Some(id_str) = query.strip_prefix('#') {
            // Parse object ID like #123.0
            if let Some(dot_pos) = id_str.find('.') {
                let num = id_str[..dot_pos]
                    .parse::<u32>()
                    .map_err(|_| "Invalid object number")?;
                let gen = id_str[dot_pos + 1..]
                    .parse::<u16>()
                    .map_err(|_| "Invalid generation number")?;
                return Ok(QuerySelector::ObjectId(num, gen));
            }
        }

        // Parse node type
        Ok(QuerySelector::NodeTypeName(query.to_lowercase()))
    }

    fn parse_pseudo(query: &str) -> Result<QuerySelector, String> {
        match query {
            ":first" | ":first-child" => Ok(QuerySelector::First),
            ":last" | ":last-child" => Ok(QuerySelector::Last),
            _ => {
                if query.starts_with(":nth-child(") && query.ends_with(')') {
                    let inner = &query[11..query.len() - 1];
                    let idx = inner.parse::<usize>().map_err(|_| "Invalid index")?;
                    Ok(QuerySelector::Index(idx))
                } else {
                    Err(format!("Unknown pseudo-selector: {}", query))
                }
            }
        }
    }

    fn parse_attribute(attr: &str) -> Result<QuerySelector, String> {
        if let Some(eq_pos) = attr.find('=') {
            let prop = attr[..eq_pos].trim();
            let value = attr[eq_pos + 1..]
                .trim()
                .trim_matches('"')
                .trim_matches('\'');
            Ok(QuerySelector::PropertyEquals(
                prop.to_string(),
                value.to_string(),
            ))
        } else {
            Ok(QuerySelector::HasProperty(attr.trim().to_string()))
        }
    }
}

pub struct QueryBuilder {
    selector: Option<QuerySelector>,
}

impl Default for QueryBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl QueryBuilder {
    pub fn new() -> Self {
        Self { selector: None }
    }

    pub fn node_type(mut self, node_type: NodeType) -> Self {
        self.selector = Some(QuerySelector::NodeType(node_type));
        self
    }

    pub fn object_id(mut self, num: u32, gen: u16) -> Self {
        self.selector = Some(QuerySelector::ObjectId(num, gen));
        self
    }

    pub fn has_property(mut self, prop: &str) -> Self {
        let new_sel = QuerySelector::HasProperty(prop.to_string());
        self.selector = Some(self.combine_with_and(new_sel));
        self
    }

    pub fn property_equals(mut self, prop: &str, value: &str) -> Self {
        let new_sel = QuerySelector::PropertyEquals(prop.to_string(), value.to_string());
        self.selector = Some(self.combine_with_and(new_sel));
        self
    }

    pub fn child_of(mut self, parent: QuerySelector) -> Self {
        if let Some(current) = self.selector {
            self.selector = Some(QuerySelector::Child(Box::new(parent), Box::new(current)));
        }
        self
    }

    pub fn descendant_of(mut self, ancestor: QuerySelector) -> Self {
        if let Some(current) = self.selector {
            self.selector = Some(QuerySelector::Descendant(
                Box::new(ancestor),
                Box::new(current),
            ));
        }
        self
    }

    pub fn and(mut self, other: QuerySelector) -> Self {
        self.selector = Some(self.combine_with_and(other));
        self
    }

    pub fn or(mut self, other: QuerySelector) -> Self {
        if let Some(current) = self.selector {
            self.selector = Some(QuerySelector::Or(vec![current, other]));
        } else {
            self.selector = Some(other);
        }
        self
    }

    pub fn not(mut self, selector: QuerySelector) -> Self {
        let new_sel = QuerySelector::Not(Box::new(selector));
        self.selector = Some(self.combine_with_and(new_sel));
        self
    }

    pub fn build(self) -> Result<QuerySelector, String> {
        self.selector.ok_or_else(|| "Empty query".to_string())
    }

    fn combine_with_and(&self, new_sel: QuerySelector) -> QuerySelector {
        if let Some(ref current) = self.selector {
            QuerySelector::And(vec![current.clone(), new_sel])
        } else {
            new_sel
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_parser() {
        let query = QueryParser::parse("page").unwrap();
        assert!(matches!(query, QuerySelector::NodeTypeName(_)));

        let query = QueryParser::parse("#123.0").unwrap();
        assert!(matches!(query, QuerySelector::ObjectId(123, 0)));

        let query = QueryParser::parse("[Type]").unwrap();
        assert!(matches!(query, QuerySelector::HasProperty(_)));

        let query = QueryParser::parse("[Type=Page]").unwrap();
        assert!(matches!(query, QuerySelector::PropertyEquals(_, _)));

        let query = QueryParser::parse("pages > page").unwrap();
        assert!(matches!(query, QuerySelector::Child(_, _)));
    }

    #[test]
    fn test_query_builder() {
        let query = QueryBuilder::new()
            .node_type(NodeType::Page)
            .has_property("Resources")
            .build()
            .unwrap();

        assert!(matches!(query, QuerySelector::And(_)));
    }

    #[test]
    fn reversed_query_range_is_empty() {
        let mut graph = PdfAstGraph::new();
        let root = graph.create_node(NodeType::Root, PdfValue::Null);
        let page = graph.create_node(NodeType::Page, PdfValue::Null);
        graph.add_edge(root, page, crate::ast::EdgeType::Child);
        graph.set_root(root);

        let mut engine = QueryEngine::new(&graph);
        assert!(engine
            .query_from(&QuerySelector::Range(2, 1), root)
            .is_empty());
    }
}
