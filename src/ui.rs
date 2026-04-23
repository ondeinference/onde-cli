use {
    crate::app::{App, ArtifactKind, FineTuneFocus, Focus, Mode, OndeApp, Screen, StatusTone},
    crate::hf::CacheSource,
    ratatui::{
        Frame,
        layout::{Alignment, Constraint, Layout, Rect},
        style::{Color, Style, Stylize},
        text::{Line, Span},
        widgets::{Block, BorderType, Borders, Padding, Paragraph, Wrap},
    },
};

// colours from globals.css

const C_BG: Color = Color::Rgb(0, 0, 0);
const C_SURFACE: Color = Color::Rgb(13, 20, 16);
const C_SURFACE_STRONG: Color = Color::Rgb(20, 28, 24);
const C_NEON: Color = Color::Rgb(66, 255, 145);
const C_TEXT: Color = Color::Rgb(226, 226, 226);
const C_MUTED: Color = Color::Rgb(122, 144, 128);
const C_INK: Color = Color::Rgb(216, 229, 222);
const C_DANGER: Color = Color::Rgb(255, 95, 86);
const C_LINE: Color = Color::Rgb(35, 50, 42);

pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();

    frame.render_widget(Block::new().style(Style::new().bg(C_BG)), area);

    let layout = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .split(area);

    render_header(frame, layout[0]);
    render_card(frame, app, layout[1]);
    render_footer(frame, app, layout[2]);
}

fn render_header(frame: &mut Frame, area: Rect) {
    let text = vec![
        Line::from(vec![
            Span::styled("◆ onde", Style::new().fg(C_NEON).bold()),
            Span::styled("  —  ondeinference.com", Style::new().fg(C_MUTED)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Start building on Apple silicon.",
            Style::new().fg(C_TEXT),
        )),
    ];
    frame.render_widget(Paragraph::new(text).alignment(Alignment::Center), area);
}

fn render_card(frame: &mut Frame, app: &App, area: Rect) {
    let card_width = 64_u16.min(area.width.saturating_sub(4));
    let h_pad = area.width.saturating_sub(card_width) / 2;

    let cols = Layout::horizontal([
        Constraint::Length(h_pad),
        Constraint::Length(card_width),
        Constraint::Min(0),
    ])
    .split(area);

    let card_area = cols[1];

    let card = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(C_LINE))
        .style(Style::new().bg(C_SURFACE))
        .padding(Padding::new(2, 2, 1, 1));

    let inner = card.inner(card_area);
    frame.render_widget(card, card_area);

    match app.screen {
        Screen::Auth => render_form(frame, app, inner),
        Screen::Apps => render_apps(frame, app, inner),
        Screen::AppDetail => render_app_detail(frame, app, inner),
        Screen::Models => render_models(frame, app, inner),
        Screen::Downloads => render_downloads(frame, app, inner),
        Screen::ModelDetail => render_model_detail(frame, app, inner),
        Screen::GgufDetail => render_gguf_detail(frame, app, inner),
        Screen::FineTune => render_finetune(frame, app, inner),
        Screen::CloneRepo => render_clone_repo(frame, app, inner),
    }
}

// auth form

fn render_form(frame: &mut Frame, app: &App, area: Rect) {
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(3),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(3),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .split(area);

    render_tabs(frame, app, rows[0]);

    let (headline, description) = match app.mode {
        Mode::Signup => (
            "Create your account",
            "We'll send a confirmation email. You'll need to verify before signing in.",
        ),
        Mode::Signin => ("Good to have you back", "Sign in to your existing account."),
    };

    frame.render_widget(
        Paragraph::new(headline).style(Style::new().fg(C_INK).bold()),
        rows[2],
    );
    frame.render_widget(
        Paragraph::new(description)
            .style(Style::new().fg(C_MUTED))
            .wrap(Wrap { trim: true }),
        rows[3],
    );

    render_status(frame, app, rows[5]);

    frame.render_widget(
        Paragraph::new("Email").style(Style::new().fg(C_MUTED)),
        rows[7],
    );
    render_input(
        frame,
        app,
        &app.email,
        Focus::Email,
        "name@company.com",
        rows[8],
    );

    frame.render_widget(
        Paragraph::new("Password").style(Style::new().fg(C_MUTED)),
        rows[10],
    );
    let masked = "•".repeat(app.password.len());
    render_input(
        frame,
        app,
        &masked,
        Focus::Password,
        "Minimum 8 characters",
        rows[11],
    );

    let (primary_label, secondary_label) = match app.mode {
        Mode::Signup => ("[ Create account ]", "I already have an account  Ctrl+L"),
        Mode::Signin => ("[ Sign in ]", "Create a new account  Ctrl+N"),
    };

    let primary_style = if app.busy {
        Style::new().fg(C_MUTED)
    } else {
        Style::new().fg(C_SURFACE).bg(C_NEON).bold()
    };

    frame.render_widget(Paragraph::new(primary_label).style(primary_style), rows[13]);
    frame.render_widget(
        Paragraph::new(secondary_label).style(Style::new().fg(C_MUTED)),
        rows[14],
    );
}

fn render_tabs(frame: &mut Frame, app: &App, area: Rect) {
    let cols = Layout::horizontal([
        Constraint::Length(19),
        Constraint::Length(1),
        Constraint::Length(11),
        Constraint::Min(0),
    ])
    .split(area);

    let active = Style::new().fg(C_SURFACE).bg(C_NEON).bold();
    let inactive = Style::new().fg(C_MUTED).bg(C_SURFACE_STRONG);

    frame.render_widget(
        Paragraph::new(" Create account ").style(if app.mode == Mode::Signup {
            active
        } else {
            inactive
        }),
        cols[0],
    );
    frame.render_widget(
        Paragraph::new(" Sign in ").style(if app.mode == Mode::Signin {
            active
        } else {
            inactive
        }),
        cols[2],
    );
}

fn render_input(
    frame: &mut Frame,
    app: &App,
    value: &str,
    field: Focus,
    placeholder: &str,
    area: Rect,
) {
    let is_focused = app.focus == field;

    let border_style = if is_focused {
        Style::new().fg(C_NEON)
    } else {
        Style::new().fg(C_LINE)
    };

    let block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style)
        .style(Style::new().bg(C_SURFACE_STRONG));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if value.is_empty() && !is_focused {
        frame.render_widget(
            Paragraph::new(placeholder).style(Style::new().fg(C_MUTED)),
            inner,
        );
    } else {
        frame.render_widget(Paragraph::new(value).style(Style::new().fg(C_TEXT)), inner);
        if is_focused {
            let cursor_x = (inner.x + value.chars().count() as u16)
                .min(inner.x + inner.width.saturating_sub(1));
            frame.set_cursor_position((cursor_x, inner.y));
        }
    }
}

// new-app input — always focused, no focus state needed
fn render_create_input(frame: &mut Frame, value: &str, area: Rect) {
    let block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(C_NEON))
        .style(Style::new().bg(C_SURFACE_STRONG));

    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(Paragraph::new(value).style(Style::new().fg(C_TEXT)), inner);

    let cursor_x =
        (inner.x + value.chars().count() as u16).min(inner.x + inner.width.saturating_sub(1));
    frame.set_cursor_position((cursor_x, inner.y));
}

fn render_status(frame: &mut Frame, app: &App, area: Rect) {
    let (icon, color) = match app.status.tone {
        StatusTone::Neutral => ("●", C_MUTED),
        StatusTone::Success => ("✓", C_NEON),
        StatusTone::Error => ("✗", C_DANGER),
    };

    let prefix = if app.busy { "⠿ " } else { "" };

    let line = Line::from(vec![
        Span::styled(format!("{prefix}{icon} "), Style::new().fg(color)),
        Span::styled(&app.status.message, Style::new().fg(color)),
    ]);

    frame.render_widget(Paragraph::new(line), area);
}

// apps screen

/// Tab bar shared between the Apps and Downloads screens.
fn render_nav_tabs(frame: &mut Frame, app: &App, area: Rect) {
    let cols = Layout::horizontal([
        Constraint::Length(9),  // [ Apps ]
        Constraint::Length(1),  // gap
        Constraint::Length(11), // [ Models ]
        Constraint::Min(0),
    ])
    .split(area);

    let active = Style::new().fg(C_SURFACE).bg(C_NEON).bold();
    let inactive = Style::new().fg(C_MUTED).bg(C_SURFACE_STRONG);

    let apps_style = if app.screen == Screen::Apps {
        active
    } else {
        inactive
    };
    let models_style = if app.screen == Screen::Downloads {
        active
    } else {
        inactive
    };

    frame.render_widget(Paragraph::new(" Apps ").style(apps_style), cols[0]);
    frame.render_widget(Paragraph::new(" Models ").style(models_style), cols[2]);
}

