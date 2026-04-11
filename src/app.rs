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

// app credentials — Onde is a tenant in smbCloud Auth

const APP_ID: &str = "e1098adf-9859-43bf-ae98-514cf66252a5";
const APP_SECRET: &str = "5bc60b32-c072-40ce-a91f-b289fdee075d";

fn credentials() -> ClientCredentials<'static> {
    ClientCredentials {
        app_id: APP_ID,
        app_secret: APP_SECRET,
    }
}

// types

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
    SignupOk(String),
    SigninOk(Profile),
    ProfileOk(Profile),
    SignedOut,
    Failed(String),
}

// app state

pub struct App {
    pub mode: Mode,
    pub email: String,
    pub password: String,
    pub focus: Focus,
    pub status: Status,
    pub busy: bool,
    pub profile: Option<Profile>,
    pub should_quit: bool,
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
            AuthEvent::SignupOk(message) => {
                self.password.clear();
                self.status = Status::success(message);
            }
            AuthEvent::SigninOk(profile) => {
                self.password.clear();
                self.profile = Some(profile);
                self.status = Status::success("You're in.");
            }
            AuthEvent::ProfileOk(profile) => {
                self.profile = Some(profile);
                self.status = Status::success("Still signed in.");
            }
            AuthEvent::SignedOut => {
                token::clear();
                self.profile = None;
                self.status = Status::neutral("Signed out.");
            }
            AuthEvent::Failed(message) => {
                self.status = Status::error(message);
            }
        }
    }
}

// event loop

pub async fn run(terminal: &mut DefaultTerminal) -> Result<()> {
    let mut app = App::new();
    let (tx, mut rx) = mpsc::unbounded_channel::<AuthEvent>();
    let mut events = EventStream::new();

    // pick up where we left off if there's a saved token
    if let Some(saved_token) = token::load() {
        app.busy = true;
        app.status = Status::neutral("Restoring session…");
        let tx = tx.clone();
        tokio::spawn(async move {
            match me_with_client(Environment::Production, credentials(), &saved_token).await {
                Ok(user) => {
                    let _ = tx.send(AuthEvent::ProfileOk(Profile { email: user.email }));
                }
                Err(_) => {
                    token::clear();
                    let _ = tx.send(AuthEvent::Failed("Session expired. Sign in again.".into()));
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
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}

// key handling

fn handle_key(
    app: &mut App,
    key: crossterm::event::KeyEvent,
    tx: mpsc::UnboundedSender<AuthEvent>,
) {
    use KeyCode::*;

    // don't process input while a request is in flight — only quit
    if app.busy {
        if matches!(
            (key.code, key.modifiers),
            (Char('c'), KeyModifiers::CONTROL)
        ) {
            app.should_quit = true;
        }
        return;
    }

    match (key.code, key.modifiers) {
        // Quit
        (Char('c'), KeyModifiers::CONTROL) | (Esc, _) => {
            app.should_quit = true;
        }

        // Tab: cycle focus between fields (only on auth form)
        (Tab, _) if app.profile.is_none() => {
            app.focus = match app.focus {
                Focus::Email => Focus::Password,
                Focus::Password => Focus::Email,
            };
        }

        // Mode switching
        (Char('l'), KeyModifiers::CONTROL) if app.profile.is_none() => {
            app.switch_mode(Mode::Signin);
        }
        (Char('n'), KeyModifiers::CONTROL) if app.profile.is_none() => {
            app.switch_mode(Mode::Signup);
        }

        // Enter: submit form or sign out
        (Enter, _) => {
            if app.profile.is_some() {
                sign_out(app, tx);
            } else {
                submit(app, tx);
            }
        }

        // Backspace
        (Backspace, _) if app.profile.is_none() => match app.focus {
            Focus::Email => {
                app.email.pop();
            }
            Focus::Password => {
                app.password.pop();
            }
        },

        // Regular character input
        (Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) if app.profile.is_none() => {
            match app.focus {
                Focus::Email => app.email.push(c),
                Focus::Password => app.password.push(c),
            }
        }

        _ => {}
    }
}

// auth actions

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
                        // Fetch profile; fall back to the email typed by the user
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
                        AuthEvent::Failed("That email isn't in our system.".into())
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
        if let Some(token) = saved_token {
            // fire and forget — if the server call fails we still clear locally
            let _ = logout_with_client(Environment::Production, credentials(), token).await;
        }
        let _ = tx.send(AuthEvent::SignedOut);
    });
}

fn extract_error(e: &ErrorResponse) -> String {
    match e {
        ErrorResponse::Error { message, .. } => message.clone(),
    }
}
