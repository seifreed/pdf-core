const platformPackages = {
  "darwin-arm64": "pdf-ast-darwin-arm64",
  "linux-x64": "pdf-ast-linux-x64",
  "win32-x64": "pdf-ast-win32-x64",
};

const packageName = platformPackages[`${process.platform}-${process.arch}`];

try {
  module.exports = require(packageName);
} catch (platformError) {
  try {
    module.exports = require("./index.node");
  } catch {
    if (!packageName) {
      const error = new Error(
        `pdf-ast has no native package for ${process.platform}-${process.arch}`,
      );
      error.cause = platformError;
      throw error;
    }
    throw platformError;
  }
}
