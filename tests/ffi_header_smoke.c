#include "pdf_ast.h"

#include <stdio.h>

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

    result = pdf_ast_parse_file("missing-pdf-ast-fixture.pdf", &buffer_document);
    if (result.error_code != PDF_AST_INVALID_INPUT || buffer_document != NULL) {
        pdf_ast_free_result(&result);
        pdf_ast_free_node(root);
        pdf_ast_free_document(document);
        return 10;
    }
    pdf_ast_free_result(&result);

    CAstNode **children = NULL;
    size_t child_count = 0;
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

    printf("pdf-ast %s: nodes=%zu root=%llu\n", pdf_ast_version(),
           pdf_ast_get_node_count(document), (unsigned long long)info.id);
    pdf_ast_free_node(root);
    pdf_ast_free_document(document);
    return 0;
}
