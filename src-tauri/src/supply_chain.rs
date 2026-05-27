use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

pub const SIDECAR_NAME: &str = "subunit-bridge";
const SIDECAR_MANIFEST: &str = include_str!("../../scripts/sidecar-sha256.txt");
const TARGET_TRIPLE: &str = env!("SUBUNIT_TARGET_TRIPLE");

#[cfg(windows)]
const SIDECAR_EXTENSION: &str = ".exe";
#[cfg(not(windows))]
const SIDECAR_EXTENSION: &str = "";

pub fn resolved_sidecar_path() -> Result<PathBuf, String> {
    let exe_path = std::env::current_exe()
        .map_err(|error| format!("failed to resolve current executable path: {error}"))?;
    let exe_dir = exe_path
        .parent()
        .ok_or_else(|| "failed to resolve current executable directory".to_string())?;
    let base_dir = if exe_dir.ends_with("deps") {
        exe_dir.parent().unwrap_or(exe_dir)
    } else {
        exe_dir
    };

    let candidates = [
        base_dir.join(bundled_sidecar_filename()),
        base_dir.join(dev_sidecar_filename()),
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("binaries")
            .join(dev_sidecar_filename()),
    ];

    for candidate in &candidates {
        if candidate.is_file() {
            return Ok(candidate.clone());
        }
    }

    Err(format!(
        "failed to resolve bridge sidecar for target {TARGET_TRIPLE}; tried {}",
        candidates
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

pub fn verify_resolved_sidecar() -> Result<PathBuf, String> {
    let sidecar_path = resolved_sidecar_path()?;
    verify_sidecar(&sidecar_path)?;
    Ok(sidecar_path)
}

pub fn verify_sidecar(path: &Path) -> Result<(), String> {
    let actual = sha256_file(path)
        .map_err(|error| format!("failed to hash sidecar at {}: {error}", path.display()))?;
    let expected = expected_sidecar_sha256()?;

    if constant_time_eq_32(&actual, &expected.bytes) {
        return Ok(());
    }

    Err(format!(
        "sidecar integrity check failed for {}: expected SHA-256 {}, got {}",
        path.display(),
        expected.hex,
        hex_encode(&actual)
    ))
}

fn bundled_sidecar_filename() -> String {
    format!("{SIDECAR_NAME}{SIDECAR_EXTENSION}")
}

fn dev_sidecar_filename() -> String {
    format!("{SIDECAR_NAME}-{TARGET_TRIPLE}{SIDECAR_EXTENSION}")
}

struct ExpectedSha256 {
    hex: &'static str,
    bytes: [u8; 32],
}

fn expected_sidecar_sha256() -> Result<ExpectedSha256, String> {
    let artifact = dev_sidecar_filename();

    for line in SIDECAR_MANIFEST.lines() {
        let mut parts = line.split_whitespace();
        let Some(hex) = parts.next() else {
            continue;
        };
        let Some(filename) = parts.next() else {
            continue;
        };
        let filename = filename.strip_prefix('*').unwrap_or(filename);

        if filename == artifact {
            return Ok(ExpectedSha256 {
                hex,
                bytes: hex_decode_32(hex)?,
            });
        }
    }

    Err(format!(
        "sidecar integrity check failed closed: no SHA-256 manifest entry for {artifact}"
    ))
}

fn sha256_file(path: &Path) -> std::io::Result<[u8; 32]> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];

    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    let digest = hasher.finalize();
    let mut bytes = [0_u8; 32];
    bytes.copy_from_slice(&digest);
    Ok(bytes)
}

fn constant_time_eq_32(left: &[u8; 32], right: &[u8; 32]) -> bool {
    let mut diff = 0_u8;
    for index in 0..32 {
        diff |= left[index] ^ right[index];
    }
    diff == 0
}

fn hex_decode_32(hex: &str) -> Result<[u8; 32], String> {
    if hex.len() != 64 {
        return Err(format!(
            "sidecar integrity check failed closed: expected 64 hex chars, got {}",
            hex.len()
        ));
    }

    let mut bytes = [0_u8; 32];
    for (index, byte) in bytes.iter_mut().enumerate() {
        let offset = index * 2;
        let high = hex_value(hex.as_bytes()[offset])?;
        let low = hex_value(hex.as_bytes()[offset + 1])?;
        *byte = (high << 4) | low;
    }

    Ok(bytes)
}

fn hex_value(byte: u8) -> Result<u8, String> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err("sidecar integrity check failed closed: invalid SHA-256 hex".to_string()),
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}
