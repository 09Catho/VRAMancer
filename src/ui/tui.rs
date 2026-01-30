use crate::core::heuristics::FitStatus;
use crate::ui::app::App;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Gauge, List, ListItem, Paragraph, Wrap},
    Frame,
};

pub fn render_ui(f: &mut Frame, app: &mut App) {
    let size = f.size();

    // Vertical Layout: Header, Main, Footer
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Length(3), // Header
                Constraint::Min(0),    // Main
                Constraint::Length(1), // Footer
            ]
            .as_ref(),
        )
        .split(size);

    render_header(f, app, chunks[0]);
    render_main(f, app, chunks[1]);
    render_footer(f, app, chunks[2]);
}

fn render_header(f: &mut Frame, app: &App, area: Rect) {
    let sys = &app.system;
    let gpu_text = format!(
        "{} ({:.1} GB VRAM)",
        sys.gpu.name,
        sys.gpu.vram_total_bytes as f64 / 1e9
    );
    let ram_text = format!(
        "RAM: {:.1}/{:.1} GB",
        sys.ram_available_bytes as f64 / 1e9,
        sys.ram_total_bytes as f64 / 1e9
    );

    let info = format!(
        " {} | {} | {} | CPU: {} ({} cores) ",
        "VRAMancer", gpu_text, ram_text, sys.cpu_name, sys.cpu_cores
    );

    let paragraph = Paragraph::new(info)
        .style(Style::default().fg(Color::Cyan))
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_type(BorderType::Thick),
        );

    f.render_widget(paragraph, area);
}

fn render_main(f: &mut Frame, app: &mut App, area: Rect) {
    // Horizontal: Left (List), Middle (Stats), Right (Settings/Info)
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(
            [
                Constraint::Percentage(25),
                Constraint::Percentage(45),
                Constraint::Percentage(30),
            ]
            .as_ref(),
        )
        .split(area);

    render_models_list(f, app, chunks[0]);
    render_prediction(f, app, chunks[1]);
    render_settings(f, app, chunks[2]);
}

fn render_models_list(f: &mut Frame, app: &mut App, area: Rect) {
    let items: Vec<ListItem> = app
        .filtered_models
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let style = if i == app.selected_index {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            // Show name and size
            let content = format!("{} [{}]", m.name, m.size_label);
            ListItem::new(content).style(style)
        })
        .collect();

    let title = if app.search_query.is_empty() {
        " Models "
    } else {
        // " Models (Search: query) "
        // We can't format easily in title string without allocation, but it's fine.
        // Let's just put search in a block title
        " Models (Searching) "
    };

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_type(BorderType::Rounded),
        )
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));

    // We handle selection state manually in rendering via style above,
    // or we can use List::state but we are using App state index.
    // Actually List widget renders the passed list.
    // Scroll needs to be handled if list is long.
    // Since we just render a slice or the full list, `List` handles scrolling if we give it state.
    // For simplicity, let's just render. But without state, `List` shows from top.
    // We should implement scrolling.

    let mut state = ratatui::widgets::ListState::default();
    state.select(Some(app.selected_index));

    f.render_stateful_widget(list, area, &mut state);
}

