import { spawn } from "node:child_process";
import { mkdir, readFile, rm, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const viteBinPath = path.join(repoRoot, "node_modules", "vite", "bin", "vite.js");
const orphanGuardPath = path.join(scriptDir, "vite-orphan-guard.cjs");
const pidFilePath = path.join(
  repoRoot,
  "node_modules",
  ".cache",
  "supacodex-tauri-dev-vite.json",
);
const devPorts = [1420, 1421];
const isWindows = process.platform === "win32";

let child = null;
let shuttingDown = false;

function sleep(ms) {
  return new Promise((resolve) => {
    setTimeout(resolve, ms);
  });
}

function pidExists(pid) {
  if (!Number.isInteger(pid) || pid <= 0) {
    return false;
  }

  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    return error?.code === "EPERM";
  }
}

function runCapture(command, args, { allowFailure = false } = {}) {
  return new Promise((resolve, reject) => {
    let stdout = "";
    let stderr = "";
    let settled = false;
    const proc = spawn(command, args, {
      cwd: repoRoot,
      stdio: ["ignore", "pipe", "pipe"],
      windowsHide: true,
    });

    proc.stdout.on("data", (chunk) => {
      stdout += chunk.toString();
    });
    proc.stderr.on("data", (chunk) => {
      stderr += chunk.toString();
    });

    proc.on("error", (error) => {
      if (settled) {
        return;
      }
      settled = true;
      if (allowFailure) {
        resolve({ ok: false, stdout, stderr, error, code: null, signal: null });
        return;
      }
      reject(error);
    });

    proc.on("exit", (code, signal) => {
      if (settled) {
        return;
      }
      settled = true;
      const result = { ok: code === 0, stdout, stderr, code, signal };
      if (code === 0 || allowFailure) {
        resolve(result);
        return;
      }

      reject(
        new Error(
          signal
            ? `${command} ${args.join(" ")} exited with signal ${signal}`
            : `${command} ${args.join(" ")} exited with code ${code}\n${stderr}`.trim(),
        ),
      );
    });
  });
}

async function readPidFile() {
  try {
    const raw = await readFile(pidFilePath, "utf8");
    return JSON.parse(raw);
  } catch {
    return null;
  }
}

async function removePidFile() {
  await rm(pidFilePath, { force: true });
}

async function writePidFile(pid) {
  await mkdir(path.dirname(pidFilePath), { recursive: true });
  await writeFile(
    pidFilePath,
    `${JSON.stringify({
      pid,
      repoRoot,
      viteBinPath,
      createdAt: new Date().toISOString(),
    })}\n`,
    "utf8",
  );
}

function normalizePathLike(value) {
  return value.replaceAll("\\", "/");
}

function isRepoViteCommand(commandLine) {
  if (!commandLine) {
    return false;
  }

  const normalizedCommand = normalizePathLike(commandLine);
  const normalizedRepoRoot = normalizePathLike(repoRoot);
  return (
    normalizedCommand.includes(normalizedRepoRoot) &&
    normalizedCommand.includes("/vite/bin/vite.js")
  );
}

async function readProcessCommandLine(pid) {
  if (!Number.isInteger(pid) || pid <= 0) {
    return "";
  }

  if (isWindows) {
    const script = [
      `$process = Get-CimInstance Win32_Process -Filter "ProcessId = ${pid}"`,
      "if ($null -eq $process) { exit 1 }",
      "Write-Output $process.CommandLine",
    ].join("; ");
    const result = await runCapture(
      "powershell.exe",
      ["-NoLogo", "-NoProfile", "-NonInteractive", "-Command", script],
      { allowFailure: true },
    );
    return result.stdout.trim();
  }

  const result = await runCapture(
    "ps",
    ["-o", "args=", "-p", String(pid)],
    { allowFailure: true },
  );
  return result.stdout.trim();
}

