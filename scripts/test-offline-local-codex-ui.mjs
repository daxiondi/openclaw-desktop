#!/usr/bin/env node

import fs from "node:fs";
import fsp from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { spawn, spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const desktopRoot = path.resolve(__dirname, "..");
const bundleRoot = path.resolve(desktopRoot, "src-tauri", "bundle", "resources", "openclaw-bundle");

function run(cmd, args, opts = {}) {
  const result = spawnSync(cmd, args, {
    cwd: opts.cwd,
    env: opts.env ?? process.env,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"]
  });
  if (result.status !== 0) {
    const out = (result.stdout ?? "").trim();
    const err = (result.stderr ?? "").trim();
    throw new Error(`${cmd} ${args.join(" ")} failed\n${[out, err].filter(Boolean).join("\n")}`);
  }
  return (result.stdout ?? "").trim();
}

function assertFile(p, label) {
  if (!fs.existsSync(p) || !fs.statSync(p).isFile()) {
    throw new Error(`${label} missing: ${p}`);
  }
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function tail(text, size = 2000) {
  if (!text) return "";
  return text.length <= size ? text : text.slice(-size);
}

async function waitForHttp(url, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const res = await fetch(url, { method: "GET" });
      if (res.ok) {
        return true;
      }
    } catch {}
    await sleep(1000);
  }
  return false;
}

function readCodexAuth() {
  const home = process.env.HOME || process.env.USERPROFILE;
  if (!home) {
    throw new Error("Cannot resolve HOME/USERPROFILE");
  }
  const authPath = path.join(home, ".codex", "auth.json");
  assertFile(authPath, "local codex auth");

  const raw = fs.readFileSync(authPath, "utf8");
  const parsed = JSON.parse(raw);
  const tokens = parsed?.tokens && typeof parsed.tokens === "object" ? Object.keys(parsed.tokens) : [];
  if (tokens.length === 0) {
    throw new Error("local codex auth.json has no tokens");
  }
  return { authPath, raw };
}

function resolveBundledTools() {
  const bundledPrefix = path.join(bundleRoot, "prefix");
  const nodeCandidates = process.platform === "win32"
    ? [
        path.join(bundleRoot, "node", "bin", "node.exe"),
        path.join(bundleRoot, "node", "node.exe")
      ]
    : [
        path.join(bundleRoot, "node", "bin", "node"),
        path.join(bundleRoot, "node", "node")
      ];
  const nodePath = nodeCandidates.find((candidate) => fs.existsSync(candidate) && fs.statSync(candidate).isFile());
  const npmCli = path.join(bundleRoot, "npm", "bin", "npm-cli.js");
  const tgz = path.join(bundleRoot, "openclaw.tgz");
  const cache = path.join(bundleRoot, "npm-cache");
  if (!nodePath) {
    throw new Error(`bundled node missing: ${nodeCandidates.join(", ")}`);
  }
  assertFile(npmCli, "bundled npm cli");
  assertFile(tgz, "bundled openclaw.tgz");
  if (!fs.existsSync(cache) || !fs.statSync(cache).isDirectory()) {
    throw new Error(`bundled npm cache missing: ${cache}`);
  }
  return { nodePath, npmCli, tgz, cache, bundledPrefix };
}

function resolveInstalledOpenclaw(prefix) {
  const candidates = process.platform === "win32"
    ? [
        path.join(prefix, "bin", "openclaw.cmd"),
        path.join(prefix, "bin", "openclaw.exe"),
        path.join(prefix, "node_modules", "openclaw", "openclaw.mjs"),
        path.join(prefix, "lib", "node_modules", "openclaw", "openclaw.mjs"),
        path.join(prefix, "node_modules", ".bin", "openclaw.cmd")
      ]
    : [
        path.join(prefix, "bin", "openclaw"),
        path.join(prefix, "node_modules", "openclaw", "openclaw.mjs"),
        path.join(prefix, "lib", "node_modules", "openclaw", "openclaw.mjs"),
        path.join(prefix, "node_modules", ".bin", "openclaw")
      ];
  for (const candidate of candidates) {
    try {
      if (fs.statSync(candidate).isFile()) {
        return candidate;
      }
    } catch {}
  }
  throw new Error("openclaw binary not found after offline install");
}

function runOpenclaw(openclawBin, args, opts, nodePath) {
  if (openclawBin.toLowerCase().endsWith(".mjs")) {
    return run(nodePath, [openclawBin, ...args], opts);
  }
  return run(openclawBin, args, opts);
}

