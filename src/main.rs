mod cli;
mod kubectl;
mod pod_spec;

use anyhow::{Context, Result};
use clap::Parser;
use is_terminal::IsTerminal;
use rand::RngExt;
use serde_json::json;
use std::env;
use std::process;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use cli::Cli;

fn random_suffix(len: usize) -> String {
    let charset = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::rng();
    (0..len)
        .map(|_| charset[rng.random_range(0..charset.len())] as char)
        .collect()
}

fn main() {
    process::exit(run());
}

fn run() -> i32 {
    match inner_run() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("Error: {e:#}");
            1
        }
    }
}

fn inner_run() -> Result<i32> {
    let cli = Cli::parse();
    let is_tty = std::io::stdin().is_terminal();
    let node = cli.node.trim_start_matches("node/").to_string();

    // Env-based defaults
    let container_cpu =
        env::var("KUBECTL_NODE_SHELL_POD_CPU").unwrap_or_else(|_| "100m".to_string());
    let container_memory =
        env::var("KUBECTL_NODE_SHELL_POD_MEMORY").unwrap_or_else(|_| "256Mi".to_string());
    let labels = env::var("KUBECTL_NODE_SHELL_LABELS").unwrap_or_default();
    let pod_running_timeout = env::var("KUBECTL_NODE_SHELL_POD_RUNNING_TIMEOUT")
        .unwrap_or_else(|_| "1m".to_string());
    let image_pull_secret_name =
        env::var("KUBECTL_NODE_SHELL_IMAGE_PULL_SECRET_NAME").unwrap_or_default();

    // Build kubectl args: start with kubeconfig (applies to config commands too)
    let mut base_args: Vec<String> = Vec::new();
    if let Some(kc) = &cli.kubeconfig {
        base_args.extend_from_slice(&["--kubeconfig".to_string(), kc.to_string()]);
    }

    // Resolve context and namespace unless running inside a pod
    let inside_pod = env::var("KUBERNETES_PORT").is_ok();
    if !inside_pod {
        if let Some(ctx) = &cli.context {
            base_args.extend_from_slice(&["--context".to_string(), ctx.to_string()]);
        } else {
            let ctx = kubectl::get_current_context(&base_args)
                .context("resolving current kubectl context")?;
            base_args.extend_from_slice(&["--context".to_string(), ctx]);
        }
        if let Some(ns) = &cli.namespace {
            base_args.extend_from_slice(&["--namespace".to_string(), ns.to_string()]);
        } else {
            let ns = kubectl::get_current_namespace(&base_args)
                .context("resolving current kubectl namespace")?;
            if !ns.is_empty() {
                base_args.extend_from_slice(&["--namespace".to_string(), ns]);
            }
        }
    } else {
        if let Some(ctx) = &cli.context {
            base_args.extend_from_slice(&["--context".to_string(), ctx.to_string()]);
        }
        if let Some(ns) = &cli.namespace {
            base_args.extend_from_slice(&["--namespace".to_string(), ns.to_string()]);
        }
    }

    // Get node OS label
    let os = kubectl::get_node_os(&base_args, &node)?;

    // Determine image and container name based on OS
    let (image, container_name) = if os == "windows" {
        let default_image = "mcr.microsoft.com/oss/kubernetes/windows-host-process-containers-base-image:v1.0.0";
        let img = cli
            .image
            .clone()
            .or_else(|| env::var("KUBECTL_NODE_SHELL_IMAGE_WINDOWS").ok())
            .unwrap_or_else(|| default_image.to_string());
        (img, "pwsh")
    } else {
        let img = cli
            .image
            .clone()
            .or_else(|| env::var("KUBECTL_NODE_SHELL_IMAGE").ok())
            .unwrap_or_else(|| "docker.io/library/alpine".to_string());
        (img, "nsenter")
    };

    let pod_name = format!("{}-{}", container_name, random_suffix(6));

    // Build volumes and volumeMounts
    let mut volumes = json!([]);
    let mut volume_mounts = json!([]);

    if cli.x_mode {
        volumes
            .as_array_mut()
            .unwrap()
            .push(json!({ "hostPath": { "path": "/", "type": "" }, "name": "host-root" }));
        volume_mounts
            .as_array_mut()
            .unwrap()
            .push(json!({ "mountPath": "/host", "name": "host-root" }));
    }

    if let Some(pvc) = &cli.pvc {
        volumes.as_array_mut().unwrap().push(
            json!({ "persistentVolumeClaim": { "claimName": pvc, "type": "" }, "name": "pvc" }),
        );
        volume_mounts
            .as_array_mut()
            .unwrap()
            .push(json!({ "mountPath": "/opt-pvc", "name": "pvc" }));
    }

    // Build container command JSON
    let cmd_json = if os == "windows" {
        pod_spec::build_windows_cmd(&cli.cmd)
    } else {
        pod_spec::build_linux_cmd(
            cli.no_mount,
            cli.no_uts,
            cli.no_ipc,
            cli.no_net,
            cli.no_pid,
            cli.x_mode,
            &cli.cmd,
        )
    };

    // Security context
    let security_context = if os == "windows" {
        json!({
            "privileged": true,
            "windowsOptions": { "hostProcess": true, "runAsUserName": "NT AUTHORITY\\SYSTEM" }
        })
    } else {
        json!({ "privileged": true })
    };

    // Resource limits (only when cluster quota requires it)
    let resources = if kubectl::check_quota_needed(&base_args, &image, &pod_name) {
        json!({
            "limits":   { "cpu": container_cpu, "memory": container_memory },
            "requests": { "cpu": container_cpu, "memory": container_memory }
        })
    } else {
        json!({})
    };

    // Image pull secrets
    let image_pull_secrets = if image_pull_secret_name.is_empty() {
        json!(null)
    } else {
        json!([{ "name": image_pull_secret_name }])
    };

    let overrides = pod_spec::build_overrides(
        &node,
        &image,
        container_name,
        cmd_json,
        security_context,
        !cli.no_pid,
        !cli.no_ipc,
        !cli.no_net,
        volumes,
        volume_mounts,
        resources,
        image_pull_secrets,
        is_tty,
    );

    // Determine generator flag for kubectl < 1.18
    let generator_str = match kubectl::get_kubectl_minor_version() {
        Some(v) if v < 18 => Some("--generator=run-pod/v1".to_string()),
        _ => None,
    };

    // Register Ctrl+C handler to set interrupted flag.
    // On Unix, the child kubectl process also receives SIGINT, so Command::status() returns naturally.
    let interrupted = Arc::new(AtomicBool::new(false));
    let interrupted_ctrlc = interrupted.clone();
    ctrlc::set_handler(move || {
        interrupted_ctrlc.store(true, Ordering::SeqCst);
    })
    .context("failed to set Ctrl+C handler")?;

    eprintln!("spawning \"{}\" on \"{}\"", pod_name, node);

    // Run kubectl - capture result before cleanup so cleanup always runs
    let result = kubectl::run_pod(
        &base_args,
        &image,
        &pod_name,
        &overrides,
        &labels,
        &pod_running_timeout,
        is_tty,
        generator_str.as_deref(),
    );

    // Always clean up the pod, regardless of how kubectl run exited
    kubectl::delete_pod(&base_args, &pod_name);

    if interrupted.load(Ordering::SeqCst) {
        // Propagate SIGINT exit code convention
        Ok(130)
    } else {
        Ok(result?)
    }
}

