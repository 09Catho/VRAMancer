const { execSync } = require('child_process');
const fs = require('fs');
const path = require('path');

console.log('Building vramancer via Cargo...');

try {
  // Check if cargo exists
  execSync('cargo --version', { stdio: 'ignore' });
} catch (e) {
  console.error('Error: Rust/Cargo is required to install vramancer. Please install Rust from https://rustup.rs/');
  process.exit(1);
}

try {
  // Install to local bin directory
  // --root . installs to ./bin/vramancer
  execSync('cargo install --path . --root . --force', { stdio: 'inherit' });
  console.log('vramancer built successfully!');
} catch (e) {
  console.error('Failed to build vramancer.');
  process.exit(1);
}
