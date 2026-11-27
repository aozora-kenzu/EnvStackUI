# Contributing to EnvStackUI

Thank you for your interest in contributing to **EnvStackUI**! All contributions are welcome under the [MIT License](LICENSE).

## Prerequisites

- **OS**: Windows 10 / Windows 11 (64-bit)
- **Node.js**: 20+ (includes `npm`)
- **Rust**: Stable toolchain targeting `x86_64-pc-windows-msvc` with Visual Studio Build Tools (C++ workload)
- **Runtime**: Microsoft Edge WebView2 Runtime (pre-installed on Windows 10/11)

## Getting Started

```bash
git clone https://github.com/aozora-kenzu/EnvStackUI.git
cd EnvStackUI
npm install
```

Run the desktop app in development mode (creates an isolated `.portable_dev/` sandbox beside the repo):

```bash
npm run tauri dev
```

Build the standalone portable executable:

```bash
npm run build:portable
```

## Architecture Overview

- **`src/`** — React 19 + TypeScript + Tailwind CSS v4 frontend
  - `src/views/` — Main feature pages (`OverviewView`, `ServicesView`, `SitesView`, `RuntimesView`, `PackagesView`, `LogsView`, `SettingsView`)
  - `src/components/` — Modals, sidebar, titlebar, command palette, status bar
  - `src/i18n/` — Bilingual English (`en.ts`) and Indonesian (`id.ts`) dictionaries
  - `src/lib/api.ts` & `src/types/envstack.ts` — Typed Tauri IPC bridge
- **`src-tauri/src/`** — Rust + Tauri v2 backend
  - `lib.rs` / `main.rs` — App bootstrap, Tauri commands, system tray, single-instance handling
  - `portable.rs` — Portable directory layout, `envstack.json` config, UAC PowerShell helper
  - `services.rs` — Process lifecycle, port diagnostics, config generators (`httpd.conf`, `nginx.conf`, `php.ini`, `my.ini`)
  - `vhosts.rs`, `ssl.rs`, `dns.rs` — Virtual hosts, Windows `hosts` sync, Local Dev CA/TLS, Wildcard NRPT DNS
  - `packages.rs`, `scanner.rs`, `phpext.rs`, `terminal.rs`, `logs.rs`, `mailpit.rs`, `updater.rs` — Package fetcher/extractor, runtime scanner, PHP extensions, PATH/terminal, logs, Mailpit watcher, and self-updater

## Verification & Pull Request Checklist

Before opening a Pull Request, please verify both frontend typechecking/bundling and backend unit tests pass with zero warnings:

```powershell
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
```

- Keep both `src/i18n/en.ts` and `src/i18n/id.ts` updated if adding UI text.
- Preserve existing portable backward compatibility in `AppConfig` using `#[serde(default)]` where needed.
- Never commit runtime folders (`bin/`, `data/`, `etc/`, `logs/`, `tmp/`, `usr/`, `www/`, `.portable_dev/`, `envstack.json`).

## License

By contributing, you agree that your contributions will be licensed under the [MIT License](LICENSE).
