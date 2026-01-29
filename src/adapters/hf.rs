use crate::core::heuristics::{Model, ModelSource, parse_model_string};

pub fn parse_hf_model_id(model_id: &str) -> Model {
    // e.g. "TheBloke/Llama-2-7B-Chat-GGUF"
    // Heuristic: take the part after slash, use that for parsing.
    let name_part = model_id.split('/').last().unwrap_or(model_id);
    // If it has GGUF in name, we might be able to extract more info, but parse_model_string should handle it if regex covers it.

    // We might want to preserve the full ID as the name, but parse characteristics from the suffix.
    let mut model = parse_model_string(name_part, ModelSource::HuggingFace);
    model.name = model_id.to_string(); // Keep full ID
    model
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hf() {
        let m = parse_hf_model_id("TheBloke/Llama-2-7B-Chat-GGUF");
        assert_eq!(m.family, "llama");
        assert_eq!(m.total_params_billions, 7.0);
        assert_eq!(m.name, "TheBloke/Llama-2-7B-Chat-GGUF");
    }
}