fn render_apps(frame: &mut Frame, app: &App, area: Rect) {
    let email = app.profile.as_ref().map(|p| p.email.as_str()).unwrap_or("");

    let top = Layout::vertical([
        Constraint::Length(1), // profile badge
        Constraint::Length(1), // spacer
        Constraint::Length(1), // nav tabs
        Constraint::Length(1), // spacer
        Constraint::Length(1), // status
        Constraint::Length(1), // spacer
        Constraint::Length(1), // column header
        Constraint::Length(1), // divider
        Constraint::Min(0),    // rest (list + optional form + hint)
    ])
    .split(area);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("✓ ", Style::new().fg(C_NEON)),
            Span::styled(email, Style::new().fg(C_TEXT).bold()),
        ])),
        top[0],
    );

    render_nav_tabs(frame, app, top[2]);
    render_status(frame, app, top[4]);

    frame.render_widget(
        Paragraph::new("  Name                   Status   Model").style(Style::new().fg(C_MUTED)),
        top[6],
    );

    let divider = "─".repeat(area.width as usize);
    frame.render_widget(
        Paragraph::new(divider).style(Style::new().fg(C_LINE)),
        top[7],
    );

    let rest = top[8];
    if app.creating_app {
        let bottom = Layout::vertical([
            Constraint::Min(0),    // list
            Constraint::Length(1), // spacer
            Constraint::Length(1), // "New app name:" label
            Constraint::Length(3), // input
            Constraint::Length(1), // hint
        ])
        .split(rest);

        render_apps_list(frame, app, bottom[0]);

        frame.render_widget(
            Paragraph::new("New app name:").style(Style::new().fg(C_MUTED)),
            bottom[2],
        );
        render_create_input(frame, &app.new_app_name, bottom[3]);
        frame.render_widget(
            Paragraph::new("Enter · create   Esc · cancel").style(Style::new().fg(C_MUTED)),
            bottom[4],
        );
    } else {
        let bottom = Layout::vertical([
            Constraint::Min(0),    // list
            Constraint::Length(1), // hint
        ])
        .split(rest);

        render_apps_list(frame, app, bottom[0]);
        frame.render_widget(
            Paragraph::new("n · new   Enter · open   s · sign out").style(Style::new().fg(C_MUTED)),
            bottom[1],
        );
    }
}

fn render_apps_list(frame: &mut Frame, app: &App, area: Rect) {
    if app.apps.is_empty() {
        if app.busy {
            frame.render_widget(
                Paragraph::new("  Loading…").style(Style::new().fg(C_MUTED)),
                area,
            );
        } else if app.apps_loaded {
            frame.render_widget(
                Paragraph::new("  No apps yet. Press n to create one.")
                    .style(Style::new().fg(C_MUTED)),
                area,
            );
        }
        return;
    }

    let max_rows = area.height as usize;
    for (list_idx, onde_app) in app
        .apps
        .iter()
        .enumerate()
        .skip(app.apps_offset)
        .take(max_rows)
    {
        let row_y = area.y + (list_idx - app.apps_offset) as u16;
        let row_area = Rect::new(area.x, row_y, area.width, 1);

        let is_selected = list_idx == app.apps_cursor;
        let prefix = if is_selected { "▶ " } else { "  " };

        let model_name = resolve_model_name(app, onde_app);
        let status_str = onde_app.status.as_deref().unwrap_or("–");
        let name_str = &onde_app.name;

        let line = format!(
            "{}{:<22} {:<8} {}",
            prefix, name_str, status_str, model_name
        );

        let style = if is_selected {
            Style::new().fg(C_NEON)
        } else {
            Style::new().fg(C_TEXT)
        };

        frame.render_widget(Paragraph::new(line).style(style), row_area);
    }
}

// Falls back to "–" if the app has no model assigned or the model isn’t in
// the loaded list yet (models load lazily on first visit to the picker).
fn resolve_model_name<'a>(app: &'a App, onde_app: &'a OndeApp) -> &'a str {
    // Prefer the name the API returned directly on the app object.
    if let Some(name) = onde_app.active_model.as_deref() {
        return name;
    }
    // Fall back to a lookup in the lazily-loaded models list.
    if let Some(name) = onde_app
        .current_model_id
        .as_deref()
        .and_then(|id| app.models.iter().find(|m| m.id == id))
        .and_then(|m| m.name.as_deref())
    {
        return name;
    }
    "No model assigned yet"
}

// app detail screen

fn render_app_detail(frame: &mut Frame, app: &App, area: Rect) {
    let Some(onde_app) = app.apps.get(app.apps_cursor) else {
        return;
    };

    let model_name = resolve_model_name(app, onde_app);
    let app_id = onde_app.id.as_str();
    let app_secret = onde_app.app_secret.as_deref().unwrap_or("–");
    let status_str = onde_app.status.as_deref().unwrap_or("–");

    if app.renaming_app {
        let rows = Layout::vertical([
            Constraint::Length(1), // app name heading
            Constraint::Length(1), // spacer
            Constraint::Length(1), // status row
            Constraint::Length(1), // spacer
            Constraint::Length(1), // App ID label
            Constraint::Length(1), // App ID value
            Constraint::Length(1), // spacer
            Constraint::Length(1), // App Secret label
            Constraint::Length(1), // App Secret value
            Constraint::Length(1), // spacer
            Constraint::Length(1), // Model label
            Constraint::Length(1), // Model value
            Constraint::Length(1), // spacer
            Constraint::Length(1), // rename label
            Constraint::Length(3), // rename input
            Constraint::Length(1), // rename hint
            Constraint::Min(0),
        ])
        .split(area);

        render_app_detail_header(frame, onde_app, status_str, rows[0]);
        render_status(frame, app, rows[2]);
        render_detail_field(frame, "App ID", app_id, rows[4], rows[5]);
        render_detail_field(frame, "App Secret", app_secret, rows[7], rows[8]);
        render_detail_field(frame, "Model", model_name, rows[10], rows[11]);

        frame.render_widget(
            Paragraph::new("New name:").style(Style::new().fg(C_MUTED)),
            rows[13],
        );
        render_rename_input(frame, &app.rename_input, rows[14]);
        frame.render_widget(
            Paragraph::new("Enter · save   Esc · cancel").style(Style::new().fg(C_MUTED)),
            rows[15],
        );
    } else {
        let rows = Layout::vertical([
            Constraint::Length(1), // app name heading
            Constraint::Length(1), // spacer
            Constraint::Length(1), // status row
            Constraint::Length(1), // spacer
            Constraint::Length(1), // App ID label
            Constraint::Length(1), // App ID value
            Constraint::Length(1), // spacer
            Constraint::Length(1), // App Secret label
            Constraint::Length(1), // App Secret value
            Constraint::Length(1), // spacer
            Constraint::Length(1), // Model label
            Constraint::Length(1), // Model value
            Constraint::Min(0),    // spacer
            Constraint::Length(1), // hint
        ])
        .split(area);

        render_app_detail_header(frame, onde_app, status_str, rows[0]);
        render_status(frame, app, rows[2]);
        render_detail_field(frame, "App ID", app_id, rows[4], rows[5]);
        render_detail_field(frame, "App Secret", app_secret, rows[7], rows[8]);
        render_detail_field(frame, "Model", model_name, rows[10], rows[11]);

        frame.render_widget(
            Paragraph::new("m · assign model   r · rename   s · sign out   Esc · back")
                .style(Style::new().fg(C_MUTED)),
            rows[13],
        );
    }
}

fn render_app_detail_header(frame: &mut Frame, onde_app: &OndeApp, status_str: &str, area: Rect) {
    let line = Line::from(vec![
        Span::styled(&onde_app.name, Style::new().fg(C_INK).bold()),
        Span::styled("  ", Style::new()),
        Span::styled(status_str, Style::new().fg(C_MUTED)),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn render_detail_field(
    frame: &mut Frame,
    label: &str,
    value: &str,
    label_area: Rect,
    value_area: Rect,
) {
    frame.render_widget(
        Paragraph::new(label).style(Style::new().fg(C_MUTED)),
        label_area,
    );
    frame.render_widget(
        Paragraph::new(value).style(Style::new().fg(C_TEXT).bold()),
        value_area,
    );
}

fn render_rename_input(frame: &mut Frame, value: &str, area: Rect) {
    let block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(C_NEON))
        .style(Style::new().bg(C_SURFACE_STRONG));

    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(Paragraph::new(value).style(Style::new().fg(C_TEXT)), inner);

    let cursor_x =
        (inner.x + value.chars().count() as u16).min(inner.x + inner.width.saturating_sub(1));
    frame.set_cursor_position((cursor_x, inner.y));
}

// models screen

fn render_models(frame: &mut Frame, app: &App, area: Rect) {
    let app_name: String = app
        .assigning_for_app_index
        .and_then(|i| app.apps.get(i))
        .map(|a| format!("Assign model — {}", a.name))
        .unwrap_or_else(|| "Models".to_string());

    let rows = Layout::vertical([
        Constraint::Length(1), // heading
        Constraint::Length(1), // spacer
        Constraint::Length(1), // column header
        Constraint::Length(1), // divider
        Constraint::Min(0),    // models list
        Constraint::Length(1), // spacer
        Constraint::Length(1), // status
    ])
    .split(area);

    frame.render_widget(
        Paragraph::new(app_name.as_str()).style(Style::new().fg(C_INK).bold()),
        rows[0],
    );

    frame.render_widget(
        Paragraph::new("  Name                   Family   Size       Format")
            .style(Style::new().fg(C_MUTED)),
        rows[2],
    );

    let divider = "─".repeat(area.width as usize);
    frame.render_widget(
        Paragraph::new(divider).style(Style::new().fg(C_LINE)),
        rows[3],
    );

    // models list
    let list_area = rows[4];
    if app.models.is_empty() {
        if app.busy {
            frame.render_widget(
                Paragraph::new("  Loading…").style(Style::new().fg(C_MUTED)),
                list_area,
            );
        }
    } else {
        let max_rows = list_area.height as usize;
        for (list_idx, model) in app
            .models
            .iter()
            .enumerate()
            .skip(app.models_offset)
            .take(max_rows)
        {
            let row_y = list_area.y + (list_idx - app.models_offset) as u16;
            let row_area = Rect::new(list_area.x, row_y, list_area.width, 1);

            let is_selected = list_idx == app.models_cursor;
            let prefix = if is_selected { "▶ " } else { "  " };

            let name = model.name.as_deref().unwrap_or("–");
            let family = model.family.as_deref().unwrap_or("–");
            let size = model
                .approx_size_bytes
                .map(fmt_bytes)
                .unwrap_or_else(|| "–".to_string());
            let format = model.format.as_deref().unwrap_or("–");

            let line = format!(
                "{}{:<22} {:<8} {:<10} {}",
                prefix, name, family, size, format
            );

            let style = if is_selected {
                Style::new().fg(C_NEON)
            } else {
                Style::new().fg(C_TEXT)
            };

            frame.render_widget(Paragraph::new(line).style(style), row_area);
        }
    }

    render_status(frame, app, rows[6]);
}

// downloads screen

fn render_downloads(frame: &mut Frame, app: &App, area: Rect) {
    let email = app.profile.as_ref().map(|p| p.email.as_str()).unwrap_or("");

    let top = Layout::vertical([
        Constraint::Length(1), // profile badge
        Constraint::Length(1), // spacer
        Constraint::Length(1), // nav tabs
        Constraint::Length(1), // spacer
        Constraint::Length(1), // status
        Constraint::Length(1), // spacer
        Constraint::Length(1), // column header
        Constraint::Length(1), // divider
        Constraint::Min(0),    // body
    ])
    .split(area);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("✓ ", Style::new().fg(C_NEON)),
            Span::styled(email, Style::new().fg(C_TEXT).bold()),
        ])),
        top[0],
    );

    render_nav_tabs(frame, app, top[2]);
    render_status(frame, app, top[4]);

    // Column header changes based on state.
    let col_header = if app.hf_search_active {
        "  Model                                        Downloads"
    } else {
        "  Model                                   Size       Source"
    };
    frame.render_widget(
        Paragraph::new(col_header).style(Style::new().fg(C_MUTED)),
        top[6],
    );

    let divider = "─".repeat(area.width as usize);
    frame.render_widget(
        Paragraph::new(divider).style(Style::new().fg(C_LINE)),
        top[7],
    );

    if app.downloading {
        render_download_progress_panel(frame, app, top[8]);
    } else if app.hf_search_active {
        render_hf_search_panel(frame, app, top[8]);
    } else {
        let bottom = Layout::vertical([
            Constraint::Min(0),    // list
            Constraint::Length(1), // hint
        ])
        .split(top[8]);

        render_downloads_list(frame, app, bottom[0]);

        frame.render_widget(
            Paragraph::new("↑↓ · navigate   / · search HF   Tab · apps")
                .style(Style::new().fg(C_MUTED)),
            bottom[1],
        );
    }
}

