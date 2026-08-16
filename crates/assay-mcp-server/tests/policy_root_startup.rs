use assay_mcp_server::config::ServerConfig;
use assay_mcp_server::server::Server;

#[tokio::test]
async fn server_rejects_regular_file_as_policy_root() {
    let root = tempfile::NamedTempFile::new().expect("temporary policy-root file");

    let error = Server::run(root.path().to_path_buf(), ServerConfig::default())
        .await
        .expect_err("a policy root must be a directory");
    let diagnostic = format!("{error:#}");

    assert!(
        diagnostic.contains("invalid --policy-root") && diagnostic.contains("not a directory"),
        "diagnostic must classify the regular-file root: {diagnostic}"
    );
}
