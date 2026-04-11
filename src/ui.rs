use {
    crate::app::{App, Focus, Mode, Profile, StatusTone},
    ratatui::{
        Frame,
        layout::{Alignment, Constraint, Layout},
        style::{Color, Style, Stylize},
        text::{Line, Span},
        widgets::{Block, BorderType, Borders, Padding, Paragraph, Wrap},
    },
};

// theme — colours pulled directly from globals.css

const C_BG: Color = Color::Rgb(0, 0, 0);
const C_SURFACE: Color = Color::Rgb(13, 20, 16);
const C_SURFACE_STRONG: Color = Color::Rgb(20, 28, 24);
const C_NEON: Color = Color::Rgb(66, 255, 145);
const C_TEXT: Color = Color::Rgb(226, 226, 226);
const C_MUTED: Color = Color::Rgb(122, 144, 128);
const C_INK: Color = Color::Rgb(216, 229, 222);
const C_DANGER: Color = Color::Rgb(255, 95, 86);
const C_LINE: Color = Color::Rgb(35, 50, 42);

// render

pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();

    // Full-screen obsidian background
    frame.render_widget(Block::new().style(Style::new().bg(C_BG)), area);

    let layout = Layout::vertical([
        Constraint::Length(3), // header
        Constraint::Min(0),    // card
        Constraint::Length(1), // footer
    ])
    .split(area);

    render_header(frame, layout[0]);
    render_card(frame, app, layout[1]);
    render_footer(frame, app, layout[2]);
}

// header

fn render_header(frame: &mut Frame, area: ratatui::layout::Rect) {
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

// card

fn render_card(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    // Centre the card horizontally (max 64 wide, minimum terminal width − 4)
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

    if let Some(profile) = &app.profile {
        render_session(frame, app, profile, inner);
    } else {
        render_form(frame, app, inner);
    }
}

// auth form

fn render_form(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let rows = Layout::vertical([
        Constraint::Length(1), // tabs
        Constraint::Length(1), // spacer
        Constraint::Length(1), // headline
        Constraint::Length(2), // description
        Constraint::Length(1), // spacer
        Constraint::Length(1), // status
        Constraint::Length(1), // spacer
        Constraint::Length(1), // email label
        Constraint::Length(3), // email input
        Constraint::Length(1), // spacer
        Constraint::Length(1), // password label
        Constraint::Length(3), // password input
        Constraint::Length(1), // spacer
        Constraint::Length(1), // primary button
        Constraint::Length(1), // secondary hint
        Constraint::Min(0),    // remainder
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

    // Email
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

    // Password
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

    // Buttons
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

// mode tabs

fn render_tabs(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let cols = Layout::horizontal([
        Constraint::Length(19), // signup tab
        Constraint::Length(1),  // gap
        Constraint::Length(11), // signin tab
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

// input field

fn render_input(
    frame: &mut Frame,
    app: &App,
    value: &str,
    field: Focus,
    placeholder: &str,
    area: ratatui::layout::Rect,
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

// status bar

fn render_status(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
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

// session view — shown after sign-in

fn render_session(frame: &mut Frame, app: &App, profile: &Profile, area: ratatui::layout::Rect) {
    let rows = Layout::vertical([
        Constraint::Length(1), // signed-in badge
        Constraint::Length(1), // spacer
        Constraint::Length(1), // email
        Constraint::Length(1), // spacer
        Constraint::Length(2), // beta message
        Constraint::Length(1), // spacer
        Constraint::Length(1), // status
        Constraint::Length(1), // spacer
        Constraint::Length(1), // sign-out button
        Constraint::Min(0),
    ])
    .split(area);

    frame.render_widget(
        Paragraph::new("✓ Signed in").style(Style::new().fg(C_NEON).bold()),
        rows[0],
    );
    frame.render_widget(
        Paragraph::new(profile.email.as_str()).style(Style::new().fg(C_TEXT).bold()),
        rows[2],
    );
    frame.render_widget(
        Paragraph::new(
            "We are in private beta. Thanks for your interest.\nEmail work@setoelkahfi.se for questions.",
        )
        .style(Style::new().fg(C_MUTED))
        .wrap(Wrap { trim: true }),
        rows[4],
    );
    render_status(frame, app, rows[6]);
    frame.render_widget(
        Paragraph::new("[ Sign out ]  Enter").style(Style::new().fg(C_MUTED)),
        rows[8],
    );
}

// footer key hints

fn render_footer(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let keys: Vec<Span> = if app.profile.is_some() {
        vec![
            Span::styled("Enter", Style::new().fg(C_NEON)),
            Span::styled(" · sign out    ", Style::new().fg(C_MUTED)),
            Span::styled("Ctrl+C", Style::new().fg(C_NEON)),
            Span::styled(" · quit", Style::new().fg(C_MUTED)),
        ]
    } else {
        vec![
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
        ]
    };

    frame.render_widget(
        Paragraph::new(Line::from(keys)).alignment(Alignment::Center),
        area,
    );
}
