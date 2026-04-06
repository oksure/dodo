use anyhow::{bail, Context, Result};
use std::fs;

const REPO: &str = "oksure/dodo";
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

struct Release {
    tag: String,
    version: String,
    assets: Vec<Asset>,
}

struct Asset {
    name: String,
    url: String,
}

fn detect_target() -> Result<&'static str> {
    if cfg!(target_os = "macos") {
        Ok("universal-apple-darwin")
    } else if cfg!(target_os = "linux") && cfg!(target_arch = "x86_64") {
        Ok("x86_64-unknown-linux-gnu")
    } else if cfg!(target_os = "linux") && cfg!(target_arch = "aarch64") {
        Ok("aarch64-unknown-linux-gnu")
    } else if cfg!(target_os = "windows") && cfg!(target_arch = "x86_64") {
        Ok("x86_64-pc-windows-msvc")
    } else {
        bail!("Unsupported platform for self-update")
    }
}

fn fetch_latest_release() -> Result<Release> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let body = ureq::get(&url)
        .header("User-Agent", "dodo-cli")
        .call()
        .context("Failed to check for updates")?
        .into_body()
        .read_to_string()
        .context("Failed to read release info")?;
    let resp: serde_json::Value =
        serde_json::from_str(&body).context("Failed to parse release info")?;

    let tag = resp["tag_name"].as_str().unwrap_or("").to_string();
    let version = tag.strip_prefix('v').unwrap_or(&tag).to_string();

    let assets = resp["assets"]
        .as_array()
        .map(|arr: &Vec<serde_json::Value>| {
            arr.iter()
                .filter_map(|a| {
                    let name = a["name"].as_str()?.to_string();
                    let url = a["browser_download_url"].as_str()?.to_string();
                    Some(Asset { name, url })
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(Release {
        tag,
        version,
        assets,
    })
}

fn download_and_replace(asset_url: &str) -> Result<()> {
    let exe_path = std::env::current_exe().context("Cannot determine current executable path")?;

    // Download to a temp file next to the current binary
    let tmp_path = exe_path.with_extension("tmp");

    let body = ureq::get(asset_url)
        .header("User-Agent", "dodo-cli")
        .call()
        .context("Failed to download update")?
        .into_body()
        .read_to_vec()
        .context("Failed to read download")?;

    // Decompress tar.gz
    let decoder = flate2::read::GzDecoder::new(&body[..]);
    let mut archive = tar::Archive::new(decoder);

    let binary_name = if cfg!(target_os = "windows") {
        "dodo.exe"
    } else {
        "dodo"
    };
    let mut found = false;

    for entry in archive.entries().context("Failed to read archive")? {
        let mut entry = entry?;
        let path = entry.path()?;
        if path.file_name().and_then(|f| f.to_str()) == Some(binary_name) {
            let mut contents = Vec::new();
            std::io::Read::read_to_end(&mut entry, &mut contents)?;
            fs::write(&tmp_path, &contents).context("Failed to write update")?;
            found = true;
            break;
        }
    }

    if !found {
        let _ = fs::remove_file(&tmp_path);
        bail!("Binary not found in release archive");
    }

    // Set executable permission on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tmp_path, fs::Permissions::from_mode(0o755))?;
    }

    // Atomic swap: rename tmp over current exe
    // On Unix, rename replaces the target atomically
    // On Windows, we need to move the old one out first
    #[cfg(windows)]
    {
        let backup = exe_path.with_extension("old");
        let _ = fs::remove_file(&backup);
        fs::rename(&exe_path, &backup).context("Failed to replace binary")?;
    }

    fs::rename(&tmp_path, &exe_path).context("Failed to replace binary")?;

    Ok(())
}

pub fn check_update() -> Result<()> {
    let release = fetch_latest_release()?;

    if release.version == CURRENT_VERSION {
        println!("Already up to date (v{CURRENT_VERSION})");
        return Ok(());
    }

    println!(
        "Update available: v{CURRENT_VERSION} → v{}",
        release.version
    );
    println!("Downloading...");

    let target = detect_target()?;
    if cfg!(target_os = "windows") {
        println!("Self-update is not supported on Windows.");
        println!(
            "Download the latest release: https://github.com/{REPO}/releases/tag/{}",
            release.tag
        );
        return Ok(());
    }

    let expected = format!("dodo-{target}.tar.gz");

    let asset = release
        .assets
        .iter()
        .find(|a| a.name == expected)
        .with_context(|| format!("No release binary for {target}"))?;

    download_and_replace(&asset.url)?;

    println!("Updated to v{}", release.version);
    Ok(())
}
