use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const SALT: &str = "IMAGE_UPSCALE_PRO_MAHMUDUL_HASAN_2026_SECRET_SALT";

// Master Passwords (Accepted keys)
pub const MASTER_KEY_1: &str = "MAHMUDUL-PRO-2026-X9";
pub const MASTER_KEY_2: &str = "CYBROOT-SUPER-SCALE-99";

fn compute_hash(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(SALT.as_bytes());
    hasher.update(input.trim().as_bytes());
    hex::encode(hasher.finalize())
}

/// Check if a given string matches any of the 2 master passwords
pub fn verify_master_password(password: &str) -> bool {
    let trimmed = password.trim();
    trimmed == MASTER_KEY_1 || trimmed == MASTER_KEY_2
}

/// Get the persistent license file path for the current OS user
pub fn get_license_file_path() -> Result<PathBuf> {
    let base_dir = if cfg!(windows) {
        std::env::var("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."))
    } else {
        std::env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|_| {
                std::env::var("HOME")
                    .map(|h| PathBuf::from(h).join(".config"))
            })
            .unwrap_or_else(|_| PathBuf::from("."))
    };

    let license_dir = base_dir.join("image_upscale_pro");
    fs::create_dir_all(&license_dir)
        .with_context(|| format!("Failed to create config dir: {}", license_dir.display()))?;

    Ok(license_dir.join(".license_active"))
}

/// Check if product is currently activated on this machine
pub fn is_product_activated() -> bool {
    let path = match get_license_file_path() {
        Ok(p) => p,
        Err(_) => return false,
    };

    if !path.exists() {
        return false;
    }

    match fs::read_to_string(&path) {
        Ok(content) => {
            let lines: Vec<&str> = content.lines().collect();
            if lines.len() >= 2 {
                let token = lines[0].trim();
                let sig = lines[1].trim();
                let expected_sig = compute_hash(token);
                sig == expected_sig
            } else {
                false
            }
        }
        Err(_) => false,
    }
}

/// Activate product using a master password
pub fn activate_product(password: &str) -> Result<()> {
    if !verify_master_password(password) {
        anyhow::bail!("Invalid Master Password. Please check your activation key.");
    }

    let license_path = get_license_file_path()?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let token = format!("ACTIVATED_MACHINE_TOKEN_{}_{}", now, password.trim());
    let sig = compute_hash(&token);

    let content = format!("{}\n{}\n", token, sig);
    fs::write(&license_path, content)
        .with_context(|| format!("Failed to write license file to {}", license_path.display()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_master_keys_validity() {
        assert!(verify_master_password("MAHMUDUL-PRO-2026-X9"));
        assert!(verify_master_password("CYBROOT-SUPER-SCALE-99"));
        assert!(verify_master_password(" MAHMUDUL-PRO-2026-X9 "));
        assert!(!verify_master_password("WRONG_KEY_123"));
    }
}
