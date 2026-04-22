use {
    crate::token,
    anyhow::Result,
    crossterm::event::{Event, EventStream, KeyCode, KeyEventKind, KeyModifiers},
    futures::StreamExt,
    ratatui::DefaultTerminal,
    smbcloud_auth_sdk::{
        client_credentials::ClientCredentials, login::login_with_client,
        logout::logout_with_client, me::me_with_client, signup::signup_with_client,
    },
    smbcloud_model::{error_codes::ErrorResponse, login::AccountStatus},
    smbcloud_network::environment::Environment,
    tokio::sync::mpsc,
};

// ui.rs imports OndeApp and OndeModel from here instead of depending on the SDK directly.
pub use smbcloud_gresiq_sdk::{OndeApp, OndeModel};

// Onde is a tenant in smbCloud Auth. These identify this CLI to the backend.
// smbCloud Auth credentials — baked in at compile time from the environment.
// Set these in .env for local builds; inject as secrets in CI.
pub(crate) const ONDE_APP_ID: &str = env!("ONDE_APP_ID");
pub(crate) const ONDE_APP_SECRET: &str = env!("ONDE_APP_SECRET");

// GresIQ API credentials for the Onde tenant — distinct from Auth above.
pub(crate) const GRESIQ_API_KEY: &str = env!("GRESIQ_API_KEY");
pub(crate) const GRESIQ_API_SECRET: &str = env!("GRESIQ_API_SECRET");

