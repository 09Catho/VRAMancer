mod core;
mod adapters;
mod ui;

use clap::Parser;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{error::Error, io, time::Duration};
use crate::ui::app::App;
use crate::ui::tui::render_ui;
use crate::core::heuristics::{ModelSource, parse_model_string, estimate_usage};
use crate::core::system::detect;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Model name (e.g., llama3:8b)
    #[arg(short, long)]
    model: Option<String>,

    /// Context length
    #[arg(long, default_value_t = 4096)]
    ctx: usize,

    /// Batch size
    #[arg(long, default_value_t = 1)]
    batch: usize,

    /// Quantization override (e.g., q4_0, fp16)
    #[arg(long)]
    quant: Option<String>,

    /// Output JSON instead of TUI
    #[arg(long)]
    json: bool,

    /// Use HuggingFace lookup (online)
    #[arg(long)]
    hf: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    if args.json {
        run_cli(args);
        Ok(())
    } else {
        run_tui()
    }
}

fn run_cli(args: Args) {
    let sys = detect();

    let model_name = args.model.unwrap_or_else(|| "llama3:8b".to_string());
    let mut model = parse_model_string(&model_name, ModelSource::Manual);

    if let Some(q) = args.quant {
        model.quant = q;
    }

    let estimation = estimate_usage(&model, &sys, args.ctx, args.batch);

    let json_output = serde_json::json!({
        "model": model,
        "system": sys,
        "estimation": estimation
    });

    println!("{}", serde_json::to_string_pretty(&json_output).unwrap());
}

fn run_tui() -> Result<(), Box<dyn Error>> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app
    let mut app = App::new();

    // Main loop
    let res = run_app(&mut terminal, &mut app);

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{:?}", err);
    }

    Ok(())
}

fn run_app<B: ratatui::backend::Backend>(terminal: &mut Terminal<B>, app: &mut App) -> io::Result<()> {
    loop {
        terminal.draw(|f| render_ui(f, app))?;
        
        // Update background tasks (notifications)
        app.on_tick();

        if crossterm::event::poll(Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    // Global exit
                    if key.code == KeyCode::Char('c') && key.modifiers.contains(event::KeyModifiers::CONTROL) {
                         return Ok(());
                    }

                    if app.show_input {
                    match key.code {
                        KeyCode::Enter => app.submit_input(),
                        KeyCode::Esc => app.toggle_input(),
                        KeyCode::Backspace => {
                             app.input_buffer.pop();
                        }
                        KeyCode::Char(c) => {
                             app.input_buffer.push(c);
                        }
                        _ => {}
                    }
                }
                } else if app.is_searching {
                    match key.code {
                        KeyCode::Enter => {
                            app.is_searching = false;
                        }
                        KeyCode::Esc => {
                            app.is_searching = false;
                            // Optional: clear search on ESC?
                            // app.update_search(String::new());
                        }
                        KeyCode::Backspace => {
                            let mut q = app.search_query.clone();
                            q.pop();
                            app.update_search(q);
                        }
                        KeyCode::Char(c) => {
                            let mut q = app.search_query.clone();
                            q.push(c);
                            app.update_search(q);
                        }
                        _ => {}
                    }
                } else {
                    // Normal Navigation Mode
                    match key.code {
                        KeyCode::Char('q') => return Ok(()),
                        KeyCode::Char('i') => app.toggle_input(),
                        KeyCode::Char('e') => {
                            if let Err(e) = app.export_report() {
                                // In a real app we would show an error popup
                                eprintln!("Failed to export: {}", e);
                            }
                        }
                        KeyCode::Char('/') => {
                            app.is_searching = true;
                            // Clear previous search if user hits / again?
                            // app.update_search(String::new());
                        }
                        KeyCode::Down | KeyCode::Char('j') => app.next_model(),
                        KeyCode::Up | KeyCode::Char('k') => app.prev_model(),
                        KeyCode::Tab => {
                            // Cycle context length
                            if app.context_length == 4096 { app.context_length = 8192; }
                            else if app.context_length == 8192 { app.context_length = 32768; }
                            else if app.context_length == 32768 { app.context_length = 2048; }
                            else { app.context_length = 4096; }
                            app.recalculate();
                        }
                        KeyCode::Char('+') => {
                            app.context_length = app.context_length.saturating_add(1024);
                            app.recalculate();
                        }
                        KeyCode::Char('-') => {
                             app.context_length = app.context_length.saturating_sub(1024).max(512);
                             app.recalculate();
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}
