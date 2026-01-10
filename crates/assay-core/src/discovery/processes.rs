use super::types::{
    AuthStatus, DiscoveredServer, DiscoverySource, PolicyStatus, ServerStatus, Transport,
};
use sysinfo::{System, Users};

pub fn scan_processes() -> Vec<DiscoveredServer> {
    let mut sys = System::new_all();
    sys.refresh_all();
    let mut servers = Vec::new();

    // sysinfo 0.30 separates Users list
    let users = Users::new_with_refreshed_list();

    for (pid, process) in sys.processes() {
        let cmd_slice: &[String] = process.cmd();
        let cmdline: String = cmd_slice.join(" ");
        let msg = cmdline.to_lowercase();

        let is_mcp = msg.contains("mcp-server")
            || msg.contains("@modelcontextprotocol/server")
            || msg.contains("mcp_server");

        if is_mcp {
            let pid_u32: u32 = pid.as_u32();
            let started = Some(process.start_time().to_string());

            let uid_opt = process.user_id();
            let user = if let Some(uid) = uid_opt {
                users.get_user_by_id(uid).map(|u| u.name().to_string())
            } else {
                None
            };

            servers.push(DiscoveredServer {
                id: format!("proc-{}", pid_u32),
                name: None,
                source: DiscoverySource::RunningProcess {
                    pid: pid_u32,
                    cmdline: cmdline.clone(),
                    started_at: started,
                    user,
                },
                transport: Transport::Unknown,
                status: ServerStatus::Running,
                policy_status: PolicyStatus::Unmanaged,
                auth: AuthStatus::Unknown,
                env_vars: vec![],
                risk_hints: vec![],
            });
        }
    }
    servers
}
