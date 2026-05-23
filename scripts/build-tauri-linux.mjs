import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const perlShimDir = path.join(scriptDir, "perl5");

function run(command, args, options = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, {
      cwd: repoRoot,
      stdio: "inherit",
      shell: process.platform === "win32",
      windowsHide: true,
      ...options,
    });

    child.on("error", reject);
    child.on("exit", (code, signal) => {
      if (code === 0) {
        resolve();
        return;
      }

      reject(
        new Error(
          signal
            ? `${command} ${args.join(" ")} exited with signal ${signal}`
            : `${command} ${args.join(" ")} exited with code ${code}`,
        ),
      );
    });
  });
}

function commandSucceeds(command, args, options = {}) {
  return new Promise((resolve) => {
    const child = spawn(command, args, {
      cwd: repoRoot,
      stdio: "ignore",
      shell: process.platform === "win32",
      windowsHide: true,
      ...options,
    });

    child.on("error", () => resolve(false));
    child.on("exit", (code) => resolve(code === 0));
  });
}

function prependPathList(entry, value) {
  return value ? `${entry}${path.delimiter}${value}` : entry;
}

const tauriArgs =
  process.argv.length > 2
    ? process.argv.slice(2)
    : ["build", "--bundles", "deb,appimage"];

const env = { ...process.env };

if (process.platform === "linux") {
  const perlHasFindBin = await commandSucceeds("perl", ["-MFindBin", "-e", "1"]);
  const hasSystemOpenSsl = await commandSucceeds("pkg-config", [
    "--exists",
    "openssl",
  ]);

  if (!perlHasFindBin && hasSystemOpenSsl && !env.OPENSSL_NO_VENDOR) {
    env.OPENSSL_NO_VENDOR = "1";
    console.log(
      "Using system OpenSSL because the active Perl installation lacks FindBin.",
    );
  } else if (!perlHasFindBin) {
    env.PERL5LIB = prependPathList(perlShimDir, env.PERL5LIB);
    console.log("Using local FindBin shim for OpenSSL vendor builds.");
  }
}

async function runCargoFallback() {
  console.log("Falling back to direct frontend + cargo release build.");
  await run("pnpm", ["run", "build"]);
  await run("pnpm", ["run", "build:claude-sidecar"]);
  await run("cargo", ["build", "--release"], {
    cwd: path.join(repoRoot, "src-tauri"),
    env,
  });
}

try {
  await run("pnpm", ["tauri", ...tauriArgs], { env });
} catch (error) {
  console.warn(
    `Tauri bundle build failed (${error instanceof Error ? error.message : String(error)}).`,
  );
  await runCargoFallback();
}
