#![allow(dead_code)]

use crate::ast::{AstNode, NodeId, PdfDocument, PdfVersion};
use crate::parser::PdfParser;
use crate::plugins::api::PluginManager;
use crate::validation::{SchemaRegistry, ValidationReport};
use neon::prelude::*;
use neon::types::buffer::TypedArray;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// JavaScript wrapper for PdfDocument
pub struct JsPdfDocument {
    inner: Arc<Mutex<PdfDocument>>,
}

impl Finalize for JsPdfDocument {}

impl JsPdfDocument {
    fn js_new(mut cx: FunctionContext) -> JsResult<JsObject> {
        let document = JsPdfDocument {
            inner: Arc::new(Mutex::new(PdfDocument::new(PdfVersion {
                major: 1,
                minor: 7,
            }))),
        };
        let document = wrap_box(&mut cx, document)?;
        attach_pdf_document_methods(&mut cx, document)?;
        Ok(document)
    }

    fn js_from_buffer(mut cx: FunctionContext) -> JsResult<JsObject> {
        let buffer = cx.argument::<JsBuffer>(0)?;
        let data = buffer.as_slice(&cx);

        let parser = PdfParser::new();
        match parser.parse_bytes(data) {
            Ok(document) => {
                let js_document = JsPdfDocument {
                    inner: Arc::new(Mutex::new(document)),
                };
                let js_document = wrap_box(&mut cx, js_document)?;
                attach_pdf_document_methods(&mut cx, js_document)?;
                Ok(js_document)
            }
            Err(e) => cx.throw_error(format!("Failed to parse PDF: {:?}", e)),
        }
    }

    fn js_get_version(mut cx: FunctionContext) -> JsResult<JsObject> {
        let this = cx
            .this()
            .downcast_or_throw::<JsObject, _>(&mut cx)?
            .get::<JsBox<JsPdfDocument>, _, _>(&mut cx, "__pdf_ast_inner")?;
        let document = this.inner.lock().unwrap();

        let version = cx.empty_object();
        let major = cx.number(document.version.major);
        let minor = cx.number(document.version.minor);

        version.set(&mut cx, "major", major)?;
        version.set(&mut cx, "minor", minor)?;

        Ok(version)
    }

    fn js_get_all_nodes(mut cx: FunctionContext) -> JsResult<JsArray> {
        let this = cx
            .this()
            .downcast_or_throw::<JsObject, _>(&mut cx)?
            .get::<JsBox<JsPdfDocument>, _, _>(&mut cx, "__pdf_ast_inner")?;
        let document = this.inner.lock().unwrap();

        let nodes = document.ast.get_all_nodes();
        let js_array = cx.empty_array();

        for (i, node) in nodes.into_iter().enumerate() {
            let js_node = JsAstNode::from_node(&mut cx, (*node).clone())?;
            js_array.set(&mut cx, i as u32, js_node)?;
        }

        Ok(js_array)
    }

    fn js_get_root(mut cx: FunctionContext) -> JsResult<JsValue> {
        let this = cx
            .this()
            .downcast_or_throw::<JsObject, _>(&mut cx)?
            .get::<JsBox<JsPdfDocument>, _, _>(&mut cx, "__pdf_ast_inner")?;
        let document = this.inner.lock().unwrap();

        if let Some(root_id) = document.ast.get_root() {
            if let Some(root_node) = document.ast.get_node(root_id) {
                let js_node = JsAstNode::from_node(&mut cx, root_node.clone())?;
                return Ok(js_node.upcast());
            }
        }

        Ok(cx.null().upcast())
    }

    fn js_get_node(mut cx: FunctionContext) -> JsResult<JsValue> {
        let this = cx
            .this()
            .downcast_or_throw::<JsObject, _>(&mut cx)?
            .get::<JsBox<JsPdfDocument>, _, _>(&mut cx, "__pdf_ast_inner")?;
        let node_id = cx.argument::<JsNumber>(0)?.value(&mut cx) as u64;
        let document = this.inner.lock().unwrap();

        let node_id = match usize::try_from(node_id) {
            Ok(id) => NodeId(id),
            Err(_) => return Ok(cx.null().upcast()),
        };

        if let Some(node) = document.ast.get_node(node_id) {
            let js_node = JsAstNode::from_node(&mut cx, node.clone())?;
            Ok(js_node.upcast())
        } else {
            Ok(cx.null().upcast())
        }
    }

