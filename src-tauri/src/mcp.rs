use crate::logs;
use crate::portable::{load_config, PortablePaths, APP_VERSION};
use crate::scanner;
use crate::services::{self, ServiceManager, ALL_SERVICES};
use crate::vhosts;
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};

fn resolve_active_web_ports_simple(config: &crate::portable::AppConfig, inventory: &scanner::Inventory) -> (u16, u16) {
    let has_apache = scanner::get_active_binary(inventory, config, "apache").is_some();
    let has_nginx = scanner::get_active_binary(inventory, config, "nginx").is_some();
    let apache_running = !ServiceManager::is_port_free(config.ports.apache_http);
    let nginx_running = !ServiceManager::is_port_free(config.ports.nginx_http);

    if apache_running {
        (config.ports.apache_http, config.ports.apache_https)
    } else if nginx_running {
        (config.ports.nginx_http, config.ports.nginx_https)
    } else if has_apache || !has_nginx {
        (config.ports.apache_http, config.ports.apache_https)
    } else {
        (config.ports.nginx_http, config.ports.nginx_https)
    }
}

fn tool_definitions() -> Value {
    json!([
        {
            "name": "envstack_status",
            "description": "Inspect EnvStackUI portable root, active runtime versions, port bindings, and live service listening state.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }
        },
        {
            "name": "envstack_list_sites",
            "description": "List local virtual host sites discovered in www/ and linked projects with domains, frameworks, SSL status, and pinned PHP versions.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }
        },
        {
            "name": "envstack_list_logs",
            "description": "List available EnvStackUI application and service log files with byte sizes.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }
        },
        {
            "name": "envstack_read_log",
            "description": "Read the tail of a log file (e.g. envstack.log, apache-error.log, nginx-error.log, php-cgi.log, mysql-error.log, postgres-error.log, redis.log).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Log file name (e.g. apache-error.log)"
                    },
                    "lines": {
                        "type": "integer",
                        "description": "Maximum trailing lines to return (default 200)"
                    },
                    "filter": {
                        "type": "string",
                        "description": "Optional case-insensitive substring filter"
                    }
                },
                "required": ["name"],
                "additionalProperties": false
            }
        },
        {
            "name": "envstack_mailpit_messages",
            "description": "Fetch recent local emails captured by Mailpit.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of messages to retrieve (default 20)"
                    }
                },
                "additionalProperties": false
            }
        },
        {
            "name": "envstack_list_databases",
            "description": "List user databases in running MariaDB/MySQL or PostgreSQL.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "engine": {
                        "type": "string",
                        "enum": ["mysql", "postgres"],
                        "description": "Database engine (default mysql)"
                    }
                },
                "additionalProperties": false
            }
        }
    ])
}

