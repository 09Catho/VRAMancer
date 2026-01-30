use crate::core::heuristics::FitStatus;
use crate::ui::app::App;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    symbols,
    text::{Line, Span},
    widgets::{
        Axis, Block, BorderType, Borders, Chart, Dataset, Gauge, GraphType, List, ListItem,
        Paragraph, Wrap,
    },
    Frame,
};
use std::sync::OnceLock;

// --- THEME DEFINITIONS ---

// "Void" - Deepest background
const C_VOID: Color = Color::Rgb(9, 9, 11);
// "Deep Hull" - Panel background
const C_HULL: Color = Color::Rgb(18, 18, 23);
// "Cyber Cyan" - Primary accents
const C_CYAN: Color = Color::Rgb(0, 243, 255);
// "Flux Violet" - Secondary accents
const C_VIOLET: Color = Color::Rgb(189, 0, 255);
// "Solar Amber" - Highlights/Selection
const C_AMBER: Color = Color::Rgb(255, 184, 0);
// "Drive Green" - Success
const C_GREEN: Color = Color::Rgb(0, 255, 157);
// "Core Red" - Error
const C_RED: Color = Color::Rgb(255, 42, 109);
// "Starlight" - Main text
const C_TEXT: Color = Color::Rgb(224, 224, 224);
// "Dust" - Dim text
const C_DIM: Color = Color::Rgb(82, 82, 91);

// --- STARFIELD GENERATOR ---
// Generates a deterministic static noise pattern
fn get_starfield(width: u16, height: u16) -> String {
    // Deterministic pseudo-random chars based on coordinates
    let mut s = String::with_capacity((width as usize + 1) * height as usize);
    let glyphs = ['·', '✦', '✧', '⋆', '∘', ' '];

    for y in 0..height {
        for x in 0..width {
            // Simple hash:
            // ((x * large_prime) ^ (y * other_large_prime)) % range
            let n = (x as u32)
                .wrapping_mul(374761393)
                .wrapping_add((y as u32).wrapping_mul(668265263));
            // Mix bits
            let n = (n ^ (n >> 13)).wrapping_mul(127412495);

            if n % 53 == 0 {
                // ~2% density
                let idx = (n as usize) % (glyphs.len() - 1);
                s.push(glyphs[idx]);
            } else {
                s.push(' ');
            }
        }
        s.push('\n');
    }
    s
}

pub fn render_ui(f: &mut Frame, app: &mut App) {
    let size = f.size();

    // 1. Render Background (Starfield)
    // We render this over the entire screen first.
    let starfield = get_starfield(size.width, size.height);
    let bg_p =
        Paragraph::new(starfield).style(Style::default().fg(Color::Rgb(40, 40, 50)).bg(C_VOID));
    f.render_widget(bg_p, size);

    // 2. Render Notifications (Overlay)
    if let Some((msg, _)) = &app.notification {
        let area = Rect::new(size.width.saturating_sub(50) / 2, 2, 50, 3);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .style(Style::default().fg(C_GREEN).bg(C_HULL));
        let p = Paragraph::new(msg.as_str())
            .block(block)
            .alignment(Alignment::Center);
        f.render_widget(ratatui::widgets::Clear, area);
        f.render_widget(p, area);
    }

    // 3. Main Layout
    // We add a margin so the starfield frames the app
    let main_area = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Length(1), // Top Bar (Logo + System)
                Constraint::Min(0),    // Main Content
                Constraint::Length(1), // Footer (Keys)
            ]
            .as_ref(),
        )
        .margin(1)
        .split(size);

    render_header(f, app, main_area[0]);
    render_main(f, app, main_area[1]);
    render_footer(f, app, main_area[2]);
}

fn render_header(f: &mut Frame, app: &App, area: Rect) {
    let sys = &app.system;

    // Style: " LOGO  |  GPU  |  RAM  |  CPU "
    // We use spans to color-code

    let _gpu_color = if sys.gpu.vram_total_bytes > 16_000_000_000 {
        C_CYAN
    } else {
        C_TEXT
    };

    let line = Line::from(vec![
        Span::styled(
            " ✦ VRAMANCER ",
            Style::default().fg(C_VIOLET).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" :: ", Style::default().fg(C_DIM)),
        Span::styled(
            format!("{}", sys.gpu.name),
            Style::default().fg(gpu_text_color(&sys.gpu.name)),
        ),
        Span::styled(
            format!(" [{:.1}GB]", sys.gpu.vram_total_bytes as f64 / 1e9),
            Style::default().fg(C_CYAN),
        ),
        Span::styled(" :: ", Style::default().fg(C_DIM)),
        Span::raw("RAM "),
        Span::styled(
            format!("{:.1}GB", sys.ram_available_bytes as f64 / 1e9),
            Style::default().fg(C_TEXT),
        ),
        Span::styled("/", Style::default().fg(C_DIM)),
        Span::styled(
            format!("{:.1}GB", sys.ram_total_bytes as f64 / 1e9),
            Style::default().fg(C_DIM),
        ),
    ]);

    // We render this without a block, just floating text on the stars (or with a bg if needed for readability)
    // To ensure readability, let's put a subtle background behind the header line?
    // Actually, design said "Mask stars behind panels". Header is small, let's keep it transparent or minimal.
    // Let's use a Paragraph with no block.

    f.render_widget(Paragraph::new(line).alignment(Alignment::Center), area);
}

