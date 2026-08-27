use crate::{AstNode, NodeType, PdfDocument, PdfParser};
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::panic::AssertUnwindSafe;
use std::ptr;
use std::slice;

/// Opaque handle for PdfDocument
#[repr(C)]
pub struct CPdfDocument(*mut PdfDocument);

/// Opaque handle for AstNode
#[repr(C)]
pub struct CAstNode(*mut AstNode);

/// Error codes for C API
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CErrorCode {
    Success = 0,
    InvalidInput = 1,
    ParseError = 2,
    MemoryError = 3,
    NullPointer = 4,
    InvalidHandle = 5,
}

/// Result structure for C API
#[repr(C)]
pub struct CResult {
    pub error_code: CErrorCode,
    pub message: *mut c_char,
}

/// Node information for C API
#[repr(C)]
pub struct CNodeInfo {
    pub id: u64,
    pub node_type: u32,
    pub has_children: bool,
    pub children_count: usize,
}

impl CResult {
    fn success() -> Self {
        CResult {
            error_code: CErrorCode::Success,
            message: ptr::null_mut(),
        }
    }

    fn error(code: CErrorCode, message: &str) -> Self {
        let c_message = CString::new(message).unwrap_or_default();
        CResult {
            error_code: code,
            message: c_message.into_raw(),
        }
    }
}

/// Initialize the PDF-AST library
#[no_mangle]
pub extern "C" fn pdf_ast_init() -> CResult {
    // Initialize logging if needed
    let _ = env_logger::try_init();
    CResult::success()
}

/// Free an array returned by `pdf_ast_get_children`.
///
/// The child node handles remain owned by the caller and must be released with
/// `pdf_ast_free_node` individually.
///
/// # Safety
///
/// `children` must be the exact pointer returned by `pdf_ast_get_children`,
/// and `count` must be the returned child count.
#[no_mangle]
pub unsafe extern "C" fn pdf_ast_free_children(children: *mut *mut CAstNode, count: usize) {
    if !children.is_null() {
        unsafe {
            let _ = Vec::from_raw_parts(children, count, count);
        }
    }
}

/// Parse PDF from byte buffer
///
/// # Safety
///
/// This function is unsafe because:
/// - `data` must be a valid pointer to a buffer of at least `len` bytes
/// - `document` must be a valid pointer to a location where a document pointer can be stored
/// - The caller must ensure the data buffer remains valid for the duration of this call
#[no_mangle]
pub unsafe extern "C" fn pdf_ast_parse(
    data: *const u8,
    len: usize,
    document: *mut *mut CPdfDocument,
) -> CResult {
    if data.is_null() || document.is_null() {
        return CResult::error(CErrorCode::NullPointer, "Null pointer provided");
    }

    let pdf_data = unsafe { slice::from_raw_parts(data, len) };

    let parser = PdfParser::new();
    let reader = std::io::Cursor::new(pdf_data);

    let parsed = std::panic::catch_unwind(AssertUnwindSafe(|| {
        parser.parse(std::io::BufReader::new(reader))
    }));
    match parsed {
        Ok(Ok(doc)) => {
            let boxed_doc = Box::new(doc);
            let c_doc = Box::new(CPdfDocument(Box::into_raw(boxed_doc)));
            unsafe {
                *document = Box::into_raw(c_doc);
            }
            CResult::success()
        }
        Ok(Err(e)) => CResult::error(CErrorCode::ParseError, &format!("Parse error: {}", e)),
        Err(_) => CResult::error(CErrorCode::ParseError, "Parser panicked during parsing"),
    }
}

/// Parse PDF from file path
///
/// # Safety
///
/// This function is unsafe because:
/// - `path` must be a valid null-terminated C string pointer
/// - `document` must be a valid pointer to a location where a document pointer can be stored
/// - The caller must ensure the path string remains valid for the duration of this call
#[no_mangle]
pub unsafe extern "C" fn pdf_ast_parse_file(
    path: *const c_char,
    document: *mut *mut CPdfDocument,
) -> CResult {
    if path.is_null() || document.is_null() {
        return CResult::error(CErrorCode::NullPointer, "Null pointer provided");
    }

    let c_path = unsafe { CStr::from_ptr(path) };
    let path_str = match c_path.to_str() {
        Ok(s) => s,
        Err(_) => return CResult::error(CErrorCode::InvalidInput, "Invalid UTF-8 path"),
    };

    let file = match std::fs::File::open(path_str) {
        Ok(f) => f,
        Err(e) => {
            return CResult::error(
                CErrorCode::InvalidInput,
                &format!("Cannot open file: {}", e),
            )
        }
    };

    let parser = PdfParser::new();
    let reader = std::io::BufReader::new(file);

    let parsed = std::panic::catch_unwind(AssertUnwindSafe(|| parser.parse(reader)));
    match parsed {
        Ok(Ok(doc)) => {
            let boxed_doc = Box::new(doc);
            let c_doc = Box::new(CPdfDocument(Box::into_raw(boxed_doc)));
            unsafe {
                *document = Box::into_raw(c_doc);
            }
            CResult::success()
        }
        Ok(Err(e)) => CResult::error(CErrorCode::ParseError, &format!("Parse error: {}", e)),
        Err(_) => CResult::error(CErrorCode::ParseError, "Parser panicked during parsing"),
    }
}

