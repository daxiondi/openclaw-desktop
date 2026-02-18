#!/usr/bin/env node

import fs from "node:fs";
import fsp from "node:fs/promises";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const desktopRoot = path.resolve(__dirname, "..");
const workspaceRoot = path.resolve(desktopRoot, "..");
const openclawRoot = path.resolve(workspaceRoot, "openclaw");
const bundleDir = path.resolve(desktopRoot, "src-tauri", "bundle", "resources", "openclaw-bundle");
const tempDir = path.resolve(desktopRoot, ".tmp", "openclaw-bundle");
const OPENCLAW_MIN_NODE = "22.12.0";

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
    const detail = [out, err].filter(Boolean).join("\n");
    throw new Error(`${cmd} ${args.join(" ")} failed${detail ? `\n${detail}` : ""}`);
  }

  return (result.stdout ?? "").trim();
}

function parseVersion(v) {
  const match = String(v).trim().match(/^v?(\d+)\.(\d+)\.(\d+)/);
  if (!match) {
    return null;
  }
  return {
    major: Number(match[1]),
    minor: Number(match[2]),
    patch: Number(match[3])
  };
}

function versionGte(left, right) {
  const a = parseVersion(left);
  const b = parseVersion(right);
  if (!a || !b) {
    return false;
  }
  if (a.major !== b.major) return a.major > b.major;
  if (a.minor !== b.minor) return a.minor > b.minor;
  return a.patch >= b.patch;
}

function ensureFile(p, label) {
  if (!fs.existsSync(p) || !fs.statSync(p).isFile()) {
    throw new Error(`${label} not found: ${p}`);
  }
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

  throw new Error(`bundled openclaw executable not found under: ${prefix}`);
}

async function ensureCleanDir(p) {
  await fsp.rm(p, { recursive: true, force: true });
  await fsp.mkdir(p, { recursive: true });
}

async function ensureUserWritableRecursive(rootDir) {
  const queue = [rootDir];
  while (queue.length > 0) {
    const current = queue.pop();
    const stat = await fsp.lstat(current);
    await fsp.chmod(current, stat.mode | 0o200);
    if (!stat.isDirectory()) {
      continue;
    }
    const children = await fsp.readdir(current);
    for (const child of children) {
      queue.push(path.join(current, child));
    }
  }
}

function resolveNpmDir() {
  const npmRoot = run("npm", ["root", "-g"]);
  const candidate = path.join(npmRoot, "npm");
  if (fs.existsSync(candidate)) {
    return candidate;
  }

  const prefix = run("npm", ["config", "get", "prefix"]);
  const extra = process.platform === "win32"
    ? path.join(prefix, "node_modules", "npm")
    : path.join(prefix, "lib", "node_modules", "npm");
  if (fs.existsSync(extra)) {
    return extra;
  }

  throw new Error("Unable to locate npm directory for offline bundle");
}

function npmPack(args, cwd) {
  const raw = run("npm", ["pack", "--json", ...args], { cwd });
  let parsed;
  try {
    parsed = JSON.parse(raw);
  } catch (error) {
    throw new Error(`Failed to parse npm pack --json output: ${String(error)}\n${raw}`);
  }

  const record = Array.isArray(parsed) ? parsed[0] : parsed;
  const filename = record?.filename;
  if (!filename || typeof filename !== "string") {
    throw new Error(`npm pack --json missing filename: ${raw}`);
  }

  const packedFile = path.resolve(cwd, filename);
  ensureFile(packedFile, "packed openclaw tarball");
  return {
    filename,
    packedFile,
    version: typeof record?.version === "string" ? record.version : null
  };
}

function localOpenclawBuildReady() {
  const packageJsonPath = path.join(openclawRoot, "package.json");
  if (!fs.existsSync(packageJsonPath)) {
    return false;
  }
  const hasDistEntryJs = fs.existsSync(path.join(openclawRoot, "dist", "entry.js"));
  const hasDistEntryMjs = fs.existsSync(path.join(openclawRoot, "dist", "entry.mjs"));
  return hasDistEntryJs || hasDistEntryMjs;
}