    fn js_get_children(mut cx: FunctionContext) -> JsResult<JsArray> {
        let this = cx
            .this()
            .downcast_or_throw::<JsObject, _>(&mut cx)?
            .get::<JsBox<JsPdfDocument>, _, _>(&mut cx, "__pdf_ast_inner")?;
        let node_id = cx.argument::<JsNumber>(0)?.value(&mut cx) as u64;
        let document = this.inner.lock().unwrap();

        let node_id = match usize::try_from(node_id) {
            Ok(id) => NodeId(id),
            Err(_) => return Ok(cx.empty_array()),
        };

        let children = document.ast.get_children(node_id);
        let js_array = cx.empty_array();

        for (i, &child_id) in children.iter().enumerate() {
            if let Some(child_node) = document.ast.get_node(child_id) {
                let js_node = JsAstNode::from_node(&mut cx, child_node.clone())?;
                js_array.set(&mut cx, i as u32, js_node)?;
            }
        }

        Ok(js_array)
    }

    fn js_get_nodes_by_type(mut cx: FunctionContext) -> JsResult<JsArray> {
        let this = cx
            .this()
            .downcast_or_throw::<JsObject, _>(&mut cx)?
            .get::<JsBox<JsPdfDocument>, _, _>(&mut cx, "__pdf_ast_inner")?;
        let type_str = cx.argument::<JsString>(0)?.value(&mut cx);
        let document = this.inner.lock().unwrap();

        let js_array = cx.empty_array();

        if let Ok(node_type) = super::utils::parse_node_type(&type_str) {
            let nodes = document.ast.get_nodes_by_type(node_type);

            for (i, node_id) in nodes.iter().enumerate() {
                if let Some(node) = document.ast.get_node(*node_id) {
                    let js_node = JsAstNode::from_node(&mut cx, node.clone())?;
                    js_array.set(&mut cx, i as u32, js_node)?;
                }
            }
        }

        Ok(js_array)
    }

    fn js_validate(mut cx: FunctionContext) -> JsResult<JsObject> {
        let this = cx
            .this()
            .downcast_or_throw::<JsObject, _>(&mut cx)?
            .get::<JsBox<JsPdfDocument>, _, _>(&mut cx, "__pdf_ast_inner")?;
        let schema_name = cx.argument::<JsString>(0)?.value(&mut cx);
        let document = this.inner.lock().unwrap();

        let registry = SchemaRegistry::new();

        match registry.validate(&document, &schema_name) {
            Some(report) => {
                let js_report = JsValidationReport {
                    inner: Arc::new(report),
                };
                let js_report = wrap_box(&mut cx, js_report)?;
                attach_validation_report_methods(&mut cx, js_report)?;
                Ok(js_report)
            }
            None => cx.throw_error(format!("Schema '{}' not found", schema_name)),
        }
    }

    fn js_get_statistics(mut cx: FunctionContext) -> JsResult<JsObject> {
        let this = cx
            .this()
            .downcast_or_throw::<JsObject, _>(&mut cx)?
            .get::<JsBox<JsPdfDocument>, _, _>(&mut cx, "__pdf_ast_inner")?;
        let document = this.inner.lock().unwrap();

        let stats = cx.empty_object();
        let total_nodes = document.ast.get_all_nodes().len();
        let total_nodes_value = cx.number(total_nodes as f64);
        stats.set(&mut cx, "totalNodes", total_nodes_value)?;

        // Count nodes by type
        let mut type_counts = HashMap::new();
        for node in document.ast.get_all_nodes() {
            let type_name = super::utils::node_type_to_string(&node.node_type);
            *type_counts.entry(type_name).or_insert(0) += 1;
        }

        let type_stats = cx.empty_object();
        for (node_type, count) in type_counts {
            let count_value = cx.number(count as f64);
            type_stats.set(&mut cx, node_type.as_str(), count_value)?;
        }
        stats.set(&mut cx, "nodeTypes", type_stats)?;

        let version_str = format!("{}.{}", document.version.major, document.version.minor);
        let version_value = cx.string(version_str);
        stats.set(&mut cx, "version", version_value)?;

        Ok(stats)
    }
}

