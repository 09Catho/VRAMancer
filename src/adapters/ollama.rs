use std::process::Command;
use crate::core::heuristics::{Model, ModelSource, parse_model_string};

pub fn list_models() -> Vec<Model> {
    let output = Command::new("ollama")
        .arg("list")
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            parse_ollama_list(&stdout)
        },
        _ => vec![], // Return empty if ollama not found or fails
    }
}

fn parse_ollama_list(output: &str) -> Vec<Model> {
    // NAME                     ID              SIZE    MODIFIED
    // llama3:8b                ...             4.7 GB  ...
    // Skip header.
    output.lines().skip(1).filter_map(|line| {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() { return None; }
        let name = parts[0];
        if name == "NAME" { return None; } // Just in case header repeats or something
        Some(parse_model_string(name, ModelSource::Ollama))
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ollama_output() {
        let output = "NAME                     ID              SIZE    MODIFIED
llama3:8b                a6990ed6be41    4.7 GB  2 weeks ago
mistral:latest           61e88e884507    4.1 GB  4 weeks ago
";
        let models = parse_ollama_list(output);
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].name, "llama3:8b");
        assert_eq!(models[0].family, "llama");
        assert_eq!(models[1].name, "mistral:latest");
        assert_eq!(models[1].family, "mistral");
    }
}
