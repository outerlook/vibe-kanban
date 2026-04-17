#!/usr/bin/env node

const fs = require("fs");
const path = require("path");
const { spawn } = require("child_process");

const INTEGRATION_DIR = path.join(
  __dirname,
  "..",
  "integrations",
  "trigger-vibe-kanban"
);
const DEFAULT_ENV_FILE = path.join(INTEGRATION_DIR, ".env");

function usage() {
  console.error(
    [
      "Usage: node scripts/trigger-vk.js <command> [extra args]",
      "",
      "Commands:",
      "  dev",
      "  mqtt",
      "  coderabbit",
      "  deploy",
      "  push",
      "  deploy:staging",
      "  deploy:dry-run",
      "",
      "Environment:",
      "  TRIGGER_VK_ENV_FILE   Override the env file path (defaults to integrations/trigger-vibe-kanban/.env)",
    ].join("\n")
  );
}

function stripWrappingQuotes(value) {
  if (
    (value.startsWith('"') && value.endsWith('"')) ||
    (value.startsWith("'") && value.endsWith("'"))
  ) {
    return value.slice(1, -1);
  }

  return value;
}

function loadEnvFile(filePath) {
  if (!fs.existsSync(filePath)) {
    return {};
  }

  const env = {};
  const source = fs.readFileSync(filePath, "utf8");

  for (const rawLine of source.split(/\r?\n/)) {
    const line = rawLine.trim();

    if (!line || line.startsWith("#")) {
      continue;
    }

    const normalized = line.startsWith("export ") ? line.slice(7) : line;
    const separatorIndex = normalized.indexOf("=");

    if (separatorIndex === -1) {
      continue;
    }

    const key = normalized.slice(0, separatorIndex).trim();
    const value = stripWrappingQuotes(normalized.slice(separatorIndex + 1).trim());

    if (!key) {
      continue;
    }

    env[key] = value;
  }

  return env;
}

function resolveEnvFile() {
  if (!process.env.TRIGGER_VK_ENV_FILE) {
    return DEFAULT_ENV_FILE;
  }

  return path.resolve(process.cwd(), process.env.TRIGGER_VK_ENV_FILE);
}

function buildTriggerCliArgs(env, extraArgs) {
  const args = [...extraArgs];

  if (env.TRIGGER_API_URL && !args.includes("--api-url") && !args.includes("-a")) {
    args.unshift(env.TRIGGER_API_URL);
    args.unshift("--api-url");
  }

  if (env.TRIGGER_PROFILE && !args.includes("--profile")) {
    args.unshift(env.TRIGGER_PROFILE);
    args.unshift("--profile");
  }

  return args;
}

function runBunScript(scriptName, env, forwardedArgs) {
  const bunArgs = ["run", scriptName];

  if (forwardedArgs.length > 0) {
    bunArgs.push("--", ...forwardedArgs);
  }

  const child = spawn("bun", bunArgs, {
    cwd: INTEGRATION_DIR,
    env,
    stdio: "inherit",
  });

  child.on("error", (error) => {
    console.error(`Failed to start bun for "${scriptName}": ${error.message}`);
    process.exit(1);
  });

  child.on("exit", (code) => {
    process.exit(code ?? 0);
  });
}

const command = process.argv[2];
const extraArgs = process.argv.slice(3);

if (!command) {
  usage();
  process.exit(1);
}

const envFile = resolveEnvFile();
const fileEnv = loadEnvFile(envFile);
const mergedEnv = {
  ...process.env,
  ...fileEnv,
};

console.error(`[trigger-vk] using env file: ${envFile}`);

switch (command) {
  case "dev":
    runBunScript("dev", mergedEnv, buildTriggerCliArgs(mergedEnv, extraArgs));
    break;

  case "mqtt":
    runBunScript("mqtt", mergedEnv, extraArgs);
    break;

  case "coderabbit":
    runBunScript("coderabbit", mergedEnv, extraArgs);
    break;

  case "deploy":
  case "push":
  case "deploy:staging":
  case "deploy:dry-run":
    runBunScript(command, mergedEnv, buildTriggerCliArgs(mergedEnv, extraArgs));
    break;

  default:
    usage();
    process.exit(1);
}