fn gpu_text_color(name: &str) -> Color {
    if name.to_lowercase().contains("nvidia") {
        Color::Green
    } else if name.to_lowercase().contains("amd") {
        Color::Red
    } else if name.to_lowercase().contains("apple") {
        Color::White
    } else {
        C_CYAN
    }
}

fn render_main(f: &mut Frame, app: &mut App, area: Rect) {
    // 3-Column Layout: Models (25%) | Prediction (45%) | Config (30%)
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

    render_models_panel(f, app, chunks[0]);
    render_prediction_panel(f, app, chunks[1]);
    render_config_panel(f, app, chunks[2]);

    if app.show_input {
        render_input_popup(f, app);
    }
}

// --- LEFT PANEL: MODELS ---
fn render_models_panel(f: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(C_DIM))
        .title(Span::styled(" M O D E L S ", Style::default().fg(C_VIOLET)))
        .title_alignment(Alignment::Center)
        .bg(C_HULL); // Mask the stars

    let items: Vec<ListItem> = app
        .filtered_models
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let is_selected = i == app.selected_index;

            // Selection Indicator
            let prefix = if is_selected { "▍ " } else { "  " };
            let name_style = if is_selected {
                Style::default().fg(C_AMBER).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(C_TEXT)
            };
            let info_style = if is_selected {
                Style::default().fg(C_AMBER)
            } else {
                Style::default().fg(C_DIM)
            };

            let content = Line::from(vec![
                Span::styled(prefix, Style::default().fg(C_AMBER)),
                Span::styled(m.name.clone(), name_style),
                Span::styled(format!("  {}", m.size_label), info_style),
            ]);

            ListItem::new(content)
        })
        .collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(Style::default().bg(Color::Rgb(25, 25, 30))); // Subtle highlight bar

    let mut state = ratatui::widgets::ListState::default();
    state.select(Some(app.selected_index));
    f.render_stateful_widget(list, area, &mut state);
}

