import { Channel, invoke } from "@tauri-apps/api/core";
import type {
  BaseModelView,
  DatasetInfo,
  DownloadMsg,
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
  baseModels: () => invoke<BaseModelView[]>("list_base_models"),
  createProject: (repoId: string, baseModelId: string) =>
    invoke<ProjectSummary>("create_project", { repoId, baseModelId }),
  /** Resolves when the download finishes, fails or is cancelled; progress
   *  arrives on `onEvent` meanwhile. */
  downloadBaseModel: (repoId: string, onEvent: (msg: DownloadMsg) => void) => {
    const channel = new Channel<DownloadMsg>();
    channel.onmessage = onEvent;
    return invoke<void>("download_base_model", { repoId, onEvent: channel });
  },
  cancelBaseDownload: (repoId: string) => invoke<boolean>("cancel_base_download", { repoId }),
};
