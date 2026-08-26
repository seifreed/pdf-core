#!/usr/bin/env python3
"""Test script for PDF-AST Python bindings"""

import pdf_ast

def test_basic_functionality():
    """Test basic functionality of PDF-AST Python bindings"""
    assert pdf_ast.__version__ == "0.2.0-alpha.1"
    # Test with a minimal PDF
    minimal_pdf = b"""%PDF-1.4
1 0 obj
<<
/Type /Catalog
/Pages 2 0 R
>>
endobj
2 0 obj
<<
/Type /Pages
/Kids [3 0 R]
/Count 1
>>
endobj
3 0 obj
<<
/Type /Page
/Parent 2 0 R
/MediaBox [0 0 612 792]
>>
endobj
xref
0 4
0000000000 65535 f 
0000000010 00000 n 
0000000079 00000 n 
0000000136 00000 n 
trailer
<<
/Size 4
/Root 1 0 R
>>
startxref
200
%%EOF"""
    
    doc = pdf_ast.parse_pdf(minimal_pdf)
    assert doc.get_version() == (1, 4)
    assert doc.get_root() is not None
    stats = dict(doc.get_statistics())
    assert stats["version"] == "1.4"
    assert pdf_ast.get_available_schemas()

    try:
        pdf_ast.parse_pdf(b"This is not a PDF file")
    except ValueError:
        pass
    else:
        raise AssertionError("invalid PDF input was accepted")

if __name__ == "__main__":
    test_basic_functionality()
    print("\n✅ All tests completed!")
