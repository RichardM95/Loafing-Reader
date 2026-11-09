#[cfg(feature = "desktop")]
use std::{env, path::PathBuf, process::Command};

fn main() {
    #[cfg(feature = "desktop")]
    {
        slint_build::compile_with_config(
            "ui/app.slint",
            slint_build::CompilerConfiguration::new().with_style("fluent".into()),
        )
        .unwrap();
        if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
            windows_resources();
        }
    }
}

#[cfg(feature = "desktop")]
fn windows_resources() {
    println!("cargo:rerun-if-changed=assets/loafing-reader.rc");
    println!("cargo:rerun-if-changed=assets/loafing-reader.ico");
    println!("cargo:rerun-if-env-changed=RC");
    println!("cargo:rerun-if-env-changed=WindowsSdkDir");
    let rc = env::var_os("RC")
        .map(PathBuf::from)
        .or_else(|| {
            let root = env::var_os("WindowsSdkDir")
                .map(PathBuf::from)
                .or_else(|| {
                    let query = Command::new("reg.exe")
                        .args([
                            "query",
                            r"HKLM\SOFTWARE\Microsoft\Windows Kits\Installed Roots",
                            "/v",
                            "KitsRoot10",
                        ])
                        .output()
                        .ok()?;
                    let output = String::from_utf8_lossy(&query.stdout);
                    let (_, path) = output
                        .lines()
                        .find(|line| line.contains("REG_SZ"))?
                        .split_once("REG_SZ")?;
                    Some(PathBuf::from(path.trim()))
                })?;
            let mut candidates: Vec<_> = std::fs::read_dir(root.join("bin"))
                .ok()?
                .flatten()
                .map(|entry| entry.path().join("x64/rc.exe"))
                .filter(|p| p.is_file())
                .collect();
            candidates.sort();
            candidates.pop()
        })
        .expect("Windows SDK rc.exe not found. Install Windows SDK or set RC to rc.exe.");
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("loafing-reader.res");
    let result = Command::new(rc)
        .arg("/nologo")
        .arg("/fo")
        .arg(&output)
        .arg("assets/loafing-reader.rc")
        .output()
        .expect("Failed to launch Windows resource compiler");
    assert!(
        result.status.success(),
        "Resource compilation failed: {} {}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    println!("cargo:rustc-link-arg-bin=thief-reader={}", output.display());
}
