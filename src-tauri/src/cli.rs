use crate::logs;
use crate::mcp;
use crate::portable::{load_config, save_config, PortablePaths, APP_VERSION};
use crate::scanner;
use crate::services::{self, ServiceManager, ALL_SERVICES};
use crate::vhosts;
use anyhow::{anyhow, Result};
use std::path::Path;

#[cfg(windows)]
fn attach_parent_console_if_needed() {
    const ATTACH_PARENT_PROCESS: u32 = 0xFFFF_FFFF;
    const STD_OUTPUT_HANDLE: u32 = (-11i32) as u32;
    const FILE_TYPE_UNKNOWN: u32 = 0;

    extern "system" {
        fn AttachConsole(dwProcessId: u32) -> i32;
        fn GetStdHandle(nStdHandle: u32) -> isize;
        fn GetFileType(hFile: isize) -> u32;
    }

    unsafe {
        let out_handle = GetStdHandle(STD_OUTPUT_HANDLE);
        if out_handle == 0 || out_handle == -1 || GetFileType(out_handle) == FILE_TYPE_UNKNOWN {
            let _ = AttachConsole(ATTACH_PARENT_PROCESS);
        }
    }
}

fn print_help() {
    println!(
        "EnvStackUI v{APP_VERSION} — Portable CLI & MCP Server\n\n\
USAGE:\n  envstack <COMMAND> [OPTIONS]\n\n\
COMMANDS:\n  status [--json]             Show service ports, listening state, and active versions\n  sites [--json]              List local and linked virtual host sites\n  link <path>                 Link an external project folder into virtual hosts\n  unlink <path>               Unlink an external project folder\n  pin <domain> <ver|default>  Pin a PHP version for a site (or reset to global default)\n  logs [name] [--lines N]     List log files or print trailing lines of a log\n  db list [mysql|postgres]    List user databases (default: mysql)\n  db create <name> [engine]   Create a database in MySQL/MariaDB or PostgreSQL\n  mcp                         Start built-in stdio MCP server for AI agents\n  --version | -v              Print application version\n  --help | -h                 Print this help message"
    );
}

pub fn try_handle_cli() -> Option<i32> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args.first().map(String::as_str) == Some("--wait-pid") {
        return None;
    }

    let cmd = args[0].as_str();
    if cmd == "mcp" || cmd == "--mcp" {
        return Some(mcp::run_mcp_stdio_server());
    }

    #[cfg(windows)]
    attach_parent_console_if_needed();

    let exit_code = match run_cli_command(&args) {
        Ok(()) => 0,
        Err(err) => {
            eprintln!("Error: {err:#}");
            1
        }
    };
    Some(exit_code)
}