function resolveLocalOpenclawVersion() {
  const packageJsonPath = path.join(openclawRoot, "package.json");
  if (!fs.existsSync(packageJsonPath)) {
    return null;
  }
  try {
    const pkg = JSON.parse(fs.readFileSync(packageJsonPath, "utf8"));
    return typeof pkg?.version === "string" ? pkg.version : null;
  } catch {
    return null;
  }
}

function packOpenclawTarball() {
  const localVersion = resolveLocalOpenclawVersion();

  if (localOpenclawBuildReady()) {
    console.log("[bundle] packing openclaw from local source (dist ready)...");
    const localPack = npmPack(["--ignore-scripts"], openclawRoot);
    return {
      ...localPack,
      source: "local-source",
      cleanup: async () => fsp.rm(localPack.packedFile, { force: true })
    };
  }

  const spec = localVersion ? `openclaw@${localVersion}` : "openclaw@latest";
  console.log(`[bundle] local source missing dist, fetching ${spec} from npm registry...`);
  const remotePack = npmPack([spec], tempDir);
  return {
    ...remotePack,
    source: `npm-registry:${spec}`,
    cleanup: async () => {}
  };
}

async function resolveBundledNodeRuntime() {
  const customNode = process.env.OPENCLAW_BUNDLE_NODE;
  if (customNode && fs.existsSync(customNode)) {
    const customVersion = run(customNode, ["-v"]);
    if (versionGte(customVersion, OPENCLAW_MIN_NODE)) {
      return {
        nodePath: customNode,
        nodeVersion: customVersion,
        nodeSource: "env:OPENCLAW_BUNDLE_NODE"
      };
    }
  }

  console.log(`[bundle] provisioning portable node@${OPENCLAW_MIN_NODE} runtime...`);
  const nodeProvisionPrefix = path.join(tempDir, "node-runtime");
  await ensureCleanDir(nodeProvisionPrefix);
  run("npm", [
    "install",
    "--prefix",
    nodeProvisionPrefix,
    "--no-audit",
    "--no-fund",
    "--loglevel=error",
    `node@${OPENCLAW_MIN_NODE}`
  ]);

  const bundledNodePath = process.platform === "win32"
    ? path.join(nodeProvisionPrefix, "node_modules", "node", "bin", "node.exe")
    : path.join(nodeProvisionPrefix, "node_modules", "node", "bin", "node");
  ensureFile(bundledNodePath, "bundled node runtime");

  const bundledNodeVersion = run(bundledNodePath, ["-v"]);
  if (!versionGte(bundledNodeVersion, OPENCLAW_MIN_NODE)) {
    throw new Error(
      `bundled node ${bundledNodeVersion} does not satisfy >=${OPENCLAW_MIN_NODE}`
    );
  }

  return {
    nodePath: bundledNodePath,
    nodeVersion: bundledNodeVersion,
    nodeSource: `npm:node@${OPENCLAW_MIN_NODE}`
  };
}

