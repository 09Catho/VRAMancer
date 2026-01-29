use crate::core::heuristics::{Model, Estimation, estimate_usage, ModelSource};
use crate::core::system::{SystemInfo, detect};
use crate::adapters::ollama::list_models;
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;

#[derive(Debug, PartialEq)]
pub enum ActiveTab {
    Models,
    // Scenario is integrated into the main view, but maybe we want focus shifting?
    // Let's keep it simple: Arrow keys navigate models. Tab toggles settings focus?
    // For now, let's say "Search" is one mode, "List" is another?
    // User requirement: "First screen: pick source".
    // "Then: model picker with search".
    // "Then: scenario pane".
    // "Results update live".
    // Let's stick to a single main view.
}

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
    pub should_quit: bool,
    pub is_searching: bool,

    pub matcher: SkimMatcherV2,
}

impl App {
    pub fn new() -> Self {
        let system = detect();
        let mut models = list_models();

        // If no models found, add some examples or manual entry hints
        if models.is_empty() {
            models.push(Model {
                name: "No local models found (ollama list)".to_string(),
                family: "unknown".to_string(),
                size_label: "?".to_string(),
                total_params_billions: 0.0,
                active_params_billions: 0.0,
                quant: "unknown".to_string(),
                source: ModelSource::Manual,
            });
        }

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
            should_quit: false,
            is_searching: false,
            matcher: SkimMatcherV2::default(),
        };
        app.recalculate();
        app
    }

    pub fn on_tick(&mut self) {
        // Background updates if needed
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

        let estimation = estimate_usage(
            &model,
            &self.system,
            self.context_length,
            self.batch_size
        );
        self.current_estimation = Some(estimation);
    }

    pub fn update_search(&mut self, query: String) {
        self.search_query = query;
        if self.search_query.is_empty() {
            self.filtered_models = self.models.clone();
        } else {
            let mut scored_models: Vec<(i64, Model)> = self.models.iter().filter_map(|m| {
                self.matcher.fuzzy_match(&m.name, &self.search_query).map(|score| (score, m.clone()))
            }).collect();

            scored_models.sort_by(|a, b| b.0.cmp(&a.0));
            self.filtered_models = scored_models.into_iter().map(|(_, m)| m).collect();
        }
        self.selected_index = 0;
        self.recalculate();
    }

    pub fn next_model(&mut self) {
        if self.filtered_models.is_empty() { return; }
        if self.selected_index < self.filtered_models.len() - 1 {
            self.selected_index += 1;
        }
        self.recalculate();
    }

    pub fn prev_model(&mut self) {
         if self.filtered_models.is_empty() { return; }
         if self.selected_index > 0 {
             self.selected_index -= 1;
         }
         self.recalculate();
    }

    pub fn export_report(&self) -> std::io::Result<()> {
        if let Some(est) = &self.current_estimation {
            if self.filtered_models.is_empty() { return Ok(()); }
            let model = &self.filtered_models[self.selected_index];

            // JSON
            let json_output = serde_json::json!({
                "model": model,
                "system": self.system,
                "estimation": est
            });
            std::fs::write("modelfit_report.json", serde_json::to_string_pretty(&json_output)?)?;

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
        }
        Ok(())
    }
}