fn render_prediction(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Prediction ")
        .border_type(BorderType::Rounded);
    f.render_widget(block.clone(), area);

    let inner_area = block.inner(area);

    if let Some(est) = &app.current_estimation {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(
                [
                    Constraint::Length(4), // Status
                    Constraint::Length(3), // VRAM Bar
                    Constraint::Length(3), // RAM Bar
                    Constraint::Min(0),    // Metrics
                ]
                .as_ref(),
            )
            .split(inner_area);

        // 1. Status Card
        let (status_color, status_text) = match &est.vram_status {
            FitStatus::Fits => (Color::Green, "YES - RUNS FAST".to_string()),
            FitStatus::Partial(s) => (Color::Yellow, format!("MAYBE - {}", s)),
            FitStatus::No => (Color::Red, "NO - INSUFFICIENT VRAM".to_string()),
        };

        let status_p = Paragraph::new(status_text)
            .style(
                Style::default()
                    .fg(status_color)
                    .add_modifier(Modifier::BOLD),
            )
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::BOTTOM));
        f.render_widget(status_p, chunks[0]);

        // 2. VRAM Usage
        let vram_ratio = est.vram_usage_bytes as f64 / app.system.gpu.vram_total_bytes as f64;
        let vram_label = format!(
            "{:.1} / {:.1} GB",
            est.vram_usage_bytes as f64 / 1e9,
            app.system.gpu.vram_total_bytes as f64 / 1e9
        );
        let vram_gauge = Gauge::default()
            .block(Block::default().title("VRAM Usage"))
            .gauge_style(Style::default().fg(if vram_ratio > 0.9 {
                Color::Red
            } else {
                Color::Cyan
            }))
            .ratio(vram_ratio.min(1.0))
            .label(vram_label);
        f.render_widget(vram_gauge, chunks[1]);

        // 3. RAM Usage (if any)
        if est.ram_usage_bytes > 0 {
            let ram_ratio = est.ram_usage_bytes as f64 / app.system.ram_available_bytes as f64;
            let ram_label = format!(
                "{:.1} / {:.1} GB (Offload)",
                est.ram_usage_bytes as f64 / 1e9,
                app.system.ram_available_bytes as f64 / 1e9
            );
            let ram_gauge = Gauge::default()
                .block(Block::default().title("System RAM Usage"))
                .gauge_style(Style::default().fg(Color::Magenta))
                .ratio(ram_ratio.min(1.0))
                .label(ram_label);
            f.render_widget(ram_gauge, chunks[2]);
        }

        // 4. Metrics
        let metrics_text = vec![
            Line::from(vec![
                Span::raw("Est. Speed: "),
                Span::styled(
                    format!("{:.1} tok/s", est.tokens_per_sec),
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::raw("Est. TTFT:  "),
                Span::styled(
                    format!("{:.0} ms", est.ttft_ms),
                    Style::default().fg(Color::Yellow),
                ),
            ]),
            Line::from(""),
            Line::from(vec![Span::styled(
                "Recommendation:",
                Style::default().add_modifier(Modifier::UNDERLINED),
            )]),
            Line::from(est.recommendation.clone()),
        ];

        let metrics_p = Paragraph::new(metrics_text)
            .block(Block::default().padding(ratatui::widgets::Padding::new(1, 1, 1, 1)))
            .wrap(Wrap { trim: true });
        f.render_widget(metrics_p, chunks[3]);
    } else {
        let p = Paragraph::new("Select a model to see estimates")
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::Gray));
        f.render_widget(p, inner_area);
    }
}

fn render_settings(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Scenario ")
        .border_type(BorderType::Rounded);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let text = vec![
        Line::from(vec![
            Span::styled("Context Length: ", Style::default().fg(Color::Blue)),
            Span::raw(format!("{}", app.context_length)),
        ]),
        Line::from(vec![
            Span::styled("Batch Size:     ", Style::default().fg(Color::Blue)),
            Span::raw(format!("{}", app.batch_size)),
        ]),
        Line::from(vec![
            Span::styled("Quantization:   ", Style::default().fg(Color::Blue)),
            Span::raw(&app.quant_override),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Assumptions:",
            Style::default().add_modifier(Modifier::UNDERLINED),
        )),
    ];

    let mut assumptions = text;
    if let Some(est) = &app.current_estimation {
        for note in &est.notes {
            assumptions.push(Line::from(format!("- {}", note)));
        }
    }

    let p = Paragraph::new(assumptions).wrap(Wrap { trim: true });
    f.render_widget(p, inner);
}

fn render_footer(f: &mut Frame, app: &App, area: Rect) {
    let keys = if !app.search_query.is_empty() {
        "ESC clear search | ENTER select"
    } else {
        "Q quit | / search | J/K navigate | TAB settings | +/- ctx | E export"
    };

    let p = Paragraph::new(keys)
        .style(Style::default().bg(Color::DarkGray).fg(Color::White))
        .alignment(Alignment::Center);
    f.render_widget(p, area);
}
