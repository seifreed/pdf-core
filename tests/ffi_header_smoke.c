#include "pdf_ast.h"

#include <stddef.h>
#include <stdio.h>

_Static_assert(offsetof(pdf_ast_result_t, error_code) == 0,
               "pdf_ast_result_t error_code layout changed");
_Static_assert(sizeof(pdf_ast_error_t) == sizeof(int32_t),
               "pdf_ast_error_t width changed");
_Static_assert(offsetof(pdf_ast_result_t, message) >= sizeof(pdf_ast_error_t) &&
                   offsetof(pdf_ast_result_t, message) % _Alignof(char *) == 0,
               "pdf_ast_result_t message layout changed");
_Static_assert(offsetof(pdf_ast_node_info_t, id) == 0,
               "pdf_ast_node_info_t id layout changed");
_Static_assert(offsetof(pdf_ast_node_info_t, node_type) == sizeof(uint64_t),
               "pdf_ast_node_info_t node_type layout changed");
_Static_assert(offsetof(pdf_ast_node_info_t, has_children) ==
                   sizeof(uint64_t) + sizeof(uint32_t),
               "pdf_ast_node_info_t has_children layout changed");
_Static_assert(sizeof(((pdf_ast_node_info_t *)0)->has_children) == sizeof(uint8_t),
               "pdf_ast_node_info_t has_children width changed");
_Static_assert(sizeof(((pdf_ast_node_info_t *)0)->children_count) == sizeof(uint64_t),
               "pdf_ast_node_info_t children_count width changed");

int main(void) {
    static const char *path =
        "pdfs/11351007ef426374078a18ac99e5822ef97028785b2068389755d266b97e2ba0.pdf";
    CPdfDocument *document = NULL;
    CAstNode *root = NULL;
    pdf_ast_node_info_t info = {0};
    pdf_ast_result_t result = pdf_ast_init();

    if (result.error_code != PDF_AST_SUCCESS) {
        pdf_ast_free_result(&result);
        return 1;
    }
    if (pdf_ast_abi_version() !=
        ((PDF_AST_ABI_VERSION_MAJOR << 16) | PDF_AST_ABI_VERSION_MINOR)) {
        return 5;
    }
    result = pdf_ast_parse_file(path, &document);
    if (result.error_code != PDF_AST_SUCCESS || document == NULL) {
        pdf_ast_free_result(&result);
        return 2;
    }
    result = pdf_ast_get_root_node(document, &root);
    if (result.error_code != PDF_AST_SUCCESS || root == NULL) {
        pdf_ast_free_result(&result);
        pdf_ast_free_document(document);
        return 3;
    }
    result = pdf_ast_get_node_info(root, &info);
    if (result.error_code != PDF_AST_SUCCESS) {
        pdf_ast_free_result(&result);
        pdf_ast_free_node(root);
        pdf_ast_free_document(document);
        return 4;
    }

    char *json = NULL;
    result = pdf_ast_to_json(document, &json);
    if (result.error_code != PDF_AST_SUCCESS || json == NULL) {
        pdf_ast_free_result(&result);
        pdf_ast_free_node(root);
        pdf_ast_free_document(document);
        return 8;
    }
    pdf_ast_free_string(json);

    CPdfDocument *buffer_document = NULL;
    result = pdf_ast_parse(NULL, 0, &buffer_document);
    if (result.error_code != PDF_AST_NULL_POINTER || buffer_document != NULL) {
        pdf_ast_free_result(&result);
        pdf_ast_free_node(root);
        pdf_ast_free_document(document);
        return 9;
    }
    pdf_ast_free_result(&result);

    static const uint8_t invalid_pdf[] = "not a PDF";
    result = pdf_ast_parse(invalid_pdf, sizeof(invalid_pdf) - 1, &buffer_document);
    if (result.error_code != PDF_AST_PARSE_ERROR || buffer_document != NULL) {
        pdf_ast_free_result(&result);
        pdf_ast_free_node(root);
        pdf_ast_free_document(document);
        return 11;
    }
    pdf_ast_free_result(&result);

    result = pdf_ast_parse_file("missing-pdf-ast-fixture.pdf", &buffer_document);
    if (result.error_code != PDF_AST_INVALID_INPUT || buffer_document != NULL) {
        pdf_ast_free_result(&result);
        pdf_ast_free_node(root);
        pdf_ast_free_document(document);
        return 10;
    }
    pdf_ast_free_result(&result);

    CAstNode **children = NULL;
    uint64_t child_count = 0;
    result = pdf_ast_get_children(document, root, NULL, &child_count);
    if (result.error_code != PDF_AST_NULL_POINTER) {
        pdf_ast_free_node(root);
        pdf_ast_free_document(document);
        return 6;
    }
    result = pdf_ast_get_children(document, root, &children, &child_count);
    if (result.error_code != PDF_AST_SUCCESS) {
        pdf_ast_free_node(root);
        pdf_ast_free_document(document);
        return 7;
    }
    pdf_ast_free_children(children, child_count);

    printf("pdf-ast %s: nodes=%llu root=%llu\n", pdf_ast_version(),
           (unsigned long long)pdf_ast_get_node_count(document),
           (unsigned long long)info.id);
    pdf_ast_free_node(root);
    pdf_ast_free_document(document);
    return 0;
}