fn render_downloads_list(frame: &mut Frame, app: &App, area: Rect) {
    if app.downloads.is_empty() {
        if app.busy {
            frame.render_widget(
                Paragraph::new("  Scanning…").style(Style::new().fg(C_MUTED)),
                area,
            );
        } else if app.downloads_loaded {
            frame.render_widget(
                Paragraph::new("  No models in catalog yet.").style(Style::new().fg(C_MUTED)),
                area,
            );
        }
        return;
    }

    let max_rows = area.height as usize;
    for (list_idx, model) in app
        .downloads
        .iter()
        .enumerate()
        .skip(app.downloads_offset)
        .take(max_rows)
    {
        let row_y = area.y + (list_idx - app.downloads_offset) as u16;
        let row_area = Rect::new(area.x, row_y, area.width, 1);

        let is_selected = list_idx == app.downloads_cursor;
        let prefix = if is_selected { "▶ " } else { "  " };

        let (status_label, status_style) = if model.downloaded {
            let src = model
                .source
                .as_ref()
                .map(|s| s.label())
                .unwrap_or("Downloaded");
            let style = match model.source.as_ref() {
                Some(CacheSource::AppGroup) => Style::new().fg(C_NEON),
                _ => Style::new().fg(C_TEXT),
            };
            (src, style)
        } else {
            ("–", Style::new().fg(C_MUTED))
        };

        let left = format!(
            "{}{:<36} {:<10}",
            prefix, model.display_name, model.size_display
        );

        let text_style = if is_selected {
            Style::new().fg(C_NEON)
        } else if model.downloaded {
            Style::new().fg(C_TEXT)
        } else {
            Style::new().fg(C_MUTED)
        };

        let line = Line::from(vec![
            Span::styled(left, text_style),
            Span::styled(status_label, status_style),
        ]);

        frame.render_widget(Paragraph::new(line), row_area);
    }
}

// model detail screen

fn render_hf_search_panel(frame: &mut Frame, app: &App, area: Rect) {
    let rows = Layout::vertical([
        Constraint::Length(1), // "Search HuggingFace Hub" label
        Constraint::Length(3), // search input box
        Constraint::Length(1), // spacer
        Constraint::Min(0),    // results list
        Constraint::Length(1), // hint
    ])
    .split(area);

    frame.render_widget(
        Paragraph::new("Search HuggingFace Hub").style(Style::new().fg(C_MUTED)),
        rows[0],
    );

    // Search input — always neon-bordered (it's always focused when active).
    let block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(C_NEON))
        .style(Style::new().bg(C_SURFACE_STRONG));
    let inner = block.inner(rows[1]);
    frame.render_widget(block, rows[1]);

    let display_query = if app.hf_search_loading {
        format!("{} ⠿", app.hf_search_query)
    } else {
        app.hf_search_query.clone()
    };
    frame.render_widget(
        Paragraph::new(display_query.as_str()).style(Style::new().fg(C_TEXT)),
        inner,
    );

    // Show cursor only when not loading.
    if !app.hf_search_loading {
        let cursor_x = (inner.x + app.hf_search_query.chars().count() as u16)
            .min(inner.x + inner.width.saturating_sub(1));
        frame.set_cursor_position((cursor_x, inner.y));
    }

    // Results area.
    if app.hf_search_results.is_empty() && !app.hf_search_loading {
        frame.render_widget(
            Paragraph::new("  Type a query and press Enter to search.")
                .style(Style::new().fg(C_MUTED)),
            rows[3],
        );
    } else {
        render_hf_results_list(frame, app, rows[3]);
    }

    let hint = if app.hf_search_loading {
        "Searching…"
    } else if app.hf_search_results.is_empty() {
        "Enter · search   Esc · cancel"
    } else {
        "↑↓ · navigate   Enter · download   Esc · cancel"
    };
    frame.render_widget(
        Paragraph::new(hint).style(Style::new().fg(C_MUTED)),
        rows[4],
    );
}

fn render_hf_results_list(frame: &mut Frame, app: &App, area: Rect) {
    let max_rows = area.height as usize;
    for (i, model) in app.hf_search_results.iter().enumerate().take(max_rows) {
        let row_y = area.y + i as u16;
        let row_area = Rect::new(area.x, row_y, area.width, 1);
        let is_selected = i == app.hf_search_cursor;
        let prefix = if is_selected { "▶ " } else { "  " };

        let dl_str = format_downloads(model.downloads);
        // Truncate model_id to keep the line from wrapping.
        let max_id_len = (area.width as usize).saturating_sub(12);
        let model_id_display = if model.model_id.len() > max_id_len {
            format!("{}…", &model.model_id[..max_id_len.saturating_sub(1)])
        } else {
            model.model_id.clone()
        };
        let left = format!("{}{:<44}", prefix, model_id_display);

        let text_style = if is_selected {
            Style::new().fg(C_NEON)
        } else {
            Style::new().fg(C_TEXT)
        };

        let line = Line::from(vec![
            Span::styled(left, text_style),
            Span::styled(dl_str, Style::new().fg(C_MUTED)),
        ]);
        frame.render_widget(Paragraph::new(line), row_area);
    }
}

fn format_downloads(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M ↓", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.0}K ↓", n as f64 / 1_000.0)
    } else if n > 0 {
        format!("{n} ↓")
    } else {
        "–".to_string()
    }
}

fn render_download_progress_panel(frame: &mut Frame, app: &App, area: Rect) {
    let rows = Layout::vertical([
        Constraint::Length(1), // model ID heading
        Constraint::Length(1), // spacer
        Constraint::Length(1), // filename
        Constraint::Length(1), // progress bar
        Constraint::Length(1), // byte count
        Constraint::Min(0),    // spacer
        Constraint::Length(1), // hint
    ])
    .split(area);

    if let Some(dp) = &app.download_progress {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("⠿ ", Style::new().fg(C_NEON)),
                Span::styled(dp.model_id.as_str(), Style::new().fg(C_INK).bold()),
            ])),
            rows[0],
        );

        let file_label = format!(
            "File {}/{}: {}",
            dp.file_index + 1,
            dp.total_files,
            dp.filename
        );
        frame.render_widget(
            Paragraph::new(file_label).style(Style::new().fg(C_TEXT)),
            rows[2],
        );

        // Progress bar.
        let progress = if dp.file_bytes_total > 0 {
            dp.file_bytes_done as f64 / dp.file_bytes_total as f64
        } else {
            0.0
        };
        let bar_width = area.width.saturating_sub(4) as usize;
        let filled = (progress * bar_width as f64) as usize;
        let empty = bar_width.saturating_sub(filled);
        let bar = format!("{}{}", "█".repeat(filled), "░".repeat(empty));
        frame.render_widget(Paragraph::new(bar).style(Style::new().fg(C_NEON)), rows[3]);

        let bytes_label = format!(
            "{} / {}",
            fmt_bytes(dp.file_bytes_done as i64),
            fmt_bytes(dp.file_bytes_total as i64),
        );
        frame.render_widget(
            Paragraph::new(bytes_label).style(Style::new().fg(C_MUTED)),
            rows[4],
        );
    } else {
        frame.render_widget(
            Paragraph::new("⠿ Starting download…").style(Style::new().fg(C_MUTED)),
            rows[0],
        );
    }

    frame.render_widget(
        Paragraph::new("Ctrl+C · quit").style(Style::new().fg(C_MUTED)),
        rows[6],
    );
}

