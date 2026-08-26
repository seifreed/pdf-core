const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { spawnSync } = require("node:child_process");

const root = path.resolve(__dirname, "..");
const temp = fs.mkdtempSync(path.join(os.tmpdir(), "pdf-ast-package-smoke-"));
const npm = process.platform === "win32" ? "npm.cmd" : "npm";

function run(command, args, cwd) {
  const result = spawnSync(command, args, { cwd, encoding: "utf8" });
  if (result.status !== 0) {
    throw new Error(`${command} ${args.join(" ")} failed:\n${result.stdout}\n${result.stderr}`);
  }
  return result.stdout.trim();
}

try {
  const packageNames = {
    "darwin-arm64": "pdf-ast-darwin-arm64",
    "linux-x64": "pdf-ast-linux-x64",
    "win32-x64": "pdf-ast-win32-x64",
  };
  const packageName = packageNames[`${process.platform}-${process.arch}`];
  assert(packageName, `unsupported smoke platform: ${process.platform}-${process.arch}`);

  const nativeDir = path.join(temp, "native");
  fs.mkdirSync(nativeDir);
  fs.copyFileSync(path.join(root, "index.node"), path.join(nativeDir, "index.node"));
  fs.writeFileSync(
    path.join(nativeDir, "package.json"),
    `${JSON.stringify({ name: packageName, version: "0.2.0-alpha.1", main: "index.node" })}\n`,
  );
  const nativeTarball = path.join(temp, run(npm, ["pack", nativeDir, "--pack-destination", temp], root));

  const mainDir = path.join(temp, "main");
  fs.mkdirSync(mainDir);
  for (const file of ["index.js", "index.d.ts", "README.md"]) {
    fs.copyFileSync(path.join(root, file), path.join(mainDir, file));
  }
  const packageJson = JSON.parse(fs.readFileSync(path.join(root, "package.json"), "utf8"));
  packageJson.optionalDependencies = {};
  packageJson.files = ["index.js", "index.d.ts", "README.md"];
  fs.writeFileSync(path.join(mainDir, "package.json"), `${JSON.stringify(packageJson)}\n`);
  const mainTarball = path.join(temp, run(npm, ["pack", mainDir, "--pack-destination", temp], root));

  const appDir = path.join(temp, "app");
  fs.mkdirSync(appDir);
  run(npm, ["install", "--ignore-scripts", mainTarball, nativeTarball], appDir);
  const installed = require(path.join(appDir, "node_modules", "pdf-ast"));
  const document = installed.parseDocument(Buffer.from(`%PDF-1.4
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
`));
  assert.equal(typeof document.getStatistics, "function");
  console.log("JavaScript packaged install smoke test passed");
} finally {
  fs.rmSync(temp, { recursive: true, force: true });
}
