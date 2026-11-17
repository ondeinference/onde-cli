import { invoke } from "@tauri-apps/api/core";
import type {
  DatasetInfo,
  OAuthProvider,
  Profile,
  ProjectDetail,
  ProjectSummary,
  ProjectsLocation,
  StepRecord,
} from "@/lib/types";

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
  projectsLocation: () => invoke<ProjectsLocation>("projects_location"),
  linkProjectsFolder: () => invoke<ProjectsLocation>("link_projects_folder"),
  unlinkProjectsFolder: () => invoke<ProjectsLocation>("unlink_projects_folder"),
  listProjects: () => invoke<ProjectSummary[]>("list_projects"),
  projectDetail: (repoId: string) => invoke<ProjectDetail>("project_detail", { repoId }),
  runSteps: (repoId: string, run: string) => invoke<StepRecord[]>("run_steps", { repoId, run }),
  importDataset: (repoId: string) => invoke<DatasetInfo>("import_dataset", { repoId }),
};
