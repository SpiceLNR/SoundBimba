# Third-party notices / Сторонние компоненты

SoundBimba is licensed under AGPL-3.0-or-later. It uses or installs the components below.
Keep this list up to date: every dependency and bundled resource must appear here.

| Component | Use | License | Source |
|---|---|---|---|
| Virtual Audio Driver (MikeTheTech / VirtualDrivers) | Virtual speaker and microphone, installed alongside, not modified | MIT | https://github.com/VirtualDrivers/Virtual-Audio-Driver |
| Tauri | Application shell | MIT or Apache-2.0 | https://github.com/tauri-apps/tauri |
| React | UI | MIT | https://github.com/facebook/react |
| React Flow (xyflow) | Node map | MIT | https://github.com/xyflow/xyflow |
| Microsoft Edge WebView2 Runtime | UI engine (offline installer bundled in installer) | Microsoft redistributable terms | https://developer.microsoft.com/microsoft-edge/webview2 |
| Selawik (planned, theme "Metro") | Font | SIL OFL 1.1 | verify before bundling |

Before release, generate a complete list from Cargo.lock and package-lock.json and verify each
license is compatible (MIT, Apache-2.0, BSD, MPL-2.0, zlib, ISC, OFL).

Note: the Microsoft WebView2 redistributable is not open source. Check with SignPath Foundation
whether it must be excluded from the signed package (it is a system component installed
separately, not part of the signed binaries).