async function listListeningPids(port) {
  if (isWindows) {
    const result = await runCapture("netstat.exe", ["-ano", "-p", "tcp"], {
      allowFailure: true,
    });
    return result.stdout
      .split(/\r?\n/)
      .map((line) => line.trim())
      .filter((line) => line.includes("LISTENING"))
      .filter((line) => line.match(new RegExp(`[:.]${port}\\s`)))
      .map((line) => {
        const parts = line.split(/\s+/);
        return Number.parseInt(parts.at(-1) ?? "", 10);
      })
      .filter((pid) => Number.isInteger(pid) && pid > 0);
  }

  const result = await runCapture(
    "lsof",
    ["-nP", `-iTCP:${port}`, "-sTCP:LISTEN", "-t"],
    { allowFailure: true },
  );
  return result.stdout
    .split(/\r?\n/)
    .map((value) => Number.parseInt(value.trim(), 10))
    .filter((pid) => Number.isInteger(pid) && pid > 0);
}

async function terminatePid(pid) {
  if (!pidExists(pid)) {
    return;
  }

  if (isWindows) {
    await runCapture("taskkill.exe", ["/PID", String(pid), "/T", "/F"], {
      allowFailure: true,
    });
    return;
  }

  try {
    process.kill(pid, "SIGTERM");
  } catch (error) {
    if (error?.code !== "ESRCH") {
      throw error;
    }
    return;
  }

  for (let attempt = 0; attempt < 20; attempt += 1) {
    if (!pidExists(pid)) {
      return;
    }
    await sleep(100);
  }

  try {
    process.kill(pid, "SIGKILL");
  } catch (error) {
    if (error?.code !== "ESRCH") {
      throw error;
    }
  }
}

async function cleanupStaleVite() {
  const candidatePids = new Set();
  const stored = await readPidFile();
  if (Number.isInteger(stored?.pid) && stored.pid > 0) {
    candidatePids.add(stored.pid);
  }

  for (const port of devPorts) {
    for (const pid of await listListeningPids(port)) {
      candidatePids.add(pid);
    }
  }

  for (const pid of candidatePids) {
    if (pid === process.pid) {
      continue;
    }

    const commandLine = await readProcessCommandLine(pid);
    if (!isRepoViteCommand(commandLine)) {
      continue;
    }

    console.warn(`[tauri-dev-vite] Stopping stale Vite process ${pid}.`);
    await terminatePid(pid);
  }

  await removePidFile();
}

async function stopChild(exitCode) {
  if (shuttingDown) {
    return;
  }
  shuttingDown = true;

  const childPid = child?.pid;
  await removePidFile();

  if (Number.isInteger(childPid) && pidExists(childPid)) {
    await terminatePid(childPid);
  }

  process.exit(exitCode);
}

function installSignalHandlers() {
  for (const signal of ["SIGINT", "SIGTERM", "SIGHUP"]) {
    process.on(signal, () => {
      void stopChild(0);
    });
  }

  process.on("uncaughtException", (error) => {
    console.error("[tauri-dev-vite] Uncaught exception:", error);
    void stopChild(1);
  });

  process.on("unhandledRejection", (reason) => {
    console.error("[tauri-dev-vite] Unhandled rejection:", reason);
    void stopChild(1);
  });

  process.on("exit", () => {
    if (shuttingDown || !Number.isInteger(child?.pid) || !pidExists(child.pid)) {
      return;
    }

    try {
      process.kill(child.pid, "SIGTERM");
    } catch {
      // Best-effort cleanup during process teardown.
    }
  });
}

async function main() {
  await cleanupStaleVite();
  installSignalHandlers();

  child = spawn(
    process.execPath,
    ["-r", orphanGuardPath, viteBinPath, ...process.argv.slice(2)],
    {
      cwd: repoRoot,
      stdio: "inherit",
      windowsHide: true,
      env: {
        ...process.env,
        SUPACODEX_VITE_GUARD_PARENT_PID: String(process.pid),
      },
    },
  );

  if (!Number.isInteger(child.pid) || child.pid <= 0) {
    throw new Error("Vite process did not expose a PID");
  }

  child.on("error", (error) => {
    console.error("[tauri-dev-vite] Failed to launch Vite:", error);
    void stopChild(1);
  });

  child.on("exit", (code, signal) => {
    void (async () => {
      await removePidFile();

      if (shuttingDown) {
        process.exit(0);
        return;
      }

      if (signal) {
        console.error(`[tauri-dev-vite] Vite exited with signal ${signal}.`);
        process.exit(1);
        return;
      }

      process.exit(code ?? 1);
    })();
  });

  await writePidFile(child.pid);
}

await main();