fn render_model_detail(frame: &mut Frame, app: &App, area: Rect) {
    let Some(model) = app.downloads.get(app.downloads_cursor) else {
        return;
    };

    // Resolve fields from the catalog entry when available.
    let hf_repo = model.model_id.as_str();
    let format_str = model
        .catalog_model
        .as_ref()
        .and_then(|m| m.format.as_deref())
        .unwrap_or("–");
    let gguf_file = model
        .catalog_model
        .as_ref()
        .and_then(|m| m.gguf_file.as_deref())
        .unwrap_or("–");
    let description = model
        .catalog_model
        .as_ref()
        .and_then(|m| m.description.as_deref())
        .unwrap_or("–");
    let family = model
        .catalog_model
        .as_ref()
        .and_then(|m| m.family.as_deref())
        .unwrap_or("–");

    let (dl_label, dl_style) = if model.downloaded {
        let src = model.source.as_ref().map(|s| s.label()).unwrap_or("Yes");
        (format!("✓  {src}"), Style::new().fg(C_NEON).bold())
    } else {
        ("–  Not downloaded".to_string(), Style::new().fg(C_MUTED))
    };

    let rows = Layout::vertical([
        Constraint::Length(1), // 0  model name heading
        Constraint::Length(1), // 1  spacer
        Constraint::Length(1), // 2  status
        Constraint::Length(1), // 3  spacer
        Constraint::Length(1), // 4  HF Repo label
        Constraint::Length(1), // 5  HF Repo value
        Constraint::Length(1), // 6  spacer
        Constraint::Length(1), // 7  Downloaded label
        Constraint::Length(1), // 8  Downloaded value
        Constraint::Length(1), // 9  spacer
        Constraint::Length(1), // 10 Size label
        Constraint::Length(1), // 11 Size value
        Constraint::Length(1), // 12 spacer
        Constraint::Length(1), // 13 Format label
        Constraint::Length(1), // 14 Format value
        Constraint::Length(1), // 15 spacer
        Constraint::Length(1), // 16 Family label
        Constraint::Length(1), // 17 Family value
        Constraint::Length(1), // 18 spacer
        Constraint::Length(1), // 19 File label
        Constraint::Length(1), // 20 File value
        Constraint::Length(1), // 21 spacer
        Constraint::Length(1), // 22 Catalog ID label
        Constraint::Length(1), // 23 Catalog ID value
        Constraint::Length(1), // 24 spacer
        Constraint::Length(1), // 25 Description label
        Constraint::Length(1), // 26 Description value
        Constraint::Length(1), // 27 spacer
        Constraint::Length(1), // 28 Adapters heading
        Constraint::Min(0),    // 29 Adapters list
    ])
    .split(area);

    // Heading
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            &model.display_name,
            Style::new().fg(C_INK).bold(),
        ))),
        rows[0],
    );

    render_status(frame, app, rows[2]);

    render_detail_field(frame, "HF Repo", hf_repo, rows[4], rows[5]);

    // Downloaded — styled separately
    frame.render_widget(
        Paragraph::new("Downloaded").style(Style::new().fg(C_MUTED)),
        rows[7],
    );
    frame.render_widget(Paragraph::new(dl_label).style(dl_style), rows[8]);

    render_detail_field(frame, "Size", &model.size_display, rows[10], rows[11]);
    render_detail_field(frame, "Format", format_str, rows[13], rows[14]);
    render_detail_field(frame, "Family", family, rows[16], rows[17]);
    render_detail_field(frame, "File", gguf_file, rows[19], rows[20]);

    let catalog_id_str = model.catalog_id.as_deref().unwrap_or("–");
    render_detail_field(frame, "Catalog ID", catalog_id_str, rows[22], rows[23]);

    frame.render_widget(
        Paragraph::new("Description").style(Style::new().fg(C_MUTED)),
        rows[25],
    );
    frame.render_widget(
        Paragraph::new(description)
            .style(Style::new().fg(C_TEXT))
            .wrap(Wrap { trim: true }),
        rows[26],
    );

    // Artifacts section (adapters + exported GGUFs)
    let adapter_heading = if app.adapter_list.is_empty() {
        "Artifacts"
    } else {
        "Artifacts  (↑↓ select · Enter merge & export)"
    };
    frame.render_widget(
        Paragraph::new(adapter_heading).style(Style::new().fg(C_MUTED)),
        rows[28],
    );
    render_model_detail_adapters(frame, app, rows[29]);
}

/// Render the selectable artifact list inside the Model Detail screen.
fn render_model_detail_adapters(frame: &mut Frame, app: &App, area: Rect) {
    if app.adapter_list.is_empty() {
        frame.render_widget(
            Paragraph::new("  No artifacts found. Press f to fine-tune.")
                .style(Style::new().fg(C_MUTED)),
            area,
        );
        return;
    }

    let max_rows = area.height as usize;
    for (i, adapter) in app.adapter_list.iter().enumerate().take(max_rows) {
        let row_y = area.y + i as u16;
        let row_area = Rect::new(area.x, row_y, area.width, 1);

        let is_selected = i == app.adapter_cursor;
        let marker = if is_selected { "▸ " } else { "  " };
        let marker_style = if is_selected {
            Style::new().fg(C_NEON).bold()
        } else {
            Style::new().fg(C_NEON)
        };
        let name_style = if is_selected {
            Style::new().fg(C_TEXT).bold()
        } else {
            Style::new().fg(C_TEXT)
        };
        let meta_style = Style::new().fg(C_MUTED);

        // Kind badge + icon
        let (kind_icon, kind_label) = match adapter.kind {
            ArtifactKind::LoraAdapter => ("◆ ", "LoRA"),
            ArtifactKind::Gguf => ("● ", "GGUF"),
        };
        let kind_color = match adapter.kind {
            ArtifactKind::LoraAdapter => C_NEON,
            ArtifactKind::Gguf => Color::Rgb(100, 200, 255),
        };

        // Show file_name for GGUF, dir_name for LoRA
        let display_name = match adapter.kind {
            ArtifactKind::Gguf => &adapter.file_name,
            ArtifactKind::LoraAdapter => &adapter.dir_name,
        };
        let name_truncated = if display_name.len() > 28 {
            format!("{}…", &display_name[..27])
        } else {
            display_name.clone()
        };

        let line = Line::from(vec![
            Span::styled(marker, marker_style),
            Span::styled(kind_icon, Style::new().fg(kind_color)),
            Span::styled(format!("{:<6}", kind_label), Style::new().fg(kind_color)),
            Span::styled(format!("{:<30}", name_truncated), name_style),
            Span::styled(format!("{:<10}", adapter.size), meta_style),
            Span::styled(&adapter.modified, meta_style),
        ]);

        if is_selected {
            frame.render_widget(
                Paragraph::new(line).style(Style::new().bg(C_SURFACE_STRONG)),
                row_area,
            );
        } else {
            frame.render_widget(Paragraph::new(line), row_area);
        }
    }
}

fn render_gguf_detail(frame: &mut Frame, app: &App, area: Rect) {
    let Some(ref gguf) = app.selected_gguf else {
        return;
    };

    let rows = Layout::vertical([
        Constraint::Length(1), // 0  heading
        Constraint::Length(1), // 1  spacer
        Constraint::Length(1), // 2  status
        Constraint::Length(1), // 3  spacer
        Constraint::Length(1), // 4  File label
        Constraint::Length(1), // 5  File value
        Constraint::Length(1), // 6  spacer
        Constraint::Length(1), // 7  Size label
        Constraint::Length(1), // 8  Size value
        Constraint::Length(1), // 9  spacer
        Constraint::Length(1), // 10 Location label
        Constraint::Length(3), // 11 Location value (bordered, wraps)
        Constraint::Length(1), // 12 spacer
        Constraint::Length(1), // 13 Upload heading
        Constraint::Length(1), // 14 Repo Name label
        Constraint::Length(3), // 15 Repo Name input
        Constraint::Length(1), // 16 spacer
        Constraint::Length(1), // 17 upload status / progress
        Constraint::Length(1), // 18 spacer
        Constraint::Min(0),    // 19 rest
        Constraint::Length(1), // 20 hint
    ])
    .split(area);

    // Heading
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!("GGUF Model — {}", gguf.file_name),
            Style::new().fg(C_INK).bold(),
        ))),
        rows[0],
    );

    render_status(frame, app, rows[2]);

    // File name
    render_detail_field(frame, "File", &gguf.file_name, rows[4], rows[5]);

    // Size + modified
    let size_modified = format!("{}   {}", gguf.size, gguf.modified);
    render_detail_field(frame, "Size", &size_modified, rows[7], rows[8]);

    // Full path in a bordered box
    frame.render_widget(
        Paragraph::new("Location").style(Style::new().fg(C_MUTED)),
        rows[10],
    );
    let path_block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(C_LINE))
        .style(Style::new().bg(C_SURFACE_STRONG));
    let path_inner = path_block.inner(rows[11]);
    frame.render_widget(path_block, rows[11]);
    frame.render_widget(
        Paragraph::new(gguf.path.to_string_lossy().to_string())
            .style(Style::new().fg(C_NEON))
            .wrap(Wrap { trim: true }),
        path_inner,
    );

    // Upload section
    frame.render_widget(
        Paragraph::new("Upload to HuggingFace").style(Style::new().fg(C_MUTED)),
        rows[13],
    );

    // Repo name input
    frame.render_widget(
        Paragraph::new("Repo Name").style(Style::new().fg(C_MUTED)),
        rows[14],
    );

    let input_focused = !app.upload_running
        && !matches!(
            app.upload_progress,
            Some(crate::hf_upload::UploadProgress::Done { .. })
        );
    let border_style = if input_focused {
        Style::new().fg(C_NEON)
    } else {
        Style::new().fg(C_LINE)
    };
    let input_block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style)
        .style(Style::new().bg(C_SURFACE_STRONG));
    let input_inner = input_block.inner(rows[15]);
    frame.render_widget(input_block, rows[15]);
    frame.render_widget(
        Paragraph::new(app.upload_repo_name.as_str()).style(Style::new().fg(C_TEXT)),
        input_inner,
    );
    if input_focused {
        let cursor_x = (input_inner.x + app.upload_repo_name.chars().count() as u16)
            .min(input_inner.x + input_inner.width.saturating_sub(1));
        frame.set_cursor_position((cursor_x, input_inner.y));
    }

    // Upload progress / status
    render_upload_status(frame, app, rows[17]);

    // Hint
    let hint = if app.upload_running {
        "Ctrl+C · quit"
    } else if matches!(
        app.upload_progress,
        Some(crate::hf_upload::UploadProgress::Done { .. })
    ) {
        "Esc · back"
    } else {
        "Enter · upload    Esc · back"
    };
    frame.render_widget(
        Paragraph::new(hint).style(Style::new().fg(C_MUTED)),
        rows[20],
    );
}

