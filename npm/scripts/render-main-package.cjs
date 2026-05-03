#!/usr/bin/env node

const fs = require("fs");
const path = require("path");

const [outputPath, version] = process.argv.slice(2);

if (!outputPath || !version) {
  throw new Error("Usage: render-main-package.cjs <output-path> <version>");
}

const npmRoot = path.resolve(__dirname, "..");

const vars = {
  release_version: version,
};

/**
 * Replace `${key}` placeholders in the template with values from vars.
 */
function interpolate(template, variables) {
  return template.replace(/\$\{(\w+)\}/g, (match, key) => {
    if (key in variables) return variables[key];
    return match;
  });
}

// Read package-main.json.tmpl and fill in the release version.
const template = fs.readFileSync(
  path.join(npmRoot, "package-main.json.tmpl"),
  "utf-8",
);

fs.writeFileSync(path.resolve(outputPath), interpolate(template, vars));
