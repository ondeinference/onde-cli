#!/usr/bin/env node

const fs = require("fs");
const path = require("path");

const [packageName, version, operatingSystem, architecture] =
  process.argv.slice(2);

if (!packageName || !version || !operatingSystem || !architecture) {
  throw new Error(
    "Usage: render-platform-package.cjs <package-name> <version> <os> <arch>",
  );
}

const npmRoot = path.resolve(__dirname, "..");
const packageDirectory = path.resolve(npmRoot, packageName);

fs.mkdirSync(packageDirectory, { recursive: true });

// Values we drop into the templates.
const vars = {
  node_pkg: packageName,
  node_version: version,
  node_os: operatingSystem,
  node_arch: architecture,
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

// Read package.json.tmpl and fill it in.
const packageTemplate = fs.readFileSync(
  path.join(npmRoot, "package.json.tmpl"),
  "utf-8",
);
fs.writeFileSync(
  path.join(packageDirectory, "package.json"),
  interpolate(packageTemplate, vars),
);

// Read README.md.tmpl and fill it in.
const readmeTemplate = fs.readFileSync(
  path.join(npmRoot, "README.md.tmpl"),
  "utf-8",
);
fs.writeFileSync(
  path.join(packageDirectory, "README.md"),
  interpolate(readmeTemplate, vars),
);
