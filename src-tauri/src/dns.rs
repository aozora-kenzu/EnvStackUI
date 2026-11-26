use crate::portable::run_elevated_powershell;
use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::net::UdpSocket as StdUdpSocket;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::net::UdpSocket;
use tokio::sync::oneshot;
use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ};
use winreg::RegKey;

pub const WILDCARD_DNS_IP: &str = "127.0.0.54";
pub const WILDCARD_DNS_PORT: u16 = 53;
pub const NRPT_COMMENT: &str = "EnvStackUI Wildcard DNS";
const NRPT_REG_PATH: &str = r"SYSTEM\CurrentControlSet\Services\Dnscache\Parameters\DnsPolicyConfig";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WildcardDnsStatus {
    pub enabled: bool,
    pub server_running: bool,
    pub nrpt_installed: bool,
    pub nrpt_tld: Option<String>,
    pub matches_tld: bool,
    pub listen_addr: String,
    pub last_error: Option<String>,
}

#[derive(Clone)]
pub struct WildcardDnsManager {
    running: Arc<AtomicBool>,
    stop_tx: Arc<Mutex<Option<oneshot::Sender<()>>>>,
    active_tld: Arc<Mutex<String>>,
    last_error: Arc<Mutex<Option<String>>>,
}

impl WildcardDnsManager {
    pub fn new(tld: &str) -> Self {
        Self {
            running: Arc::new(AtomicBool::new(false)),
            stop_tx: Arc::new(Mutex::new(None)),
            active_tld: Arc::new(Mutex::new(normalize_tld(tld))),
            last_error: Arc::new(Mutex::new(None)),
        }
    }

