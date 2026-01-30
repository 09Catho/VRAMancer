#!/usr/bin/env node
const { spawn } = require('child_process');
const path = require('path');

// The binary is expected to be in the same directory as this wrapper after installation
const binName = process.platform === 'win32' ? 'vramancer.exe' : 'vramancer';
const binPath = path.join(__dirname, binName);

const child = spawn(binPath, process.argv.slice(2), { stdio: 'inherit' });

child.on('error', (err) => {
    console.error(`Failed to start vramancer: ${err.message}`);
    console.error(`Expected binary at: ${binPath}`);
});

child.on('exit', (code) => {
  process.exit(code);
});
