import { platform } from "@tauri-apps/plugin-os";

// platform() throws outside Tauri (plain `pnpm dev` in a browser), so a
// module-level call has to be guarded or the whole page blanks at import.
const currentPlatform = (() => {
  try {
    return platform();
  } catch {
    return "";
  }
})();

export const isMac = currentPlatform === "macos";
export const isDesktop = currentPlatform !== "ios" && currentPlatform !== "android";