/// Get document node count
/// Get the total number of nodes in the document AST
///
/// # Safety
///
/// This function is unsafe because:
/// - `document` must be a valid pointer to a CPdfDocument
/// - The document must have been created by this library and not freed
#[no_mangle]
pub unsafe extern "C" fn pdf_ast_get_node_count(document: *const CPdfDocument) -> usize {
    if document.is_null() {
        return 0;
    }

    let doc = unsafe { &*((*document).0) };
    doc.ast.node_count()
}

/// Get document edge count
/// Get the total number of edges in the document AST
///
/// # Safety
///
/// This function is unsafe because:
/// - `document` must be a valid pointer to a CPdfDocument
/// - The document must have been created by this library and not freed
#[no_mangle]
pub unsafe extern "C" fn pdf_ast_get_edge_count(document: *const CPdfDocument) -> usize {
    if document.is_null() {
        return 0;
    }

    let doc = unsafe { &*((*document).0) };
    doc.ast.edge_count()
}

/// Get document root node
/// Get the root node of the document AST
///
/// # Safety
///
/// This function is unsafe because:
/// - `document` must be a valid pointer to a CPdfDocument
/// - `node` must be a valid pointer to a location where a node pointer can be stored
/// - The document must have been created by this library and not freed
#[no_mangle]
pub unsafe extern "C" fn pdf_ast_get_root_node(
    document: *const CPdfDocument,
    node: *mut *mut CAstNode,
) -> CResult {
    if document.is_null() || node.is_null() {
        return CResult::error(CErrorCode::NullPointer, "Null pointer provided");
    }

    let doc = unsafe { &*((*document).0) };
    if let Some(root_id) = doc.ast.get_root() {
        if let Some(root_node) = doc.ast.get_node(root_id) {
            let cloned_node = Box::new(root_node.clone());
            let c_node = Box::new(CAstNode(Box::into_raw(cloned_node)));
            unsafe {
                *node = Box::into_raw(c_node);
            }
            return CResult::success();
        }
    }

    CResult::error(CErrorCode::InvalidHandle, "No root node found")
}

/// Get node information
/// Get information about a specific node
///
/// # Safety
///
/// This function is unsafe because:
/// - `node` must be a valid pointer to a CAstNode
/// - `info` must be a valid pointer to a CNodeInfo structure
/// - The node must have been created by this library and not freed
#[no_mangle]
pub unsafe extern "C" fn pdf_ast_get_node_info(
    node: *const CAstNode,
    info: *mut CNodeInfo,
) -> CResult {
    if node.is_null() || info.is_null() {
        return CResult::error(CErrorCode::NullPointer, "Null pointer provided");
    }

    let ast_node = unsafe { &*((*node).0) };

    unsafe {
        (*info).id = ast_node.id.0 as u64;
        (*info).node_type = node_type_to_u32(&ast_node.node_type);
        (*info).has_children = !ast_node.children.is_empty();
        (*info).children_count = ast_node.children.len();
    }

    CResult::success()
}

/// Get child nodes
/// Get the child nodes of a specific node
///
/// # Safety
///
/// This function is unsafe because:
/// - `document` must be a valid pointer to a CPdfDocument
/// - `parent_node` must be a valid pointer to a CAstNode
/// - `children` must be a valid pointer to store an array of child node pointers
/// - `count` must be a valid pointer to store the number of children
/// - All pointers must reference valid, non-freed objects created by this library
#[no_mangle]
pub unsafe extern "C" fn pdf_ast_get_children(
    document: *const CPdfDocument,
    parent_node: *const CAstNode,
    children: *mut *mut *mut CAstNode,
    count: *mut usize,
) -> CResult {
    if document.is_null() || parent_node.is_null() || children.is_null() || count.is_null() {
        return CResult::error(CErrorCode::NullPointer, "Null pointer provided");
    }

    let doc = unsafe { &*((*document).0) };
    let parent = unsafe { &*((*parent_node).0) };

    let child_nodes: Vec<*mut CAstNode> = parent
        .children
        .iter()
        .filter_map(|&child_id| doc.ast.get_node(child_id))
        .map(|child_node| {
            let cloned = Box::new(child_node.clone());
            let c_node = Box::new(CAstNode(Box::into_raw(cloned)));
            Box::into_raw(c_node)
        })
        .collect();

    if !child_nodes.is_empty() {
        let children_array = child_nodes.into_boxed_slice();
        unsafe {
            *count = children_array.len();
            *children = children_array.as_ptr() as *mut *mut CAstNode;
            std::mem::forget(children_array);
        }
    } else {
        unsafe {
            *count = 0;
            *children = ptr::null_mut();
        }
    }

    CResult::success()
}

