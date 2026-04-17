use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    name = "kubectl-node-shell",
    about = "Start a root shell in the node's host OS via nsenter (Linux) or HostProcess (Windows)",
    version
)]
pub struct Cli {
    /// Node name (e.g. my-node or node/my-node)
    pub node: String,

    /// Kubernetes context to use
    #[arg(long)]
    pub context: Option<String>,

    /// Path to kubeconfig file
    #[arg(long)]
    pub kubeconfig: Option<String>,

    /// Kubernetes namespace
    #[arg(short = 'n', long)]
    pub namespace: Option<String>,

    /// Container image to use
    #[arg(long)]
    pub image: Option<String>,

    /// X-mode: mount host root at /host, do not enter host namespaces
    #[arg(short = 'x')]
    pub x_mode: bool,

    /// Mount a PVC at /opt-pvc
    #[arg(short = 'm', long = "mount")]
    pub pvc: Option<String>,

    /// Do not enter IPC namespace
    #[arg(long = "no-ipc")]
    pub no_ipc: bool,

    /// Do not enter mount namespace
    #[arg(long = "no-mount")]
    pub no_mount: bool,

    /// Do not enter PID namespace
    #[arg(long = "no-pid")]
    pub no_pid: bool,

    /// Do not enter network namespace
    #[arg(long = "no-net")]
    pub no_net: bool,

    /// Do not enter UTS namespace
    #[arg(long = "no-uts")]
    pub no_uts: bool,

    /// Command to run in the container (after --)
    #[arg(last = true)]
    pub cmd: Vec<String>,
}