// --- CENTER PANEL: PREDICTION ---
fn render_prediction_panel(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if app.current_estimation.is_some() {
            C_CYAN
        } else {
            C_DIM
        }))
        .title(Span::styled(
            " A N A L Y S I S ",
            Style::default().fg(C_CYAN),
        ))
        .title_alignment(Alignment::Center)
        .bg(C_HULL);

    f.render_widget(block.clone(), area);
    let inner = block.inner(area);

    if let Some(est) = &app.current_estimation {
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints(
                [
                    Constraint::Length(3), // Status Header
                    Constraint::Length(1), // Spacer
                    Constraint::Length(3), // VRAM Bar
                    Constraint::Length(3), // RAM Bar
                    Constraint::Length(1), // Spacer
                    Constraint::Length(2), // Metrics Text
                    Constraint::Min(0),    // Chart
                ]
                .as_ref(),
            )
            .margin(1)
            .split(inner);

        // 1. Status Header
        let (color, text, icon) = match est.vram_status {
            FitStatus::Fits => (C_GREEN, "SYSTEM OPTIMAL", "✓"),
            FitStatus::Partial(_) => (C_AMBER, "OFFLOADING REQUIRED", "⚠"),
            FitStatus::No => (C_RED, "CRITICAL: INSUFFICIENT VRAM", "✕"),
        };

        let status = Paragraph::new(vec![
            Line::from(vec![
                Span::styled(format!("{} ", icon), Style::default().fg(color)),
                Span::styled(
                    text,
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(Span::styled(
                est.recommendation.clone(),
                Style::default().fg(C_TEXT),
            )),
        ])
        .alignment(Alignment::Center);
        f.render_widget(status, layout[0]);

        // 2. VRAM Gauge
        let vram_ratio =
            (est.vram_usage_bytes as f64 / app.system.gpu.vram_total_bytes as f64).min(1.0);
        let vram_label = format!(
            "{:.1}GB / {:.1}GB",
            est.vram_usage_bytes as f64 / 1e9,
            app.system.gpu.vram_total_bytes as f64 / 1e9
        );

        let vram_gauge = Gauge::default()
            .block(
                Block::default()
                    .title("VRAM USAGE")
                    .title_style(Style::default().fg(C_DIM).add_modifier(Modifier::BOLD)),
            )
            .gauge_style(
                Style::default()
                    .fg(if vram_ratio > 0.9 { C_RED } else { C_CYAN })
                    .bg(Color::Rgb(30, 30, 35)),
            )
            .ratio(vram_ratio)
            .label(vram_label)
            .use_unicode(true);
        f.render_widget(vram_gauge, layout[2]);

        // 3. RAM Gauge
        let ram_ratio =
            (est.ram_usage_bytes as f64 / app.system.ram_available_bytes as f64).min(1.0);
        let ram_label = if est.ram_usage_bytes > 0 {
            format!("{:.1}GB (Offload)", est.ram_usage_bytes as f64 / 1e9)
        } else {
            "0.0 GB".to_string()
        };
        let ram_gauge = Gauge::default()
            .block(
                Block::default()
                    .title("RAM SPILLOVER")
                    .title_style(Style::default().fg(C_DIM).add_modifier(Modifier::BOLD)),
            )
            .gauge_style(Style::default().fg(C_VIOLET).bg(Color::Rgb(30, 30, 35)))
            .ratio(ram_ratio)
            .label(ram_label)
            .use_unicode(true);
        f.render_widget(ram_gauge, layout[3]);

        // 4. Metrics Text
        let tps_color = if est.tokens_per_sec > 10.0 {
            C_GREEN
        } else if est.tokens_per_sec > 5.0 {
            C_AMBER
        } else {
            C_RED
        };

        let metrics_text = Paragraph::new(vec![
            Line::from(vec![
                Span::styled("SPEED: ", Style::default().fg(C_DIM)),
                Span::styled(
                    format!("{:.1} t/s", est.tokens_per_sec),
                    Style::default().fg(tps_color).add_modifier(Modifier::BOLD),
                ),
                Span::raw("   "),
                Span::styled("TTFT: ", Style::default().fg(C_DIM)),
                Span::styled(
                    format!("{:.0} ms", est.ttft_ms),
                    Style::default().fg(C_TEXT).add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::styled("NOTES: ", Style::default().fg(C_DIM)),
                Span::styled(
                    est.notes.first().cloned().unwrap_or_default(),
                    Style::default().fg(C_DIM),
                ),
            ]),
        ]);
        f.render_widget(metrics_text, layout[5]);

        // 5. Chart
        let metrics_area = layout[6];

        // Simple Chart showing VRAM scaling with context
        // X: 1k to 32k
        // Y: VRAM in GB

        let mut data = Vec::new();
        let mut limit_line = Vec::new();
        let total_vram_gb = app.system.gpu.vram_total_bytes as f64 / 1e9;

        for ctx in (1024..=32768).step_by(1024) {
            // Rough calc using same heuristic
            let e = crate::core::heuristics::estimate_usage(
                &app.filtered_models[app.selected_index],
                &app.system,
                ctx,
                app.batch_size,
            );
            data.push((ctx as f64, e.vram_usage_bytes as f64 / 1e9));
            limit_line.push((ctx as f64, total_vram_gb));
        }

        let datasets = vec![
            Dataset::default()
                .name("Limit")
                .marker(symbols::Marker::Braille)
                .style(Style::default().fg(C_RED))
                .data(&limit_line),
            Dataset::default()
                .name("VRAM")
                .marker(symbols::Marker::Braille)
                .style(Style::default().fg(C_CYAN))
                .graph_type(GraphType::Line)
                .data(&data),
        ];

        let chart = Chart::new(datasets)
            .block(
                Block::default()
                    .title("SCALING (VRAM vs Context)")
                    .title_style(Style::default().fg(C_DIM)),
            )
            .x_axis(
                Axis::default()
                    .style(Style::default().fg(C_DIM))
                    .bounds([1024.0, 32768.0])
                    .labels(vec![
                        Span::styled("1k", Style::default().fg(C_DIM)),
                        Span::styled("16k", Style::default().fg(C_DIM)),
                        Span::styled("32k", Style::default().fg(C_DIM)),
                    ]),
            )
            .y_axis(
                Axis::default()
                    .style(Style::default().fg(C_DIM))
                    .bounds([0.0, (total_vram_gb * 1.5).max(10.0)])
                    .labels(vec![
                        Span::styled("0", Style::default().fg(C_DIM)),
                        Span::styled(format!("{:.0}G", total_vram_gb), Style::default().fg(C_RED)),
                    ]),
            );

        f.render_widget(chart, metrics_area);
    } else {
        let p = Paragraph::new("SELECT MODEL FOR TELEMETRY")
            .style(Style::default().fg(C_DIM))
            .alignment(Alignment::Center);
        // vertically center?
        let v_center = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
            .split(inner);
        f.render_widget(p, v_center[0]);
    }
}

// --- RIGHT PANEL: CONFIG ---
fn render_config_panel(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(C_DIM))
        .title(Span::styled(" C O N F I G ", Style::default().fg(C_DIM)))
        .title_alignment(Alignment::Center)
        .bg(C_HULL);

    f.render_widget(block.clone(), area);
    let inner = block.inner(area).inner(&ratatui::layout::Margin {
        vertical: 1,
        horizontal: 1,
    });

    let lines = vec![
        Line::from(Span::styled("CONTEXT LEN", Style::default().fg(C_DIM))),
        Line::from(Span::styled(
            format!("{} tk", app.context_length),
            Style::default().fg(C_CYAN).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled("BATCH SIZE", Style::default().fg(C_DIM))),
        Line::from(Span::styled(
            format!("{}", app.batch_size),
            Style::default().fg(C_CYAN).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled("QUANTIZATION", Style::default().fg(C_DIM))),
        Line::from(Span::styled(
            app.quant_override.clone(),
            Style::default().fg(C_VIOLET).add_modifier(Modifier::BOLD),
        )),
    ];

    f.render_widget(Paragraph::new(lines), inner);
}

// --- POPUPS & FOOTER ---

fn render_footer(f: &mut Frame, app: &App, area: Rect) {
    // Style: "[ KEY ] Action  [ KEY ] Action"
    let keys = if app.show_input {
        vec![
            Span::styled(" ENTER ", Style::default().fg(C_HULL).bg(C_AMBER)),
            Span::styled(" Submit ", Style::default().fg(C_DIM)),
            Span::raw("  "),
            Span::styled(" ESC ", Style::default().fg(C_HULL).bg(C_TEXT)),
            Span::styled(" Cancel ", Style::default().fg(C_DIM)),
        ]
    } else {
        vec![
            Span::styled(" / ", Style::default().fg(C_HULL).bg(C_CYAN)),
            Span::styled(" Search ", Style::default().fg(C_DIM)),
            Span::raw("  "),
            Span::styled(" i ", Style::default().fg(C_HULL).bg(C_VIOLET)),
            Span::styled(" Add Model ", Style::default().fg(C_DIM)),
            Span::raw("  "),
            Span::styled(" TAB ", Style::default().fg(C_HULL).bg(C_TEXT)),
            Span::styled(" Config ", Style::default().fg(C_DIM)),
            Span::raw("  "),
            Span::styled(" E ", Style::default().fg(C_HULL).bg(C_TEXT)),
            Span::styled(" Export ", Style::default().fg(C_DIM)),
            Span::raw("  "),
            Span::styled(" Q ", Style::default().fg(C_HULL).bg(C_RED)),
            Span::styled(" Quit ", Style::default().fg(C_DIM)),
        ]
    };

    f.render_widget(
        Paragraph::new(Line::from(keys)).alignment(Alignment::Center),
        area,
    );
}

fn render_input_popup(f: &mut Frame, app: &App) {
    let area = centered_rect(50, 20, f.size()); // Smaller, tighter popup

    // Clear area
    f.render_widget(ratatui::widgets::Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double) // Distinction
        .border_style(Style::default().fg(C_AMBER))
        .title(" INPUT MODEL SOURCE ")
        .bg(C_HULL);

    let text = Paragraph::new(format!("> {}_", app.input_buffer)) // Manual cursor
        .block(block)
        .style(Style::default().fg(C_AMBER));

    f.render_widget(text, area);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Percentage((100 - percent_y) / 2),
                Constraint::Percentage(percent_y),
                Constraint::Percentage((100 - percent_y) / 2),
            ]
            .as_ref(),
        )
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints(
            [
                Constraint::Percentage((100 - percent_x) / 2),
                Constraint::Percentage(percent_x),
                Constraint::Percentage((100 - percent_x) / 2),
            ]
            .as_ref(),
        )
        .split(popup_layout[1])[1]
}
