<p align="center"><img src="assets/logo.png" alt="SoundBimba" width="160"></p>

# SoundBimba

[Русский](README.md) · **English** · [Українська](README.uk.md)

SoundBimba is a free Windows program where you build sound like a diagram: sources on the left,
outputs on the right, wires in between. No internet, no ads, no telemetry.

- **App sound** — a copy of a game, player or browser; you keep hearing it as before.
- **Microphone**, **sound buttons** (a soundpad), **volume** on any wire.
- **Outputs:** headphones and a **virtual microphone** you pick in Discord, OBS or a game.
- **Button panel** — a separate always-on-top window; **simple mode** without the map.
- **Ready-made setups** in one click and a virtual cable wizard for beginners. Save your own maps
  as templates — `.bmb` files that are easy to share (they open with a double click).
- Languages: Russian, English, Ukrainian. Themes "Bimba" and "Metro".
- Hotkeys work only inside the SoundBimba window (they don't interfere with games) and can be remapped.

## Requirements

- **Windows 11, 64-bit.** Windows 10 has not been tested.
- For the virtual microphone — the free **VB-CABLE** from the official site
  [vb-audio.com/Cable](https://vb-audio.com/Cable/). SoundBimba neither installs nor contains
  it: you install the cable yourself, once. The program has step-by-step instructions
  ("Project → Virtual cable…").

## Installation

1. Download `SoundBimba_…_x64-setup.exe` and `SHA256SUMS.txt` from
   [Releases](https://github.com/SpiceLNR/SoundBimba/releases).
2. Optionally check the file was not tampered with: in PowerShell run
   `Get-FileHash .\SoundBimba_0.5.0_x64-setup.exe` — the value must match `SHA256SUMS.txt`.
3. Run the installer. No administrator rights, no internet needed: the window engine
   (Microsoft WebView2) is inside the installer.

**Why Windows says "Windows protected your PC".** Releases are not signed yet (see "Code signing
policy" below). Click "More info" → "Run anyway". If in doubt, compare the SHA-256 (step 2) or build
the program yourself.

**Uninstall:** "Settings → Apps → SoundBimba → Uninstall". Your setups and settings are in
`%APPDATA%\SoundBimba\` — delete that folder if you no longer need them.

## First steps

1. On first launch choose "Use a ready-made setup", e.g. "Sound buttons in Discord".
2. Click a node — its settings appear on the right: which app, which device, which sound.
3. Turn the sound on with the "Sound off" button at the top left.
4. In Discord (or OBS, a game) pick the **"CABLE Output"** microphone.

Latency to the virtual microphone is about 90 ms, of which about 70 ms is added by VB-CABLE
itself. Check yours: "Project → Virtual cable… → Test the link".

If something doesn't work, use "Project → Save diagnostics" and attach the zip to an
[issue](https://github.com/SpiceLNR/SoundBimba/issues). It contains the program log, your setup
and settings, Windows and sound device information. Your sound files are not included, but their
paths are (a path may contain your Windows user name) — look through the zip before sending it.

## Code signing policy

Releases are currently **not signed** — each has a `SHA256SUMS.txt` for checking. Later, once the
project has users, we plan to apply for free open-source code signing from
[SignPath Foundation](https://signpath.org). Installers will then be signed in the GitHub build
([.github/workflows/build.yml](.github/workflows/build.yml)), and every signing request will be approved
manually. There are no keys or certificates in the repository and there never will be.

- Committers and reviewers: [SpiceLNR](https://github.com/SpiceLNR)
- Approvers: [SpiceLNR](https://github.com/SpiceLNR)

**Privacy policy.** This program will not transfer any information to other networked systems unless
specifically requested by the user or the person installing or operating it. SoundBimba does not go
online at all; "Save diagnostics" writes a file only where you choose.

## Building from source

You need Rust (MSVC), Node.js 24, Visual Studio 2022 with the C++ workload and the Windows 11 SDK.
Run the commands in PowerShell (not Git Bash):

```powershell
cd app
npm ci
npx tauri build
```

The installer ends up in `target\release\bundle\nsis\`. Tests: `cargo test --workspace` and
`npm test` (in `app`). The design is described in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) (Russian).

## License

[AGPL-3.0-or-later](LICENSE). Third-party components and their licenses are listed in
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
