import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

import { renderHomebrewCask, resolveMacOsDmgAsset } from "../scripts/lib/homebrew-cask.mjs";

describe("resolveMacOsDmgAsset", () => {
  it("returns the only macOS DMG asset", () => {
    const asset = resolveMacOsDmgAsset([
      { name: "SupaCodex_0.30.0_universal.dmg.sig" },
      { name: "SupaCodex_0.30.0_universal.dmg", browser_download_url: "https://example.com/SupaCodex.dmg" },
    ]);

    expect(asset).toEqual({
      name: "SupaCodex_0.30.0_universal.dmg",
      browser_download_url: "https://example.com/SupaCodex.dmg",
    });
  });

  it("throws when the release has no macOS DMG asset", () => {
    expect(() =>
      resolveMacOsDmgAsset([{ name: "SupaCodex_0.30.0_universal.dmg.sig" }]),
    ).toThrow("Expected exactly one macOS DMG asset, found none.");
  });

  it("throws when the release has multiple macOS DMG assets", () => {
    expect(() =>
      resolveMacOsDmgAsset([
        { name: "SupaCodex_0.30.0_universal.dmg" },
        { name: "SupaCodex_0.30.0_backup.dmg" },
      ]),
    ).toThrow("Expected exactly one macOS DMG asset");
  });
});

describe("renderHomebrewCask", () => {
  it("renders the cask with version, checksum, and URL", () => {
    const template = [
      'cask "supacodex" do',
      '  version "__VERSION__"',
      '  sha256 "__SHA256__"',
      '  url "__URL__"',
      "end",
      "",
    ].join("\n");

    const rendered = renderHomebrewCask(template, {
      version: "0.30.0",
      sha256: "abc123",
      url: "https://example.com/SupaCodex_0.30.0_universal.dmg",
    });

    expect(rendered).toContain('version "0.30.0"');
    expect(rendered).toContain('sha256 "abc123"');
    expect(rendered).toContain('url "https://example.com/SupaCodex_0.30.0_universal.dmg"');
  });

  it("fails when the template is missing required placeholders", () => {
    expect(() =>
      renderHomebrewCask('cask "supacodex" do\n  version "__VERSION__"\nend\n', {
        version: "0.30.0",
        sha256: "abc123",
        url: "https://example.com/SupaCodex_0.30.0_universal.dmg",
      }),
    ).toThrow("Template is missing placeholder __SHA256__");
  });

  it("renders the shipped cask without an architecture restriction", () => {
    const template = readFileSync(new URL("../scripts/templates/homebrew-cask.rb.tpl", import.meta.url), "utf-8");

    const rendered = renderHomebrewCask(template, {
      version: "0.30.0",
      sha256: "abc123",
      url: "https://example.com/SupaCodex_0.30.0_universal.dmg",
    });

    expect(rendered).not.toContain("depends_on arch:");
  });
});
