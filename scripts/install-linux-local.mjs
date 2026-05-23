import { spawn } from "node:child_process";
import { promises as fs } from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const tauriConfigPath = path.join(repoRoot, "src-tauri", "tauri.conf.json");

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

async function commandExists(command) {
  try {
    await run("bash", ["-lc", `command -v ${command}`], {
      stdio: "ignore",
    });
    return true;
  } catch {
    return false;
  }
}

async function installIcon(sourcePath, destinationDir, iconNames) {
  await fs.mkdir(destinationDir, { recursive: true });

  for (const iconName of iconNames) {
    await fs.copyFile(sourcePath, path.join(destinationDir, `${iconName}.png`));
  }
}

async function copyExecutable(sourcePath, destinationPath) {
  await fs.mkdir(path.dirname(destinationPath), { recursive: true });
  await fs.copyFile(sourcePath, destinationPath);
  await fs.chmod(destinationPath, 0o755);
}

function replaceDesktopEntryValue(lines, key, value) {
  const entry = `${key}=${value}`;
  const index = lines.findIndex((line) => line.startsWith(`${key}=`));

  if (index >= 0) {
    lines[index] = entry;
    return;
  }

  const desktopEntryIndex = lines.findIndex((line) => line.trim() === "[Desktop Entry]");
  if (desktopEntryIndex >= 0) {
    lines.splice(desktopEntryIndex + 1, 0, entry);
    return;
  }

  lines.unshift("[Desktop Entry]", entry);
}

function buildDesktopEntry(templateContent, overrides) {
  const baseContent =
    templateContent ??
    [
      "[Desktop Entry]",
      "Type=Application",
      `Name=${overrides.name}`,
      "Comment=AI-assisted coding workspace",
      "Terminal=false",
      "Categories=Development;IDE;",
      "StartupWMClass=supacodex",
    ].join("\n");

  const lines = baseContent
    .split(/\r?\n/)
    .filter((line, index, all) => !(line === "" && index === all.length - 1));

  replaceDesktopEntryValue(lines, "Type", "Application");
  replaceDesktopEntryValue(lines, "Name", overrides.name);
  replaceDesktopEntryValue(lines, "Exec", overrides.execPath);
  replaceDesktopEntryValue(lines, "TryExec", overrides.execPath);
  replaceDesktopEntryValue(lines, "Icon", overrides.iconName);
  replaceDesktopEntryValue(lines, "Terminal", "false");

  if (!lines.some((line) => line.startsWith("Categories="))) {
    replaceDesktopEntryValue(lines, "Categories", "Development;IDE;");
  }

  return `${lines.join("\n")}\n`;
}

async function refreshDesktopCaches(homeDir) {
  const applicationsDir = path.join(homeDir, ".local", "share", "applications");
  const iconThemeDir = path.join(homeDir, ".local", "share", "icons", "hicolor");

  const refreshCommands = [
    ["update-desktop-database", [applicationsDir]],
    ["gtk-update-icon-cache", ["-f", "-t", iconThemeDir]],
    ["kbuildsycoca6", ["--noincremental"]],
  ];

  for (const [command, args] of refreshCommands) {
    if (!(await commandExists(command))) {
      continue;
    }

    try {
      await run(command, args, { stdio: "ignore" });
    } catch {
      // Ignore cache refresh failures; the install is still usable.
    }
  }
}

if (process.platform !== "linux") {
  throw new Error("This installer only supports Linux.");
}

const skipBuild = process.argv.includes("--skip-build");

if (!skipBuild) {
  await run("node", [path.join(scriptDir, "build-tauri-linux.mjs")]);
}

const tauriConfig = JSON.parse(await fs.readFile(tauriConfigPath, "utf8"));
const { productName, version, identifier } = tauriConfig;

const homeDir = os.homedir();
const installDir = path.join(homeDir, ".local", "opt", productName);
const applicationsDir = path.join(homeDir, ".local", "share", "applications");
const binaryName = "supacodex";
const desktopFileName = `${binaryName}.desktop`;
const sourceBinaryPath = path.join(
  repoRoot,
  "src-tauri",
  "target",
  "release",
  binaryName,
);
const sourceSidecarPath = path.join(
  repoRoot,
  "src-tauri",
  "sidecar-dist",
  "claude-agent-sdk-server.mjs",
);
const installedBinaryPath = path.join(installDir, "bin", binaryName);
const installedSidecarPath = path.join(
  installDir,
  "sidecar-dist",
  "claude-agent-sdk-server.mjs",
);
const iconNames = [
  identifier,
  productName.toLowerCase().replace(/[^a-z0-9]+/g, "-"),
];

await fs.mkdir(installDir, { recursive: true });
await fs.mkdir(applicationsDir, { recursive: true });
await fs.mkdir(path.dirname(installedSidecarPath), { recursive: true });

await copyExecutable(sourceBinaryPath, installedBinaryPath);
await fs.copyFile(sourceSidecarPath, installedSidecarPath);

const iconSources = [
  ["32x32", path.join(repoRoot, "src-tauri", "icons", "32x32.png")],
  ["64x64", path.join(repoRoot, "src-tauri", "icons", "64x64.png")],
  ["128x128", path.join(repoRoot, "src-tauri", "icons", "128x128.png")],
  ["256x256", path.join(repoRoot, "src-tauri", "icons", "128x128@2x.png")],
  ["512x512", path.join(repoRoot, "src-tauri", "icons", "icon.png")],
];

for (const [size, sourcePath] of iconSources) {
  await installIcon(
    sourcePath,
    path.join(homeDir, ".local", "share", "icons", "hicolor", size, "apps"),
    iconNames,
  );
}

const desktopEntry = buildDesktopEntry(null, {
  execPath: installedBinaryPath,
  iconName: binaryName,
  name: productName,
});

const desktopEntryPath = path.join(applicationsDir, desktopFileName);
await fs.rm(path.join(applicationsDir, `${identifier}.desktop`), {
  force: true,
});
await fs.writeFile(desktopEntryPath, desktopEntry, "utf8");
await refreshDesktopCaches(homeDir);

console.log(`Installed ${productName} ${version}`);
console.log(`Binary: ${installedBinaryPath}`);
console.log(`Sidecar: ${installedSidecarPath}`);
console.log(`Desktop entry: ${desktopEntryPath}`);
