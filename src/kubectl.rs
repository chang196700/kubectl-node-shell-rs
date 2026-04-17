use anyhow::{bail, Context, Result};
use std::process::{Command, Stdio};

fn output_str(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).trim().to_string()
}

pub fn get_current_context(base_args: &[String]) -> Result<String> {
    let out = Command::new("kubectl")
        .args(base_args)
        .args(["config", "current-context"])
        .output()
        .context("kubectl config current-context")?;
    if !out.status.success() {
        bail!(
            "kubectl config current-context failed: {}",
            output_str(&out.stderr)
        );
    }
    Ok(output_str(&out.stdout))
}

pub fn get_current_namespace(base_args: &[String]) -> Result<String> {
    let out = Command::new("kubectl")
        .args(base_args)
        .args([
            "config",
            "view",
            "--minify",
            "--output",
            "jsonpath={.contexts..namespace}",
        ])
        .output()
        .context("kubectl config view namespace")?;
    Ok(output_str(&out.stdout))
}

pub fn get_node_os(base_args: &[String], node: &str) -> Result<String> {
    let out = Command::new("kubectl")
        .args(base_args)
        .args([
            "get",
            "node",
            node,
            "-o",
            r#"jsonpath={.metadata.labels.kubernetes\.io/os}"#,
        ])
        .output()
        .context("kubectl get node")?;
    if !out.status.success() {
        bail!(
            "kubectl get node {} failed: {}",
            node,
            output_str(&out.stderr)
        );
    }
    let os = output_str(&out.stdout);
    // Default to linux if label is missing
    Ok(if os.is_empty() {
        "linux".to_string()
    } else {
        os
    })
}

/// Returns true if the cluster requires resource limits (quota in effect).
pub fn check_quota_needed(base_args: &[String], image: &str, pod_name: &str) -> bool {
    let out = Command::new("kubectl")
        .args(base_args)
        .args(["run", "--image", image, pod_name, "--dry-run=server"])
        .output();
    match out {
        Ok(o) => {
            let combined = format!(
                "{}{}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr)
            );
            combined.contains("failed quota")
        }
        Err(_) => false,
    }
}

/// Returns the kubectl client minor version, parsed from JSON output.
pub fn get_kubectl_minor_version() -> Option<u32> {
    let out = Command::new("kubectl")
        .args(["version", "--client", "-o", "json"])
        .output()
        .ok()?;
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    let minor_str = json["clientVersion"]["minor"].as_str()?;
    minor_str.trim_end_matches('+').parse().ok()
}

pub fn delete_pod(base_args: &[String], pod: &str) {
    let _ = Command::new("kubectl")
        .args(base_args)
        .args(["delete", "pod", "--wait=false", pod])
        .stderr(Stdio::null())
        .status();
}

pub fn run_pod(
    base_args: &[String],
    image: &str,
    pod: &str,
    overrides: &str,
    labels: &str,
    pod_running_timeout: &str,
    is_tty: bool,
    generator: Option<&str>,
) -> Result<i32> {
    let mut cmd = Command::new("kubectl");
    cmd.args(base_args);
    cmd.args(["run", "--image", image, "--restart=Never", "--overrides", overrides]);
    if !labels.is_empty() {
        cmd.args(["--labels", labels]);
    }
    cmd.args(["--pod-running-timeout", pod_running_timeout]);
    if is_tty {
        cmd.arg("-t");
    }
    cmd.arg("-i");
    cmd.arg(pod);
    if let Some(g) = generator {
        cmd.arg(g);
    }
    let status = cmd.status().context("failed to spawn kubectl run")?;
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(sig) = status.signal() {
            return Ok(128 + sig);
        }
    }
    Ok(status.code().unwrap_or(1))
}
