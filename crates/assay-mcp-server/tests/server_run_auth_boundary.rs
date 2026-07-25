use assay_mcp_server::config::ServerConfig;
use assay_mcp_server::server::Server;
use std::path::PathBuf;

const AUTH_VARIABLE: &str = "AsSaY_Auth_DIRECT_LIBRARY_TEST";
const SECRET_VALUE: &str = "library-secret-must-not-appear";

struct AuthEnvGuard;

impl AuthEnvGuard {
    fn set() -> Self {
        std::env::set_var(AUTH_VARIABLE, SECRET_VALUE);
        Self
    }
}

impl Drop for AuthEnvGuard {
    fn drop(&mut self) {
        std::env::remove_var(AUTH_VARIABLE);
    }
}

#[tokio::test]
async fn public_server_run_rejects_auth_configuration_before_other_work() {
    let _guard = AuthEnvGuard::set();

    let error = Server::run(
        PathBuf::from("policy-root-must-not-be-read"),
        ServerConfig::default(),
    )
    .await
    .expect_err("public library entrypoint must reject stdio auth configuration");
    let diagnostic = format!("{error:#}");

    assert!(
        diagnostic.contains("unsupported") && diagnostic.contains(AUTH_VARIABLE),
        "diagnostic must name the unsupported boundary and variable: {diagnostic}"
    );
    assert!(
        !diagnostic.contains(SECRET_VALUE),
        "diagnostic leaked the configured auth value: {diagnostic}"
    );
    assert!(
        !diagnostic.contains("invalid --policy-root"),
        "auth refusal must happen before policy-root access: {diagnostic}"
    );
}