async function main() {
  if (process.env.OPENCLAW_DESKTOP_SKIP_BUNDLE_PREP === "1") {
    console.log("[bundle] skip prepare because OPENCLAW_DESKTOP_SKIP_BUNDLE_PREP=1");
    return;
  }

  await ensureCleanDir(bundleDir);
  await ensureCleanDir(tempDir);

  const packed = packOpenclawTarball();

  const runtime = await resolveBundledNodeRuntime();
  const bundledTgz = path.join(bundleDir, "openclaw.tgz");
  await fsp.copyFile(packed.packedFile, bundledTgz);
  await packed.cleanup();

  console.log("[bundle] copying node runtime and npm...");
  const nodeDir = path.join(bundleDir, "node");
  await ensureCleanDir(nodeDir);
  const nodeTarget = path.join(nodeDir, process.platform === "win32" ? "node.exe" : "node");
  await fsp.copyFile(runtime.nodePath, nodeTarget);
  if (process.platform !== "win32") {
    await fsp.chmod(nodeTarget, 0o755);
  }
  ensureFile(nodeTarget, "bundled node runtime");

  const npmDir = resolveNpmDir();
  const npmTarget = path.join(bundleDir, "npm");
  await fsp.rm(npmTarget, { recursive: true, force: true });
  await fsp.cp(npmDir, npmTarget, {
    recursive: true,
    // npm package ships a root .npmrc; tauri resource scanner may fail on it on some hosts.
    filter: (src) => path.basename(src) !== ".npmrc"
  });

  console.log("[bundle] warming offline npm cache...");
  const cacheDir = path.join(bundleDir, "npm-cache");
  const installPrefix = path.join(tempDir, "install-prefix");
  await fsp.mkdir(cacheDir, { recursive: true });
  run("npm", [
    "install",
    "--prefix",
    installPrefix,
    bundledTgz,
    "--cache",
    cacheDir,
    "--no-audit",
    "--no-fund",
    "--loglevel=error"
  ]);

  console.log("[bundle] snapshot installed prefix for fully-offline install...");
  const bundledPrefix = path.join(bundleDir, "prefix");
  await fsp.rm(bundledPrefix, { recursive: true, force: true });
  // npm local install 会产生指向临时目录的绝对软链；这里解引用，避免打包后出现失效链接。
  await fsp.cp(installPrefix, bundledPrefix, { recursive: true, dereference: true });

  if (process.env.OPENCLAW_BUNDLE_SKIP_VERIFY === "1") {
    console.log("[bundle] skip prefix verification because OPENCLAW_BUNDLE_SKIP_VERIFY=1");
  } else {
    console.log("[bundle] verifying bundled prefix snapshot...");
    const verifyPrefix = path.join(tempDir, "verify-prefix");
    await fsp.cp(bundledPrefix, verifyPrefix, { recursive: true });
    const verifyOpenclaw = resolveInstalledOpenclaw(verifyPrefix);
    const verifyEnv = {
      ...process.env,
      PATH: `${path.dirname(nodeTarget)}${path.delimiter}${process.env.PATH || ""}`
    };
    if (process.platform === "win32") {
      run("cmd", ["/C", verifyOpenclaw, "--version"], { env: verifyEnv });
    } else {
      run(verifyOpenclaw, ["--version"], { env: verifyEnv });
    }
    await fsp.rm(verifyPrefix, { recursive: true, force: true });
  }
  await fsp.rm(installPrefix, { recursive: true, force: true });

  const npmCli = path.join(bundleDir, "npm", "bin", "npm-cli.js");
  const manifest = {
    name: "openclaw-offline-bundle",
    generatedAt: new Date().toISOString(),
    openclawVersion: packed.version ?? resolveLocalOpenclawVersion() ?? "unknown",
    openclawSource: packed.source,
    nodeVersion: runtime.nodeVersion,
    nodeSource: runtime.nodeSource,
    nodePlatform: `${process.platform}-${process.arch}`,
    files: {
      openclawTgz: "openclaw.tgz",
      npmCache: "npm-cache",
      node: path.relative(bundleDir, nodeTarget),
      npmCli: path.relative(bundleDir, npmCli)
    }
  };
  await fsp.writeFile(
    path.join(bundleDir, "manifest.json"),
    JSON.stringify(manifest, null, 2),
    "utf8"
  );

  // npm tarballs may preserve read-only bits. Ensure resources stay writable so
  // repeated local builds can overwrite copied files without EACCES.
  await ensureUserWritableRecursive(bundleDir);

  await fsp.rm(tempDir, { recursive: true, force: true });
  console.log("[bundle] ready:", bundleDir);
}

main().catch((error) => {
  console.error("[bundle] failed:", error instanceof Error ? error.message : String(error));
  process.exit(1);
});
