use super::*;
use tempfile::TempDir;
use tokio::fs;

#[tokio::test]
async fn test_resolve_local_file() {
    let temp_dir = TempDir::new().unwrap();
    let pack_path = temp_dir.path().join("test.yaml");
    fs::write(&pack_path, "name: test\nversion: 1.0.0")
        .await
        .unwrap();

    let config = ResolverConfig::default().allow_unsigned();
    let resolver = PackResolver::with_config(config).unwrap();

    let result = resolver.resolve(pack_path.to_str().unwrap()).await.unwrap();

    assert!(matches!(result.source, ResolveSource::Local(_)));
    assert!(result.content.contains("name: test"));
}

#[tokio::test]
async fn test_resolve_local_file_not_found() {
    let config = ResolverConfig::default().allow_unsigned();
    let resolver = PackResolver::with_config(config).unwrap();

    let result = resolver.resolve("/nonexistent/pack.yaml").await;
    assert!(matches!(
        result,
        Err(crate::error::RegistryError::NotFound { .. })
    ));
}

#[tokio::test]
async fn test_resolve_bundled_not_found() {
    let config = ResolverConfig::default().allow_unsigned();
    let resolver = PackResolver::with_config(config).unwrap();

    let result = resolver.resolve("nonexistent-pack").await;
    assert!(matches!(
        result,
        Err(crate::error::RegistryError::NotFound { .. })
    ));
}

#[tokio::test]
async fn test_resolve_bundled_from_config_dir() {
    let temp_dir = TempDir::new().unwrap();
    let pack_path = temp_dir.path().join("my-pack.yaml");
    fs::write(&pack_path, "name: my-pack\nversion: 1.0.0")
        .await
        .unwrap();

    let config = ResolverConfig::default()
        .allow_unsigned()
        .with_bundled_dir(temp_dir.path().to_str().unwrap());
    let resolver = PackResolver::with_config(config).unwrap();

    let result = resolver.resolve("my-pack").await.unwrap();

    assert!(matches!(result.source, ResolveSource::Bundled(_)));
    assert!(result.content.contains("name: my-pack"));
}

#[tokio::test]
async fn test_with_config_bootstraps_embedded_production_roots() -> crate::RegistryResult<()> {
    let resolver = PackResolver::with_config(ResolverConfig::default().allow_unsigned())?;
    let keys = resolver.trust_store().list_keys().await;
    assert!(!keys.is_empty());
    Ok(())
}

#[test]
fn test_resolve_source_display() {
    assert_eq!(
        ResolveSource::Local("/path/to/pack.yaml".to_string()).to_string(),
        "local:/path/to/pack.yaml"
    );
    assert_eq!(
        ResolveSource::Bundled("my-pack".to_string()).to_string(),
        "bundled:my-pack"
    );
    assert_eq!(ResolveSource::Cache.to_string(), "cache");
    assert_eq!(
        ResolveSource::Registry("https://registry.example.com".to_string()).to_string(),
        "registry:https://registry.example.com"
    );
    assert_eq!(
        ResolveSource::Byos("s3://bucket/pack.yaml".to_string()).to_string(),
        "byos:s3://bucket/pack.yaml"
    );
}
