use crate::adapters::ollama::list_models;
use crate::core::heuristics::{estimate_usage, Estimation, Model, ModelSource};
use crate::core::system::{detect, SystemInfo};
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;

pub struct App {
    pub system: SystemInfo,
    pub models: Vec<Model>,
    pub filtered_models: Vec<Model>,
    pub search_query: String,
    pub selected_index: usize,

    // Scenario settings
    pub batch_size: usize,
    pub context_length: usize,
    pub quant_override: String, // "Original" or specific

    pub current_estimation: Option<Estimation>,
    pub is_searching: bool,

    // Input Popup State
    pub show_input: bool,
    pub input_buffer: String,
    pub input_cursor_position: usize,

    pub matcher: SkimMatcherV2,

    pub notification: Option<(String, std::time::Instant)>,
}

impl App {
    pub fn new() -> Self {
        let system = detect();
        let mut models = list_models();

        // Removed the "No local models" placeholder logic to keep list clean
        // if models.is_empty() { ... }

        let filtered = models.clone();

        let mut app = App {
            system,
            models,
            filtered_models: filtered,
            search_query: String::new(),
            selected_index: 0,
            batch_size: 1,
            context_length: 4096,
            quant_override: "Original".to_string(),
            current_estimation: None,
            is_searching: false,
            show_input: false,
            input_buffer: String::new(),
            input_cursor_position: 0,
            matcher: SkimMatcherV2::default(),
            notification: None,
        };
        app.recalculate();
        app
    }

    pub fn on_tick(&mut self) {
        // Clear notification after 3 seconds
        if let Some((_, time)) = self.notification {
            if time.elapsed() > std::time::Duration::from_secs(3) {
                self.notification = None;
            }
        }
    }

    pub fn recalculate(&mut self) {
        if self.filtered_models.is_empty() {
            self.current_estimation = None;
            return;
        }
        if self.selected_index >= self.filtered_models.len() {
            self.selected_index = 0;
        }

        let mut model = self.filtered_models[self.selected_index].clone();

        // Apply quant override if not "Original"
        if self.quant_override != "Original" {
            model.quant = self.quant_override.clone();
        }

        let estimation = estimate_usage(&model, &self.system, self.context_length, self.batch_size);
        self.current_estimation = Some(estimation);
    }

    pub fn toggle_input(&mut self) {
        self.show_input = !self.show_input;
        if self.show_input {
            self.is_searching = false; // Disable search if opening input
            self.input_buffer.clear();
            self.input_cursor_position = 0;
        }
    }

    pub fn submit_input(&mut self) {
        if self.input_buffer.trim().is_empty() {
            self.show_input = false;
            return;
        }

        use crate::adapters::hf::parse_hf_model_id;
        use crate::core::heuristics::{parse_model_string, ModelSource};

        let mut input = self.input_buffer.trim().to_string();

        // 1. Clean URL
        if input.contains("://") {
            if let Some(pos) = input.find("://") {
                input = input[pos + 3..].to_string();
            }
        }

        // 2. Identify Source & Extract Name
        let new_model = if input.contains("ollama.com/library/") {
            // e.g. ollama.com/library/deepseek-r1 -> deepseek-r1
            let name = input.split("ollama.com/library/").last().unwrap_or(&input);
            let clean_name = name.split('?').next().unwrap_or(name); // remove query params

            let mut m = parse_model_string(clean_name, ModelSource::Manual);
            m.name = format!("[O] {}", clean_name);
            m.source = ModelSource::Ollama;
            m
        } else if input.contains("huggingface.co/") {
            // e.g. huggingface.co/TheBloke/Llama-2-7B-Chat-GGUF
            let name_part = input.split("huggingface.co/").last().unwrap_or(&input);
            let clean_name = name_part.split("/tree/").next().unwrap_or(name_part); // handle tree views

            let mut m = parse_hf_model_id(clean_name);
            m.source = ModelSource::HuggingFace;
            m.name = format!("[HF] {}", m.name);
            m
        } else if input.contains('/') {
            // Assume HF ID like "TheBloke/Llama-2"
            let mut m = parse_hf_model_id(&input);
            m.source = ModelSource::HuggingFace;
            m.name = format!("[HF] {}", m.name);
            m
        } else {
            // Assume Ollama Tag or simple Name
            let mut m = parse_model_string(&input, ModelSource::Manual);
            m.name = format!("[M] {}", m.name);
            m
        };

        // Add to models list
        self.models.insert(0, new_model); // Add to top

        // Reset filters
        self.search_query.clear();
        self.filtered_models = self.models.clone();
        self.selected_index = 0;
        self.recalculate();

        self.show_input = false;
    }

    // ... existing search methods ...

    pub fn update_search(&mut self, query: String) {
        self.search_query = query;
        if self.search_query.is_empty() {
            self.filtered_models = self.models.clone();
        } else {
            let mut scored_models: Vec<(i64, Model)> = self
                .models
                .iter()
                .filter_map(|m| {
                    self.matcher
                        .fuzzy_match(&m.name, &self.search_query)
                        .map(|score| (score, m.clone()))
                })
                .collect();

            scored_models.sort_by(|a, b| b.0.cmp(&a.0));
            self.filtered_models = scored_models.into_iter().map(|(_, m)| m).collect();
        }
        self.selected_index = 0;
        self.recalculate();
    }

    pub fn next_model(&mut self) {
        if self.filtered_models.is_empty() {
            return;
        }
        if self.selected_index < self.filtered_models.len() - 1 {
            self.selected_index += 1;
        }
        self.recalculate();
    }

    pub fn prev_model(&mut self) {
        if self.filtered_models.is_empty() {
            return;
        }
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
        self.recalculate();
    }

    pub fn export_report(&mut self) -> std::io::Result<()> {
        if let Some(est) = &self.current_estimation {
            if self.filtered_models.is_empty() {
                return Ok(());
            }
            let model = &self.filtered_models[self.selected_index];

            // JSON
            let json_output = serde_json::json!({
                "model": model,
                "system": self.system,
                "estimation": est
            });
            std::fs::write(
                "modelfit_report.json",
                serde_json::to_string_pretty(&json_output)?,
            )?;

            // Markdown
            let md_output = format!(
                "# Model Fit Report: {}\n\n## System\n- CPU: {}\n- GPU: {} ({:.1} GB)\n- RAM: {:.1} GB\n\n## Estimation\n- Status: {:?}\n- VRAM Usage: {:.2} GB\n- RAM Usage: {:.2} GB\n- Est. Speed: {:.2} tok/s\n- Est. TTFT: {:.0} ms\n\n## Recommendation\n{}\n",
                model.name,
                self.system.cpu_name,
                self.system.gpu.name,
                self.system.gpu.vram_total_bytes as f64 / 1e9,
                self.system.ram_available_bytes as f64 / 1e9,
                est.vram_status,
                est.vram_usage_bytes as f64 / 1e9,
                est.ram_usage_bytes as f64 / 1e9,
                est.tokens_per_sec,
                est.ttft_ms,
                est.recommendation
            );
            std::fs::write("modelfit_report.md", md_output)?;

            self.notification = Some((
                "Report exported successfully!".to_string(),
                std::time::Instant::now(),
            ));
        }
        Ok(())
    }
}
