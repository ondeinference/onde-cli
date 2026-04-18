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
pub(crate) const ONDE_APP_ID: &str = "e1098adf-9859-43bf-ae98-514cf66252a5";
pub(crate) const ONDE_APP_SECRET: &str = "5bc60b32-c072-40ce-a91f-b289fdee075d";

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
    Models,
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
        Self { tone: StatusTone::Neutral, message: msg.into() }
    }
    pub fn success(msg: impl Into<String>) -> Self {
        Self { tone: StatusTone::Success, message: msg.into() }
    }
    pub fn error(msg: impl Into<String>) -> Self {
        Self { tone: StatusTone::Error, message: msg.into() }
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
    // models
    ModelsLoaded(Vec<OndeModel>),
    ModelsLoadFailed(String),
    ModelAssigned { app_index: usize, model_id: String },
    ModelAssignFailed(String),
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
    // models list
    pub models: Vec<OndeModel>,
    pub models_cursor: usize,
    pub models_offset: usize,
    pub models_loaded: bool,
    // inline create form
    pub creating_app: bool,
    pub new_app_name: String,
    // which app we're picking a model for
    pub assigning_for_app_index: Option<usize>,
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
            assigning_for_app_index: None,
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
        self.busy = false;
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
                self.assigning_for_app_index = None;
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
            AuthEvent::ModelAssigned { app_index, model_id } => {
                self.busy = false;
                self.screen = Screen::Apps;
                self.assigning_for_app_index = None;
                if let Some(onde_app) = self.apps.get_mut(app_index) {
                    onde_app.current_model_id = Some(model_id);
                }
                self.status = Status::success("Model assigned.");
            }
            AuthEvent::ModelAssignFailed(msg) => {
                self.busy = false;
                self.status = Status::error(msg);
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
                    let _ = tx2.send(AuthEvent::Failed(
                        "Session expired. Sign in again.".into(),
                    ));
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
        if matches!((key.code, key.modifiers), (Char('c'), KeyModifiers::CONTROL)) {
            app.should_quit = true;
        }
        return;
    }

    match app.screen {
        Screen::Auth => handle_key_auth(app, key, tx),
        Screen::Apps => handle_key_apps(app, key, tx),
        Screen::Models => handle_key_models(app, key, tx),
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
            Focus::Email => { app.email.pop(); }
            Focus::Password => { app.password.pop(); }
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
            (Enter, _) => {
                open_model_picker(app, tx);
            }
            (Char('n'), KeyModifiers::NONE) => {
                app.creating_app = true;
                app.status = Status::neutral("Type a name and press Enter.");
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
            app.screen = Screen::Apps;
            app.assigning_for_app_index = None;
            app.status = Status::neutral("Back to apps.");
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
                let _ = tx.send(AuthEvent::ModelAssigned { app_index, model_id });
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
                match signup_with_client(
                    Environment::Production,
                    credentials(),
                    email,
                    password,
                )
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
                        AuthEvent::SigninOk(Profile { email: profile_email })
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