/// JavaScript wrapper for AstNode
pub struct JsAstNode {
    inner: AstNode,
}

impl Finalize for JsAstNode {}

impl JsAstNode {
    fn from_node<'a>(cx: &mut FunctionContext<'a>, node: AstNode) -> JsResult<'a, JsObject> {
        let js_node = JsAstNode { inner: node };
        let js_node = wrap_box(cx, js_node)?;
        attach_ast_node_methods(cx, js_node)?;
        Ok(js_node)
    }

    fn js_get_id(mut cx: FunctionContext) -> JsResult<JsNumber> {
        let this = cx
            .this()
            .downcast_or_throw::<JsObject, _>(&mut cx)?
            .get::<JsBox<JsAstNode>, _, _>(&mut cx, "__pdf_ast_inner")?;
        Ok(cx.number(this.inner.id.0 as f64))
    }

    fn js_get_type(mut cx: FunctionContext) -> JsResult<JsString> {
        let this = cx
            .this()
            .downcast_or_throw::<JsObject, _>(&mut cx)?
            .get::<JsBox<JsAstNode>, _, _>(&mut cx, "__pdf_ast_inner")?;
        let type_name = super::utils::node_type_to_string(&this.inner.node_type);
        Ok(cx.string(type_name))
    }

    fn js_get_value(mut cx: FunctionContext) -> JsResult<JsString> {
        let this = cx
            .this()
            .downcast_or_throw::<JsObject, _>(&mut cx)?
            .get::<JsBox<JsAstNode>, _, _>(&mut cx, "__pdf_ast_inner")?;
        let value_str = format!("{:?}", this.inner.value);
        Ok(cx.string(value_str))
    }

    fn js_get_metadata(mut cx: FunctionContext) -> JsResult<JsValue> {
        let this = cx
            .this()
            .downcast_or_throw::<JsObject, _>(&mut cx)?
            .get::<JsBox<JsAstNode>, _, _>(&mut cx, "__pdf_ast_inner")?;
        let meta = cx.empty_object();
        if let Some(offset) = this.inner.metadata.offset {
            let offset_value = cx.number(offset as f64);
            meta.set(&mut cx, "offset", offset_value)?;
        }
        if let Some(size) = this.inner.metadata.size {
            let size_value = cx.number(size as f64);
            meta.set(&mut cx, "size", size_value)?;
        }
        let warnings = cx.empty_array();
        for (i, warning) in this.inner.metadata.warnings.iter().enumerate() {
            let warning_value = cx.string(warning);
            warnings.set(&mut cx, i as u32, warning_value)?;
        }
        meta.set(&mut cx, "warnings", warnings)?;
        let error_count = cx.number(this.inner.metadata.errors.len() as f64);
        meta.set(&mut cx, "errorCount", error_count)?;
        Ok(meta.upcast())
    }

    fn js_has_property(mut cx: FunctionContext) -> JsResult<JsBoolean> {
        let this = cx
            .this()
            .downcast_or_throw::<JsObject, _>(&mut cx)?
            .get::<JsBox<JsAstNode>, _, _>(&mut cx, "__pdf_ast_inner")?;
        let key = cx.argument::<JsString>(0)?.value(&mut cx);

        let has_prop = match &this.inner.value {
            crate::types::PdfValue::Dictionary(dict) => dict.contains_key(&key),
            _ => false,
        };

        Ok(cx.boolean(has_prop))
    }

    fn js_get_property(mut cx: FunctionContext) -> JsResult<JsValue> {
        let this = cx
            .this()
            .downcast_or_throw::<JsObject, _>(&mut cx)?
            .get::<JsBox<JsAstNode>, _, _>(&mut cx, "__pdf_ast_inner")?;
        let key = cx.argument::<JsString>(0)?.value(&mut cx);

        match &this.inner.value {
            crate::types::PdfValue::Dictionary(dict) => {
                if let Some(value) = dict.get(&key) {
                    let value_str = format!("{:?}", value);
                    Ok(cx.string(value_str).upcast())
                } else {
                    Ok(cx.null().upcast())
                }
            }
            _ => Ok(cx.null().upcast()),
        }
    }
}

