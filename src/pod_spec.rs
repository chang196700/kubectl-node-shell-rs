use serde_json::{json, Value};

/// Build the pod spec `--overrides` JSON string for `kubectl run`.
pub fn build_overrides(
    node: &str,
    image: &str,
    container_name: &str,
    cmd: Value,
    security_context: Value,
    use_host_pid: bool,
    use_host_ipc: bool,
    use_host_net: bool,
    volumes: Value,
    volume_mounts: Value,
    resources: Value,
    image_pull_secrets: Value,
    is_tty: bool,
) -> String {
    json!({
        "spec": {
            "nodeName": node,
            "hostPID": use_host_pid,
            "hostIPC": use_host_ipc,
            "hostNetwork": use_host_net,
            "imagePullSecrets": image_pull_secrets,
            "containers": [{
                "securityContext": security_context,
                "image": image,
                "name": container_name,
                "stdin": true,
                "stdinOnce": true,
                "tty": is_tty,
                "command": cmd,
                "resources": resources,
                "volumeMounts": volume_mounts,
            }],
            "tolerations": [
                { "key": "CriticalAddonsOnly", "operator": "Exists" },
                { "effect": "NoExecute", "operator": "Exists" }
            ],
            "volumes": volumes,
        }
    })
    .to_string()
}

/// Build the container command for a Linux node.
/// - Normal mode: wraps user cmd (or bash -l) in nsenter.
/// - X-mode: null for default shell, or raw user cmd.
pub fn build_linux_cmd(
    no_mount: bool,
    no_uts: bool,
    no_ipc: bool,
    no_net: bool,
    no_pid: bool,
    x_mode: bool,
    user_cmd: &[String],
) -> Value {
    if x_mode {
        if user_cmd.is_empty() {
            json!(null)
        } else {
            json!(user_cmd)
        }
    } else {
        let mut parts: Vec<Value> = vec![json!("nsenter"), json!("--target"), json!("1")];
        if !no_mount {
            parts.push(json!("--mount"));
        }
        if !no_uts {
            parts.push(json!("--uts"));
        }
        if !no_ipc {
            parts.push(json!("--ipc"));
        }
        if !no_net {
            parts.push(json!("--net"));
        }
        if !no_pid {
            parts.push(json!("--pid"));
        }
        parts.push(json!("--"));
        if user_cmd.is_empty() {
            parts.push(json!("bash"));
            parts.push(json!("-l"));
        } else {
            for arg in user_cmd {
                parts.push(json!(arg));
            }
        }
        json!(parts)
    }
}

/// Build the container command for a Windows node (HostProcess pod).
pub fn build_windows_cmd(user_cmd: &[String]) -> Value {
    let mut cmd: Vec<Value> = vec![
        json!("cmd.exe"),
        json!("/c"),
        json!("powershell.exe"),
        json!("-nol"),
    ];
    if !user_cmd.is_empty() {
        cmd.push(json!("-Command"));
        for arg in user_cmd {
            cmd.push(json!(arg));
        }
    }
    json!(cmd)
}