/// Convert node type to u32 for C API
fn node_type_to_u32(node_type: &NodeType) -> u32 {
    match node_type {
        NodeType::Root => 0,
        NodeType::Catalog => 1,
        NodeType::Pages => 2,
        NodeType::Page => 3,
        NodeType::Resource => 4,
        NodeType::Font => 5,
        NodeType::Image => 6,
        NodeType::ContentStream => 7,
        NodeType::Annotation => 8,
        NodeType::Action => 9,
        NodeType::Metadata => 10,
        NodeType::EmbeddedFile => 11,
        NodeType::Signature => 12,
        NodeType::Unknown => 13,
        NodeType::Stream => 14,
        NodeType::XObject => 15,
        NodeType::JavaScriptAction => 16,
        NodeType::URIAction => 17,
        NodeType::LaunchAction => 18,
        _ => 999, // Other types
    }
}

/// Serialize document to JSON
/// Convert the document AST to JSON format
///
/// # Safety
///
/// This function is unsafe because:
/// - `document` must be a valid pointer to a CPdfDocument
/// - `json_str` must be a valid pointer to store the resulting JSON string
/// - The document must have been created by this library and not freed
/// - The caller is responsible for freeing the returned JSON string using pdf_ast_free_string
#[no_mangle]
pub unsafe extern "C" fn pdf_ast_to_json(
    document: *const CPdfDocument,
    json_str: *mut *mut c_char,
) -> CResult {
    if document.is_null() || json_str.is_null() {
        return CResult::error(CErrorCode::NullPointer, "Null pointer provided");
    }

    let doc = unsafe { &*((*document).0) };

    match crate::serialization::to_json(doc) {
        Ok(json) => match CString::new(json) {
            Ok(c_string) => {
                unsafe {
                    *json_str = c_string.into_raw();
                }
                CResult::success()
            }
            Err(_) => CResult::error(CErrorCode::MemoryError, "Failed to create C string"),
        },
        Err(e) => CResult::error(
            CErrorCode::ParseError,
            &format!("Serialization error: {}", e),
        ),
    }
}

/// Free document handle
/// Free a document and all associated resources
///
/// # Safety
///
/// This function is unsafe because:
/// - `document` must be a valid pointer to a CPdfDocument created by this library
/// - The document must not be accessed after calling this function
/// - Double-free will result in undefined behavior
#[no_mangle]
pub unsafe extern "C" fn pdf_ast_free_document(document: *mut CPdfDocument) {
    if !document.is_null() {
        unsafe {
            let c_doc = Box::from_raw(document);
            let _ = Box::from_raw(c_doc.0);
        }
    }
}

/// Free node handle
/// Free a node structure
///
/// # Safety
///
/// This function is unsafe because:
/// - `node` must be a valid pointer to a CAstNode created by this library
/// - The node must not be accessed after calling this function
/// - Double-free will result in undefined behavior
#[no_mangle]
pub unsafe extern "C" fn pdf_ast_free_node(node: *mut CAstNode) {
    if !node.is_null() {
        unsafe {
            let c_node = Box::from_raw(node);
            let _ = Box::from_raw(c_node.0);
        }
    }
}

/// Free C string
/// Free a string allocated by this library
///
/// # Safety
///
/// This function is unsafe because:
/// - `string` must be a valid pointer to a C string allocated by this library
/// - The string must not be accessed after calling this function
/// - Double-free will result in undefined behavior
#[no_mangle]
pub unsafe extern "C" fn pdf_ast_free_string(string: *mut c_char) {
    if !string.is_null() {
        unsafe {
            let _ = CString::from_raw(string);
        }
    }
}

/// Free result message
/// Free a result structure
///
/// # Safety
///
/// This function is unsafe because:
/// - `result` must be a valid pointer to a CResult created by this library
/// - The result must not be accessed after calling this function
/// - Double-free will result in undefined behavior
#[no_mangle]
pub unsafe extern "C" fn pdf_ast_free_result(result: *mut CResult) {
    if !result.is_null() {
        unsafe {
            let res = &mut *result;
            if !res.message.is_null() {
                let _ = CString::from_raw(res.message);
                res.message = ptr::null_mut();
            }
        }
    }
}

/// Get library version
#[no_mangle]
pub extern "C" fn pdf_ast_version() -> *const c_char {
    static VERSION: &[u8] = concat!(env!("CARGO_PKG_VERSION"), "\0").as_bytes();
    VERSION.as_ptr() as *const c_char
}

/// Get the version of the C ABI contract, encoded as `(major << 16) | minor`.
#[no_mangle]
pub extern "C" fn pdf_ast_abi_version() -> u32 {
    1 << 16
}

#[cfg(test)]
mod tests {
    use super::{
        pdf_ast_abi_version, pdf_ast_free_children, pdf_ast_init, pdf_ast_version, CErrorCode,
    };
    use std::ffi::CStr;

    #[test]
    fn c_api_smoke_contract() {
        assert_eq!(pdf_ast_init().error_code, CErrorCode::Success);
        let version = unsafe { CStr::from_ptr(pdf_ast_version()) };
        assert_eq!(version.to_str().unwrap(), env!("CARGO_PKG_VERSION"));
        assert_eq!(pdf_ast_abi_version(), 0x0001_0000);
        unsafe { pdf_ast_free_children(std::ptr::null_mut(), 0) };
    }
}