/// JavaScript wrapper for ValidationReport
pub struct JsValidationReport {
    inner: Arc<ValidationReport>,
}

impl Finalize for JsValidationReport {}

impl JsValidationReport {
    fn js_is_valid(mut cx: FunctionContext) -> JsResult<JsBoolean> {
        let this = cx
            .this()
            .downcast_or_throw::<JsObject, _>(&mut cx)?
            .get::<JsBox<JsValidationReport>, _, _>(&mut cx, "__pdf_ast_inner")?;
        Ok(cx.boolean(this.inner.is_valid))
    }

    fn js_get_schema_name(mut cx: FunctionContext) -> JsResult<JsString> {
        let this = cx
            .this()
            .downcast_or_throw::<JsObject, _>(&mut cx)?
            .get::<JsBox<JsValidationReport>, _, _>(&mut cx, "__pdf_ast_inner")?;
        Ok(cx.string(&this.inner.schema_name))
    }

    fn js_get_schema_version(mut cx: FunctionContext) -> JsResult<JsString> {
        let this = cx
            .this()
            .downcast_or_throw::<JsObject, _>(&mut cx)?
            .get::<JsBox<JsValidationReport>, _, _>(&mut cx, "__pdf_ast_inner")?;
        Ok(cx.string(&this.inner.schema_version))
    }

    fn js_get_issues(mut cx: FunctionContext) -> JsResult<JsArray> {
        let this = cx
            .this()
            .downcast_or_throw::<JsObject, _>(&mut cx)?
            .get::<JsBox<JsValidationReport>, _, _>(&mut cx, "__pdf_ast_inner")?;
        let js_array = cx.empty_array();

        for (i, issue) in this.inner.issues.iter().enumerate() {
            let js_issue = JsValidationIssue::from_issue(&mut cx, issue.clone())?;
            js_array.set(&mut cx, i as u32, js_issue)?;
        }

        Ok(js_array)
    }

    fn js_get_statistics(mut cx: FunctionContext) -> JsResult<JsObject> {
        let this = cx
            .this()
            .downcast_or_throw::<JsObject, _>(&mut cx)?
            .get::<JsBox<JsValidationReport>, _, _>(&mut cx, "__pdf_ast_inner")?;
        let stats = cx.empty_object();

        let total_checks_value = cx.number(this.inner.statistics.total_checks as f64);
        stats.set(&mut cx, "totalChecks", total_checks_value)?;
        let passed_checks_value = cx.number(this.inner.statistics.passed_checks as f64);
        stats.set(&mut cx, "passedChecks", passed_checks_value)?;
        let failed_checks_value = cx.number(this.inner.statistics.failed_checks as f64);
        stats.set(&mut cx, "failedChecks", failed_checks_value)?;
        let info_count_value = cx.number(this.inner.statistics.info_count as f64);
        stats.set(&mut cx, "infoCount", info_count_value)?;
        let warning_count_value = cx.number(this.inner.statistics.warning_count as f64);
        stats.set(&mut cx, "warningCount", warning_count_value)?;
        let error_count_value = cx.number(this.inner.statistics.error_count as f64);
        stats.set(&mut cx, "errorCount", error_count_value)?;
        let critical_count_value = cx.number(this.inner.statistics.critical_count as f64);
        stats.set(&mut cx, "criticalCount", critical_count_value)?;

        Ok(stats)
    }
}

/// JavaScript wrapper for ValidationIssue
pub struct JsValidationIssue {
    inner: crate::validation::ValidationIssue,
}

impl Finalize for JsValidationIssue {}