fn credentials() -> ClientCredentials<'static> {
    ClientCredentials {
        app_id: ONDE_APP_ID,
        app_secret: ONDE_APP_SECRET,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Screen {
    Auth,
    Apps,
    AppDetail,
    Models,
    Downloads,
    ModelDetail,
    FineTune,
}

/// What kind of artifact was found on disk.
#[derive(Clone, PartialEq)]
pub enum ArtifactKind {
    /// A LoRA adapter (lora_adapter.safetensors).
    LoraAdapter,
    /// An exported GGUF model (.gguf file).
    Gguf,
}

/// A discovered artifact (adapter or exported model) on disk.
#[derive(Clone)]
pub struct AdapterEntry {
    /// Full path to the file.
    pub path: std::path::PathBuf,
    /// Directory name (snapshot hash or "lora-adapter").
    pub dir_name: String,
    /// File name for GGUF files, or "lora_adapter.safetensors" for adapters.
    pub file_name: String,
    /// Human-readable file size.
    pub size: String,
    /// Relative timestamp ("just now", "3h ago", etc.).
    pub modified: String,
    /// Whether this is a LoRA adapter or a GGUF export.
    pub kind: ArtifactKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Mode {
    Signup,
    Signin,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Focus {
    Email,
    Password,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FineTuneFocus {
    ModelDir,
    DataPath,
    Rank,
    Epochs,
    Lr,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StatusTone {
    Neutral,
    Success,
    Error,
}

#[derive(Debug, Clone)]
pub struct Status {
    pub tone: StatusTone,
    pub message: String,
}

impl Status {
    pub fn neutral(msg: impl Into<String>) -> Self {
        Self {
            tone: StatusTone::Neutral,
            message: msg.into(),
        }
    }
    pub fn success(msg: impl Into<String>) -> Self {
        Self {
            tone: StatusTone::Success,
            message: msg.into(),
        }
    }
    pub fn error(msg: impl Into<String>) -> Self {
        Self {
            tone: StatusTone::Error,
            message: msg.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Profile {
    pub email: String,
}

pub enum AuthEvent {
    // auth
    SignupOk(String),
    SigninOk(Profile),
    ProfileOk(Profile),
    SignedOut,
    Failed(String),
    // apps
    AppsLoaded(Vec<OndeApp>),
    AppsLoadFailed(String),
    AppCreated(OndeApp),
    AppCreateFailed(String),
    AppRenamedOk {
        app_index: usize,
        new_name: String,
    },
    AppRenameFailed(String),
    // models (remote catalog for assignment)
    ModelsLoaded(Vec<OndeModel>),
    ModelsLoadFailed(String),
    ModelAssigned {
        app_index: usize,
        model_id: String,
    },
    ModelAssignFailed(String),
    // downloads (catalog merged with local HF cache)
    DownloadsLoaded(Vec<crate::hf::MergedModel>),
    #[allow(dead_code)] // reserved for future explicit error reporting
    DownloadsLoadFailed(String),
    // HF Hub search
    HfSearchResults(Vec<crate::hf_search::HfModelInfo>),
    HfSearchFailed(String),
    // Model download
    ModelDownloadProgress(crate::hf_search::DownloadProgress),
    ModelDownloadComplete(String), // model_id
    ModelDownloadFailed(String),
    // fine-tune
    FineTuneProgress(crate::finetune::FineTuneProgress),
    // merge + GGUF export
    MergeProgress(crate::merge::MergeProgress),
    GgufProgress(crate::gguf::GgufProgress),
}

pub struct App {
    // auth
    pub mode: Mode,
    pub email: String,
    pub password: String,
    pub focus: Focus,
    pub status: Status,
    pub busy: bool,
    pub profile: Option<Profile>,
    pub should_quit: bool,
    // navigation
    pub screen: Screen,
    // apps list
    pub apps: Vec<OndeApp>,
    pub apps_cursor: usize,
    pub apps_offset: usize,
    pub apps_loaded: bool,
    // models list (remote catalog for assignment)
    pub models: Vec<OndeModel>,
    pub models_cursor: usize,
    pub models_offset: usize,
    pub models_loaded: bool,
    // inline create form
    pub creating_app: bool,
    pub new_app_name: String,
    // rename form (used on AppDetail screen)
    pub renaming_app: bool,
    pub rename_input: String,
    // which app we're picking a model for
    pub assigning_for_app_index: Option<usize>,
    // local HF cache downloads merged with remote catalog
    pub downloads: Vec<crate::hf::MergedModel>,
    pub downloads_cursor: usize,
    pub downloads_offset: usize,
    pub downloads_loaded: bool,
    // HF Hub search
    pub hf_search_active: bool,
    pub hf_search_query: String,
    pub hf_search_results: Vec<crate::hf_search::HfModelInfo>,
    pub hf_search_cursor: usize,
    pub hf_search_loading: bool,
    // Model download (runs in background, does not set app.busy)
    pub downloading: bool,
    pub download_progress: Option<crate::hf_search::DownloadProgress>,
    // fine-tune
    pub finetune_model_id: String,
    pub finetune_model_dir: String,
    pub finetune_data_path: String,
    pub finetune_rank: String,
    pub finetune_epochs: String,
    pub finetune_lr: String,
    pub finetune_focus: FineTuneFocus,
    pub finetune_running: bool,
    pub finetune_progress: Option<crate::finetune::FineTuneProgress>,
    // merge + GGUF export
    pub merge_running: bool,
    pub merge_progress: Option<crate::merge::MergeProgress>,
    pub gguf_running: bool,
    pub gguf_progress: Option<crate::gguf::GgufProgress>,
    pub merged_model_dir: Option<std::path::PathBuf>,
    // adapters discovered on the model detail screen
    pub adapter_list: Vec<AdapterEntry>,
    pub adapter_cursor: usize,
}

impl App {
    pub fn new() -> Self {
        Self {
            mode: Mode::Signup,
            email: String::new(),
            password: String::new(),
            focus: Focus::Email,
            status: Status::neutral("Type your email and password."),
            busy: false,
            profile: None,
            should_quit: false,
            screen: Screen::Auth,
            apps: Vec::new(),
            apps_cursor: 0,
            apps_offset: 0,
            apps_loaded: false,
            models: Vec::new(),
            models_cursor: 0,
            models_offset: 0,
            models_loaded: false,
            creating_app: false,
            new_app_name: String::new(),
            renaming_app: false,
            rename_input: String::new(),
            assigning_for_app_index: None,
            downloads: Vec::new(),
            downloads_cursor: 0,
            downloads_offset: 0,
            downloads_loaded: false,
            hf_search_active: false,
            hf_search_query: String::new(),
            hf_search_results: Vec::new(),
            hf_search_cursor: 0,
            hf_search_loading: false,
            downloading: false,
            download_progress: None,
            finetune_model_id: String::new(),
            finetune_model_dir: String::new(),
            finetune_data_path: "~/.onde/finetune/train.jsonl".to_string(),
            finetune_rank: "8".to_string(),
            finetune_epochs: "3".to_string(),
            finetune_lr: "0.0001".to_string(),
            finetune_focus: FineTuneFocus::ModelDir,
            finetune_running: false,
            finetune_progress: None,
            merge_running: false,
            merge_progress: None,
            gguf_running: false,
            gguf_progress: None,
            merged_model_dir: None,
            adapter_list: Vec::new(),
            adapter_cursor: 0,
        }
    }

    fn idle_status(&self) -> Status {
        match self.mode {
            Mode::Signup => Status::neutral("Type your email and password."),
            Mode::Signin => Status::neutral("Sign in to your existing account."),
        }
    }

    pub fn switch_mode(&mut self, mode: Mode) {
        if self.busy {
            return;
        }
        self.mode = mode;
        if self.profile.is_none() {
            self.status = self.idle_status();
        }
    }

    pub fn apply(&mut self, event: AuthEvent) {
        // Download, search, merge, and gguf events run in the background without the busy flag.
        match &event {
            AuthEvent::ModelDownloadProgress(_)
            | AuthEvent::ModelDownloadComplete(_)
            | AuthEvent::ModelDownloadFailed(_)
            | AuthEvent::HfSearchResults(_)
            | AuthEvent::HfSearchFailed(_)
            | AuthEvent::MergeProgress(_)
            | AuthEvent::GgufProgress(_) => {}
            _ => {
                self.busy = false;
            }
        }
        match event {
            // auth
            AuthEvent::SignupOk(message) => {
                self.password.clear();
                self.status = Status::success(message);
            }
            AuthEvent::SigninOk(profile) => {
                self.password.clear();
                self.profile = Some(profile);
                self.screen = Screen::Apps;
                self.status = Status::success("You're in.");
            }
            AuthEvent::ProfileOk(profile) => {
                self.profile = Some(profile);
                self.screen = Screen::Apps;
                self.status = Status::success("Still signed in.");
            }
            AuthEvent::SignedOut => {
                token::clear();
                self.profile = None;
                self.screen = Screen::Auth;
                self.apps.clear();
                self.apps_loaded = false;
                self.apps_cursor = 0;
                self.apps_offset = 0;
                self.models.clear();
                self.models_loaded = false;
                self.models_cursor = 0;
                self.models_offset = 0;
                self.creating_app = false;
                self.new_app_name.clear();
                self.renaming_app = false;
                self.rename_input.clear();
                self.assigning_for_app_index = None;
                self.downloads.clear();
                self.downloads_loaded = false;
                self.downloads_cursor = 0;
                self.downloads_offset = 0;
                self.status = Status::neutral("Signed out.");
            }
            AuthEvent::Failed(message) => {
                self.status = Status::error(message);
            }
            // apps
            AuthEvent::AppsLoaded(apps) => {
                self.busy = false;
                self.apps = apps;
                self.apps_loaded = true;
                self.status = Status::success("Apps loaded.");
            }
            AuthEvent::AppsLoadFailed(msg) => {
                self.busy = false;
                self.status = Status::error(msg);
            }
            AuthEvent::AppCreated(app) => {
                self.busy = false;
                self.apps.insert(0, app);
                self.creating_app = false;
                self.new_app_name.clear();
                self.status = Status::success("App created.");
            }
            AuthEvent::AppCreateFailed(msg) => {
                self.busy = false;
                self.status = Status::error(msg);
            }
            AuthEvent::AppRenamedOk {
                app_index,
                new_name,
            } => {
                self.busy = false;
                self.renaming_app = false;
                self.rename_input.clear();
                if let Some(onde_app) = self.apps.get_mut(app_index) {
                    onde_app.name = new_name;
                }
                self.status = Status::success("App renamed.");
            }
            AuthEvent::AppRenameFailed(msg) => {
                self.busy = false;
                self.status = Status::error(msg);
            }
            // models
            AuthEvent::ModelsLoaded(models) => {
                self.busy = false;
                self.models = models;
                self.models_loaded = true;
                self.status = Status::neutral("Choose a model.");
            }
            AuthEvent::ModelsLoadFailed(msg) => {
                self.busy = false;
                self.status = Status::error(msg);
            }
            AuthEvent::ModelAssigned {
                app_index,
                model_id,
            } => {
                self.busy = false;
                self.screen = Screen::AppDetail;
                self.assigning_for_app_index = None;
                if let Some(onde_app) = self.apps.get_mut(app_index) {
                    // Resolve the display name from the loaded models list so
                    // the apps list and detail view update immediately without
                    // a round-trip. Falls back to the raw ID if not found.
                    let resolved_name = self
                        .models
                        .iter()
                        .find(|m| m.id == model_id)
                        .and_then(|m| m.name.clone())
                        .unwrap_or_else(|| model_id.clone());
                    onde_app.current_model_id = Some(model_id);
                    onde_app.active_model = Some(resolved_name);
                }
                self.status = Status::success("Model assigned.");
            }
            AuthEvent::ModelAssignFailed(msg) => {
                self.busy = false;
                self.status = Status::error(msg);
            }
            // downloads
            AuthEvent::DownloadsLoaded(models) => {
                self.busy = false;
                self.downloads = models;
                self.downloads_loaded = true;
                if self.downloads.is_empty() {
                    self.status = Status::neutral("No models downloaded yet.");
                } else {
                    self.status = Status::success(format!(
                        "{} model{} found.",
                        self.downloads.len(),
                        if self.downloads.len() == 1 { "" } else { "s" }
                    ));
                }
            }
            AuthEvent::DownloadsLoadFailed(msg) => {
                self.busy = false;
                self.downloads_loaded = true;
                self.status = Status::error(msg);
            }
            AuthEvent::FineTuneProgress(progress) => {
                match &progress {
                    crate::finetune::FineTuneProgress::Done { .. }
                    | crate::finetune::FineTuneProgress::Failed(_) => {
                        self.finetune_running = false;
                    }
                    _ => {}
                }
                self.finetune_progress = Some(progress);
            }
            AuthEvent::HfSearchResults(results) => {
                self.hf_search_loading = false;
                self.hf_search_cursor = 0;
                self.hf_search_results = results;
                self.status = if self.hf_search_results.is_empty() {
                    Status::neutral("No models found.")
                } else {
                    Status::neutral(format!("{} models found.", self.hf_search_results.len()))
                };
            }
            AuthEvent::HfSearchFailed(msg) => {
                self.hf_search_loading = false;
                self.status = Status::error(msg);
            }
            AuthEvent::ModelDownloadProgress(progress) => {
                self.download_progress = Some(progress);
            }
            AuthEvent::ModelDownloadComplete(model_id) => {
                self.downloading = false;
                self.download_progress = None;
                self.hf_search_active = false;
                self.hf_search_query.clear();
                self.hf_search_results.clear();
                self.hf_search_cursor = 0;
                self.downloads_loaded = false; // triggers list reload
                self.status = Status::success(format!("{model_id} downloaded."));
            }
            AuthEvent::ModelDownloadFailed(msg) => {
                self.downloading = false;
                self.download_progress = None;
                self.status = Status::error(msg);
            }
            AuthEvent::MergeProgress(progress) => {
                match &progress {
                    crate::merge::MergeProgress::Done { output_path } => {
                        self.merge_running = false;
                        // Store the merged model directory for GGUF export
                        if let Some(parent) = output_path.parent() {
                            self.merged_model_dir = Some(parent.to_path_buf());
                        }
                    }
                    crate::merge::MergeProgress::Failed(_) => {
                        self.merge_running = false;
                    }
                    _ => {}
                }
                self.merge_progress = Some(progress);
            }
            AuthEvent::GgufProgress(progress) => {
                match &progress {
                    crate::gguf::GgufProgress::Done { .. }
                    | crate::gguf::GgufProgress::Failed(_) => {
                        self.gguf_running = false;
                    }
                    _ => {}
                }
                self.gguf_progress = Some(progress);
            }
        }
    }
}

pub async fn run(terminal: &mut DefaultTerminal) -> Result<()> {
    let mut app = App::new();
    let (tx, mut rx) = mpsc::unbounded_channel::<AuthEvent>();
    let mut events = EventStream::new();

    // pick up where we left off if there is a saved token
    if let Some(saved_token) = token::load() {
        app.busy = true;
        app.status = Status::neutral("Restoring session…");
        let tx2 = tx.clone();
        tokio::spawn(async move {
            match me_with_client(Environment::Production, credentials(), &saved_token).await {
                Ok(user) => {
                    let _ = tx2.send(AuthEvent::ProfileOk(Profile { email: user.email }));
                }
                Err(_) => {
                    token::clear();
                    let _ = tx2.send(AuthEvent::Failed("Session expired. Sign in again.".into()));
                }
            }
        });
    }

    loop {
        terminal.draw(|frame| crate::ui::render(frame, &app))?;

        tokio::select! {
            maybe = events.next() => {
                match maybe {
                    Some(Ok(Event::Key(key))) if key.kind == KeyEventKind::Press => {
                        handle_key(&mut app, key, tx.clone());
                    }
                    None => break,
                    _ => {}
                }
            }
            maybe = rx.recv() => {
                if let Some(event) = maybe {
                    app.apply(event);
                    // kick off apps loading whenever we land on the Apps screen fresh
                    if app.profile.is_some()
                        && app.screen == Screen::Apps
                        && !app.apps_loaded
                        && !app.busy
                    {
                        trigger_load_apps(&mut app, tx.clone());
                    }
                    // Reload downloads list after a successful download.
                    if app.profile.is_some()
                        && app.screen == Screen::Downloads
                        && !app.downloads_loaded
                        && !app.busy
                        && !app.downloading
                    {
                        trigger_load_downloads(&mut app, tx.clone());
                    }
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}

fn trigger_load_apps(app: &mut App, tx: mpsc::UnboundedSender<AuthEvent>) {
    if app.busy {
        return;
    }
    let token = token::load().unwrap_or_default();
    app.busy = true;
    app.status = Status::neutral("Loading apps…");
    tokio::spawn(async move {
        match crate::gresiq::load_apps(&token).await {
            Ok(apps) => {
                let _ = tx.send(AuthEvent::AppsLoaded(apps));
            }
            Err(e) => {
                let _ = tx.send(AuthEvent::AppsLoadFailed(e.to_string()));
            }
        }
    });
}

const MAX_VISIBLE: usize = 8;

fn handle_key(
    app: &mut App,
    key: crossterm::event::KeyEvent,
    tx: mpsc::UnboundedSender<AuthEvent>,
) {
    use KeyCode::*;

    // while a request is in flight only Ctrl+C is allowed
    if app.busy {
        if matches!(
            (key.code, key.modifiers),
            (Char('c'), KeyModifiers::CONTROL)
        ) {
            app.should_quit = true;
        }
        return;
    }

    match app.screen {
        Screen::Auth => handle_key_auth(app, key, tx),
        Screen::Apps => handle_key_apps(app, key, tx),
        Screen::AppDetail => handle_key_app_detail(app, key, tx),
        Screen::Models => handle_key_models(app, key, tx),
        Screen::Downloads => handle_key_downloads(app, key, tx),
        Screen::ModelDetail => handle_key_model_detail(app, key, tx),
        Screen::FineTune => handle_key_finetune(app, key, tx),
    }
}

fn handle_key_auth(
    app: &mut App,
    key: crossterm::event::KeyEvent,
    tx: mpsc::UnboundedSender<AuthEvent>,
) {
    use KeyCode::*;

    match (key.code, key.modifiers) {
        (Char('c'), KeyModifiers::CONTROL) | (Esc, _) => {
            app.should_quit = true;
        }
        (Tab, _) => {
            app.focus = match app.focus {
                Focus::Email => Focus::Password,
                Focus::Password => Focus::Email,
            };
        }
        (Char('l'), KeyModifiers::CONTROL) => {
            app.switch_mode(Mode::Signin);
        }
        (Char('n'), KeyModifiers::CONTROL) => {
            app.switch_mode(Mode::Signup);
        }
        (Enter, _) => {
            submit(app, tx);
        }
        (Backspace, _) => match app.focus {
            Focus::Email => {
                app.email.pop();
            }
            Focus::Password => {
                app.password.pop();
            }
        },
        (Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => match app.focus {
            Focus::Email => app.email.push(c),
            Focus::Password => app.password.push(c),
        },
        _ => {}
    }
}

fn handle_key_apps(
    app: &mut App,
    key: crossterm::event::KeyEvent,
    tx: mpsc::UnboundedSender<AuthEvent>,
) {
    use KeyCode::*;

    if app.creating_app {
        match (key.code, key.modifiers) {
            (Esc, _) => {
                app.creating_app = false;
                app.new_app_name.clear();
                app.status = app.idle_status();
            }
            (Enter, _) => {
                submit_create_app(app, tx);
            }
            (Backspace, _) => {
                app.new_app_name.pop();
            }
            (Char('c'), KeyModifiers::CONTROL) => {
                app.should_quit = true;
            }
            (Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                app.new_app_name.push(c);
            }
            _ => {}
        }
    } else {
        match (key.code, key.modifiers) {
            (Up, _) | (Char('k'), KeyModifiers::NONE) => {
                app.apps_cursor = app.apps_cursor.saturating_sub(1);
                clamp_apps_scroll(app, MAX_VISIBLE);
            }
            (Down, _) | (Char('j'), KeyModifiers::NONE) => {
                if app.apps_cursor + 1 < app.apps.len() {
                    app.apps_cursor += 1;
                }
                clamp_apps_scroll(app, MAX_VISIBLE);
            }
            (Enter, _) if !app.apps.is_empty() => {
                app.screen = Screen::AppDetail;
                app.renaming_app = false;
                app.rename_input.clear();
                let app_name = app
                    .apps
                    .get(app.apps_cursor)
                    .map(|a| a.name.as_str())
                    .unwrap_or("app");
                app.status =
                    Status::neutral(format!("{app_name} — m · model   r · rename   Esc · back"));
            }
            (Char('n'), KeyModifiers::NONE) => {
                app.creating_app = true;
                app.status = Status::neutral("Type a name and press Enter.");
            }
            (Tab, _) => {
                app.screen = Screen::Downloads;
                app.downloads_cursor = 0;
                app.downloads_offset = 0;
                app.downloads_loaded = false;
                app.downloads.clear();
                trigger_load_downloads(app, tx);
            }
            (Char('s'), KeyModifiers::NONE) | (Char('s'), KeyModifiers::CONTROL) => {
                sign_out(app, tx);
            }
            (Esc, _) | (Char('c'), KeyModifiers::CONTROL) => {
                app.should_quit = true;
            }
            _ => {}
        }
    }
}

fn handle_key_app_detail(
    app: &mut App,
    key: crossterm::event::KeyEvent,
    tx: mpsc::UnboundedSender<AuthEvent>,
) {
    use KeyCode::*;

    if app.renaming_app {
        match (key.code, key.modifiers) {
            (Esc, _) => {
                app.renaming_app = false;
                app.rename_input.clear();
                app.status = Status::neutral("Rename cancelled.");
            }
            (Enter, _) => {
                submit_rename_app(app, tx);
            }
            (Backspace, _) => {
                app.rename_input.pop();
            }
            (Char('c'), KeyModifiers::CONTROL) => {
                app.should_quit = true;
            }
            (Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                app.rename_input.push(c);
            }
            _ => {}
        }
    } else {
        match (key.code, key.modifiers) {
            (Esc, _) => {
                app.screen = Screen::Apps;
                app.status = Status::neutral("Back to apps.");
            }
            (Char('m'), KeyModifiers::NONE) => {
                open_model_picker(app, tx);
            }
            (Char('r'), KeyModifiers::NONE) => {
                let current_name = app
                    .apps
                    .get(app.apps_cursor)
                    .map(|a| a.name.clone())
                    .unwrap_or_default();
                app.rename_input = current_name;
                app.renaming_app = true;
                app.status = Status::neutral("Edit the name and press Enter.");
            }
            (Char('s'), KeyModifiers::NONE) => {
                sign_out(app, tx);
            }
            (Char('c'), KeyModifiers::CONTROL) => {
                app.should_quit = true;
            }
            _ => {}
        }
    }
}

fn submit_rename_app(app: &mut App, tx: mpsc::UnboundedSender<AuthEvent>) {
    let name = app.rename_input.trim().to_string();
    if name.is_empty() {
        app.status = Status::error("Name cannot be empty.");
        return;
    }
    let Some(onde_app) = app.apps.get(app.apps_cursor) else {
        return;
    };
    let token = token::load().unwrap_or_default();
    let onde_app_id = onde_app.id.clone();
    let app_index = app.apps_cursor;
    app.busy = true;
    app.status = Status::neutral(format!("Renaming to \"{name}\"\u{2026}"));
    tokio::spawn(async move {
        match crate::gresiq::rename_app(&token, &onde_app_id, &name).await {
            Ok(_) => {
                let _ = tx.send(AuthEvent::AppRenamedOk {
                    app_index,
                    new_name: name,
                });
            }
            Err(e) => {
                let _ = tx.send(AuthEvent::AppRenameFailed(e.to_string()));
            }
        }
    });
}

fn handle_key_models(
    app: &mut App,
    key: crossterm::event::KeyEvent,
    tx: mpsc::UnboundedSender<AuthEvent>,
) {
    use KeyCode::*;

    match (key.code, key.modifiers) {
        (Up, _) | (Char('k'), KeyModifiers::NONE) => {
            app.models_cursor = app.models_cursor.saturating_sub(1);
            clamp_models_scroll(app, MAX_VISIBLE);
        }
        (Down, _) | (Char('j'), KeyModifiers::NONE) => {
            if app.models_cursor + 1 < app.models.len() {
                app.models_cursor += 1;
            }
            clamp_models_scroll(app, MAX_VISIBLE);
        }
        (Enter, _) => {
            submit_assign_model(app, tx);
        }
        (Esc, _) => {
            app.screen = Screen::AppDetail;
            app.assigning_for_app_index = None;
            app.status = Status::neutral("Back to app.");
        }
        (Char('c'), KeyModifiers::CONTROL) => {
            app.should_quit = true;
        }
        _ => {}
    }
}

fn clamp_apps_scroll(app: &mut App, max_visible: usize) {
    if app.apps_cursor < app.apps_offset {
        app.apps_offset = app.apps_cursor;
    } else if app.apps_cursor >= app.apps_offset + max_visible {
        app.apps_offset = app.apps_cursor + 1 - max_visible;
    }
}

fn handle_key_downloads(
    app: &mut App,
    key: crossterm::event::KeyEvent,
    tx: mpsc::UnboundedSender<AuthEvent>,
) {
    use KeyCode::*;

    // While a download is running, only allow quit.
    if app.downloading {
        if matches!(
            (key.code, key.modifiers),
            (Char('c'), KeyModifiers::CONTROL)
        ) {
            app.should_quit = true;
        }
        return;
    }

    if app.hf_search_active {
        match (key.code, key.modifiers) {
            (Esc, _) => {
                app.hf_search_active = false;
                app.hf_search_query.clear();
                app.hf_search_results.clear();
                app.hf_search_cursor = 0;
                app.hf_search_loading = false;
                app.status = Status::neutral("Search cancelled.");
            }
            (Enter, _) => {
                if !app.hf_search_results.is_empty() {
                    trigger_download(app, tx);
                } else if !app.hf_search_query.trim().is_empty() && !app.hf_search_loading {
                    trigger_hf_search(app, tx);
                }
            }
            (Up, _) | (Char('k'), KeyModifiers::NONE) => {
                app.hf_search_cursor = app.hf_search_cursor.saturating_sub(1);
            }
            (Down, _) | (Char('j'), KeyModifiers::NONE)
                if app.hf_search_cursor + 1 < app.hf_search_results.len() =>
            {
                app.hf_search_cursor += 1;
            }
            (Backspace, _) => {
                if !app.hf_search_results.is_empty() {
                    // Editing query clears old results.
                    app.hf_search_results.clear();
                    app.hf_search_cursor = 0;
                }
                if app.hf_search_query.pop().is_none() {
                    app.hf_search_active = false;
                }
            }
            (Char('c'), KeyModifiers::CONTROL) => {
                app.should_quit = true;
            }
            (Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                // Editing query clears old results.
                if !app.hf_search_results.is_empty() {
                    app.hf_search_results.clear();
                    app.hf_search_cursor = 0;
                }
                app.hf_search_query.push(c);
            }
            _ => {}
        }
        return;
    }

    // Normal Downloads screen navigation.
    match (key.code, key.modifiers) {
        (Up, _) | (Char('k'), KeyModifiers::NONE) => {
            app.downloads_cursor = app.downloads_cursor.saturating_sub(1);
            clamp_downloads_scroll(app, MAX_VISIBLE);
        }
        (Down, _) | (Char('j'), KeyModifiers::NONE) => {
            if app.downloads_cursor + 1 < app.downloads.len() {
                app.downloads_cursor += 1;
            }
            clamp_downloads_scroll(app, MAX_VISIBLE);
        }
        (Enter, _) if !app.downloads.is_empty() => {
            // Scan for existing adapters before entering the detail screen
            if let Some(model) = app.downloads.get(app.downloads_cursor) {
                let resolved = resolve_hf_cache_path(&model.model_id);
                app.adapter_list = scan_adapters(&resolved);
                app.adapter_cursor = 0;
            }
            app.screen = Screen::ModelDetail;
            if app.adapter_list.is_empty() {
                app.status = Status::neutral("Model details.");
            } else {
                app.status = Status::success(format!(
                    "{} adapter{} found. Enter · merge & export   f · fine-tune new",
                    app.adapter_list.len(),
                    if app.adapter_list.len() == 1 { "" } else { "s" }
                ));
            }
        }
        (Char('/'), KeyModifiers::NONE) => {
            app.hf_search_active = true;
            app.hf_search_query.clear();
            app.hf_search_results.clear();
            app.hf_search_cursor = 0;
            app.hf_search_loading = false;
            app.status = Status::neutral("Search HuggingFace Hub.");
        }
        (Esc, _) | (Tab, _) => {
            app.screen = Screen::Apps;
            app.status = app.idle_status();
        }
        (Char('c'), KeyModifiers::CONTROL) => {
            app.should_quit = true;
        }
        _ => {}
    }
}

fn handle_key_model_detail(
    app: &mut App,
    key: crossterm::event::KeyEvent,
    _tx: mpsc::UnboundedSender<AuthEvent>,
) {
    use KeyCode::*;

    match (key.code, key.modifiers) {
        (Esc, _) => {
            app.screen = Screen::Downloads;
            app.adapter_list.clear();
            app.adapter_cursor = 0;
            app.status = Status::neutral("Back to models.");
        }
        (Char('c'), KeyModifiers::CONTROL) => {
            app.should_quit = true;
        }
        // Navigate adapter list
        (Up, _) | (Char('k'), KeyModifiers::NONE) => {
            app.adapter_cursor = app.adapter_cursor.saturating_sub(1);
        }
        (Down, _) | (Char('j'), KeyModifiers::NONE) => {
            if !app.adapter_list.is_empty() && app.adapter_cursor + 1 < app.adapter_list.len() {
                app.adapter_cursor += 1;
            }
        }
        // Enter or 'm' — merge the selected LoRA adapter (skip fine-tuning)
        (Enter, _) | (Char('m'), KeyModifiers::NONE)
            if app
                .adapter_list
                .get(app.adapter_cursor)
                .is_some_and(|a| a.kind == ArtifactKind::LoraAdapter) =>
        {
            if let Some(adapter) = app.adapter_list.get(app.adapter_cursor) {
                if let Some(model) = app.downloads.get(app.downloads_cursor) {
                    let resolved = resolve_hf_cache_path(&model.model_id);
                    if resolved.is_empty() {
                        app.status = Status::error("Model not downloaded locally.");
                        return;
                    }

                    let adapter_path = adapter.path.clone();
                    app.finetune_model_id = model.model_id.clone();
                    app.finetune_model_dir = resolved.clone();

                    // Set finetune_progress to Done so the FineTune screen
                    // shows the post-training view with merge/export actions.
                    app.finetune_progress = Some(crate::finetune::FineTuneProgress::Done {
                        adapter_path: adapter_path.clone(),
                    });
                    app.finetune_running = false;
                    app.merge_progress = None;
                    app.gguf_progress = None;
                    app.merged_model_dir = None;
                    app.screen = Screen::FineTune;
                    app.status =
                        Status::neutral("Adapter selected. Press m to merge, Esc to go back.");
                }
            } else {
                app.status = Status::error("No adapter selected.");
            }
        }
        (Char('f'), KeyModifiers::NONE) => {
            if let Some(model) = app.downloads.get(app.downloads_cursor) {
                let resolved = resolve_hf_cache_path(&model.model_id);
                if resolved.is_empty() {
                    app.status = Status::error("Model not downloaded locally.");
                } else if !std::path::Path::new(&resolved).join("config.json").exists() {
                    app.status = Status::error(
                        "Fine-tuning requires a safetensors model (GGUF not supported).",
                    );
                } else {
                    app.finetune_model_id = model.model_id.clone();
                    app.finetune_model_dir = resolved;
                    app.finetune_focus = FineTuneFocus::DataPath;
                    app.finetune_running = false;
                    app.finetune_progress = None;
                    app.screen = Screen::FineTune;
                    app.status = Status::neutral("Configure fine-tuning.");
                }
            }
        }
        _ => {}
    }
}

/// Resolve a HuggingFace model ID to its local cache snapshot directory.
///
/// Priority: App Group (onde-cli download cache) → `$HF_HOME` → `~/.cache/huggingface/hub`.
/// Looks for `{hub}/models--{org}--{name}/snapshots/{hash}/` and returns the first match,
/// or an empty string if not found.
fn resolve_hf_cache_path(model_id: &str) -> String {
    let dir_name = format!("models--{}", model_id.replace('/', "--"));

    // App Group first (that's where onde-cli downloads), then standard HF cache
    let candidates: Vec<std::path::PathBuf> = {
        let mut c = Vec::new();
        #[cfg(target_os = "macos")]
        if let Some(home) = dirs::home_dir() {
            c.push(
                home.join("Library")
                    .join("Group Containers")
                    .join("group.com.ondeinference.apps")
                    .join("models")
                    .join("hub"),
            );
        }
        // Check HF_HOME env
        if let Ok(hf_home) = std::env::var("HF_HOME") {
            c.push(std::path::PathBuf::from(hf_home).join("hub"));
        }
        // ~/.cache/huggingface/hub
        if let Some(home) = dirs::home_dir() {
            c.push(home.join(".cache").join("huggingface").join("hub"));
        }
        c
    };

    for hub in candidates {
        let snapshots_dir = hub.join(&dir_name).join("snapshots");
        if let Ok(entries) = std::fs::read_dir(&snapshots_dir) {
            // Pick the first directory that looks like a commit hash (hex string).
            // This skips non-hash dirs like "lora-adapter" that fine-tuning creates.
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    let name = entry.file_name();
                    let name = name.to_string_lossy();
                    if name.len() >= 7 && name.chars().all(|c| c.is_ascii_hexdigit()) {
                        return p.to_string_lossy().to_string();
                    }
                }
            }
        }
    }

    String::new()
}

/// Scan for existing `lora_adapter.safetensors` files and `.gguf` exports under
/// the model's snapshots directory. Returns entries sorted by modification time
/// (newest first).
fn scan_adapters(model_dir: &str) -> Vec<AdapterEntry> {
    if model_dir.is_empty() {
        return Vec::new();
    }
    let snapshots_dir = std::path::Path::new(model_dir)
        .parent()
        .unwrap_or_else(|| std::path::Path::new(""));

    let mut artifacts: Vec<AdapterEntry> = Vec::new();

    let Ok(entries) = std::fs::read_dir(snapshots_dir) else {
        return artifacts;
    };

    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let dir_name = entry.file_name().to_string_lossy().to_string();

        // Check for LoRA adapter
        let adapter_file = dir.join("lora_adapter.safetensors");
        if adapter_file.exists()
            && let Some(ae) = make_artifact_entry(
                &adapter_file,
                &dir_name,
                "lora_adapter.safetensors",
                ArtifactKind::LoraAdapter,
            )
        {
            artifacts.push(ae);
        }

        // Check for any .gguf files in this subdirectory
        if let Ok(sub_entries) = std::fs::read_dir(&dir) {
            for sub in sub_entries.flatten() {
                let fname = sub.file_name().to_string_lossy().to_string();
                if fname.ends_with(".gguf")
                    && let Some(ae) =
                        make_artifact_entry(&sub.path(), &dir_name, &fname, ArtifactKind::Gguf)
                {
                    artifacts.push(ae);
                }
            }
        }
    }

    // Also check the snapshots parent directory itself for .gguf files
    // (GGUF export writes to e.g. .../snapshots/model-finetuned-q8_0.gguf)
    if let Ok(parent_entries) = std::fs::read_dir(snapshots_dir) {
        for entry in parent_entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                let fname = entry.file_name().to_string_lossy().to_string();
                if fname.ends_with(".gguf")
                    && let Some(ae) =
                        make_artifact_entry(&path, "snapshots", &fname, ArtifactKind::Gguf)
                {
                    artifacts.push(ae);
                }
            }
        }
    }

    // Sort newest first
    artifacts.sort_by(|a, b| {
        let ma = std::fs::metadata(&a.path)
            .ok()
            .and_then(|m| m.modified().ok());
        let mb = std::fs::metadata(&b.path)
            .ok()
            .and_then(|m| m.modified().ok());
        mb.cmp(&ma)
    });

    artifacts
}

fn make_artifact_entry(
    path: &std::path::Path,
    dir_name: &str,
    file_name: &str,
    kind: ArtifactKind,
) -> Option<AdapterEntry> {
    let meta = std::fs::metadata(path).ok()?;
    let size = format_adapter_size(meta.len());
    let modified = meta
        .modified()
        .ok()
        .map(|t| {
            let elapsed = t.elapsed().unwrap_or_default();
            let secs = elapsed.as_secs();
            if secs < 60 {
                "just now".to_string()
            } else if secs < 3600 {
                format!("{}m ago", secs / 60)
            } else if secs < 86400 {
                format!("{}h ago", secs / 3600)
            } else {
                format!("{}d ago", secs / 86400)
            }
        })
        .unwrap_or_else(|| "–".to_string());
    Some(AdapterEntry {
        path: path.to_path_buf(),
        dir_name: dir_name.to_string(),
        file_name: file_name.to_string(),
        size,
        modified,
        kind,
    })
}

fn format_adapter_size(bytes: u64) -> String {
    if bytes >= 1_000_000_000 {
        format!("{:.1}GB", bytes as f64 / 1e9)
    } else if bytes >= 1_000_000 {
        format!("{:.0}MB", bytes as f64 / 1e6)
    } else {
        format!("{:.0}KB", bytes as f64 / 1e3)
    }
}

fn handle_key_finetune(
    app: &mut App,
    key: crossterm::event::KeyEvent,
    tx: mpsc::UnboundedSender<AuthEvent>,
) {
    use KeyCode::*;

    // While any background task is running, only allow Ctrl+C
    if app.finetune_running || app.merge_running || app.gguf_running {
        if matches!(
            (key.code, key.modifiers),
            (Char('c'), KeyModifiers::CONTROL)
        ) {
            app.should_quit = true;
        }
        return;
    }

    // After fine-tune is done: handle merge (m) and GGUF export (g)
    if let Some(crate::finetune::FineTuneProgress::Done { adapter_path }) = &app.finetune_progress {
        match (key.code, key.modifiers) {
            (Char('m'), KeyModifiers::NONE) => {
                // Start merge: adapter into base model
                let base_dir = std::path::PathBuf::from({
                    let raw = &app.finetune_model_dir;
                    dirs::home_dir()
                        .and_then(|home| {
                            raw.strip_prefix("~/")
                                .map(|rest| home.join(rest).to_string_lossy().to_string())
                        })
                        .unwrap_or_else(|| raw.to_string())
                });
                let output_dir = base_dir.parent().unwrap_or(&base_dir).join("merged-model");

                let merge_config = crate::merge::MergeConfig {
                    base_dir,
                    adapter_path: adapter_path.clone(),
                    output_dir,
                };

                app.merge_running = true;
                app.merge_progress = None;
                app.status = Status::neutral("Merging adapter…");

                let (merge_tx, mut merge_rx) = mpsc::unbounded_channel();
                crate::merge::start_merge(merge_config, merge_tx);

                let auth_tx = tx.clone();
                tokio::spawn(async move {
                    while let Some(progress) = merge_rx.recv().await {
                        let _ = auth_tx.send(AuthEvent::MergeProgress(progress));
                    }
                });
                return;
            }
            (Char('g'), KeyModifiers::NONE) if app.merged_model_dir.is_some() => {
                // Start GGUF export from merged model
                let model_dir = app.merged_model_dir.clone().unwrap_or_default();
                let output_path = model_dir
                    .parent()
                    .unwrap_or(&model_dir)
                    .join("model-finetuned-q8_0.gguf");

                let gguf_config = crate::gguf::GgufConfig {
                    model_dir,
                    output_path,
                    dtype: crate::gguf::GgufDtype::Q8_0,
                };

                app.gguf_running = true;
                app.gguf_progress = None;
                app.status = Status::neutral("Exporting GGUF…");

                let (gguf_tx, mut gguf_rx) = mpsc::unbounded_channel();
                crate::gguf::start_gguf_export(gguf_config, gguf_tx);

                let auth_tx = tx.clone();
                tokio::spawn(async move {
                    while let Some(progress) = gguf_rx.recv().await {
                        let _ = auth_tx.send(AuthEvent::GgufProgress(progress));
                    }
                });
                return;
            }
            (Esc, _) => {
                app.screen = Screen::ModelDetail;
                app.finetune_progress = None;
                app.merge_progress = None;
                app.gguf_progress = None;
                app.merged_model_dir = None;
                app.status = Status::neutral("Back to model.");
                return;
            }
            (Char('c'), KeyModifiers::CONTROL) => {
                app.should_quit = true;
                return;
            }
            _ => return,
        }
    }

    match (key.code, key.modifiers) {
        (Tab, _) => {
            app.finetune_focus = match app.finetune_focus {
                FineTuneFocus::ModelDir => FineTuneFocus::DataPath,
                FineTuneFocus::DataPath => FineTuneFocus::Rank,
                FineTuneFocus::Rank => FineTuneFocus::Epochs,
                FineTuneFocus::Epochs => FineTuneFocus::Lr,
                FineTuneFocus::Lr => FineTuneFocus::ModelDir,
            };
        }
        (Backspace, _) => match app.finetune_focus {
            FineTuneFocus::ModelDir => {
                app.finetune_model_dir.pop();
            }
            FineTuneFocus::DataPath => {
                app.finetune_data_path.pop();
            }
            FineTuneFocus::Rank => {
                app.finetune_rank.pop();
            }
            FineTuneFocus::Epochs => {
                app.finetune_epochs.pop();
            }
            FineTuneFocus::Lr => {
                app.finetune_lr.pop();
            }
        },
        (Enter, _) => {
            let rank: usize = app.finetune_rank.parse().unwrap_or(8);
            let epochs: usize = app.finetune_epochs.parse().unwrap_or(3);
            let lr: f64 = app.finetune_lr.parse().unwrap_or(0.0001);

            // Expand ~ in paths
            let expand = |s: &str| -> String {
                dirs::home_dir()
                    .and_then(|home| {
                        s.strip_prefix("~/")
                            .map(|rest| home.join(rest).to_string_lossy().to_string())
                    })
                    .unwrap_or_else(|| s.to_string())
            };

            let model_dir = std::path::PathBuf::from(expand(&app.finetune_model_dir));
            let data_path = std::path::PathBuf::from(expand(&app.finetune_data_path));
            let output_dir = model_dir
                .parent()
                .unwrap_or(&model_dir)
                .join("lora-adapter");

            let config = crate::finetune::FineTuneConfig {
                model_dir,
                data_path,
                output_dir,
                lora_rank: rank,
                lora_alpha: (rank as f32) * 2.0,
                learning_rate: lr,
                epochs,
                max_seq_len: 512,
            };

            app.finetune_running = true;
            app.finetune_progress = None;
            app.status = Status::neutral("Fine-tuning started…");

            let (ft_tx, mut ft_rx) = mpsc::unbounded_channel();
            crate::finetune::start_finetune(config, ft_tx);

            let auth_tx = tx.clone();
            tokio::spawn(async move {
                while let Some(progress) = ft_rx.recv().await {
                    let _ = auth_tx.send(AuthEvent::FineTuneProgress(progress));
                }
            });
        }
        (Esc, _) => {
            app.screen = Screen::ModelDetail;
            app.status = Status::neutral("Back to model.");
        }
        (Char('c'), KeyModifiers::CONTROL) => {
            app.should_quit = true;
        }
        (Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => match app.finetune_focus {
            FineTuneFocus::ModelDir => {
                app.finetune_model_dir.push(c);
            }
            FineTuneFocus::DataPath => {
                app.finetune_data_path.push(c);
            }
            FineTuneFocus::Rank => {
                app.finetune_rank.push(c);
            }
            FineTuneFocus::Epochs => {
                app.finetune_epochs.push(c);
            }
            FineTuneFocus::Lr => {
                app.finetune_lr.push(c);
            }
        },
        _ => {}
    }
}

fn trigger_hf_search(app: &mut App, tx: mpsc::UnboundedSender<AuthEvent>) {
    let query = app.hf_search_query.trim().to_string();
    if query.is_empty() {
        return;
    }
    app.hf_search_loading = true;
    app.hf_search_results.clear();
    app.hf_search_cursor = 0;
    app.status = Status::neutral(format!("Searching for \"{query}\"…"));
    tokio::spawn(async move {
        match crate::hf_search::search_hf(&query).await {
            Ok(results) => {
                let _ = tx.send(AuthEvent::HfSearchResults(results));
            }
            Err(e) => {
                let _ = tx.send(AuthEvent::HfSearchFailed(e.to_string()));
            }
        }
    });
}

fn trigger_download(app: &mut App, tx: mpsc::UnboundedSender<AuthEvent>) {
    let Some(model) = app.hf_search_results.get(app.hf_search_cursor) else {
        return;
    };
    let model_id = model.model_id.clone();
    let hub = crate::hf::preferred_download_hub();

    app.downloading = true;
    app.download_progress = None;
    app.status = Status::neutral(format!("Downloading {model_id}…"));

    let (dl_tx, mut dl_rx) = mpsc::unbounded_channel::<crate::hf_search::DownloadEvent>();

    // Task 1: run the download and send DownloadEvents.
    let model_id_clone = model_id.clone();
    tokio::spawn(async move {
        crate::hf_search::download_model(model_id_clone, hub, dl_tx).await;
    });

    // Task 2: forward DownloadEvents → AuthEvents.
    let auth_tx = tx.clone();
    tokio::spawn(async move {
        while let Some(event) = dl_rx.recv().await {
            match event {
                crate::hf_search::DownloadEvent::Progress(p) => {
                    let _ = auth_tx.send(AuthEvent::ModelDownloadProgress(p));
                }
                crate::hf_search::DownloadEvent::Complete => {
                    let _ = auth_tx.send(AuthEvent::ModelDownloadComplete(model_id.clone()));
                    break;
                }
                crate::hf_search::DownloadEvent::Failed(e) => {
                    let _ = auth_tx.send(AuthEvent::ModelDownloadFailed(e));
                    break;
                }
            }
        }
    });
}

fn trigger_load_downloads(app: &mut App, tx: mpsc::UnboundedSender<AuthEvent>) {
    if app.busy {
        return;
    }
    let token = token::load().unwrap_or_default();
    app.busy = true;
    app.status = Status::neutral("Scanning models…");
    tokio::spawn(async move {
        // Fetch the remote catalog and scan the local cache concurrently.
        let (catalog_result, local_result) = tokio::join!(
            crate::gresiq::load_models(&token),
            tokio::task::spawn_blocking(crate::hf::list_local_models),
        );

        let catalog = catalog_result.unwrap_or_default();
        let local = local_result.unwrap_or_default();
        let merged = crate::hf::merge_models(&catalog, local);

        let _ = tx.send(AuthEvent::DownloadsLoaded(merged));
    });
}

fn clamp_downloads_scroll(app: &mut App, max_visible: usize) {
    if app.downloads_cursor < app.downloads_offset {
        app.downloads_offset = app.downloads_cursor;
    } else if app.downloads_cursor >= app.downloads_offset + max_visible {
        app.downloads_offset = app.downloads_cursor + 1 - max_visible;
    }
}

fn clamp_models_scroll(app: &mut App, max_visible: usize) {
    if app.models_cursor < app.models_offset {
        app.models_offset = app.models_cursor;
    } else if app.models_cursor >= app.models_offset + max_visible {
        app.models_offset = app.models_cursor + 1 - max_visible;
    }
}

fn open_model_picker(app: &mut App, tx: mpsc::UnboundedSender<AuthEvent>) {
    if app.apps.is_empty() || app.busy {
        return;
    }
    app.assigning_for_app_index = Some(app.apps_cursor);
    app.screen = Screen::Models;
    app.models_cursor = 0;
    app.models_offset = 0;
    if !app.models_loaded {
        let token = token::load().unwrap_or_default();
        app.busy = true;
        app.status = Status::neutral("Loading models…");
        tokio::spawn(async move {
            match crate::gresiq::load_models(&token).await {
                Ok(models) => {
                    let _ = tx.send(AuthEvent::ModelsLoaded(models));
                }
                Err(e) => {
                    let _ = tx.send(AuthEvent::ModelsLoadFailed(e.to_string()));
                }
            }
        });
    } else {
        app.status = Status::neutral("Choose a model.");
    }
}

fn submit_create_app(app: &mut App, tx: mpsc::UnboundedSender<AuthEvent>) {
    let name = app.new_app_name.trim().to_string();
    if name.is_empty() {
        return;
    }
    let token = token::load().unwrap_or_default();
    app.busy = true;
    app.status = Status::neutral(format!("Creating “{name}”…"));
    tokio::spawn(async move {
        match crate::gresiq::create_app(&token, &name).await {
            Ok(created) => {
                let _ = tx.send(AuthEvent::AppCreated(created));
            }
            Err(e) => {
                let _ = tx.send(AuthEvent::AppCreateFailed(e.to_string()));
            }
        }
    });
}

fn submit_assign_model(app: &mut App, tx: mpsc::UnboundedSender<AuthEvent>) {
    if app.busy {
        return;
    }
    let Some(app_index) = app.assigning_for_app_index else {
        return;
    };
    let Some(onde_app) = app.apps.get(app_index) else {
        return;
    };
    let Some(model) = app.models.get(app.models_cursor) else {
        return;
    };
    let token = token::load().unwrap_or_default();
    let onde_app_id = onde_app.id.clone();
    let model_id = model.id.clone();
    let model_name = model.name.clone().unwrap_or_else(|| model_id.clone());
    app.busy = true;
    app.status = Status::neutral(format!("Assigning {model_name}…"));
    tokio::spawn(async move {
        match crate::gresiq::assign_model(&token, &onde_app_id, &model_id).await {
            Ok(()) => {
                let _ = tx.send(AuthEvent::ModelAssigned {
                    app_index,
                    model_id,
                });
            }
            Err(e) => {
                let _ = tx.send(AuthEvent::ModelAssignFailed(e.to_string()));
            }
        }
    });
}

fn submit(app: &mut App, tx: mpsc::UnboundedSender<AuthEvent>) {
    let email = app.email.trim().to_string();
    let password = app.password.clone();

    if email.is_empty() || password.is_empty() {
        app.status = Status::error("Email and password are required.");
        return;
    }

    app.busy = true;
    app.status = match app.mode {
        Mode::Signup => Status::neutral("Creating account…"),
        Mode::Signin => Status::neutral("Signing in…"),
    };

    let mode = app.mode.clone();

    tokio::spawn(async move {
        let event = match mode {
            Mode::Signup => {
                match signup_with_client(Environment::Production, credentials(), email, password)
                    .await
                {
                    Ok(result) => AuthEvent::SignupOk(result.message),
                    Err(e) => AuthEvent::Failed(extract_error(&e)),
                }
            }
            Mode::Signin => {
                match login_with_client(
                    Environment::Production,
                    credentials(),
                    email.clone(),
                    password,
                )
                .await
                {
                    Ok(AccountStatus::Ready { access_token }) => {
                        if let Err(e) = token::save(&access_token) {
                            log::warn!("Could not persist token: {e}");
                        }
                        let profile_email = match me_with_client(
                            Environment::Production,
                            credentials(),
                            &access_token,
                        )
                        .await
                        {
                            Ok(user) => user.email,
                            Err(_) => email,
                        };
                        AuthEvent::SigninOk(Profile {
                            email: profile_email,
                        })
                    }
                    Ok(AccountStatus::Incomplete { .. }) => AuthEvent::Failed(
                        "Check your email first — we sent a confirmation link when you signed up."
                            .into(),
                    ),
                    Ok(AccountStatus::NotFound) => {
                        AuthEvent::Failed("That email isn’t in our system.".into())
                    }
                    Err(e) => AuthEvent::Failed(extract_error(&e)),
                }
            }
        };
        let _ = tx.send(event);
    });
}

fn sign_out(app: &mut App, tx: mpsc::UnboundedSender<AuthEvent>) {
    let saved_token = token::load();
    app.busy = true;
    app.status = Status::neutral("Signing out…");

    tokio::spawn(async move {
        if let Some(t) = saved_token {
            let _ = logout_with_client(Environment::Production, credentials(), t).await;
        }
        let _ = tx.send(AuthEvent::SignedOut);
    });
}

fn extract_error(e: &ErrorResponse) -> String {
    match e {
        ErrorResponse::Error { message, .. } => message.clone(),
    }
}
