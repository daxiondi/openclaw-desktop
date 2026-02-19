#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";

function usageAndExit() {
  console.error("Usage: node scripts/generate-updater-manifest.mjs <assetsDir> <tagName> <githubRepo>");
  process.exit(1);
}

const [assetsDirArg, tagNameArg, githubRepoArg] = process.argv.slice(2);
if (!assetsDirArg || !tagNameArg || !githubRepoArg) {
  usageAndExit();
}

const assetsDir = path.resolve(assetsDirArg);
const tagName = tagNameArg.trim();
const githubRepo = githubRepoArg.trim();
const version = tagName.startsWith("v") ? tagName.slice(1) : tagName;

if (!fs.existsSync(assetsDir) || !fs.statSync(assetsDir).isDirectory()) {
  throw new Error(`Assets directory not found: ${assetsDir}`);
}

function walkFiles(dir) {
  const result = [];
  const entries = fs.readdirSync(dir, { withFileTypes: true });
  for (const entry of entries) {
    const fullPath = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      result.push(...walkFiles(fullPath));
    } else if (entry.isFile()) {
      result.push(fullPath);
    }
  }
  return result;
}

function detectArch(lowerName) {
  if (/(^|[^a-z0-9])(aarch64|arm64)([^a-z0-9]|$)/i.test(lowerName)) {
    return "aarch64";
  }
  if (/(^|[^a-z0-9])(x86_64|x64|amd64)([^a-z0-9]|$)/i.test(lowerName)) {
    return "x86_64";
  }
  if (/(^|[^a-z0-9])(i686|x86)([^a-z0-9]|$)/i.test(lowerName)) {
    return "i686";
  }
  return null;
}

function scoreCandidate(lowerName) {
  let score = 0;
  if (lowerName.endsWith(".app.tar.gz")) score += 300;
  if (lowerName.endsWith(".appimage") || lowerName.endsWith(".appimage.tar.gz")) score += 300;
  if (lowerName.includes("setup") && lowerName.endsWith(".exe")) score += 300;
  if (lowerName.endsWith(".msi")) score += 260;
  if (lowerName.endsWith(".deb")) score += 240;
  if (lowerName.endsWith(".rpm")) score += 230;
  if (lowerName.endsWith(".nsis.zip")) score += 220;
  if (lowerName.endsWith(".msi.zip")) score += 210;
  if (lowerName.includes("portable")) score -= 100;
  return score;
}

function detectTargets(lowerName) {
  const arch = detectArch(lowerName);
  const isMac = lowerName.endsWith(".app.tar.gz");
  const isWin =
    lowerName.endsWith(".exe") ||
    lowerName.endsWith(".msi") ||
    lowerName.endsWith(".nsis.zip") ||
    lowerName.endsWith(".msi.zip");
  const isLinux =
    lowerName.endsWith(".appimage") ||
    lowerName.endsWith(".appimage.tar.gz") ||
    lowerName.endsWith(".deb") ||
    lowerName.endsWith(".rpm");

  if (!isMac && !isWin && !isLinux) {
    return [];
  }

  if (isMac) {
    if (arch) {
      return [`darwin-${arch}`];
    }
    return ["darwin-x86_64", "darwin-aarch64"];
  }
  if (isWin) {
    return [`windows-${arch || "x86_64"}`];
  }
  if (isLinux) {
    return [`linux-${arch || "x86_64"}`];
  }
  return [];
}

const files = walkFiles(assetsDir);
const sigFiles = files.filter((filePath) => filePath.endsWith(".sig"));

const bestByTarget = new Map();

for (const sigPath of sigFiles) {
  const artifactPath = sigPath.slice(0, -4);
  if (!fs.existsSync(artifactPath) || !fs.statSync(artifactPath).isFile()) {
    continue;
  }

  const fileName = path.basename(artifactPath);
  const lowerName = fileName.toLowerCase();
  const targets = detectTargets(lowerName);
  if (targets.length === 0) {
    continue;
  }

  const signature = fs.readFileSync(sigPath, "utf8").trim();
  if (!signature) {
    continue;
  }

  const baseScore = scoreCandidate(lowerName) + (detectArch(lowerName) ? 20 : 0);
  for (const target of targets) {
    const existing = bestByTarget.get(target);
    const candidate = {
      target,
      fileName,
      artifactPath,
      signature,
      score: baseScore,
    };
    if (!existing || candidate.score > existing.score) {
      bestByTarget.set(target, candidate);
    }
  }
}

if (bestByTarget.size === 0) {
  throw new Error(
    "No signed updater artifacts found. Ensure `bundle.createUpdaterArtifacts` is enabled and TAURI_SIGNING_PRIVATE_KEY is set in CI."
  );
}

const platforms = {};
for (const [target, candidate] of bestByTarget.entries()) {
  const assetUrl = `https://github.com/${githubRepo}/releases/download/${encodeURIComponent(tagName)}/${encodeURIComponent(candidate.fileName)}`;
  platforms[target] = {
    signature: candidate.signature,
    url: assetUrl,
  };
}

const manifest = {
  version,
  notes: `Release ${tagName}`,
  pub_date: new Date().toISOString(),
  platforms,
};

const outputPath = path.join(assetsDir, "latest.json");
fs.writeFileSync(outputPath, `${JSON.stringify(manifest, null, 2)}\n`, "utf8");

console.log(`Updater manifest generated: ${outputPath}`);
for (const [target, candidate] of bestByTarget.entries()) {
  console.log(`- ${target} -> ${candidate.fileName}`);
}
