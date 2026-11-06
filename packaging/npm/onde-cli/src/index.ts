#!/usr/bin/env node

import { spawnSync } from "child_process";

/**
 * Return the bundled executable path from node_modules.
 * Package names follow the cli-OS-ARCH pattern.
 * Windows builds use the .exe suffix.
 * @see https://nodejs.org/api/os.html#osarch
 * @see https://nodejs.org/api/os.html#osplatform
 * @example "x/xx/node_modules/cli-darwin-arm64"
 */
function getExePath() {
    const arch = process.arch;
    let os = process.platform as string;
    let extension = "";
    if (["win32", "cygwin"].includes(process.platform)) {
        os = "windows";
        extension = ".exe";
    }

    try {
        // Since the binary will be located inside node_modules, we can simply call require.resolve
        return require.resolve(`@ondeinference/cli-${os}-${arch}/bin/onde${extension}`);
    } catch (e) {
        throw new Error(
            `Couldn't find application binary inside node_modules for ${os}-${arch}`
        );
    }
}

/**
 * Run the bundled executable with the current CLI args.
 */
function run() {
    const args = process.argv.slice(2);
    const processResult = spawnSync(getExePath(), args, { stdio: "inherit" });
    process.exit(processResult.status ?? 0);
}

run();
