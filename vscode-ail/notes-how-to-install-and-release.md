There are two distinct paths: local dev install (for dogfooding right now) and marketplace publish (for distribution). Let me walk through both, and
   flag the gaps that still need filling.

  ---
  Local install (dev/dogfood)

  Option A: Extension Development Host (fastest — no packaging)

  # 1. Build the ail binary
  cargo build --release

  # 2. Install extension deps and compile TS
  cd vscode-ail && npm install && npm run compile

  # 3. Open the extension folder in VS Code
  code .

  # 4. Press F5 — opens a new "Extension Development Host" window
  #    with the extension loaded. No install needed.

  Then set in VS Code settings:
  "ail.binaryPath": "/home/achesser/src/github.com/alexchesser/ail/target/release/ail"

  Option B: Install as a real .vsix

  cd vscode-ail
  npm install -g @vscode/vsce    # one-time
  vsce package                   # produces ail-0.1.0.vsix
  code --install-extension ail-0.1.0.vsix

  This installs it as a real extension but won't have a bundled binary — it will fall back to ail on PATH, or you set ail.binaryPath.

  ---
  Marketplace publish

  One-time setup (gaps that need filling)

  1. Publisher account

  Register at https://marketplace.visualstudio.com/manage. The publisher name must match package.json:
  "publisher": "alexchesser"
  If you register a different name, update that field.

  2. Azure PAT (Personal Access Token)

  At https://dev.azure.com → User Settings → Personal Access Tokens → New Token:
  - Scope: Marketplace → Manage
  - Store it as a GitHub secret named VSCE_PAT

  3. Enable auto-publish in CI

  In GitHub repo settings → Variables → add:
  PUBLISH_TO_MARKETPLACE = true
  The release workflow already checks this before the publish step.

  Manual first publish

  cd vscode-ail
  npm install -g @vscode/vsce
  vsce login alexchesser           # prompts for PAT
  vsce publish                     # publishes without bundled binary

  Or to publish with bundled binary (matches what CI does):
  cargo build --release
  cp target/release/ail vscode-ail/dist/ail-x86_64-unknown-linux-musl
  vsce package --target linux-x64
  vsce publish --packagePath ail-linux-x64-0.1.0.vsix

  Trigger a full release via CI

  git tag vscode-v0.1.0
  git push origin vscode-v0.1.0

  This triggers release-extension.yml which cross-compiles all 4 binaries, bundles them, and creates the GitHub Release with platform-specific VSIXes.
   If PUBLISH_TO_MARKETPLACE=true, it also pushes to the Marketplace.

  ---
  What still needs to happen before marketplace

  Two small gaps in the current state:

  1. package-lock.json doesn't exist yet — run npm install in vscode-ail/ once to generate it (the CI workflow uses npm ci which requires it)
  2. language-configuration.json is declared in package.json but not created — VS Code will warn on activation. Either create a minimal one or remove
  the languages contribution from package.json for now

  To fix #2 quickly:

● Write(language-configuration.json)
  ⎿  Wrote 15 lines to language-configuration.json
       1 {
       2   "comments": {
       3     "lineComment": "#"
       4   },
       5   "brackets": [
       6     ["{", "}"],
       7     ["[", "]"]
       8   ],
       9   "autoClosingPairs": [
      10     { "open": "{", "close": "}" },
     … +5 lines (ctrl+o to expand)

● Bash(npm install 2>&1 | tail -3 && ls package-lock.json)
  ⎿    npm audit fix

     Run `npm audit` for details.
     package-lock.json

● Both gaps are now closed. To summarize the minimum path to Marketplace:

  1. Register publisher at marketplace.visualstudio.com/manage (match name in package.json)
  2. Get an Azure PAT with Marketplace:Manage scope
  3. git tag vscode-v0.1.0 && git push origin vscode-v0.1.0 — CI does the rest
  4. Set VSCE_PAT as a GitHub secret and PUBLISH_TO_MARKETPLACE=true as a repo variable for auto-publish, or run vsce publish manually from the built
  VSIX

✻ Baked for 58s