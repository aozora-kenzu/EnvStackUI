use crate::portable::{run_elevated_powershell, CREATE_NO_WINDOW};
use anyhow::{Context, Result};
use chrono::{Datelike, Duration, Utc};
use rcgen::{
    BasicConstraints, CertificateParams, DistinguishedName, DnType, ExtendedKeyUsagePurpose,
    IsCa, KeyPair, KeyUsagePurpose, SanType,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const CA_COMMON_NAME: &str = "EnvStackUI Local Dev CA";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SslStatus {
    pub ca_exists: bool,
    pub ca_trusted: bool,
    pub ca_cert_path: String,
    pub cert_count: usize,
}

pub fn ca_cert_path(ssl_dir: &Path) -> PathBuf {
    ssl_dir.join("ca.crt")
}

pub fn ca_key_path(ssl_dir: &Path) -> PathBuf {
    ssl_dir.join("ca.key")
}

pub fn site_cert_path(ssl_dir: &Path, domain: &str) -> PathBuf {
    ssl_dir.join("certs").join(format!("{domain}.crt"))
}

pub fn site_key_path(ssl_dir: &Path, domain: &str) -> PathBuf {
    ssl_dir.join("certs").join(format!("{domain}.key"))
}

fn set_validity_days(params: &mut CertificateParams, start_offset_days: i64, end_offset_days: i64) {
    let start = Utc::now() + Duration::days(start_offset_days);
    let end = Utc::now() + Duration::days(end_offset_days);
    params.not_before = rcgen::date_time_ymd(start.year(), start.month() as u8, start.day() as u8);
    params.not_after = rcgen::date_time_ymd(end.year(), end.month() as u8, end.day() as u8);
}

fn decode_base64(input: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let mut acc: u32 = 0;
    let mut bits: u8 = 0;
    for b in input.bytes() {
        if b.is_ascii_whitespace() || b == b'=' {
            continue;
        }
        let val = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        };
        acc = (acc << 6) | u32::from(val);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

fn pem_to_der(pem: &str) -> Option<Vec<u8>> {
    let start_marker = "-----BEGIN CERTIFICATE-----";
    let end_marker = "-----END CERTIFICATE-----";
    let start = pem.find(start_marker)? + start_marker.len();
    let end = pem[start..].find(end_marker)? + start;
    decode_base64(&pem[start..end])
}

fn sha1_hex(data: &[u8]) -> String {
    let mut h0: u32 = 0x6745_2301;
    let mut h1: u32 = 0xEFCD_AB89;
    let mut h2: u32 = 0x98BA_DCFE;
    let mut h3: u32 = 0x1032_5476;
    let mut h4: u32 = 0xC3D2_E1F0;

    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut msg = data.to_vec();
    msg.push(0x80);
    while (msg.len() % 64) != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.chunks_exact(64) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }

        let (mut a, mut b, mut c, mut d, mut e) = (h0, h1, h2, h3, h4);
        for (i, &wi) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A82_7999u32),
                20..=39 => (b ^ c ^ d, 0x6ED9_EBA1u32),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1B_BCDCu32),
                _ => (b ^ c ^ d, 0xCA62_C1D6u32),
            };
            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(wi);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }

        h0 = h0.wrapping_add(a);
        h1 = h1.wrapping_add(b);
        h2 = h2.wrapping_add(c);
        h3 = h3.wrapping_add(d);
        h4 = h4.wrapping_add(e);
    }

    format!("{h0:08X}{h1:08X}{h2:08X}{h3:08X}{h4:08X}")
}

pub fn cert_thumbprint_sha1(cert_path: &Path) -> Option<String> {
    let pem = fs::read_to_string(cert_path).ok()?;
    let der = pem_to_der(&pem)?;
    Some(sha1_hex(&der))
}

fn cert_has_legacy_4096_expiry(cert_path: &Path) -> bool {
    let Ok(pem) = fs::read_to_string(cert_path) else {
        return false;
    };
    let Some(der) = pem_to_der(&pem) else {
        return false;
    };
    der.windows(8).any(|w| w == b"40960101")
}

fn is_ca_trusted_by_file(crt: &Path) -> bool {
    let Some(thumbprint) = cert_thumbprint_sha1(crt) else {
        return false;
    };

    let machine_ok = Command::new("certutil")
        .args(["-verifystore", "Root", &thumbprint])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false);
    if machine_ok {
        return true;
    }

    Command::new("certutil")
        .args(["-user", "-verifystore", "Root", &thumbprint])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

