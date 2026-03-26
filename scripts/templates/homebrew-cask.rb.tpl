cask "supacodex" do
  version "__VERSION__"
  sha256 "__SHA256__"

  url "__URL__"
  name "Panes"
  desc "Local-first cockpit for AI-assisted coding"
  homepage "https://github.com/replace-with-your-fork/supacodex"

  app "SupaCodex.app"

  postflight do
    # Best-effort friction reduction for unsigned / unnotarized builds.
    system_command "/usr/bin/xattr",
      args: ["-dr", "com.apple.quarantine", "#{appdir}/SupaCodex.app"]
  end

  zap trash: [
    "~/.supacodex",
  ]
end