    pub fn set_tld(&self, tld: &str) {
        *self.active_tld.lock().unwrap() = normalize_tld(tld);
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub fn start(&self, tld: &str) -> Result<()> {
        self.set_tld(tld);
        if self.is_running() {
            return Ok(());
        }

        let bind_addr = format!("{WILDCARD_DNS_IP}:{WILDCARD_DNS_PORT}");
        let std_sock = match StdUdpSocket::bind(&bind_addr) {
            Ok(sock) => sock,
            Err(err) => {
                let msg = format!("Failed to bind local DNS listener on {bind_addr}: {err}");
                *self.last_error.lock().unwrap() = Some(msg.clone());
                return Err(anyhow!(msg));
            }
        };
        std_sock.set_nonblocking(true)?;
        *self.last_error.lock().unwrap() = None;

        let (tx, mut rx) = oneshot::channel::<()>();
        {
            let mut guard = self.stop_tx.lock().unwrap();
            if let Some(old_tx) = guard.take() {
                let _ = old_tx.send(());
            }
            *guard = Some(tx);
        }

        self.running.store(true, Ordering::SeqCst);
        let running_flag = Arc::clone(&self.running);
        let active_tld = Arc::clone(&self.active_tld);
        let last_err = Arc::clone(&self.last_error);

        tauri::async_runtime::spawn(async move {
            let sock = match UdpSocket::from_std(std_sock) {
                Ok(s) => s,
                Err(e) => {
                    *last_err.lock().unwrap() = Some(format!("DNS async socket error: {e}"));
                    running_flag.store(false, Ordering::SeqCst);
                    return;
                }
            };

            let mut buf = [0u8; 512];
            loop {
                tokio::select! {
                    _ = &mut rx => {
                        break;
                    }
                    recv = sock.recv_from(&mut buf) => {
                        match recv {
                            Ok((len, peer)) => {
                                let tld_snapshot = active_tld.lock().unwrap().clone();
                                if let Some(resp) = build_dns_response(&buf[..len], &tld_snapshot) {
                                    let _ = sock.send_to(&resp, peer).await;
                                }
                            }
                            Err(_) => {
                                break;
                            }
                        }
                    }
                }
            }
            running_flag.store(false, Ordering::SeqCst);
        });

        Ok(())
    }

    pub fn stop(&self) {
        if let Some(tx) = self.stop_tx.lock().unwrap().take() {
            let _ = tx.send(());
        }
        self.running.store(false, Ordering::SeqCst);
    }

    pub fn get_status(&self, config_enabled: bool, config_tld: &str) -> WildcardDnsStatus {
        let desired_tld = normalize_tld(config_tld);
        let (nrpt_installed, nrpt_tld) = read_nrpt_registry();
        let matches_tld = nrpt_installed
            && nrpt_tld
                .as_deref()
                .map(|t| t.eq_ignore_ascii_case(&desired_tld))
                .unwrap_or(false);

        WildcardDnsStatus {
            enabled: config_enabled,
            server_running: self.is_running(),
            nrpt_installed,
            nrpt_tld,
            matches_tld,
            listen_addr: format!("{WILDCARD_DNS_IP}:{WILDCARD_DNS_PORT}"),
            last_error: self.last_error.lock().unwrap().clone(),
        }
    }
}

pub fn normalize_tld(raw: &str) -> String {
    let trimmed = raw.trim().trim_start_matches('.').to_lowercase();
    let cleaned: String = trimmed
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect();
    if cleaned.is_empty() {
        "test".to_string()
    } else {
        cleaned
    }
}

pub fn read_nrpt_registry() -> (bool, Option<String>) {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let Ok(policies) = hklm.open_subkey_with_flags(NRPT_REG_PATH, KEY_READ) else {
        return (false, None);
    };

    for subkey_name in policies.enum_keys().flatten() {
        let Ok(subkey) = policies.open_subkey_with_flags(&subkey_name, KEY_READ) else {
            continue;
        };
        let comment: String = subkey.get_value("Comment").unwrap_or_default();
        if comment != NRPT_COMMENT {
            continue;
        }

        let rule_name = subkey
            .get_value::<Vec<String>, _>("Name")
            .ok()
            .and_then(|v| v.into_iter().next())
            .or_else(|| subkey.get_value::<String, _>("Name").ok());

        let tld = rule_name
            .map(|n| n.trim().trim_start_matches('.').to_lowercase())
            .filter(|n| !n.is_empty());

        return (true, tld);
    }

    (false, None)
}

pub fn install_nrpt_rule(tld: &str) -> Result<()> {
    let clean_tld = normalize_tld(tld);
    let ps_script = format!(
        "$existing = Get-DnsClientNrptRule -ErrorAction SilentlyContinue | Where-Object {{ $_.Comment -eq '{NRPT_COMMENT}' }}\r\n\
         if ($existing) {{ $existing | Remove-DnsClientNrptRule -Force -ErrorAction SilentlyContinue }}\r\n\
         Add-DnsClientNrptRule -Namespace '.{clean_tld}' -NameServers '{WILDCARD_DNS_IP}' -Comment '{NRPT_COMMENT}' | Out-Null\r\n\
         Clear-DnsClientCache"
    );

    run_elevated_powershell(&ps_script).context("Failed to install Windows NRPT rule")
}

pub fn remove_nrpt_rule() -> Result<()> {
    let ps_script = format!(
        "$existing = Get-DnsClientNrptRule -ErrorAction SilentlyContinue | Where-Object {{ $_.Comment -eq '{NRPT_COMMENT}' }}\r\n\
         if ($existing) {{ $existing | Remove-DnsClientNrptRule -Force -ErrorAction SilentlyContinue }}\r\n\
         Clear-DnsClientCache"
    );

    run_elevated_powershell(&ps_script).context("Failed to remove Windows NRPT rule")
}

/// Builds an RFC 1035 DNS UDP response resolving `*.<tld>` A/ANY queries to 127.0.0.1.
pub fn build_dns_response(query: &[u8], tld: &str) -> Option<Vec<u8>> {
    if query.len() < 12 {
        return None;
    }

    let qdcount = u16::from_be_bytes([query[4], query[5]]);
    if qdcount == 0 {
        return None;
    }

    let mut idx = 12usize;
    let mut labels = Vec::new();
    while idx < query.len() {
        let len = query[idx] as usize;
        idx += 1;
        if len == 0 {
            break;
        }
        if (len & 0xC0) != 0 || idx + len > query.len() {
            return None;
        }
        let label = std::str::from_utf8(&query[idx..idx + len]).ok()?;
        labels.push(label.to_ascii_lowercase());
        idx += len;
    }

    let qend = idx + 4;
    if qend > query.len() {
        return None;
    }

    let qtype = u16::from_be_bytes([query[idx], query[idx + 1]]);
    let clean_tld = normalize_tld(tld);
    let qname = labels.join(".");
    let suffix = format!(".{clean_tld}");
    let matched = qname == clean_tld || qname.ends_with(&suffix);

    let answer_a = matched && (qtype == 1 || qtype == 255);
    let rcode: u8 = if matched { 0 } else { 3 }; // NOERROR vs NXDOMAIN

    let mut resp = Vec::with_capacity(qend + 16);
    // Transaction ID
    resp.extend_from_slice(&query[0..2]);
    // Flags: QR=1, AA=1, preserve RD
    resp.push(0x84 | (query[2] & 0x01));
    resp.push(rcode);
    // QDCOUNT = 1, ANCOUNT = 1 or 0, NSCOUNT = 0, ARCOUNT = 0
    resp.extend_from_slice(&1u16.to_be_bytes());
    resp.extend_from_slice(&(if answer_a { 1u16 } else { 0u16 }).to_be_bytes());
    resp.extend_from_slice(&0u16.to_be_bytes());
    resp.extend_from_slice(&0u16.to_be_bytes());

    // Original Question section
    resp.extend_from_slice(&query[12..qend]);

    if answer_a {
        // Name pointer to offset 12 (0xC00C)
        resp.extend_from_slice(&[0xC0, 0x0C]);
        // TYPE = A (1), CLASS = IN (1)
        resp.extend_from_slice(&1u16.to_be_bytes());
        resp.extend_from_slice(&1u16.to_be_bytes());
        // TTL = 60s
        resp.extend_from_slice(&60u32.to_be_bytes());
        // RDLENGTH = 4, RDATA = 127.0.0.1
        resp.extend_from_slice(&4u16.to_be_bytes());
        resp.extend_from_slice(&[127, 0, 0, 1]);
    }

    Some(resp)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_query(domain: &str, qtype: u16) -> Vec<u8> {
        let mut pkt = vec![
            0x12, 0x34, // ID
            0x01, 0x00, // Standard query, RD=1
            0x00, 0x01, // QDCOUNT=1
            0x00, 0x00, // ANCOUNT=0
            0x00, 0x00, // NSCOUNT=0
            0x00, 0x00, // ARCOUNT=0
        ];
        for part in domain.split('.') {
            pkt.push(part.len() as u8);
            pkt.extend_from_slice(part.as_bytes());
        }
        pkt.push(0x00);
        pkt.extend_from_slice(&qtype.to_be_bytes());
        pkt.extend_from_slice(&1u16.to_be_bytes()); // CLASS IN
        pkt
    }

    #[test]
    fn dns_response_resolves_tld_and_subdomains_to_loopback() {
        let q1 = make_query("myapp.test", 1);
        let r1 = build_dns_response(&q1, "test").unwrap();
        assert_eq!(&r1[0..2], &[0x12, 0x34]);
        assert_eq!(r1[3] & 0x0F, 0); // NOERROR
        assert_eq!(u16::from_be_bytes([r1[6], r1[7]]), 1); // ANCOUNT=1
        assert_eq!(&r1[r1.len() - 4..], &[127, 0, 0, 1]);

        let q2 = make_query("api.v2.myapp.test", 1);
        let r2 = build_dns_response(&q2, ".TEST").unwrap();
        assert_eq!(r2[3] & 0x0F, 0);
        assert_eq!(u16::from_be_bytes([r2[6], r2[7]]), 1);
        assert_eq!(&r2[r2.len() - 4..], &[127, 0, 0, 1]);

        // AAAA query on matching domain -> NOERROR with 0 answers
        let q3 = make_query("myapp.test", 28);
        let r3 = build_dns_response(&q3, "test").unwrap();
        assert_eq!(r3[3] & 0x0F, 0);
        assert_eq!(u16::from_be_bytes([r3[6], r3[7]]), 0);

        // Unrelated domain -> NXDOMAIN
        let q4 = make_query("example.com", 1);
        let r4 = build_dns_response(&q4, "test").unwrap();
        assert_eq!(r4[3] & 0x0F, 3);
        assert_eq!(u16::from_be_bytes([r4[6], r4[7]]), 0);
    }
}
