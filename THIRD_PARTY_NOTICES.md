# Third-party notices / Сторонние компоненты

SoundBimba is licensed under AGPL-3.0-or-later. It uses or installs the components below.
Keep this list up to date: every dependency and bundled resource must appear here.

| Component | Use | License | Source |
|---|---|---|---|
| VB-CABLE (VB-Audio) — **not bundled** | Recommended virtual cable; the user installs it from the official site, SoundBimba only detects its devices by name | Donationware, proprietary (not distributed by us) | https://vb-audio.com/Cable/ |
| Tauri | Application shell | MIT or Apache-2.0 | https://github.com/tauri-apps/tauri |
| React | UI | MIT | https://github.com/facebook/react |
| React Flow (xyflow) | Node map | MIT | https://github.com/xyflow/xyflow |
| NSIS (Nullsoft Scriptable Install System) | Installer (`*-setup.exe`), downloaded by the Tauri bundler at build time | zlib/libpng (see NSIS COPYING) | https://nsis.sourceforge.io |
| nsis-tauri-utils | Helper plugin inside the installer | MIT or Apache-2.0 | https://github.com/tauri-apps/nsis-tauri-utils |
| Microsoft Edge WebView2 Runtime | UI engine (offline installer bundled in installer) | Microsoft redistributable terms | https://developer.microsoft.com/microsoft-edge/webview2 |
| Tauri single-instance plugin (`tauri-plugin-single-instance`) | One running copy; passes .bmb files opened in Explorer to it | MIT or Apache-2.0 | https://github.com/tauri-apps/plugins-workspace |
| Tauri dialog plugin (`tauri-plugin-dialog`, `@tauri-apps/plugin-dialog`) | File dialogs for export, import, diagnostics | MIT or Apache-2.0 | https://github.com/tauri-apps/plugins-workspace |
| serde, serde_json | Project file (de)serialization | MIT or Apache-2.0 | https://github.com/serde-rs |
| rtrb | Real-time safe ring buffers between audio devices and the engine | MIT or Apache-2.0 | https://github.com/mgeier/rtrb |
| nnnoiseless (Rust port of RNNoise, with its trained model) | Noise suppression node | BSD-3-Clause | https://github.com/jneem/nnnoiseless (RNNoise: https://github.com/xiph/rnnoise) |
| rustfft, realfft, easyfft, generic_singleton, transpose, strength_reduce, primal-check, num-complex, anymap3 (nnnoiseless dependencies) | FFT for noise suppression | MIT or Apache-2.0 (realfft: MIT) | https://crates.io |
| Symphonia (`symphonia` and its codec crates: WAV/PCM, MP3, OGG/Vorbis, FLAC) | Decoding sound files for buttons | MPL-2.0 | https://github.com/pdeljanov/Symphonia |
| Nunito font (via Fontsource) | Theme "Bimba" font, bundled offline | SIL OFL 1.1 | https://github.com/googlefonts/nunito , https://fontsource.org |
| Open Sans font (via Fontsource) | Theme "Metro" font (free alternative to Segoe UI), bundled offline | SIL OFL 1.1 | https://github.com/googlefonts/opensans , https://fontsource.org |
| Vite, @vitejs/plugin-react, TypeScript, Vitest | Build and tests only, not in the app | MIT / Apache-2.0 | https://vitejs.dev , https://www.typescriptlang.org , https://vitest.dev |
| ICU4X crates (`icu_*`, `zerovec`, `yoke`, `litemap`, `tinystr`, `writeable`, `zerotrie`, `zerofrom`, `potential_utf`) | Pulled in by Tauri (URL handling) | Unicode-3.0 (permissive, OSI-approved; allowed by the author on 2026-11-02) | https://github.com/unicode-org/icu4x |
| windows-rs: `windows`, `windows-core` and their sub-crates (`windows-link`, `-result`, `-strings`, `-implement`, `-interface`, `-collections`, `-future`, `-numerics`, `-threading`) | Windows API bindings (tools/probe) | MIT or Apache-2.0 | https://github.com/microsoft/windows-rs |
| `proc-macro2`, `quote`, `syn` | Build-time only (macros of windows-rs), not in the binary | MIT or Apache-2.0 | https://github.com/dtolnay |
| `unicode-ident` | Build-time only (used by `syn`), not in the binary | (MIT or Apache-2.0) and Unicode-3.0 (permissive, OSI-approved) | https://github.com/dtolnay/unicode-ident |

Automatic check of every dependency (including nested ones):
`node tools/licenses/check-cargo.mjs` (from the repo root) and `npm run licenses` (from `app/`).
Allowed: MIT, Apache-2.0, BSD, MPL-2.0, zlib, ISC, OFL, Unicode-3.0. Anything else is printed as REVIEW.
Before release, also generate a full notices file from Cargo.lock and package-lock.json.

Note: the Microsoft WebView2 redistributable is not open source. Check with SignPath Foundation
whether it must be excluded from the signed package (it is a system component installed
separately, not part of the signed binaries).
