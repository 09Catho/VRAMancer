# VRAMancer

> “Can my machine run this model? Estimate VRAM/RAM, tok/s, and TTFT.”

VRAMancer is a cross-platform Rust CLI + TUI tool that helps you predict if an LLM/VLM will run on your local hardware.

## Features

- **System Detection**: Automatically detects CPU, RAM, and GPU (NVIDIA, AMD, Apple, or Generic).
- **Heuristics Engine**: Estimates VRAM usage based on model size, quantization (q4, q8, fp16), and context length.
- **Performance Prediction**: Estimates Tokens/sec and Time-To-First-Token (TTFT) based on memory bandwidth.
- **Kissing TUI**: Clean, smooth terminal interface built with `ratatui`.
- **JSON Output**: Machine-readable mode for scripts.

## Installation

### Via Cargo (Recommended)

```bash
cargo install --path .
```

### Via NPM

To install directly from the source code (before publishing to npm registry):

```bash
npm install -g .
```

Or if you have published it:

```bash
npm install -g vramancer
```

## Usage

### TUI Mode

Simply run the tool to enter the interactive TUI:

```bash
vramancer
```

- **Navigate**: Arrow keys or `j`/`k`
- **Search**: `/` then type query
- **Settings**: `TAB` to cycle settings, `+`/`-` to adjust context length.
- **Quit**: `q`

### CLI Mode

Generate a JSON report:

```bash
vramancer --json --model llama3:8b --ctx 8192 --quant q4_0
```

Output:
```json
{
  "model": { ... },
  "estimation": {
    "vram_usage_bytes": 5200000000,
    "vram_status": "Fits",
    "recommendation": "Excellent. Run entirely on GPU."
  }
}
```

## Assumptions & Limitations

- **Heuristics**: Estimates are based on standard architecture params (Llama, Mistral, etc.). Custom architectures may be inaccurate.
- **Quantization**: We assume standard GGUF bits-per-weight (e.g. q4_0 ≈ 5.0 bits w/ overhead).
- **Bandwidth**: Performance is calculated using theoretical peak bandwidths of the detected hardware backend, usually heavily discounted to approximate real-world inference limits.
- **Safety Margin**: We account for KV cache and activation overhead but results are estimates.

## License

MIT