fn run_cli_command(args: &[String]) -> Result<()> {
    match args[0].as_str() {
        "help" | "--help" | "-h" => {
            print_help();
            Ok(())
        }
        "--version" | "-v" | "version" => {
            println!("envstack {APP_VERSION}");
            Ok(())
        }
        "status" => {
            let json_mode = args.iter().any(|a| a == "--json");
            let paths = PortablePaths::resolve()?;
            let mut cfg = load_config(&paths);
            let (inv, _) = scanner::scan_inventory(&paths, &mut cfg);

            let rows: Vec<serde_json::Value> = ALL_SERVICES
                .iter()
                .map(|id| {
                    let port = match *id {
                        "apache" => cfg.ports.apache_http,
                        "nginx" => cfg.ports.nginx_http,
                        "php" => cfg.ports.php_cgi,
                        "mysql" => cfg.ports.mysql,
                        "postgres" => cfg.ports.postgres,
                        "redis" => cfg.ports.redis,
                        "mailpit" => cfg.ports.mailpit_http,
                        _ => 0,
                    };
                    let bin = scanner::get_active_binary(&inv, &cfg, id);
                    let listening = port > 0 && !ServiceManager::is_port_free(port);
                    serde_json::json!({
                        "service": id,
                        "installed": bin.is_some(),
                        "version": bin.map(|b| b.version.clone()),
                        "port": port,
                        "listening": listening,
                    })
                })
                .collect();

            if json_mode {
                let payload = serde_json::json!({
                    "version": APP_VERSION,
                    "root": paths.root.to_string_lossy(),
                    "tld": cfg.tld,
                    "services": rows,
                });
                println!("{}", serde_json::to_string_pretty(&payload)?);
            } else {
                println!("EnvStackUI v{APP_VERSION} ({})", paths.root.display());
                for row in rows {
                    let svc = row["service"].as_str().unwrap_or("");
                    let ver = row["version"].as_str().unwrap_or("-");
                    let port = row["port"].as_u64().unwrap_or(0);
                    let state = if row["listening"].as_bool().unwrap_or(false) {
                        "LISTENING"
                    } else if row["installed"].as_bool().unwrap_or(false) {
                        "stopped"
                    } else {
                        "not installed"
                    };
                    println!("  {svc:<10} {state:<14} port={port:<5} version={ver}");
                }
            }
            Ok(())
        }
        "sites" => {
            let json_mode = args.iter().any(|a| a == "--json");
            let paths = PortablePaths::resolve()?;
            let mut cfg = load_config(&paths);
            let (inv, _) = scanner::scan_inventory(&paths, &mut cfg);
            let www_root = paths.active_www_root(&cfg);
            let web_ports = (cfg.ports.apache_http, cfg.ports.apache_https);
            let sites = vhosts::scan_sites(&www_root, &cfg, &inv, &paths.etc_ssl, &paths.root, web_ports);

            if json_mode {
                println!("{}", serde_json::to_string_pretty(&sites)?);
            } else if sites.is_empty() {
                println!("No sites found in {}", www_root.display());
            } else {
                for s in sites {
                    let php = s.php_version_label.as_deref().unwrap_or("global");
                    println!("  {}  [{}]  PHP={}  {}", s.domain, s.framework, php, s.path);
                }
            }
            Ok(())
        }
        "link" => {
            let target = args.get(1).ok_or_else(|| anyhow!("Usage: envstack link <path>"))?;
            let trimmed = target.trim().trim_end_matches(['\\', '/']).to_string();
            if !Path::new(&trimmed).is_dir() {
                return Err(anyhow!("Folder does not exist: {trimmed}"));
            }
            let paths = PortablePaths::resolve()?;
            let mut cfg = load_config(&paths);
            if !cfg.linked_sites.iter().any(|p| p.eq_ignore_ascii_case(&trimmed)) {
                cfg.linked_sites.push(trimmed.clone());
                save_config(&paths, &cfg)?;
            }
            let (inv, _) = scanner::scan_inventory(&paths, &mut cfg);
            let www_root = paths.active_www_root(&cfg);
            let sites = vhosts::scan_sites(
                &www_root,
                &cfg,
                &inv,
                &paths.etc_ssl,
                &paths.root,
                (cfg.ports.apache_http, cfg.ports.apache_https),
            );
            vhosts::generate_vhost_configs(&paths, &cfg, &sites, true)?;
            println!("Linked site: {trimmed}");
            Ok(())
        }
        "unlink" => {
            let target = args.get(1).ok_or_else(|| anyhow!("Usage: envstack unlink <path>"))?;
            let paths = PortablePaths::resolve()?;
            let mut cfg = load_config(&paths);
            cfg.linked_sites.retain(|p| !p.eq_ignore_ascii_case(target));
            save_config(&paths, &cfg)?;
            let (inv, _) = scanner::scan_inventory(&paths, &mut cfg);
            let www_root = paths.active_www_root(&cfg);
            let sites = vhosts::scan_sites(
                &www_root,
                &cfg,
                &inv,
                &paths.etc_ssl,
                &paths.root,
                (cfg.ports.apache_http, cfg.ports.apache_https),
            );
            vhosts::generate_vhost_configs(&paths, &cfg, &sites, true)?;
            println!("Unlinked site: {target}");
            Ok(())
        }
        "pin" => {
            let domain = args
                .get(1)
                .ok_or_else(|| anyhow!("Usage: envstack pin <domain> <version|default>"))?
                .trim()
                .to_lowercase();
            let ver = args
                .get(2)
                .ok_or_else(|| anyhow!("Usage: envstack pin <domain> <version|default>"))?
                .trim();

            let paths = PortablePaths::resolve()?;
            let mut cfg = load_config(&paths);
            let (inv, _) = scanner::scan_inventory(&paths, &mut cfg);
            let php_bins = inv.binaries.get("php").map(Vec::as_slice).unwrap_or(&[]);

            let unpin = ver.eq_ignore_ascii_case("default") || ver.eq_ignore_ascii_case("global");
            let selected = if unpin {
                cfg.site_php_versions.remove(&domain);
                None
            } else {
                let bin = vhosts::find_php_binary_for_pin(ver, php_bins)
                    .ok_or_else(|| anyhow!("No installed PHP matching '{ver}'"))?
                    .clone();
                cfg.site_php_versions.insert(domain.clone(), bin.id.clone());
                Some(bin)
            };

            save_config(&paths, &cfg)?;
            let www_root = paths.active_www_root(&cfg);
            let sites = vhosts::scan_sites(
                &www_root,
                &cfg,
                &inv,
                &paths.etc_ssl,
                &paths.root,
                (cfg.ports.apache_http, cfg.ports.apache_https),
            );
            if let Some(site) = sites.iter().find(|s| s.domain.eq_ignore_ascii_case(&domain)) {
                vhosts::write_envstackrc_php(
                    Path::new(&site.path),
                    selected.as_ref().map(|b| b.version.as_str()),
                )?;
            }
            vhosts::generate_vhost_configs(&paths, &cfg, &sites, true)?;
            match selected {
                Some(b) => println!("Pinned {domain} to PHP {}", b.version),
                None => println!("Reset {domain} to global PHP default"),
            }
            Ok(())
        }
        "logs" => {
            let paths = PortablePaths::resolve()?;
            let mut target_name: Option<&str> = None;
            let mut lines: usize = 100;
            let mut idx = 1;
            while idx < args.len() {
                if args[idx] == "--lines" {
                    if let Some(val) = args.get(idx + 1).and_then(|s| s.parse::<usize>().ok()) {
                        lines = val;
                    }
                    idx += 2;
                } else {
                    target_name = Some(args[idx].as_str());
                    idx += 1;
                }
            }

            if let Some(name) = target_name {
                let tail = logs::read_log_tail(&paths.logs, name, lines);
                println!("{}", tail.content);
            } else {
                for log in logs::list_logs(&paths.logs) {
                    println!("  {:<20} {:>8} B  exists={}", log.name, log.size_bytes, log.exists);
                }
            }
            Ok(())
        }
        "db" => {
            let sub = args.get(1).map(String::as_str).unwrap_or("");
            let paths = PortablePaths::resolve()?;
            let mut cfg = load_config(&paths);
            let (inv, _) = scanner::scan_inventory(&paths, &mut cfg);
            match sub {
                "list" => {
                    let engine = args.get(2).map(String::as_str).unwrap_or("mysql");
                    let dbs = match engine {
                        "mysql" => services::list_mysql_databases(&cfg, &inv)?,
                        "postgres" => services::list_postgres_databases(&cfg, &inv)?,
                        other => return Err(anyhow!("Unsupported engine: {other}")),
                    };
                    for d in dbs {
                        println!("{d}");
                    }
                    Ok(())
                }
                "create" => {
                    let name = args
                        .get(2)
                        .ok_or_else(|| anyhow!("Usage: envstack db create <name> [mysql|postgres]"))?;
                    let engine = args.get(3).map(String::as_str).unwrap_or("mysql");
                    let _ = match engine {
                        "mysql" => services::create_mysql_database(name, &cfg, &inv)?,
                        "postgres" => services::create_postgres_database(name, &cfg, &inv)?,
                        other => return Err(anyhow!("Unsupported engine: {other}")),
                    };
                    println!("Created database '{name}' ({engine})");
                    Ok(())
                }
                _ => Err(anyhow!("Usage: envstack db <list|create> ...")),
            }
        }
        other => Err(anyhow!("Unknown command '{other}'. Run 'envstack --help' for usage.")),
    }
}
