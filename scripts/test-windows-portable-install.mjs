#!/usr/bin/env node

import fs from "node:fs";
import fsp from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { spawn, spawnSync } from "node:child_process";

const DEFAULT_PORTABLE_ZIP_URL =
  "https://github.com/daxiondi/openclaw-desktop/releases/latest/download/openclaw-desktop-windows-portable.zip";

function run(cmd, args, opts = {}) {
  const result = spawnSync(cmd, args, {
    cwd: opts.cwd,
    env: opts.env ?? process.env,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
    windowsHide: true,
    maxBuffer: Number(process.env.OPENCLAW_DESKTOP_TEST_MAX_BUFFER || 256 * 1024 * 1024)
  });

  if (result.error) {
    throw new Error(`${cmd} ${args.join(" ")} failed\n${String(result.error)}`);
  }

  if (result.status !== 0) {
    const out = (result.stdout ?? "").trim();
    const err = (result.stderr ?? "").trim();
    const detail = [out, err].filter(Boolean).join("\n");
    throw new Error(`${cmd} ${args.join(" ")} failed${detail ? `\n${detail}` : ""}`);
  }

  return (result.stdout ?? "").trim();
}

function runStreaming(cmd, args, opts = {}) {
  const result = spawnSync(cmd, args, {
    cwd: opts.cwd,
    env: opts.env ?? process.env,
    encoding: "utf8",
    stdio: "inherit",
    windowsHide: true
  });

  if (result.error) {
    throw new Error(`${cmd} ${args.join(" ")} failed\n${String(result.error)}`);
  }

  if (result.status !== 0) {
    throw new Error(`${cmd} ${args.join(" ")} failed with exit code ${result.status}`);
  }
}

function assertFile(p, label) {
  if (!fs.existsSync(p) || !fs.statSync(p).isFile()) {
    throw new Error(`${label} missing: ${p}`);
  }
}

function assertDir(p, label) {
  if (!fs.existsSync(p) || !fs.statSync(p).isDirectory()) {
    throw new Error(`${label} missing: ${p}`);
  }
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function waitForHttp(url, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const res = await fetch(url, { method: "GET" });
      // Treat any HTTP response as reachable. Some OpenClaw gateways may
      // return 401/403 for unauthenticated requests but are still healthy.
      if (res && typeof res.status === "number") {
        return true;
      }
    } catch {}
    await sleep(1000);
  }
  return false;
}