fn render_upload_status(frame: &mut Frame, app: &App, area: Rect) {
    match &app.upload_progress {
        Some(crate::hf_upload::UploadProgress::CreatingRepo) => {
            frame.render_widget(
                Paragraph::new("⠿ Creating repository…").style(Style::new().fg(C_MUTED)),
                area,
            );
        }
        Some(crate::hf_upload::UploadProgress::Hashing {
            bytes_done,
            bytes_total,
        }) => {
            let pct = if *bytes_total > 0 {
                (*bytes_done as f64 / *bytes_total as f64 * 100.0) as u64
            } else {
                0
            };
            frame.render_widget(
                Paragraph::new(format!("⠿ Hashing… {}%", pct)).style(Style::new().fg(C_MUTED)),
                area,
            );
        }
        Some(crate::hf_upload::UploadProgress::Committing) => {
            frame.render_widget(
                Paragraph::new("⠿ Creating commit…").style(Style::new().fg(C_MUTED)),
                area,
            );
        }
        Some(crate::hf_upload::UploadProgress::Uploading {
            bytes_sent,
            bytes_total,
        }) => {
            let pct = if *bytes_total > 0 {
                (*bytes_sent as f64 / *bytes_total as f64 * 100.0) as u64
            } else {
                0
            };
            let sent_str = fmt_bytes(*bytes_sent as i64);
            let total_str = fmt_bytes(*bytes_total as i64);
            let bar_width = area.width.saturating_sub(30) as usize;
            let filled = if *bytes_total > 0 {
                (*bytes_sent as f64 / *bytes_total as f64 * bar_width as f64) as usize
            } else {
                0
            };
            let empty = bar_width.saturating_sub(filled);
            let bar = format!(
                "{}{} {}/{}  {}%",
                "█".repeat(filled),
                "░".repeat(empty),
                sent_str,
                total_str,
                pct
            );
            frame.render_widget(Paragraph::new(bar).style(Style::new().fg(C_NEON)), area);
        }
        Some(crate::hf_upload::UploadProgress::Done { url }) => {
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled("✓ ", Style::new().fg(C_NEON)),
                    Span::styled("Uploaded → ", Style::new().fg(C_NEON).bold()),
                    Span::styled(url.as_str(), Style::new().fg(C_TEXT)),
                ])),
                area,
            );
        }
        Some(crate::hf_upload::UploadProgress::Failed(msg)) => {
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled("✗ Upload failed: ", Style::new().fg(C_DANGER).bold()),
                    Span::styled(msg.as_str(), Style::new().fg(C_DANGER)),
                ])),
                area,
            );
        }
        None => {
            if crate::app::HF_TOKEN.is_empty() {
                frame.render_widget(
                    Paragraph::new("⚠ No HF_TOKEN in .env — upload disabled")
                        .style(Style::new().fg(C_DANGER)),
                    area,
                );
            } else {
                frame.render_widget(
                    Paragraph::new("Ready to upload. Press Enter to start.")
                        .style(Style::new().fg(C_MUTED)),
                    area,
                );
            }
        }
    }
}

fn fmt_bytes(bytes: i64) -> String {
    if bytes >= 1_000_000_000 {
        format!("{:.1}GB", bytes as f64 / 1e9)
    } else if bytes >= 1_000_000 {
        format!("{:.0}MB", bytes as f64 / 1e6)
    } else {
        format!("{:.0}KB", bytes as f64 / 1e3)
    }
}

fn render_finetune(frame: &mut Frame, app: &App, area: Rect) {
    if app.finetune_running
        || app.merge_running
        || app.gguf_running
        || app.finetune_progress.is_some()
    {
        render_finetune_progress(frame, app, area);
    } else {
        render_finetune_form(frame, app, area);
    }
}

fn render_finetune_form(frame: &mut Frame, app: &App, area: Rect) {
    let rows = Layout::vertical([
        Constraint::Length(1), // 0  heading
        Constraint::Length(1), // 1  spacer
        Constraint::Length(1), // 2  status
        Constraint::Length(1), // 3  spacer
        Constraint::Length(1), // 4  Model Dir label
        Constraint::Length(3), // 5  Model Dir input
        Constraint::Length(1), // 6  spacer
        Constraint::Length(1), // 7  Data Path label
        Constraint::Length(3), // 8  Data Path input
        Constraint::Length(1), // 9  spacer
        Constraint::Length(1), // 10 Rank label
        Constraint::Length(3), // 11 Rank input
        Constraint::Length(1), // 12 spacer
        Constraint::Length(1), // 13 Epochs label
        Constraint::Length(3), // 14 Epochs input
        Constraint::Length(1), // 15 spacer
        Constraint::Length(1), // 16 Learning Rate label
        Constraint::Length(3), // 17 Learning Rate input
        Constraint::Min(0),    // 18 spacer
        Constraint::Length(1), // 19 hint
    ])
    .split(area);

    // Heading — show the model ID hint
    let heading = if app.finetune_model_id.is_empty() {
        "Fine-Tune".to_string()
    } else {
        format!("Fine-Tune — {}", app.finetune_model_id)
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            heading,
            Style::new().fg(C_INK).bold(),
        ))),
        rows[0],
    );

    render_status(frame, app, rows[2]);

    // Model Dir
    frame.render_widget(
        Paragraph::new("Model Directory").style(Style::new().fg(C_MUTED)),
        rows[4],
    );
    render_finetune_input(
        frame,
        &app.finetune_model_dir,
        app.finetune_focus == FineTuneFocus::ModelDir,
        rows[5],
    );

    // Data Path
    frame.render_widget(
        Paragraph::new("Training Data (JSONL)").style(Style::new().fg(C_MUTED)),
        rows[7],
    );
    render_finetune_input(
        frame,
        &app.finetune_data_path,
        app.finetune_focus == FineTuneFocus::DataPath,
        rows[8],
    );

    // Rank
    frame.render_widget(
        Paragraph::new("LoRA Rank").style(Style::new().fg(C_MUTED)),
        rows[10],
    );
    render_finetune_input(
        frame,
        &app.finetune_rank,
        app.finetune_focus == FineTuneFocus::Rank,
        rows[11],
    );

    // Epochs
    frame.render_widget(
        Paragraph::new("Epochs").style(Style::new().fg(C_MUTED)),
        rows[13],
    );
    render_finetune_input(
        frame,
        &app.finetune_epochs,
        app.finetune_focus == FineTuneFocus::Epochs,
        rows[14],
    );

    // Learning Rate
    frame.render_widget(
        Paragraph::new("Learning Rate").style(Style::new().fg(C_MUTED)),
        rows[16],
    );
    render_finetune_input(
        frame,
        &app.finetune_lr,
        app.finetune_focus == FineTuneFocus::Lr,
        rows[17],
    );

    frame.render_widget(
        Paragraph::new("Enter · start   Esc · back").style(Style::new().fg(C_MUTED)),
        rows[19],
    );
}

fn render_finetune_input(frame: &mut Frame, value: &str, is_focused: bool, area: Rect) {
    let border_style = if is_focused {
        Style::new().fg(C_NEON)
    } else {
        Style::new().fg(C_LINE)
    };

    let block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style)
        .style(Style::new().bg(C_SURFACE_STRONG));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    frame.render_widget(Paragraph::new(value).style(Style::new().fg(C_TEXT)), inner);

    if is_focused {
        let cursor_x =
            (inner.x + value.chars().count() as u16).min(inner.x + inner.width.saturating_sub(1));
        frame.set_cursor_position((cursor_x, inner.y));
    }
}