impl JsValidationIssue {
    fn from_issue<'a>(
        cx: &mut FunctionContext<'a>,
        issue: crate::validation::ValidationIssue,
    ) -> JsResult<'a, JsObject> {
        let js_issue = JsValidationIssue { inner: issue };
        let js_issue = wrap_box(cx, js_issue)?;
        attach_validation_issue_methods(cx, js_issue)?;
        Ok(js_issue)
    }

    fn js_get_severity(mut cx: FunctionContext) -> JsResult<JsString> {
        let this = cx
            .this()
            .downcast_or_throw::<JsObject, _>(&mut cx)?
            .get::<JsBox<JsValidationIssue>, _, _>(&mut cx, "__pdf_ast_inner")?;
        let severity = format!("{:?}", this.inner.severity);
        Ok(cx.string(severity))
    }

    fn js_get_code(mut cx: FunctionContext) -> JsResult<JsString> {
        let this = cx
            .this()
            .downcast_or_throw::<JsObject, _>(&mut cx)?
            .get::<JsBox<JsValidationIssue>, _, _>(&mut cx, "__pdf_ast_inner")?;
        Ok(cx.string(&this.inner.code))
    }

    fn js_get_message(mut cx: FunctionContext) -> JsResult<JsString> {
        let this = cx
            .this()
            .downcast_or_throw::<JsObject, _>(&mut cx)?
            .get::<JsBox<JsValidationIssue>, _, _>(&mut cx, "__pdf_ast_inner")?;
        Ok(cx.string(&this.inner.message))
    }

    fn js_get_node_id(mut cx: FunctionContext) -> JsResult<JsValue> {
        let this = cx
            .this()
            .downcast_or_throw::<JsObject, _>(&mut cx)?
            .get::<JsBox<JsValidationIssue>, _, _>(&mut cx, "__pdf_ast_inner")?;

        if let Some(node_id) = this.inner.node_id {
            Ok(cx.number(node_id.0 as f64).upcast())
        } else {
            Ok(cx.null().upcast())
        }
    }

    fn js_get_location(mut cx: FunctionContext) -> JsResult<JsValue> {
        let this = cx
            .this()
            .downcast_or_throw::<JsObject, _>(&mut cx)?
            .get::<JsBox<JsValidationIssue>, _, _>(&mut cx, "__pdf_ast_inner")?;

        if let Some(ref location) = this.inner.location {
            Ok(cx.string(location).upcast())
        } else {
            Ok(cx.null().upcast())
        }
    }

    fn js_get_suggestion(mut cx: FunctionContext) -> JsResult<JsValue> {
        let this = cx
            .this()
            .downcast_or_throw::<JsObject, _>(&mut cx)?
            .get::<JsBox<JsValidationIssue>, _, _>(&mut cx, "__pdf_ast_inner")?;

        if let Some(ref suggestion) = this.inner.suggestion {
            Ok(cx.string(suggestion).upcast())
        } else {
            Ok(cx.null().upcast())
        }
    }
}

/// JavaScript wrapper for PluginManager
pub struct JsPluginManager {
    inner: Arc<Mutex<PluginManager>>,
}

impl Finalize for JsPluginManager {}

impl JsPluginManager {
    fn js_new(mut cx: FunctionContext) -> JsResult<JsObject> {
        let manager = JsPluginManager {
            inner: Arc::new(Mutex::new(PluginManager::new())),
        };
        let manager = wrap_box(&mut cx, manager)?;
        attach_plugin_manager_methods(&mut cx, manager)?;
        Ok(manager)
    }

    fn js_execute_plugins(mut cx: FunctionContext) -> JsResult<JsObject> {
        let this = cx
            .this()
            .downcast_or_throw::<JsObject, _>(&mut cx)?
            .get::<JsBox<JsPluginManager>, _, _>(&mut cx, "__pdf_ast_inner")?;
        let js_document = cx
            .argument::<JsObject>(0)?
            .get::<JsBox<JsPdfDocument>, _, _>(&mut cx, "__pdf_ast_inner")?;

        let manager = this.inner.lock().unwrap();
        let mut document = js_document.inner.lock().unwrap().clone();
        let summary = manager.execute_plugins(&mut document);

        // Update the document
        *js_document.inner.lock().unwrap() = document;

        let result = cx.empty_object();
        let total_plugins = cx.number(summary.total_plugins as f64);
        result.set(&mut cx, "totalPlugins", total_plugins)?;
        let successful_plugins_value = cx.number(summary.successful_plugins as f64);
        result.set(&mut cx, "successfulPlugins", successful_plugins_value)?;
        let failed_plugins = cx.number(summary.failed_plugins as f64);
        result.set(&mut cx, "failedPlugins", failed_plugins)?;
        let execution_time_value = cx.number(summary.total_execution_time_ms as f64);
        result.set(&mut cx, "executionTimeMs", execution_time_value)?;

        // Convert plugin results
        let results_obj = cx.empty_object();
        for (name, result) in summary.plugin_results {
            let result_str = format!("{:?}", result);
            let result_value = cx.string(result_str);
            results_obj.set(&mut cx, name.as_str(), result_value)?;
        }
        result.set(&mut cx, "pluginResults", results_obj)?;

        Ok(result)
    }

