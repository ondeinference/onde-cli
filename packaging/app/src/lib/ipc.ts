import { invoke } from "@tauri-apps/api/core";
import type { OAuthProvider, Profile } from "@/lib/types";

// One typed wrapper per Rust command. Outside Tauri (plain browser) invoke
// rejects; callers treat that as "signed out".
export const ipc = {
  appBuild: () => invoke<string | null>("app_build"),
  currentUser: () => invoke<Profile | null>("current_user"),
  signIn: (email: string, password: string) =>
    invoke<Profile>("sign_in", { email, password }),
  signUp: (email: string, password: string) =>
    invoke<string>("sign_up", { email, password }),
  oauthSignIn: (provider: OAuthProvider) =>
    invoke<Profile>("oauth_sign_in", { provider }),
  oauthSignInCancel: () => invoke<void>("oauth_sign_in_cancel"),
  signOut: () => invoke<void>("sign_out"),
};