fn render_finetune_progress(frame: &mut Frame, app: &App, area: Rect) {
    // Shared top: heading + status. The rest is state-dependent.
    let top = Layout::vertical([
        Constraint::Length(1), // 0  heading
        Constraint::Length(1), // 1  spacer
        Constraint::Length(1), // 2  status
        Constraint::Length(1), // 3  spacer
        Constraint::Min(0),    // 4  content (state-dependent)
        Constraint::Length(1), // 5  hint
    ])
    .split(area);

    // Heading
    let heading = if app.finetune_model_id.is_empty() {
        "Fine-Tune".to_string()
    } else {
        format!("Fine-Tune — {}", app.finetune_model_id)
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            heading,
            Style::new().fg(C_INK).bold(),
        ))),
        top[0],
    );

    render_status(frame, app, top[2]);

    let content = top[4];

    match &app.finetune_progress {
        Some(crate::finetune::FineTuneProgress::Validating) => {
            frame.render_widget(
                Paragraph::new("⠿ Validating model and data files…")
                    .style(Style::new().fg(C_MUTED)),
                content,
            );
        }
        Some(crate::finetune::FineTuneProgress::LoadingModel) => {
            frame.render_widget(
                Paragraph::new("⠿ Loading model weights…").style(Style::new().fg(C_MUTED)),
                content,
            );
        }
        Some(crate::finetune::FineTuneProgress::Tokenizing { done, total }) => {
            frame.render_widget(
                Paragraph::new(format!("⠿ Tokenizing… {done}/{total}"))
                    .style(Style::new().fg(C_MUTED)),
                content,
            );
        }
        Some(crate::finetune::FineTuneProgress::Training {
            epoch,
            total_epochs,
            step,
            total_steps,
            loss,
        }) => {
            let rows = Layout::vertical([
                Constraint::Length(1), // label
                Constraint::Length(1), // detail
                Constraint::Length(1), // spacer
                Constraint::Length(1), // progress bar
                Constraint::Min(0),    // rest
            ])
            .split(content);

            let detail =
                format!("epoch {epoch}/{total_epochs}  step {step}/{total_steps}  loss {loss:.3}");
            frame.render_widget(
                Paragraph::new("Training").style(Style::new().fg(C_NEON).bold()),
                rows[0],
            );
            frame.render_widget(
                Paragraph::new(detail).style(Style::new().fg(C_TEXT)),
                rows[1],
            );

            let progress = if *total_steps > 0 {
                *step as f64 / *total_steps as f64
            } else {
                0.0
            };
            let bar_width = content.width.saturating_sub(4) as usize;
            let filled = (progress * bar_width as f64) as usize;
            let empty = bar_width.saturating_sub(filled);
            let bar = format!("{}{}", "█".repeat(filled), "░".repeat(empty));
            frame.render_widget(Paragraph::new(bar).style(Style::new().fg(C_NEON)), rows[3]);
        }
        Some(crate::finetune::FineTuneProgress::Saving) => {
            frame.render_widget(
                Paragraph::new("⠿ Saving adapter weights…").style(Style::new().fg(C_MUTED)),
                content,
            );
        }
        Some(crate::finetune::FineTuneProgress::Done { adapter_path }) => {
            render_finetune_done(frame, app, adapter_path, content);
        }
        Some(crate::finetune::FineTuneProgress::Failed(msg)) => {
            let rows = Layout::vertical([
                Constraint::Length(1), // label
                Constraint::Length(1), // spacer
                Constraint::Min(0),    // error message (wraps)
            ])
            .split(content);

            frame.render_widget(
                Paragraph::new("✗ Failed").style(Style::new().fg(C_DANGER).bold()),
                rows[0],
            );
            frame.render_widget(
                Paragraph::new(msg.as_str())
                    .style(Style::new().fg(C_DANGER))
                    .wrap(Wrap { trim: true }),
                rows[2],
            );
        }
        None => {
            frame.render_widget(
                Paragraph::new("⠿ Starting…").style(Style::new().fg(C_MUTED)),
                content,
            );
        }
    }

    // Hint line
    let hint = if app.finetune_running || app.merge_running || app.gguf_running {
        "Ctrl+C · quit"
    } else {
        "Esc · back"
    };
    frame.render_widget(Paragraph::new(hint).style(Style::new().fg(C_MUTED)), top[5]);
}

/// Render the fine-tune "Done" state with full adapter details plus merge/export
/// progress and action hints.
fn render_finetune_done(frame: &mut Frame, app: &App, adapter_path: &std::path::Path, area: Rect) {
    let rows = Layout::vertical([
        Constraint::Length(1), // 0  ✓ label
        Constraint::Length(1), // 1  spacer
        Constraint::Length(1), // 2  "Adapter" label
        Constraint::Length(3), // 3  path (wrapped in bordered box)
        Constraint::Length(1), // 4  spacer
        Constraint::Length(1), // 5  size
        Constraint::Length(1), // 6  spacer
        Constraint::Length(1), // 7  config heading
        Constraint::Length(1), // 8  rank + epochs + lr
        Constraint::Length(1), // 9  spacer
        Constraint::Min(0),    // 10 merge/export section or adapters list
    ])
    .split(area);

    // ✓ label
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("✓ ", Style::new().fg(C_NEON)),
            Span::styled("Fine-tuning complete", Style::new().fg(C_NEON).bold()),
        ])),
        rows[0],
    );

    // Adapter path in a bordered box so it wraps visibly
    frame.render_widget(
        Paragraph::new("Adapter").style(Style::new().fg(C_MUTED)),
        rows[2],
    );
    let path_block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(C_LINE))
        .style(Style::new().bg(C_SURFACE_STRONG));
    let path_inner = path_block.inner(rows[3]);
    frame.render_widget(path_block, rows[3]);
    frame.render_widget(
        Paragraph::new(adapter_path.to_string_lossy().to_string())
            .style(Style::new().fg(C_NEON))
            .wrap(Wrap { trim: true }),
        path_inner,
    );

    // File size
    let size_str = std::fs::metadata(adapter_path)
        .map(|m| fmt_bytes(m.len() as i64))
        .unwrap_or_else(|_| "–".to_string());
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("Size  ", Style::new().fg(C_MUTED)),
            Span::styled(size_str, Style::new().fg(C_TEXT).bold()),
        ])),
        rows[5],
    );

    // Training config
    frame.render_widget(
        Paragraph::new("Training Config").style(Style::new().fg(C_MUTED)),
        rows[7],
    );
    let config_line = format!(
        "rank {}   epochs {}   lr {}",
        app.finetune_rank, app.finetune_epochs, app.finetune_lr
    );
    frame.render_widget(
        Paragraph::new(config_line).style(Style::new().fg(C_TEXT)),
        rows[8],
    );

    // Merge / GGUF export section
    render_merge_gguf_section(frame, app, rows[10]);
}

/// Render merge progress, GGUF export progress, and available action hints.
fn render_merge_gguf_section(frame: &mut Frame, app: &App, area: Rect) {
    let rows = Layout::vertical([
        Constraint::Length(1), // 0  merge status
        Constraint::Length(1), // 1  spacer
        Constraint::Length(1), // 2  gguf status
        Constraint::Length(1), // 3  spacer
        Constraint::Length(1), // 4  action hints
        Constraint::Length(1), // 5  spacer
        Constraint::Length(1), // 6  adapters heading
        Constraint::Min(0),    // 7  adapters list
    ])
    .split(area);

    // --- Merge status ---
    match &app.merge_progress {
        Some(crate::merge::MergeProgress::Loading) => {
            frame.render_widget(
                Paragraph::new("⠿ Loading model for merge…").style(Style::new().fg(C_MUTED)),
                rows[0],
            );
        }
        Some(crate::merge::MergeProgress::Merging { layer, total }) => {
            frame.render_widget(
                Paragraph::new(format!("⠿ Merging layer {layer}/{total}…"))
                    .style(Style::new().fg(C_MUTED)),
                rows[0],
            );
        }
        Some(crate::merge::MergeProgress::Saving) => {
            frame.render_widget(
                Paragraph::new("⠿ Saving merged model…").style(Style::new().fg(C_MUTED)),
                rows[0],
            );
        }
        Some(crate::merge::MergeProgress::Done { output_path }) => {
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled("✓ ", Style::new().fg(C_NEON)),
                    Span::styled("Merged → ", Style::new().fg(C_NEON).bold()),
                    Span::styled(
                        output_path.to_string_lossy().to_string(),
                        Style::new().fg(C_TEXT),
                    ),
                ])),
                rows[0],
            );
        }
        Some(crate::merge::MergeProgress::Failed(msg)) => {
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled("✗ Merge failed: ", Style::new().fg(C_DANGER).bold()),
                    Span::styled(msg.as_str(), Style::new().fg(C_DANGER)),
                ])),
                rows[0],
            );
        }
        None => {
            frame.render_widget(
                Paragraph::new("  Merge: not started").style(Style::new().fg(C_MUTED)),
                rows[0],
            );
        }
    }

    // --- GGUF export status ---
    match &app.gguf_progress {
        Some(crate::gguf::GgufProgress::ReadingModel) => {
            frame.render_widget(
                Paragraph::new("⠿ Reading model for GGUF export…").style(Style::new().fg(C_MUTED)),
                rows[2],
            );
        }
        Some(crate::gguf::GgufProgress::WritingTensor { index, total, name }) => {
            frame.render_widget(
                Paragraph::new(format!("⠿ Writing tensor {index}/{total}  {name}"))
                    .style(Style::new().fg(C_MUTED)),
                rows[2],
            );
        }
        Some(crate::gguf::GgufProgress::Done {
            output_path,
            size_bytes,
        }) => {
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled("✓ ", Style::new().fg(C_NEON)),
                    Span::styled("GGUF → ", Style::new().fg(C_NEON).bold()),
                    Span::styled(
                        output_path.to_string_lossy().to_string(),
                        Style::new().fg(C_TEXT),
                    ),
                    Span::styled(
                        format!("  ({})", fmt_bytes(*size_bytes as i64)),
                        Style::new().fg(C_MUTED),
                    ),
                ])),
                rows[2],
            );
        }
        Some(crate::gguf::GgufProgress::Failed(msg)) => {
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled("✗ GGUF failed: ", Style::new().fg(C_DANGER).bold()),
                    Span::styled(msg.as_str(), Style::new().fg(C_DANGER)),
                ])),
                rows[2],
            );
        }
        None => {
            if app.merged_model_dir.is_some() {
                frame.render_widget(
                    Paragraph::new("  GGUF: ready to export").style(Style::new().fg(C_MUTED)),
                    rows[2],
                );
            } else {
                frame.render_widget(
                    Paragraph::new("  GGUF: merge first").style(Style::new().fg(C_MUTED)),
                    rows[2],
                );
            }
        }
    }

    // --- Action hints ---
    let mut hints: Vec<Span> = Vec::new();
    if !app.merge_running && !app.gguf_running {
        hints.push(Span::styled("m", Style::new().fg(C_NEON)));
        hints.push(Span::styled(
            " · merge adapter    ",
            Style::new().fg(C_MUTED),
        ));
        if app.merged_model_dir.is_some() {
            hints.push(Span::styled("g", Style::new().fg(C_NEON)));
            hints.push(Span::styled(" · export GGUF    ", Style::new().fg(C_MUTED)));
        }
        hints.push(Span::styled("Esc", Style::new().fg(C_NEON)));
        hints.push(Span::styled(" · back", Style::new().fg(C_MUTED)));
    }
    if !hints.is_empty() {
        frame.render_widget(Paragraph::new(Line::from(hints)), rows[4]);
    }

    // --- Adapters list ---
    frame.render_widget(
        Paragraph::new("All Adapters").style(Style::new().fg(C_MUTED)),
        rows[6],
    );
    render_adapter_list(frame, app, rows[7]);
}