pub fn ensure_root_ca(ssl_dir: &Path) -> Result<()> {
    let certs_dir = ssl_dir.join("certs");
    fs::create_dir_all(&certs_dir)?;
    let crt = ca_cert_path(ssl_dir);
    let key = ca_key_path(ssl_dir);

    if crt.is_file() && key.is_file() {
        // Automatically upgrade untrusted legacy 1975-4096 CA certs to modern validity bounds
        if cert_has_legacy_4096_expiry(&crt) && !is_ca_trusted_by_file(&crt) {
            let _ = fs::remove_file(&crt);
            let _ = fs::remove_file(&key);
            if let Ok(entries) = fs::read_dir(&certs_dir) {
                for entry in entries.flatten() {
                    let _ = fs::remove_file(entry.path());
                }
            }
        } else {
            return Ok(());
        }
    }

    let ca_key = KeyPair::generate().context("Failed to generate CA key pair")?;
    let mut params = CertificateParams::new(Vec::new())?;
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    set_validity_days(&mut params, -1, 3650);
    params.key_usages = vec![
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::CrlSign,
        KeyUsagePurpose::DigitalSignature,
    ];
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, CA_COMMON_NAME);
    dn.push(DnType::OrganizationName, "EnvStackUI");
    params.distinguished_name = dn;

    let ca_cert = params
        .self_signed(&ca_key)
        .context("Failed to self-sign EnvStackUI Local Dev CA")?;

    fs::write(&crt, ca_cert.pem())?;
    fs::write(&key, ca_key.serialize_pem())?;
    Ok(())
}

pub fn regenerate_root_ca(ssl_dir: &Path) -> Result<()> {
    let crt = ca_cert_path(ssl_dir);
    let key = ca_key_path(ssl_dir);
    let _ = fs::remove_file(&crt);
    let _ = fs::remove_file(&key);
    let certs_dir = ssl_dir.join("certs");
    if certs_dir.is_dir() {
        for entry in fs::read_dir(&certs_dir)?.flatten() {
            let _ = fs::remove_file(entry.path());
        }
    }
    ensure_root_ca(ssl_dir)
}

pub fn ensure_site_cert(ssl_dir: &Path, domain: &str) -> Result<(PathBuf, PathBuf)> {
    ensure_root_ca(ssl_dir)?;
    let ca_crt = ca_cert_path(ssl_dir);
    let cert_file = site_cert_path(ssl_dir, domain);
    let key_file = site_key_path(ssl_dir, domain);

    if cert_file.is_file() && key_file.is_file() {
        let older_than_ca = match (
            fs::metadata(&cert_file).and_then(|m| m.modified()),
            fs::metadata(&ca_crt).and_then(|m| m.modified()),
        ) {
            (Ok(cert_t), Ok(ca_t)) => cert_t < ca_t,
            _ => false,
        };
        if !cert_has_legacy_4096_expiry(&cert_file) && !older_than_ca {
            return Ok((cert_file, key_file));
        }
    }

    let ca_key_pem = fs::read_to_string(ca_key_path(ssl_dir))?;
    let ca_key = KeyPair::from_pem(&ca_key_pem).context("Failed to load CA private key")?;

    // Reconstruct CA params matching ca.crt's subject/issuer
    let mut ca_params = CertificateParams::new(Vec::new())?;
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    set_validity_days(&mut ca_params, -1, 3650);
    ca_params.key_usages = vec![
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::CrlSign,
        KeyUsagePurpose::DigitalSignature,
    ];
    let mut ca_dn = DistinguishedName::new();
    ca_dn.push(DnType::CommonName, CA_COMMON_NAME);
    ca_dn.push(DnType::OrganizationName, "EnvStackUI");
    ca_params.distinguished_name = ca_dn;
    let ca_cert = ca_params.self_signed(&ca_key)?;

    let leaf_key = KeyPair::generate().context("Failed to generate site key pair")?;
    let mut leaf_params = CertificateParams::new(vec![
        domain.to_string(),
        format!("*.{domain}"),
        "localhost".to_string(),
    ])?;
    set_validity_days(&mut leaf_params, -1, 365);
    leaf_params.subject_alt_names = vec![
        SanType::DnsName(domain.try_into()?),
        SanType::DnsName(format!("*.{domain}").try_into()?),
        SanType::DnsName("localhost".try_into()?),
    ];
    leaf_params.key_usages = vec![
        KeyUsagePurpose::DigitalSignature,
        KeyUsagePurpose::KeyEncipherment,
    ];
    leaf_params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];

    let mut leaf_dn = DistinguishedName::new();
    leaf_dn.push(DnType::CommonName, domain);
    leaf_params.distinguished_name = leaf_dn;

    let leaf_cert = leaf_params
        .signed_by(&leaf_key, &ca_cert, &ca_key)
        .context("Failed to sign site certificate")?;

    fs::write(&cert_file, leaf_cert.pem())?;
    fs::write(&key_file, leaf_key.serialize_pem())?;

    Ok((cert_file, key_file))
}