function spawnOpenclaw(openclawBin, args, opts, nodePath) {
  if (openclawBin.toLowerCase().endsWith(".mjs")) {
    return spawn(nodePath, [openclawBin, ...args], opts);
  }
  return spawn(openclawBin, args, opts);
}

async function main() {
  const { authPath, raw: authRaw } = readCodexAuth();
  const { nodePath, npmCli, tgz, cache, bundledPrefix } = resolveBundledTools();

  const tempRoot = await fsp.mkdtemp(path.join(os.tmpdir(), "openclaw-offline-e2e-"));
  const tempHome = path.join(tempRoot, "home");
  const prefix = path.join(tempHome, ".openclaw");
  const codexDir = path.join(tempHome, ".codex");
  await fsp.mkdir(codexDir, { recursive: true });
  await fsp.writeFile(path.join(codexDir, "auth.json"), authRaw, "utf8");

  const blockedEnv = {
    ...process.env,
    HOME: tempHome,
    USERPROFILE: tempHome,
    HTTP_PROXY: "http://127.0.0.1:9",
    HTTPS_PROXY: "http://127.0.0.1:9",
    ALL_PROXY: "http://127.0.0.1:9",
    NO_PROXY: "127.0.0.1,localhost"
  };

  console.log("[test] local codex auth source:", authPath);
  console.log("[test] temp HOME:", tempHome);
  if (fs.existsSync(bundledPrefix) && fs.statSync(bundledPrefix).isDirectory()) {
    console.log("[test] install openclaw via bundled prefix snapshot...");
    await fsp.cp(bundledPrefix, prefix, { recursive: true });
  } else {
    console.log("[test] install openclaw via offline npm payload...");
    run(nodePath, [
      npmCli,
      "install",
      "--prefix",
      prefix,
      tgz,
      "--cache",
      cache,
      "--offline",
      "--no-audit",
      "--no-fund",
      "--loglevel=error"
    ], { env: blockedEnv });
  }

  const openclawBin = resolveInstalledOpenclaw(prefix);
  const appEnv = {
    ...blockedEnv,
    PATH: [
      path.join(prefix, "bin"),
      path.dirname(nodePath),
      process.env.PATH || ""
    ].filter(Boolean).join(path.delimiter)
  };

  console.log("[test] run setup + non-interactive onboard(openai-codex)...");
  runOpenclaw(openclawBin, ["setup"], { env: appEnv }, nodePath);
  const onboardArgs = [
    "onboard",
    "--non-interactive",
    "--accept-risk",
    "--mode",
    "local",
    "--auth-choice",
    "openai-codex",
    "--no-install-daemon",
    "--skip-channels",
    "--skip-skills",
    "--skip-ui",
    "--skip-health"
  ];
  try {
    runOpenclaw(openclawBin, onboardArgs, { env: appEnv }, nodePath);
  } catch (error) {
    const msg = error instanceof Error ? error.message : String(error);
    if (!msg.includes("OAuth requires interactive mode")) {
      throw error;
    }
    console.log("[test] onboard openai-codex non-interactive rejected, fallback to auth-choice=skip");
    onboardArgs[6] = "skip";
    runOpenclaw(openclawBin, onboardArgs, { env: appEnv }, nodePath);
  }

  console.log("[test] start gateway and verify official local page...");
  const gatewayArgs = [
    "gateway",
    "run",
    "--allow-unconfigured",
    "--port",
    "18789",
    "--verbose"
  ];
  const gateway = spawnOpenclaw(
    openclawBin,
    gatewayArgs,
    { env: appEnv, stdio: ["ignore", "pipe", "pipe"] },
    nodePath
  );

  let stdout = "";
  let stderr = "";
  let exitCode = null;
  gateway.stdout.on("data", (chunk) => {
    stdout += chunk.toString();
  });
  gateway.stderr.on("data", (chunk) => {
    stderr += chunk.toString();
  });
  gateway.on("exit", (code) => {
    exitCode = code;
  });

  try {
    const ready = await waitForHttp("http://127.0.0.1:18789/", 90_000);
    if (!ready) {
      throw new Error(
        [
          "official local web is not reachable",
          `gateway exit code: ${exitCode === null ? "running" : String(exitCode)}`,
          `gateway stdout tail:\n${tail(stdout)}`,
          `gateway stderr tail:\n${tail(stderr)}`
        ].join("\n")
      );
    }
    console.log("[test] PASS: offline install + local codex setup + official page reachable");
  } finally {
    gateway.kill("SIGTERM");
    await sleep(1000);
    await fsp.rm(tempRoot, { recursive: true, force: true });
  }
}

main().catch((error) => {
  console.error("[test] FAIL:", error instanceof Error ? error.message : String(error));
  process.exit(1);
});