fn call_tool(name: &str, args: &Value, paths: &PortablePaths) -> Result<Value, String> {
    let mut cfg = load_config(paths);
    let (inv, _) = scanner::scan_inventory(paths, &mut cfg);

    match name {
        "envstack_status" => {
            let service_rows: Vec<Value> = ALL_SERVICES
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
                    let active = scanner::get_active_binary(&inv, &cfg, id);
                    let listening = port > 0 && !ServiceManager::is_port_free(port);
                    json!({
                        "id": id,
                        "installed": active.is_some(),
                        "version": active.map(|b| b.version.clone()),
                        "port": port,
                        "listening": listening,
                    })
                })
                .collect();

            Ok(json!({
                "version": APP_VERSION,
                "root_dir": paths.root.to_string_lossy(),
                "www_root": paths.active_www_root(&cfg).to_string_lossy(),
                "tld": cfg.tld,
                "wildcard_dns_enabled": cfg.wildcard_dns_enabled,
                "services": service_rows,
            }))
        }
        "envstack_list_sites" => {
            let www_root = paths.active_www_root(&cfg);
            let web_ports = resolve_active_web_ports_simple(&cfg, &inv);
            let sites = vhosts::scan_sites(
                &www_root,
                &cfg,
                &inv,
                &paths.etc_ssl,
                &paths.root,
                web_ports,
            );
            Ok(serde_json::to_value(sites).map_err(|e| e.to_string())?)
        }
        "envstack_list_logs" => {
            let list = logs::list_logs(&paths.logs);
            Ok(serde_json::to_value(list).map_err(|e| e.to_string())?)
        }
        "envstack_read_log" => {
            let log_name = args
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Missing required argument: name".to_string())?;
            let max_lines = args
                .get("lines")
                .and_then(|v| v.as_u64())
                .map(|v| v as usize)
                .unwrap_or(200);
            let filter = args
                .get("filter")
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_lowercase())
                .filter(|s| !s.is_empty());

            let mut tail = logs::read_log_tail(&paths.logs, log_name, max_lines);
            if let Some(needle) = filter {
                tail.content = tail
                    .content
                    .lines()
                    .filter(|l| l.to_lowercase().contains(&needle))
                    .collect::<Vec<_>>()
                    .join("\n");
            }
            Ok(serde_json::to_value(tail).map_err(|e| e.to_string())?)
        }
        "envstack_mailpit_messages" => {
            let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(20);
            let url = format!(
                "http://127.0.0.1:{}/api/v1/messages?limit={limit}",
                cfg.ports.mailpit_http
            );
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| e.to_string())?;
            let payload: Value = rt.block_on(async move {
                let client = reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(3))
                    .build()
                    .map_err(|e| e.to_string())?;
                let resp = client
                    .get(&url)
                    .send()
                    .await
                    .map_err(|e| format!("Failed to reach Mailpit at {url}: {e}"))?;
                let body = resp
                    .error_for_status()
                    .map_err(|e| e.to_string())?
                    .json::<Value>()
                    .await
                    .map_err(|e| e.to_string())?;
                Ok::<Value, String>(body)
            })?;
            Ok(payload)
        }
        "envstack_list_databases" => {
            let engine = args
                .get("engine")
                .and_then(|v| v.as_str())
                .unwrap_or("mysql");
            let dbs = match engine {
                "mysql" => services::list_mysql_databases(&cfg, &inv).map_err(|e| format!("{e:#}"))?,
                "postgres" => {
                    services::list_postgres_databases(&cfg, &inv).map_err(|e| format!("{e:#}"))?
                }
                other => return Err(format!("Unsupported database engine: {other}")),
            };
            Ok(json!({ "engine": engine, "databases": dbs }))
        }
        _ => Err(format!("Unknown tool: {name}")),
    }
}

pub fn handle_mcp_request(raw_line: &str, paths: &PortablePaths) -> Option<Value> {
    let req: Value = match serde_json::from_str(raw_line) {
        Ok(v) => v,
        Err(_) => {
            return Some(json!({
                "jsonrpc": "2.0",
                "id": Value::Null,
                "error": { "code": -32700, "message": "Parse error" }
            }));
        }
    };

    let id = req.get("id").cloned();
    let method = req.get("method").and_then(|v| v.as_str()).unwrap_or("");

    // Notifications without `id` expect no response in JSON-RPC 2.0
    if id.is_none() || method.starts_with("notifications/") {
        return None;
    }
    let req_id = id.unwrap_or(Value::Null);

    let result_or_err: Result<Value, (i32, String)> = match method {
        "initialize" => Ok(json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": {}
            },
            "serverInfo": {
                "name": "envstack-ui",
                "version": APP_VERSION
            }
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({
            "tools": tool_definitions()
        })),
        "tools/call" => {
            let params = req.get("params").cloned().unwrap_or_else(|| json!({}));
            let tool_name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let args = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            match call_tool(tool_name, &args, paths) {
                Ok(data) => {
                    let text = serde_json::to_string_pretty(&data).unwrap_or_default();
                    Ok(json!({
                        "content": [{ "type": "text", "text": text }],
                        "isError": false
                    }))
                }
                Err(err_msg) => Ok(json!({
                    "content": [{ "type": "text", "text": err_msg }],
                    "isError": true
                })),
            }
        }
        other => Err((-32601, format!("Method not found: {other}"))),
    };

    match result_or_err {
        Ok(res) => Some(json!({
            "jsonrpc": "2.0",
            "id": req_id,
            "result": res
        })),
        Err((code, msg)) => Some(json!({
            "jsonrpc": "2.0",
            "id": req_id,
            "error": { "code": code, "message": msg }
        })),
    }
}

