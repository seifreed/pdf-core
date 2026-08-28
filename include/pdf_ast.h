#ifndef PDF_AST_H
#define PDF_AST_H

#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

// Forward declarations
typedef struct CPdfDocument CPdfDocument;
typedef struct CAstNode CAstNode;

// C ABI contract version. Numeric values are part of the ABI and only change
// when the ABI contract changes.
#define PDF_AST_ABI_VERSION_MAJOR 2u
#define PDF_AST_ABI_VERSION_MINOR 0u

// Error codes use a fixed-width type so the result ABI is compiler-independent.
typedef int32_t pdf_ast_error_t;
enum {
    PDF_AST_SUCCESS = 0,
    PDF_AST_INVALID_INPUT = 1,
    PDF_AST_PARSE_ERROR = 2,
    PDF_AST_MEMORY_ERROR = 3,
    PDF_AST_NULL_POINTER = 4,
    PDF_AST_INVALID_HANDLE = 5
};

// Result structure
typedef struct {
    pdf_ast_error_t error_code;
    char* message; /* caller frees with pdf_ast_free_result */
} pdf_ast_result_t;

// Node information
typedef struct {
    uint64_t id;
    uint32_t node_type;
    uint8_t has_children;
    uint64_t children_count;
} pdf_ast_node_info_t;

// Node types
typedef enum {
    PDF_AST_NODE_ROOT = 0,
    PDF_AST_NODE_CATALOG = 1,
    PDF_AST_NODE_PAGES = 2,
    PDF_AST_NODE_PAGE = 3,
    PDF_AST_NODE_RESOURCE = 4,
    PDF_AST_NODE_FONT = 5,
    PDF_AST_NODE_IMAGE = 6,
    PDF_AST_NODE_CONTENT_STREAM = 7,
    PDF_AST_NODE_ANNOTATION = 8,
    PDF_AST_NODE_ACTION = 9,
    PDF_AST_NODE_METADATA = 10,
    PDF_AST_NODE_EMBEDDED_FILE = 11,
    PDF_AST_NODE_SIGNATURE = 12,
    PDF_AST_NODE_UNKNOWN = 13,
    PDF_AST_NODE_STREAM = 14,
    PDF_AST_NODE_XOBJECT = 15,
    PDF_AST_NODE_JAVASCRIPT_ACTION = 16,
    PDF_AST_NODE_URI_ACTION = 17,
    PDF_AST_NODE_LAUNCH_ACTION = 18
} pdf_ast_node_type_t;

// Library functions

/**
 * Initialize the PDF-AST library
 * @return Result indicating success or failure
 */
pdf_ast_result_t pdf_ast_init(void);

/**
 * Parse PDF from byte buffer
 * @param data Pointer to PDF data
 * @param len Length of data in bytes
 * @param document Output parameter for document handle
 * @return Result indicating success or failure
 */
pdf_ast_result_t pdf_ast_parse(const uint8_t* data, uint64_t len, CPdfDocument** document);

/**
 * Parse PDF from file path
 * @param path Path to PDF file
 * @param document Output parameter for document handle
 * @return Result indicating success or failure
 */
pdf_ast_result_t pdf_ast_parse_file(const char* path, CPdfDocument** document);

/**
 * Get number of nodes in document
 * @param document Document handle, borrowed for the duration of the call
 * @return Number of nodes
 */
uint64_t pdf_ast_get_node_count(const CPdfDocument* document);

/**
 * Get number of edges in document
 * @param document Document handle
 * @return Number of edges
 */
uint64_t pdf_ast_get_edge_count(const CPdfDocument* document);

/**
 * Get root node of document
 * @param document Document handle
 * @param node Output parameter for an owned node handle
 * @return Result indicating success or failure
 */
pdf_ast_result_t pdf_ast_get_root_node(const CPdfDocument* document, CAstNode** node);

/**
 * Get node information
 * @param node Owned node handle
 * @param info Output parameter for node information
 * @return Result indicating success or failure
 */
pdf_ast_result_t pdf_ast_get_node_info(const CAstNode* node, pdf_ast_node_info_t* info);

/**
 * Get child nodes
 * @param document Document handle
 * @param parent_node Parent node handle
 * @param children Output parameter for an owned array of owned child handles
 * @param count Output parameter for number of children
 * @return Result indicating success or failure
 */
pdf_ast_result_t pdf_ast_get_children(
    const CPdfDocument* document,
    const CAstNode* parent_node,
    CAstNode*** children,
    uint64_t* count
);

/**
 * Free the array returned by pdf_ast_get_children.
 * The individual child nodes must be freed with pdf_ast_free_node.
 */
void pdf_ast_free_children(CAstNode** children, uint64_t count);

/**
 * Serialize document to JSON
 * @param document Document handle
 * @param json_str Output parameter for an owned JSON string
 * @return Result indicating success or failure
 */
pdf_ast_result_t pdf_ast_to_json(const CPdfDocument* document, char** json_str);

/**
 * Free document handle
 * @param document Document handle to free
 */
void pdf_ast_free_document(CPdfDocument* document);

/**
 * Free node handle
 * @param node Node handle to free
 */
void pdf_ast_free_node(CAstNode* node);

/**
 * Free C string allocated by library
 * @param string String to free
 */
void pdf_ast_free_string(char* string);

/**
 * Free result message
 * @param result Result structure with message to free
 */
void pdf_ast_free_result(pdf_ast_result_t* result);

/**
 * Get library version
 * @return Version string (do not free)
 */
const char* pdf_ast_version(void);

/**
 * Get the C ABI version as (major << 16) | minor.
 * @return ABI version encoded as a 32-bit integer (1.0 is 0x00010000)
 */
uint32_t pdf_ast_abi_version(void);

#ifdef __cplusplus
}
#endif

#endif // PDF_AST_H
