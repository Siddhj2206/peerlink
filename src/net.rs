use anyhow::Result;
use iroh::{Endpoint, SecretKey, endpoint::presets};

pub const PEERLINK_ALPN: &[u8] = b"peerlink/0.1.0";

pub async fn create_endpoint() -> Result<Endpoint> {
    create_endpoint_with_key(None).await
}

pub async fn create_endpoint_with_key(secret_key: Option<SecretKey>) -> Result<Endpoint> {
    let mut builder = Endpoint::builder(presets::N0)
        .alpns(vec![PEERLINK_ALPN.to_vec()]);
    if let Some(key) = secret_key {
        builder = builder.secret_key(key);
    }
    let endpoint = builder.bind().await?;
    Ok(endpoint)
}