pub fn run_mcp_stdio_server() -> i32 {
    let Ok(paths) = PortablePaths::resolve() else {
        eprintln!("Failed to resolve EnvStackUI portable paths");
        return 1;
    };

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line_res in stdin.lock().lines() {
        let Ok(line) = line_res else {
            break;
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(resp) = handle_mcp_request(trimmed, &paths) {
            if let Ok(encoded) = serde_json::to_string(&resp) {
                if writeln!(stdout, "{encoded}").is_err() || stdout.flush().is_err() {
                    break;
                }
            }
        }
    }

    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn mcp_initialize_and_tools_list_work() {
        let base = std::env::temp_dir().join(format!("envstack_mcp_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let paths = PortablePaths::from_root(base.clone());
        paths.ensure_dirs().unwrap();

        let init = handle_mcp_request(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
            &paths,
        )
        .unwrap();
        assert_eq!(init["id"], 1);
        assert_eq!(init["result"]["serverInfo"]["name"], "envstack-ui");

        assert!(handle_mcp_request(
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
            &paths
        )
        .is_none());

        let tools = handle_mcp_request(
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#,
            &paths,
        )
        .unwrap();
        let list = tools["result"]["tools"].as_array().unwrap();
        assert!(list.iter().any(|t| t["name"] == "envstack_status"));
        assert!(list.iter().any(|t| t["name"] == "envstack_list_sites"));
        assert!(list.iter().any(|t| t["name"] == "envstack_read_log"));
        assert!(list.iter().any(|t| t["name"] == "envstack_list_databases"));

        let unknown = handle_mcp_request(
            r#"{"jsonrpc":"2.0","id":3,"method":"does/not/exist"}"#,
            &paths,
        )
        .unwrap();
        assert_eq!(unknown["error"]["code"], -32601);

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn mcp_calls_status_sites_and_filtered_logs() {
        let base = std::env::temp_dir().join(format!("envstack_mcp_call_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let paths = PortablePaths::from_root(base.clone());
        paths.ensure_dirs().unwrap();
        fs::create_dir_all(paths.www_default.join("demo-site")).unwrap();
        fs::write(
            paths.logs.join("apache-error.log"),
            "[info] ok\n[error] syntax mismatch\n[info] done\n",
        )
        .unwrap();

        let status_resp = handle_mcp_request(
            r#"{"jsonrpc":"2.0","id":10,"method":"tools/call","params":{"name":"envstack_status","arguments":{}}}"#,
            &paths,
        )
        .unwrap();
        assert_eq!(status_resp["result"]["isError"], false);
        let status_text = status_resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(status_text.contains("\"postgres\""));
        assert!(status_text.contains("\"redis\""));

        let sites_resp = handle_mcp_request(
            r#"{"jsonrpc":"2.0","id":11,"method":"tools/call","params":{"name":"envstack_list_sites","arguments":{}}}"#,
            &paths,
        )
        .unwrap();
        let sites_text = sites_resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(sites_text.contains("demo-site.test"));

        let log_resp = handle_mcp_request(
            r#"{"jsonrpc":"2.0","id":12,"method":"tools/call","params":{"name":"envstack_read_log","arguments":{"name":"apache-error.log","filter":"error"}}}"#,
            &paths,
        )
        .unwrap();
        let log_text = log_resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(log_text.contains("syntax mismatch"));
        assert!(!log_text.contains("[info]"));

        let _ = fs::remove_dir_all(&base);
    }
}
