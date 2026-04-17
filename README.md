# kubectl node-shell (Rust)

A Rust reimplementation of [kubectl-node-shell](https://github.com/kvaps/kubectl-node-shell).

Opens a root shell in a Kubernetes node's host OS by launching an ephemeral privileged pod:
- **Linux nodes**: alpine image with `nsenter --target 1` to enter host namespaces
- **Windows nodes**: HostProcess pod with PowerShell

## Installation

### From source

```bash
cargo install --path .
# The binary is kubectl-node_shell — place it on your PATH so kubectl discovers it as a plugin
```

### Manual

```bash
cargo build --release
cp target/release/kubectl-node_shell /usr/local/bin/
```

## Usage

```bash
# Open a bash shell on a node
kubectl node-shell <node>

# Use a custom image
kubectl node-shell <node> --image ubuntu:22.04

# X-mode: mount host root at /host, skip namespace entry (useful for minimal OSes like Talos)
kubectl node-shell -x <node>

# Skip specific namespaces (any combination)
kubectl node-shell <node> --no-ipc --no-pid

# Run a one-off command
kubectl node-shell <node> -- uname -a

# Pipe stdin
cat /etc/hosts | kubectl node-shell <node> -- sh -c 'cat > /tmp/hosts'

# Use a specific context and namespace
kubectl node-shell --context my-ctx -n kube-system <node>

# Mount a PVC at /opt-pvc
kubectl node-shell -n my-ns -m my-pvc <node>
```

## CLI Reference

| Flag | Description |
|------|-------------|
| `<node>` | Node name (`my-node` or `node/my-node`) |
| `--context <ctx>` | Kubernetes context |
| `--kubeconfig <path>` | Path to kubeconfig file |
| `-n`, `--namespace <ns>` | Kubernetes namespace |
| `--image <image>` | Container image (default: `docker.io/library/alpine` / Windows base image) |
| `-x` | X-mode: mount `/host`, skip namespace entry |
| `-m`, `--mount <pvc>` | Mount a PVC at `/opt-pvc` |
| `--no-ipc` | Do not enter IPC namespace |
| `--no-mount` | Do not enter mount namespace |
| `--no-pid` | Do not enter PID namespace |
| `--no-net` | Do not enter network namespace |
| `--no-uts` | Do not enter UTS namespace |
| `-- <cmd...>` | Command to run (default: `bash -l`) |

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `KUBECTL_NODE_SHELL_IMAGE` | `docker.io/library/alpine` | Default Linux node image |
| `KUBECTL_NODE_SHELL_IMAGE_WINDOWS` | Windows base image | Default Windows node image |
| `KUBECTL_NODE_SHELL_LABELS` | _(empty)_ | Labels to apply to the debug pod |
| `KUBECTL_NODE_SHELL_POD_CPU` | `100m` | CPU limit/request for the pod |
| `KUBECTL_NODE_SHELL_POD_MEMORY` | `256Mi` | Memory limit/request for the pod |
| `KUBECTL_NODE_SHELL_POD_RUNNING_TIMEOUT` | `1m` | Timeout waiting for pod to be running |
| `KUBECTL_NODE_SHELL_IMAGE_PULL_SECRET_NAME` | _(empty)_ | Image pull secret name |

## How It Works

1. Resolves the current kubectl context and namespace (unless explicitly specified)
2. Reads the node's OS label (`kubernetes.io/os`) to choose the right pod spec
3. Builds a one-shot pod spec with host PID/IPC/network namespaces and `privileged: true`
4. Runs `kubectl run --overrides=<spec> -i [-t] <pod>` and streams stdin/stdout
5. Deletes the pod on exit (Ctrl+C, normal exit, or error)

## License

Same as the original project — see [LICENSE](LICENSE).