    fn js_list_plugins(mut cx: FunctionContext) -> JsResult<JsArray> {
        let this = cx
            .this()
            .downcast_or_throw::<JsObject, _>(&mut cx)?
            .get::<JsBox<JsPluginManager>, _, _>(&mut cx, "__pdf_ast_inner")?;
        let manager = this.inner.lock().unwrap();

        let plugins = manager.list_plugins();
        let js_array = cx.empty_array();

        for (i, metadata) in plugins.iter().enumerate() {
            let plugin_obj = cx.empty_object();
            let name_value = cx.string(&metadata.name);
            plugin_obj.set(&mut cx, "name", name_value)?;
            let version_value = cx.string(&metadata.version);
            plugin_obj.set(&mut cx, "version", version_value)?;
            let description_value = cx.string(&metadata.description);
            plugin_obj.set(&mut cx, "description", description_value)?;
            let author_value = cx.string(&metadata.author);
            plugin_obj.set(&mut cx, "author", author_value)?;

            let tags_array = cx.empty_array();
            for (j, tag) in metadata.tags.iter().enumerate() {
                let tag_value = cx.string(tag);
                tags_array.set(&mut cx, j as u32, tag_value)?;
            }
            plugin_obj.set(&mut cx, "tags", tags_array)?;

            js_array.set(&mut cx, i as u32, plugin_obj)?;
        }

        Ok(js_array)
    }
}

fn wrap_box<'a, T>(cx: &mut FunctionContext<'a>, value: T) -> JsResult<'a, JsObject>
where
    T: Finalize + Send + 'static,
{
    let inner = cx.boxed(value);
    let object = cx.empty_object();
    object.set(cx, "__pdf_ast_inner", inner)?;
    Ok(object)
}

fn attach_method<R>(
    cx: &mut FunctionContext,
    object: Handle<JsObject>,
    name: &str,
    method: fn(FunctionContext) -> JsResult<R>,
) -> NeonResult<()>
where
    R: Value,
{
    let function = JsFunction::new(cx, method)?;
    object.set(cx, name, function)?;
    Ok(())
}

fn attach_pdf_document_methods(
    cx: &mut FunctionContext,
    object: Handle<JsObject>,
) -> NeonResult<()> {
    attach_method(cx, object, "getVersion", JsPdfDocument::js_get_version)?;
    attach_method(cx, object, "getAllNodes", JsPdfDocument::js_get_all_nodes)?;
    attach_method(cx, object, "getRoot", JsPdfDocument::js_get_root)?;
    attach_method(cx, object, "getNode", JsPdfDocument::js_get_node)?;
    attach_method(cx, object, "getChildren", JsPdfDocument::js_get_children)?;
    attach_method(
        cx,
        object,
        "getNodesByType",
        JsPdfDocument::js_get_nodes_by_type,
    )?;
    attach_method(cx, object, "validate", JsPdfDocument::js_validate)?;
    attach_method(
        cx,
        object,
        "getStatistics",
        JsPdfDocument::js_get_statistics,
    )?;
    Ok(())
}

