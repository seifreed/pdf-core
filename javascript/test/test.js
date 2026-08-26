const assert = require("node:assert/strict");
const pdfAst = require("../index.node");

const minimalPdf = Buffer.from(`%PDF-1.4
1 0 obj
<< /Type /Catalog >>
endobj
xref
0 2
0000000000 65535 f 
0000000009 00000 n 
trailer
<< /Size 2 /Root 1 0 R >>
startxref
58
%%EOF
`);

const document = pdfAst.parseDocument(minimalPdf);
assert.equal(typeof document.getStatistics, "function");
assert.deepEqual(document.getVersion(), { major: 1, minor: 4 });
assert.equal(document.getStatistics().version, "1.4");
assert.equal(document.getRoot().getType(), "Catalog");
assert.equal(typeof new pdfAst.PluginManager().listPlugins, "function");
assert.equal(typeof pdfAst.getAvailableSchemas, "function");

console.log("JavaScript binding smoke test passed");