pub fn is_ca_trusted(ssl_dir: &Path) -> bool {
    let crt = ca_cert_path(ssl_dir);
    if !crt.is_file() {
        return false;
    }
    is_ca_trusted_by_file(&crt)
}

pub fn get_ssl_status(ssl_dir: &Path) -> SslStatus {
    let crt = ca_cert_path(ssl_dir);
    let certs_dir = ssl_dir.join("certs");
    let cert_count = fs::read_dir(&certs_dir)
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| {
                    e.path()
                        .extension()
                        .and_then(|s| s.to_str())
                        .map(|ext| ext.eq_ignore_ascii_case("crt"))
                        .unwrap_or(false)
                })
                .count()
        })
        .unwrap_or(0);

    SslStatus {
        ca_exists: crt.is_file() && ca_key_path(ssl_dir).is_file(),
        ca_trusted: is_ca_trusted(ssl_dir),
        ca_cert_path: crt.to_string_lossy().to_string(),
        cert_count,
    }
}

/// On-Demand UAC elevation to install this instance's EnvStackUI Local Dev CA into Windows Trusted Root store.
pub fn trust_root_ca(ssl_dir: &Path) -> Result<SslStatus> {
    ensure_root_ca(ssl_dir)?;
    let crt = ca_cert_path(ssl_dir);
    let escaped_crt = crt.to_string_lossy().replace('\'', "''");
    let ps_script = format!(
        "$ErrorActionPreference = 'Continue'\r\n\
         certutil.exe -delstore Root 'EnvStackUI Local Dev CA' | Out-Null\r\n\
         $ErrorActionPreference = 'Stop'\r\n\
         certutil.exe -addstore Root '{escaped_crt}' | Out-Null\r\n\
         if ($LASTEXITCODE -ne 0) {{ throw \"certutil.exe failed with exit code $LASTEXITCODE\" }}"
    );

    run_elevated_powershell(&ps_script).context("Failed to trust Root CA")?;
    Ok(get_ssl_status(ssl_dir))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha1_hex_matches_known_vectors() {
        assert_eq!(sha1_hex(b""), "DA39A3EE5E6B4B0D3255BFEF95601890AFD80709");
        assert_eq!(
            sha1_hex(b"abc"),
            "A9993E364706816ABA3E25717850C26C9CD0D89D"
        );
    }

    #[test]
    fn generated_ca_and_site_cert_have_modern_validity_and_thumbprint() {
        let base = std::env::temp_dir().join(format!("envstack_ssl_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);

        let (cert, key) = ensure_site_cert(&base, "demo.test").unwrap();
        assert!(cert.is_file());
        assert!(key.is_file());

        let ca_crt = ca_cert_path(&base);
        let thumb = cert_thumbprint_sha1(&ca_crt).unwrap();
        assert_eq!(thumb.len(), 40);
        assert!(!cert_has_legacy_4096_expiry(&ca_crt));
        assert!(!cert_has_legacy_4096_expiry(&cert));

        // Simulate legacy 4096 cert and ensure ensure_site_cert regenerates it cleanly
        fs::write(
            &cert,
            "-----BEGIN CERTIFICATE-----\nNDA5NjAxMDE=\n-----END CERTIFICATE-----\n",
        )
        .unwrap();
        assert!(cert_has_legacy_4096_expiry(&cert));
        let (regen_cert, _) = ensure_site_cert(&base, "demo.test").unwrap();
        assert!(!cert_has_legacy_4096_expiry(&regen_cert));

        let _ = fs::remove_dir_all(&base);
    }
}
