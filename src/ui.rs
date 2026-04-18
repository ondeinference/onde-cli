use {
    crate::app::{App, Focus, Mode, OndeApp, Screen, StatusTone},
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
    render_input(frame, app, &app.email, Focus::Email, "name@company.com", rows[8]);

    frame.render_widget(
        Paragraph::new("Password").style(Style::new().fg(C_MUTED)),
        rows[10],
    );
    let masked = "•".repeat(app.password.len());
    render_input(frame, app, &masked, Focus::Password, "Minimum 8 characters", rows[11]);

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
        Paragraph::new(" Create account ").style(if app.mode == Mode::Signup { active } else { inactive }),
        cols[0],
    );
    frame.render_widget(
        Paragraph::new(" Sign in ").style(if app.mode == Mode::Signin { active } else { inactive }),
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

    let cursor_x = (inner.x + value.chars().count() as u16)
        .min(inner.x + inner.width.saturating_sub(1));
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

fn render_apps(frame: &mut Frame, app: &App, area: Rect) {
    let email = app.profile.as_ref().map(|p| p.email.as_str()).unwrap_or("");

    let top = Layout::vertical([
        Constraint::Length(1), // profile badge
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

    render_status(frame, app, top[2]);

    frame.render_widget(
        Paragraph::new("  Name                   Status   Model")
            .style(Style::new().fg(C_MUTED)),
        top[4],
    );

    let divider = "─".repeat(area.width as usize);
    frame.render_widget(
        Paragraph::new(divider).style(Style::new().fg(C_LINE)),
        top[5],
    );

    let rest = top[6];
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
            Paragraph::new("Enter · create   Esc · cancel")
                .style(Style::new().fg(C_MUTED)),
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
            Paragraph::new("n · new   Enter · open   s · sign out")
                .style(Style::new().fg(C_MUTED)),
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
    onde_app
        .current_model_id
        .as_deref()
        .and_then(|id| app.models.iter().find(|m| m.id == id))
        .and_then(|m| m.name.as_deref())
        .unwrap_or("–")
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
            Paragraph::new("Enter · save   Esc · cancel")
                .style(Style::new().fg(C_MUTED)),
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

fn render_detail_field(frame: &mut Frame, label: &str, value: &str, label_area: Rect, value_area: Rect) {
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

    let cursor_x = (inner.x + value.chars().count() as u16)
        .min(inner.x + inner.width.saturating_sub(1));
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

fn fmt_bytes(bytes: i64) -> String {
    if bytes >= 1_000_000_000 {
        format!("{:.1}GB", bytes as f64 / 1e9)
    } else if bytes >= 1_000_000 {
        format!("{:.0}MB", bytes as f64 / 1e6)
    } else {
        format!("{:.0}KB", bytes as f64 / 1e3)
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
    };

    frame.render_widget(
        Paragraph::new(Line::from(keys)).alignment(Alignment::Center),
        area,
    );
}
