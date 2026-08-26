use pdf_ast::ast::{AstNode, NodeType, PdfDocument};
use pdf_ast::plugins::{
    AstPlugin, PluginCapabilities, PluginContext, PluginMetadata, PluginResult,
};

#[derive(Clone)]
pub struct ExampleMetadataPlugin {
    metadata: PluginMetadata,
}

impl ExampleMetadataPlugin {
    pub fn new() -> Self {
        Self {
            metadata: PluginMetadata::new(
                "example_metadata",
                "0.1.0",
                "Example dynamic plugin that records basic document stats",
                "PDF-AST",
            ),
        }
    }
}

impl Default for ExampleMetadataPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl AstPlugin for ExampleMetadataPlugin {
    fn metadata(&self) -> &PluginMetadata {
        &self.metadata
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities {
            can_modify_nodes: false,
            can_add_nodes: false,
            can_remove_nodes: false,
            can_validate: false,
            can_transform: false,
            requires_document_context: true,
            thread_safe: true,
        }
    }

    fn process_document(
        &self,
        document: &mut PdfDocument,
        context: &mut PluginContext,
    ) -> PluginResult {
        let node_count = document.ast.get_all_nodes().len();
        let page_count = document.get_pages().len();
        let info = serde_json::json!({
            "node_count": node_count,
            "page_count": page_count,
        });
        context.set_shared_data("example_metadata".to_string(), info);
        PluginResult::Success
    }

    fn process_node(&self, _node: &mut AstNode, _context: &mut PluginContext) -> PluginResult {
        PluginResult::Success
    }

    fn can_process_node_type(&self, _node_type: &NodeType) -> bool {
        false
    }

    fn clone_plugin(&self) -> Box<dyn AstPlugin> {
        Box::new(self.clone())
    }
}

#[no_mangle]
pub extern "C" fn pdf_ast_plugin_factory() -> *mut std::ffi::c_void {
    let plugin: Box<dyn AstPlugin> = Box::new(ExampleMetadataPlugin::new());
    let boxed = Box::new(plugin);
    Box::into_raw(boxed) as *mut std::ffi::c_void
}

#[allow(dead_code)]
pub extern "C" fn pdf_ast_plugin_api_version() -> *const u8 {
    c"1.0.0".as_ptr().cast()
}

#[allow(dead_code)]
pub extern "C" fn pdf_ast_plugin_name() -> *const u8 {
    c"example_metadata".as_ptr().cast()
}

#[allow(dead_code)]
pub extern "C" fn pdf_ast_plugin_description() -> *const u8 {
    c"Example dynamic plugin that records basic document stats"
        .as_ptr()
        .cast()
}

#[allow(dead_code)]
pub extern "C" fn pdf_ast_plugin_author() -> *const u8 {
    c"PDF-AST".as_ptr().cast()
}

#[allow(dead_code)]
pub extern "C" fn pdf_ast_plugin_license() -> *const u8 {
    c"MIT".as_ptr().cast()
}

#[allow(dead_code)]
pub extern "C" fn pdf_ast_plugin_homepage() -> *const u8 {
    c"https://github.com/seifreed/pdf-core".as_ptr().cast()
}

#[allow(dead_code)]
pub extern "C" fn pdf_ast_plugin_repository() -> *const u8 {
    c"https://github.com/seifreed/pdf-core".as_ptr().cast()
}

#[allow(dead_code)]
pub extern "C" fn pdf_ast_plugin_tags() -> *const u8 {
    c"metadata,stats".as_ptr().cast()
}

#[allow(dead_code)]
pub extern "C" fn pdf_ast_plugin_supported_node_types() -> *const u8 {
    c"Catalog,Page,Pages".as_ptr().cast()
}

#[allow(dead_code)]
pub extern "C" fn pdf_ast_plugin_dependencies() -> *const u8 {
    c"".as_ptr().cast()
}
