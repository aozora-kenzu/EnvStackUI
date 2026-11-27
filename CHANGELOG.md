# Changelog

All notable changes to **EnvStackUI** will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.1.0] - 2026-11-28

### Added
- **Per-Project PHP Version Pinning**: Pin a specific PHP version per site in the **Sites** tab (persisted via `.envstackrc` + `envstack.json`), with isolated `php-cgi` FastCGI workers, per-version `.ini` configs, Apache `mod_proxy_fcgi` routing, and matching Portable Terminal `PATH`.
- **1-Click Create Database**: Dedicated **Create Database** modal accessible from **Services** and **Sites** for MariaDB/MySQL and PostgreSQL with strict identifier validation.
- **PostgreSQL & Redis Support**: Native portable lifecycle management, default port config (`5432` / `6379`), data/config initialization, graceful shutdown, log viewing (`postgres-error.log` / `redis.log`), and online package catalog installers.
- **Built-in `envstack` CLI Mode & MCP Server**: Manage sites, PHP pins, logs, and databases via CLI (`status`, `sites`, `link`, `unlink`, `pin`, `logs`, `db`), or connect AI coding agents via `envstack-ui.exe mcp` over stdio JSON-RPC.
- **Open-Source Infrastructure & CI/CD**: Added GitHub Actions CI & automated Windows portable Release workflows, `CONTRIBUTING.md`, issue/PR templates, `.gitattributes`, and `.editorconfig`.

### Changed
- Transitioned EnvStackUI to Open-Source under the **MIT License**.

## [1.0.2] - 2026-11-27

### Added
- **1-Click Self-Replacing Updater ("Update & Restart")**: Downloads release binaries directly beside `envstack-ui.exe` as `.update.tmp` instead of leaving files in `%TEMP%`.
- Automatic Windows PE (`MZ`/`PE`) header check and SHA-256 verification against `checksums.txt`.
- Atomic rename swap with `.old` backup and automatic rollback on swap/launch failure, plus startup cleanup of leftover `.old` / `.update.tmp` files.
- In-app update notification banner alongside the About modal updater.

## [1.0.1] - 2026-11-26

### Added
- Apache as default HTTP/HTTPS service (`80 / 443`) with `combined` `LogFormat`, `ProxyFCGISetEnvIf`, and **Restart All** button across Sidebar, Overview, Command Palette, Tray, and Context Menu.
- **One-Click PHP Extensions & Xdebug Manager** inside Services for toggling `etc/php/php.ini` extensions without manual edits.
- **Linked Projects** support to register external project folders outside `www/` without moving files.
- **Wildcard DNS (`*.test` + subdomains)** via Windows NRPT + built-in local DNS responder using Base64 UTF-16LE `-EncodedCommand` for reliable UAC elevation.
- Non-blocking async UAC/service operations, pre-start config syntax validation (`httpd -t` / `nginx -t`), and port-conflict holder diagnostics.

## [0.1.0] - 2026-11-26

### Added
- Initial public portable release of **EnvStackUI** (Windows 10/11 x64).
- Core service orchestration for Apache, Nginx, PHP FastCGI, MariaDB/MySQL, and Mailpit with live Mailpit email toast notifications.
- Auto-generated local virtual hosts (`*.test`), local Dev CA & per-site TLS certificates, and Windows `hosts` synchronization.
- Online Portable Package Manager (PHP, Node.js, Python, Nginx, Apache, MariaDB, Go, Mailpit, Composer, Adminer) and external toolchain linking.
- Bilingual UI (English / Bahasa Indonesia), One Dark / One Light themes, Command Palette (`Ctrl+K`), Portable Terminal launcher, and System Tray support.
