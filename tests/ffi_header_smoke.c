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

    printf("pdf-ast %s: nodes=%zu root=%llu\n", pdf_ast_version(),
           pdf_ast_get_node_count(document), (unsigned long long)info.id);
    pdf_ast_free_node(root);
    pdf_ast_free_document(document);
    return 0;
}