function resolveBundledTools(bundleRoot) {
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
  const bundledPrefix = path.join(bundleRoot, "prefix");
  if (!nodePath) {
    throw new Error(`bundled node missing: ${nodeCandidates.join(", ")}`);
  }
  assertFile(npmCli, "bundled npm cli");
  assertFile(tgz, "bundled openclaw.tgz");
  assertDir(cache, "bundled npm cache");
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

function copyDirRobust(src, dst) {
  if (process.platform === "win32") {
    fs.mkdirSync(dst, { recursive: true });
    const result = spawnSync("robocopy", [
      src,
      dst,
      "/E",
      "/R:2",
      "/W:2",
      "/NFL",
      "/NDL",
      "/NJH",
      "/NJS",
      "/NP"
    ], { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"], windowsHide: true, maxBuffer: 256 * 1024 * 1024 });
    const exitCode = typeof result.status === "number" ? result.status : 16;
    if (exitCode >= 8) {
      const detail = [(result.stdout ?? "").trim(), (result.stderr ?? "").trim()].filter(Boolean).join("\n");
      throw new Error(`robocopy failed with exit code ${exitCode}${detail ? `\n${detail}` : ""}`);
    }
    return;
  }

  run("cp", ["-R", src, dst]);
}

async function downloadPortableZip(url, zipPath) {
  console.log("[portable] downloading:", url);
  console.log("[portable] to:", zipPath);
  runStreaming("curl", [
    "-L",
    "--fail",
    "--retry",
    "3",
    "--retry-all-errors",
    "--connect-timeout",
    "30",
    "-o",
    zipPath,
    url
  ]);
  assertFile(zipPath, "downloaded portable zip");
}

async function extractPortableZip(zipPath, extractRoot) {
  await fsp.mkdir(extractRoot, { recursive: true });
  console.log("[portable] extracting to:", extractRoot);
  runStreaming("tar", ["-xf", zipPath, "-C", extractRoot]);
}

async function main() {
  if (process.platform !== "win32") {
    throw new Error("This verification script currently targets Windows portable installs only.");
  }

  const arg = process.argv.slice(2)[0];
  const source = arg && arg.trim() ? arg.trim() : DEFAULT_PORTABLE_ZIP_URL;

  const tempRoot = await fsp.mkdtemp(path.join(os.tmpdir(), "openclaw-desktop-portable-e2e-"));
  const zipPath = source.startsWith("http://") || source.startsWith("https://")
    ? path.join(tempRoot, "openclaw-desktop-windows-portable.zip")
    : path.resolve(source);
  const extractRoot = path.join(tempRoot, "extract");

  try {
    if (zipPath.startsWith(tempRoot)) {
      await downloadPortableZip(source, zipPath);
    } else {
      assertFile(zipPath, "portable zip");
    }

    // Quick integrity check (avoid printing listing to stdout).
    const tarCheck = spawnSync("tar", ["-tf", zipPath], {
      encoding: "utf8",
      stdio: ["ignore", "ignore", "pipe"],
      windowsHide: true,
      maxBuffer: 256 * 1024 * 1024
    });
    if (tarCheck.error) {
      throw new Error(`tar -tf failed\n${String(tarCheck.error)}`);
    }
    if (tarCheck.status !== 0) {
      const err = (tarCheck.stderr ?? "").trim();
      throw new Error(`tar -tf failed with exit code ${tarCheck.status}${err ? `\n${err}` : ""}`);
    }

    await extractPortableZip(zipPath, extractRoot);

    const bundleRoot = path.join(extractRoot, "bundle", "resources", "openclaw-bundle");
    assertDir(bundleRoot, "openclaw-bundle");

    const { nodePath, npmCli, tgz, cache, bundledPrefix } = resolveBundledTools(bundleRoot);
    const tempHome = path.join(tempRoot, "home");
    const prefix = path.join(tempHome, ".openclaw");
    await fsp.mkdir(tempHome, { recursive: true });

    const installEnv = {
      ...process.env,
      HOME: tempHome,
      USERPROFILE: tempHome,
      HTTP_PROXY: "http://127.0.0.1:9",
      HTTPS_PROXY: "http://127.0.0.1:9",
      ALL_PROXY: "http://127.0.0.1:9",
      NO_PROXY: "127.0.0.1,localhost"
    };

    if (fs.existsSync(bundledPrefix) && fs.statSync(bundledPrefix).isDirectory()) {
      console.log("[test] installing openclaw via bundled prefix snapshot...");
      copyDirRobust(bundledPrefix, prefix);
    } else {
      console.log("[test] installing openclaw via offline npm payload...");
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
      ], { env: installEnv });
    }

    const openclawBin = resolveInstalledOpenclaw(prefix);
    const appEnv = {
      ...process.env,
      HOME: tempHome,
      USERPROFILE: tempHome,
      PATH: [
        path.join(prefix, "bin"),
        path.join(prefix, "node_modules", ".bin"),
        path.dirname(nodePath),
        process.env.PATH || ""
      ].filter(Boolean).join(path.delimiter)
    };

    console.log("[test] openclaw binary:", openclawBin);
    console.log("[test] openclaw --version:");
    runOpenclaw(openclawBin, ["--version"], { env: appEnv }, nodePath);

    console.log("[test] run openclaw setup...");
    runOpenclaw(openclawBin, ["setup"], { env: appEnv }, nodePath);

    console.log("[test] start gateway and verify official local page...");
    const gatewayArgs = ["gateway", "run", "--allow-unconfigured", "--port", "18789", "--verbose"];
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
      const ready = await waitForHttp("http://127.0.0.1:18789/", 180_000);
      if (!ready) {
        const tail = (text, size = 2000) => (text.length <= size ? text : text.slice(-size));
        let netstat = "";
        try {
          const probe = spawnSync("cmd", ["/C", "netstat -ano | findstr :18789"], {
            encoding: "utf8",
            stdio: ["ignore", "pipe", "pipe"],
            windowsHide: true,
            maxBuffer: 4 * 1024 * 1024
          });
          netstat = [(probe.stdout ?? "").trim(), (probe.stderr ?? "").trim()].filter(Boolean).join("\n");
        } catch {}
        throw new Error(
          [
            "official local web is not reachable",
            `gateway exit code: ${exitCode === null ? "running" : String(exitCode)}`,
            netstat ? `netstat:\n${netstat}` : "",
            `gateway stdout tail:\n${tail(stdout)}`,
            `gateway stderr tail:\n${tail(stderr)}`
          ].filter(Boolean).join("\n")
        );
      }
      console.log("[test] PASS: portable zip -> offline payload -> gateway reachable");
    } finally {
      gateway.kill("SIGTERM");
      await sleep(1000);
    }
  } finally {
    if (process.env.OPENCLAW_DESKTOP_TEST_KEEP_TEMP === "1") {
      console.log("[test] keeping temp root:", tempRoot);
    } else {
      await fsp.rm(tempRoot, { recursive: true, force: true });
    }
  }
}

main().catch((error) => {
  console.error("[test] FAIL:", error instanceof Error ? error.message : String(error));
  process.exit(1);
});