/// Scan the model's cache directory for existing LoRA adapter files and list them.
fn render_adapter_list(frame: &mut Frame, app: &App, area: Rect) {
    // Resolve the model's snapshots dir from the model_dir field.
    // model_dir points to .../snapshots/{hash}/, so parent is .../snapshots/
    let snapshots_dir = std::path::Path::new(&app.finetune_model_dir)
        .parent()
        .unwrap_or_else(|| std::path::Path::new(""));

    let mut adapters: Vec<(String, String, String)> = Vec::new(); // (name, size, date)

    // Look for lora_adapter.safetensors in any subdirectory of snapshots
    if let Ok(entries) = std::fs::read_dir(snapshots_dir) {
        for entry in entries.flatten() {
            let dir = entry.path();
            if !dir.is_dir() {
                continue;
            }
            let adapter_file = dir.join("lora_adapter.safetensors");
            if adapter_file.exists() {
                let dir_name = entry.file_name().to_string_lossy().to_string();
                let meta = std::fs::metadata(&adapter_file);
                let size = meta
                    .as_ref()
                    .map(|m| fmt_bytes(m.len() as i64))
                    .unwrap_or_else(|_| "–".to_string());
                let modified = meta
                    .ok()
                    .and_then(|m| m.modified().ok())
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
                adapters.push((dir_name, size, modified));
            }
        }
    }

    if adapters.is_empty() {
        frame.render_widget(
            Paragraph::new("  No adapters found.").style(Style::new().fg(C_MUTED)),
            area,
        );
        return;
    }

    let max_rows = area.height as usize;
    for (i, (name, size, modified)) in adapters.iter().enumerate().take(max_rows) {
        let row_y = area.y + i as u16;
        let row_area = Rect::new(area.x, row_y, area.width, 1);

        let line = Line::from(vec![
            Span::styled("  ◆ ", Style::new().fg(C_NEON)),
            Span::styled(format!("{:<20}", name), Style::new().fg(C_TEXT)),
            Span::styled(format!("{:<10}", size), Style::new().fg(C_MUTED)),
            Span::styled(modified.as_str(), Style::new().fg(C_MUTED)),
        ]);
        frame.render_widget(Paragraph::new(line), row_area);
    }
}

fn render_clone_repo(frame: &mut Frame, app: &App, area: Rect) {
    let is_picking_base = matches!(
        &app.clone_repo_status,
        Some(crate::hf_clone::RepoStatus::Empty { .. })
    );

    let rows = Layout::vertical([
        Constraint::Length(1), // 0  heading
        Constraint::Length(1), // 1  spacer
        Constraint::Length(1), // 2  status
        Constraint::Length(1), // 3  spacer
        Constraint::Length(1), // 4  Repo ID label
        Constraint::Length(3), // 5  Repo ID input
        Constraint::Length(1), // 6  spacer
        Constraint::Length(1), // 7  result heading
        Constraint::Min(0),    // 8  result / base model list
    ])
    .split(area);

    // Heading
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "Clone / Check HuggingFace Repo",
            Style::new().fg(C_INK).bold(),
        ))),
        rows[0],
    );

    render_status(frame, app, rows[2]);

    // Repo ID input
    frame.render_widget(
        Paragraph::new("Repo ID (e.g. ondeinference/joko)").style(Style::new().fg(C_MUTED)),
        rows[4],
    );

    let input_focused = !app.clone_repo_checking && !is_picking_base;
    let border_style = if input_focused {
        Style::new().fg(C_NEON)
    } else {
        Style::new().fg(C_LINE)
    };
    let input_block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style)
        .style(Style::new().bg(C_SURFACE_STRONG));
    let input_inner = input_block.inner(rows[5]);
    frame.render_widget(input_block, rows[5]);
    frame.render_widget(
        Paragraph::new(app.clone_repo_id.as_str()).style(Style::new().fg(C_TEXT)),
        input_inner,
    );
    if input_focused {
        let cursor_x = (input_inner.x + app.clone_repo_id.chars().count() as u16)
            .min(input_inner.x + input_inner.width.saturating_sub(1));
        frame.set_cursor_position((cursor_x, input_inner.y));
    }

    // Result section
    match &app.clone_repo_status {
        Some(crate::hf_clone::RepoStatus::NotFound) => {
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled("✗ ", Style::new().fg(C_DANGER)),
                    Span::styled(
                        "Repo not found. Press Enter to create it.",
                        Style::new().fg(C_MUTED),
                    ),
                ])),
                rows[7],
            );
        }
        Some(crate::hf_clone::RepoStatus::HasModel { files, .. }) => {
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled("✓ ", Style::new().fg(C_NEON)),
                    Span::styled(
                        format!("Repo has {} model file(s). Already set up.", files.len()),
                        Style::new().fg(C_NEON),
                    ),
                ])),
                rows[7],
            );
        }
        Some(crate::hf_clone::RepoStatus::Empty { .. }) => {
            frame.render_widget(
                Paragraph::new("Select a base model for fine-tuning:")
                    .style(Style::new().fg(C_MUTED)),
                rows[7],
            );

            // Render base model list
            let base_models = crate::hf_clone::BASE_MODELS;
            let items: Vec<Line> = base_models
                .iter()
                .enumerate()
                .map(|(i, m)| {
                    let selected = i == app.clone_base_cursor;
                    let prefix = if selected { "▸ " } else { "  " };
                    let style = if selected {
                        Style::new().fg(C_NEON).bold()
                    } else {
                        Style::new().fg(C_TEXT)
                    };
                    Line::from(vec![
                        Span::styled(prefix, style),
                        Span::styled(m.display_name, style),
                        Span::styled(
                            format!("  {}  {}", m.size_display, m.params),
                            Style::new().fg(C_MUTED),
                        ),
                    ])
                })
                .collect();

            // Add description of selected model below the list
            let selected_desc = base_models
                .get(app.clone_base_cursor)
                .map(|m| m.description)
                .unwrap_or("");

            let mut all_lines = items;
            all_lines.push(Line::from(""));
            all_lines.push(Line::from(Span::styled(
                selected_desc,
                Style::new().fg(C_MUTED).italic(),
            )));

            frame.render_widget(Paragraph::new(all_lines), rows[8]);
        }
        None => {
            if app.clone_repo_checking {
                frame.render_widget(
                    Paragraph::new("⠿ Checking…").style(Style::new().fg(C_MUTED)),
                    rows[7],
                );
            }
        }
    }
}

