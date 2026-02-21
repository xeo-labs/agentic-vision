#!/usr/bin/env node
// Copyright 2026 Cortex Contributors
// SPDX-License-Identifier: Apache-2.0

/**
 * Post-install script for @cortex/cli npm package.
 * Downloads the appropriate Cortex binary for the current platform.
 */

const { execSync } = require("child_process");
const fs = require("fs");
const https = require("https");
const os = require("os");
const path = require("path");
const { createWriteStream } = require("fs");

const VERSION = require("./package.json").version;
const REPO = "agentralabs/agentic-vision";

function getPlatform() {
  const platform = os.platform();
  const arch = os.arch();

  const platformMap = { darwin: "darwin", linux: "linux" };
  const archMap = { x64: "x86_64", arm64: "aarch64" };

  const p = platformMap[platform];
  const a = archMap[arch];

  if (!p || !a) {
    console.error(`Unsupported platform: ${platform}-${arch}`);
    process.exit(1);
  }

  return `${p}-${a}`;
}

function download(url, dest) {
  return new Promise((resolve, reject) => {
    const follow = (url) => {
      https
        .get(url, (res) => {
          if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
            follow(res.headers.location);
            return;
          }
          if (res.statusCode !== 200) {
            reject(new Error(`Download failed: HTTP ${res.statusCode}`));
            return;
          }
          const file = createWriteStream(dest);
          res.pipe(file);
          file.on("finish", () => {
            file.close(resolve);
          });
        })
        .on("error", reject);
    };
    follow(url);
  });
}

async function main() {
  const triple = getPlatform();
  const asset = `cortex-${VERSION}-${triple}.tar.gz`;
  const url = `https://github.com/${REPO}/releases/download/v${VERSION}/${asset}`;

  const binDir = path.join(__dirname, "bin");
  fs.mkdirSync(binDir, { recursive: true });

  const tarball = path.join(binDir, asset);
  console.log(`Downloading Cortex v${VERSION} for ${triple}...`);

  await download(url, tarball);

  execSync(`tar xzf "${tarball}" -C "${binDir}"`, { stdio: "inherit" });
  fs.unlinkSync(tarball);
  fs.chmodSync(path.join(binDir, "cortex"), 0o755);

  console.log("Cortex installed successfully.");
}

main().catch((err) => {
  console.error("Failed to install Cortex:", err.message);
  process.exit(1);
});
