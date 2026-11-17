// Validates the Tauri configs without a build: each prod overlay is
// merge-patched onto the base (RFC 7386, as Tauri does), then checked for the
// version, identifiers, referenced files and the window-chrome keys.
import { readFileSync, existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const tauri = join(root, "src-tauri");
const read = (p) => JSON.parse(readFileSync(p, "utf8"));

function mergePatch(target, patch) {
  if (patch === null || typeof patch !== "object" || Array.isArray(patch)) return patch;
  const out = target && typeof target === "object" && !Array.isArray(target) ? { ...target } : {};
  for (const [k, v] of Object.entries(patch)) {
    if (v === null) delete out[k];
    else out[k] = mergePatch(out[k], v);
  }
  return out;
}

// PNG color type 6 is RGBA, and 4 is gray with alpha. The type sits at byte 25.
const hasAlpha = (file) => [4, 6].includes(readFileSync(file)[25]);

const errors = [];
const fail = (m) => errors.push(m);

const cargoVersion = readFileSync(join(root, "..", "..", "Cargo.toml"), "utf8").match(/^version\s*=\s*"([^"]+)"/m)?.[1];
const pkgVersion = read(join(root, "package.json")).version;
const appCargo = readFileSync(join(tauri, "Cargo.toml"), "utf8").match(/^version\s*=\s*"([^"]+)"/m)?.[1];

const base = read(join(tauri, "tauri.conf.json"));
const overlays = {
  "tauri.prod-macos.conf.json": "com.ondeinference.desktop",
  "tauri.prod-macos-appstore.conf.json": "com.ondeinference.desktop",
};

if (base.identifier !== "com.ondeinference.desktop-Debug") fail(`base identifier is ${base.identifier}`);
for (const [what, v] of [["package.json", pkgVersion], ["src-tauri/Cargo.toml", appCargo], ["tauri.conf.json", base.version]]) {
  if (v !== cargoVersion) fail(`${what} version ${v} != onde-cli Cargo.toml ${cargoVersion}`);
}

const chrome = ["titleBarStyle", "hiddenTitle", "trafficLightPosition"];
const check = (name, cfg, id) => {
  if (id && cfg.identifier !== id) fail(`${name}: identifier ${cfg.identifier}, expected ${id}`);
  if (cfg.version !== cargoVersion) fail(`${name}: version ${cfg.version} != ${cargoVersion}`);
  const win = cfg.app?.windows?.[0] ?? {};
  for (const k of chrome) if (!(k in win)) fail(`${name}: window is missing ${k}`);
  const ent = cfg.bundle?.macOS?.entitlements;
  if (ent && !existsSync(join(tauri, ent))) fail(`${name}: entitlements ${ent} not found`);
  for (const icon of cfg.bundle?.icon ?? []) {
    const file = join(tauri, icon);
    if (!existsSync(file)) {
      fail(`${name}: icon ${icon} not found`);
    } else if (icon.endsWith(".png") && !hasAlpha(file)) {
      fail(`${name}: icon ${icon} has no alpha channel; tauri::generate_context! rejects it`);
    }
  }
};

check("tauri.conf.json", base, null);
for (const [file, id] of Object.entries(overlays)) {
  const merged = mergePatch(base, read(join(tauri, file)));
  check(file, merged, id);
  const mac = merged.bundle?.macOS ?? {};
  // Tauri exports this as MACOSX_DEPLOYMENT_TARGET (default 10.13), and the
  // mlx-sys build refuses anything below 14.
  if (parseFloat(mac.minimumSystemVersion ?? "0") < 14) fail(`${file}: minimumSystemVersion must be 14.0 or newer for MLX`);
  if (!mac.signingIdentity) fail(`${file}: no signingIdentity`);
  // The provisioning profile is injected by the build scripts. A committed
  // path would be one person's disk and would leak where profiles are kept.
  for (const [dest, src] of Object.entries(mac.files ?? {})) {
    if (String(src).startsWith("/") || String(src).startsWith("~")) {
      fail(`${file}: files["${dest}"] is an absolute path`);
    }
  }
  if (!mac.entitlements) {
    fail(`${file}: no entitlements`);
    continue;
  }
  const entPath = join(tauri, mac.entitlements);
  // check() already reported a missing file, so there is nothing to read.
  if (!existsSync(entPath)) continue;
  const ent = readFileSync(entPath, "utf8");
  const wants = ["com.apple.security.network.server"];
  if (file.includes("appstore")) wants.push("com.apple.security.app-sandbox");
  for (const key of wants) {
    if (!new RegExp(`<key>${key}</key>\\s*<true/>`).test(ent)) fail(`${file}: ${mac.entitlements} does not enable ${key}`);
  }
}

if (errors.length) {
  console.error(errors.map((e) => `  - ${e}`).join("\n"));
  process.exit(1);
}
console.log(`tauri configs ok (version ${cargoVersion})`);