fn render_footer(frame: &mut Frame, app: &App, area: Rect) {
    let keys: Vec<Span> = match app.screen {
        Screen::Auth => vec![
            Span::styled("Tab", Style::new().fg(C_NEON)),
            Span::styled(" · next field    ", Style::new().fg(C_MUTED)),
            Span::styled("Enter", Style::new().fg(C_NEON)),
            Span::styled(" · submit    ", Style::new().fg(C_MUTED)),
            Span::styled("Ctrl+L", Style::new().fg(C_NEON)),
            Span::styled(" · sign in    ", Style::new().fg(C_MUTED)),
            Span::styled("Ctrl+N", Style::new().fg(C_NEON)),
            Span::styled(" · new account    ", Style::new().fg(C_MUTED)),
            Span::styled("Ctrl+C", Style::new().fg(C_NEON)),
            Span::styled(" · quit", Style::new().fg(C_MUTED)),
        ],
        Screen::Apps if app.creating_app => vec![
            Span::styled("Enter", Style::new().fg(C_NEON)),
            Span::styled(" · create    ", Style::new().fg(C_MUTED)),
            Span::styled("Esc", Style::new().fg(C_NEON)),
            Span::styled(" · cancel    ", Style::new().fg(C_MUTED)),
            Span::styled("Ctrl+C", Style::new().fg(C_NEON)),
            Span::styled(" · quit", Style::new().fg(C_MUTED)),
        ],
        Screen::Apps => vec![
            Span::styled("↑↓", Style::new().fg(C_NEON)),
            Span::styled(" · navigate    ", Style::new().fg(C_MUTED)),
            Span::styled("Enter", Style::new().fg(C_NEON)),
            Span::styled(" · open    ", Style::new().fg(C_MUTED)),
            Span::styled("n", Style::new().fg(C_NEON)),
            Span::styled(" · new    ", Style::new().fg(C_MUTED)),
            Span::styled("Tab", Style::new().fg(C_NEON)),
            Span::styled(" · models    ", Style::new().fg(C_MUTED)),
            Span::styled("s", Style::new().fg(C_NEON)),
            Span::styled(" · sign out    ", Style::new().fg(C_MUTED)),
            Span::styled("Ctrl+C", Style::new().fg(C_NEON)),
            Span::styled(" · quit", Style::new().fg(C_MUTED)),
        ],
        Screen::AppDetail if app.renaming_app => vec![
            Span::styled("Enter", Style::new().fg(C_NEON)),
            Span::styled(" · save    ", Style::new().fg(C_MUTED)),
            Span::styled("Esc", Style::new().fg(C_NEON)),
            Span::styled(" · cancel    ", Style::new().fg(C_MUTED)),
            Span::styled("Ctrl+C", Style::new().fg(C_NEON)),
            Span::styled(" · quit", Style::new().fg(C_MUTED)),
        ],
        Screen::AppDetail => vec![
            Span::styled("m", Style::new().fg(C_NEON)),
            Span::styled(" · assign model    ", Style::new().fg(C_MUTED)),
            Span::styled("r", Style::new().fg(C_NEON)),
            Span::styled(" · rename    ", Style::new().fg(C_MUTED)),
            Span::styled("Esc", Style::new().fg(C_NEON)),
            Span::styled(" · back    ", Style::new().fg(C_MUTED)),
            Span::styled("Ctrl+C", Style::new().fg(C_NEON)),
            Span::styled(" · quit", Style::new().fg(C_MUTED)),
        ],
        Screen::Models => vec![
            Span::styled("↑↓", Style::new().fg(C_NEON)),
            Span::styled(" · navigate    ", Style::new().fg(C_MUTED)),
            Span::styled("Enter", Style::new().fg(C_NEON)),
            Span::styled(" · assign    ", Style::new().fg(C_MUTED)),
            Span::styled("Esc", Style::new().fg(C_NEON)),
            Span::styled(" · back    ", Style::new().fg(C_MUTED)),
            Span::styled("Ctrl+C", Style::new().fg(C_NEON)),
            Span::styled(" · quit", Style::new().fg(C_MUTED)),
        ],
        Screen::Downloads if app.downloading => vec![
            Span::styled("Ctrl+C", Style::new().fg(C_NEON)),
            Span::styled(" · quit", Style::new().fg(C_MUTED)),
        ],
        Screen::Downloads if app.hf_search_active => vec![
            Span::styled("Enter", Style::new().fg(C_NEON)),
            Span::styled(" · search / download    ", Style::new().fg(C_MUTED)),
            Span::styled("↑↓", Style::new().fg(C_NEON)),
            Span::styled(" · navigate    ", Style::new().fg(C_MUTED)),
            Span::styled("Esc", Style::new().fg(C_NEON)),
            Span::styled(" · cancel    ", Style::new().fg(C_MUTED)),
            Span::styled("Ctrl+C", Style::new().fg(C_NEON)),
            Span::styled(" · quit", Style::new().fg(C_MUTED)),
        ],
        Screen::Downloads => vec![
            Span::styled("↑↓", Style::new().fg(C_NEON)),
            Span::styled(" · navigate    ", Style::new().fg(C_MUTED)),
            Span::styled("/", Style::new().fg(C_NEON)),
            Span::styled(" · search HF    ", Style::new().fg(C_MUTED)),
            Span::styled("c", Style::new().fg(C_NEON)),
            Span::styled(" · clone repo    ", Style::new().fg(C_MUTED)),
            Span::styled("Enter", Style::new().fg(C_NEON)),
            Span::styled(" · detail    ", Style::new().fg(C_MUTED)),
            Span::styled("Tab", Style::new().fg(C_NEON)),
            Span::styled(" · apps    ", Style::new().fg(C_MUTED)),
            Span::styled("Ctrl+C", Style::new().fg(C_NEON)),
            Span::styled(" · quit", Style::new().fg(C_MUTED)),
        ],
        Screen::FineTune if app.finetune_running || app.merge_running || app.gguf_running => vec![
            Span::styled("Ctrl+C", Style::new().fg(C_NEON)),
            Span::styled(" · quit", Style::new().fg(C_MUTED)),
        ],
        Screen::FineTune
            if matches!(
                &app.finetune_progress,
                Some(crate::finetune::FineTuneProgress::Done { .. })
            ) =>
        {
            let mut keys = vec![
                Span::styled("m", Style::new().fg(C_NEON)),
                Span::styled(" · merge    ", Style::new().fg(C_MUTED)),
            ];
            if app.merged_model_dir.is_some() {
                keys.push(Span::styled("g", Style::new().fg(C_NEON)));
                keys.push(Span::styled(" · export GGUF    ", Style::new().fg(C_MUTED)));
            }
            keys.push(Span::styled("Esc", Style::new().fg(C_NEON)));
            keys.push(Span::styled(" · back    ", Style::new().fg(C_MUTED)));
            keys.push(Span::styled("Ctrl+C", Style::new().fg(C_NEON)));
            keys.push(Span::styled(" · quit", Style::new().fg(C_MUTED)));
            keys
        }
        Screen::FineTune => vec![
            Span::styled("Tab", Style::new().fg(C_NEON)),
            Span::styled(" · next field    ", Style::new().fg(C_MUTED)),
            Span::styled("Enter", Style::new().fg(C_NEON)),
            Span::styled(" · start    ", Style::new().fg(C_MUTED)),
            Span::styled("Esc", Style::new().fg(C_NEON)),
            Span::styled(" · back    ", Style::new().fg(C_MUTED)),
            Span::styled("Ctrl+C", Style::new().fg(C_NEON)),
            Span::styled(" · quit", Style::new().fg(C_MUTED)),
        ],
        Screen::ModelDetail => {
            let mut keys = Vec::new();
            if !app.adapter_list.is_empty() {
                keys.push(Span::styled("Enter", Style::new().fg(C_NEON)));
                keys.push(Span::styled(
                    " · merge & export    ",
                    Style::new().fg(C_MUTED),
                ));
            }
            keys.push(Span::styled("f", Style::new().fg(C_NEON)));
            keys.push(Span::styled(" · fine-tune    ", Style::new().fg(C_MUTED)));
            keys.push(Span::styled("Esc", Style::new().fg(C_NEON)));
            keys.push(Span::styled(" · back    ", Style::new().fg(C_MUTED)));
            keys.push(Span::styled("Ctrl+C", Style::new().fg(C_NEON)));
            keys.push(Span::styled(" · quit", Style::new().fg(C_MUTED)));
            keys
        }
        Screen::GgufDetail if app.upload_running => vec![
            Span::styled("Ctrl+C", Style::new().fg(C_NEON)),
            Span::styled(" · quit", Style::new().fg(C_MUTED)),
        ],
        Screen::GgufDetail => vec![
            Span::styled("Enter", Style::new().fg(C_NEON)),
            Span::styled(" · upload    ", Style::new().fg(C_MUTED)),
            Span::styled("Esc", Style::new().fg(C_NEON)),
            Span::styled(" · back    ", Style::new().fg(C_MUTED)),
            Span::styled("Ctrl+C", Style::new().fg(C_NEON)),
            Span::styled(" · quit", Style::new().fg(C_MUTED)),
        ],
        Screen::CloneRepo if app.clone_repo_checking => vec![
            Span::styled("Ctrl+C", Style::new().fg(C_NEON)),
            Span::styled(" · quit", Style::new().fg(C_MUTED)),
        ],
        Screen::CloneRepo
            if matches!(
                &app.clone_repo_status,
                Some(crate::hf_clone::RepoStatus::Empty { .. })
            ) =>
        {
            vec![
                Span::styled("↑↓", Style::new().fg(C_NEON)),
                Span::styled(" · navigate    ", Style::new().fg(C_MUTED)),
                Span::styled("Enter", Style::new().fg(C_NEON)),
                Span::styled(" · select & download    ", Style::new().fg(C_MUTED)),
                Span::styled("Esc", Style::new().fg(C_NEON)),
                Span::styled(" · back    ", Style::new().fg(C_MUTED)),
                Span::styled("Ctrl+C", Style::new().fg(C_NEON)),
                Span::styled(" · quit", Style::new().fg(C_MUTED)),
            ]
        }
        Screen::CloneRepo => vec![
            Span::styled("Enter", Style::new().fg(C_NEON)),
            Span::styled(" · check repo    ", Style::new().fg(C_MUTED)),
            Span::styled("Esc", Style::new().fg(C_NEON)),
            Span::styled(" · back    ", Style::new().fg(C_MUTED)),
            Span::styled("Ctrl+C", Style::new().fg(C_NEON)),
            Span::styled(" · quit", Style::new().fg(C_MUTED)),
        ],
    };

    frame.render_widget(
        Paragraph::new(Line::from(keys)).alignment(Alignment::Center),
        area,
    );
}
