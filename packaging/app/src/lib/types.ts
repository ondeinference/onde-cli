/** Mirrors `desktop::auth::Profile` in src-tauri. */
export interface Profile {
  email: string;
}

/** Mirrors `Provider` in src-tauri/src/desktop/oauth.rs. */
export type OAuthProvider = "google" | "github" | "gitlab" | "apple";

/** Mirrors `CommandError` in src-tauri/src/error.rs. */
export interface CommandError {
  kind:
    | "email_not_confirmed"
    | "unknown_email"
    | "auth"
    | "cancelled"
    | "timed_out"
    | "internal";
  message: string;
}

export function errorMessage(e: unknown): string {
  if (e && typeof e === "object" && "message" in e) {
    return String((e as { message: unknown }).message);
  }
  return String(e);
}

/** A cancelled or timed-out sign-in is not a failure worth showing. */
export function isQuietError(e: unknown): boolean {
  const kind = e && typeof e === "object" && "kind" in e ? (e as { kind: unknown }).kind : null;
  return kind === "cancelled" || kind === "timed_out";
}
