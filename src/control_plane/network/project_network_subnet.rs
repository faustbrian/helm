use sha2::{Digest, Sha256};

/// Derives one stable /24 from Stackctl's private 10.128.0.0/9 pool.
pub(super) fn project_network_subnet(installation_id: &str, project_id: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(b"stackctl-project-network-subnet-v1\0");
    digest.update(installation_id.as_bytes());
    digest.update(b"\0");
    digest.update(project_id.as_bytes());
    let bytes = digest.finalize();

    format!("10.{}.{}.0/24", 128 + (bytes[0] & 0x7f), bytes[1])
}
