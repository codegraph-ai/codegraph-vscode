#!/usr/bin/env node
"use strict";

const path = require("path");
const os = require("os");
const fs = require("fs");
const { execFileSync } = require("child_process");

const PLATFORM_MAP = {
  darwin: "darwin",
  linux: "linux",
  win32: "win32",
};

const ARCH_MAP = {
  arm64: "arm64",
  x64: "x64",
  x86_64: "x64",
};

const platform = PLATFORM_MAP[os.platform()];
const arch = ARCH_MAP[os.arch()];

if (!platform || !arch) {
  console.warn(
    `⚠ codegraph-mcp: unsupported platform ${os.platform()}-${os.arch()}`
  );
  process.exit(0); // Don't fail install
}

const ext = platform === "win32" ? ".exe" : "";
const binaryName = `codegraph-lsp-${platform}-${arch}${ext}`;
const binaryPath = path.join(__dirname, binaryName);

if (!fs.existsSync(binaryPath)) {
  console.warn(`⚠ codegraph-mcp: binary not found for ${platform}-${arch}`);
  console.warn(`  Expected: ${binaryPath}`);
  process.exit(0); // Don't fail install
}

// Ensure executable permission on Unix
if (platform !== "win32") {
  try {
    fs.chmodSync(binaryPath, 0o755);
  } catch {
    // Ignore permission errors
  }
}

// Verify binary runs
try {
  const output = execFileSync(binaryPath, ["--version"], {
    timeout: 10000,
    encoding: "utf8",
  });
  console.log(`✓ codegraph-mcp installed: ${output.trim()}`);
} catch (err) {
  console.warn(`⚠ codegraph-mcp: binary exists but --version check failed`);
  console.warn(`  ${err.message}`);
  // Don't fail install — binary might still work for MCP
}