fn attach_ast_node_methods(cx: &mut FunctionContext, object: Handle<JsObject>) -> NeonResult<()> {
    attach_method(cx, object, "getId", JsAstNode::js_get_id)?;
    attach_method(cx, object, "getType", JsAstNode::js_get_type)?;
    attach_method(cx, object, "getValue", JsAstNode::js_get_value)?;
    attach_method(cx, object, "getMetadata", JsAstNode::js_get_metadata)?;
    attach_method(cx, object, "hasProperty", JsAstNode::js_has_property)?;
    attach_method(cx, object, "getProperty", JsAstNode::js_get_property)?;
    Ok(())
}

fn attach_validation_report_methods(
    cx: &mut FunctionContext,
    object: Handle<JsObject>,
) -> NeonResult<()> {
    attach_method(cx, object, "isValid", JsValidationReport::js_is_valid)?;
    attach_method(
        cx,
        object,
        "getSchemaName",
        JsValidationReport::js_get_schema_name,
    )?;
    attach_method(
        cx,
        object,
        "getSchemaVersion",
        JsValidationReport::js_get_schema_version,
    )?;
    attach_method(cx, object, "getIssues", JsValidationReport::js_get_issues)?;
    attach_method(
        cx,
        object,
        "getStatistics",
        JsValidationReport::js_get_statistics,
    )?;
    Ok(())
}

fn attach_validation_issue_methods(
    cx: &mut FunctionContext,
    object: Handle<JsObject>,
) -> NeonResult<()> {
    attach_method(
        cx,
        object,
        "getSeverity",
        JsValidationIssue::js_get_severity,
    )?;
    attach_method(cx, object, "getCode", JsValidationIssue::js_get_code)?;
    attach_method(cx, object, "getMessage", JsValidationIssue::js_get_message)?;
    attach_method(cx, object, "getNodeId", JsValidationIssue::js_get_node_id)?;
    attach_method(
        cx,
        object,
        "getLocation",
        JsValidationIssue::js_get_location,
    )?;
    attach_method(
        cx,
        object,
        "getSuggestion",
        JsValidationIssue::js_get_suggestion,
    )?;
    Ok(())
}

fn attach_plugin_manager_methods(
    cx: &mut FunctionContext,
    object: Handle<JsObject>,
) -> NeonResult<()> {
    attach_method(
        cx,
        object,
        "executePlugins",
        JsPluginManager::js_execute_plugins,
    )?;
    attach_method(cx, object, "listPlugins", JsPluginManager::js_list_plugins)?;
    Ok(())
}

/// Module-level functions
fn js_parse_pdf(cx: FunctionContext) -> JsResult<JsObject> {
    JsPdfDocument::js_from_buffer(cx)
}

fn js_get_available_schemas(mut cx: FunctionContext) -> JsResult<JsArray> {
    let registry = SchemaRegistry::new();
    let schemas = registry.list_schemas();
    let js_array = cx.empty_array();

    for (i, schema) in schemas.iter().enumerate() {
        let schema_value = cx.string(schema);
        js_array.set(&mut cx, i as u32, schema_value)?;
    }

    Ok(js_array)
}

fn js_get_node_types(mut cx: FunctionContext) -> JsResult<JsArray> {
    let types = super::utils::list_node_types();
    let js_array = cx.empty_array();

    for (i, node_type) in types.iter().enumerate() {
        let type_value = cx.string(node_type);
        js_array.set(&mut cx, i as u32, type_value)?;
    }

    Ok(js_array)
}

/// Initialize the JavaScript module
#[neon::main]
fn main(mut cx: ModuleContext) -> NeonResult<()> {
    // PdfDocument class
    cx.export_function("PdfDocument", JsPdfDocument::js_new)?;
    cx.export_function("parseDocument", js_parse_pdf)?;

    // AstNode class methods are attached to instances

    // ValidationReport class methods are attached to instances

    // PluginManager class
    cx.export_function("PluginManager", JsPluginManager::js_new)?;

    // Module-level functions
    cx.export_function("getAvailableSchemas", js_get_available_schemas)?;
    cx.export_function("getNodeTypes", js_get_node_types)?;

    // Constants
    let version_value = cx.string("0.1.0");
    cx.export_value("VERSION", version_value)?;
    let author_value = cx.string("PDF-AST Project");
    cx.export_value("AUTHOR", author_value)?;

    Ok(())
}
