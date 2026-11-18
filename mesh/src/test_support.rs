//! Secrets for the fixtures, which set none of their own: keys generated here, and an
//! obfuscation with every minted field, so that a fixture renders the way a minted file does.

use crate::secrets::{self, ResolvedSecrets};
use sliplib_config::Obfuscation;
use sliplib_config::mesh_config::MeshConfig;
use sliplib_secrets::wireguard::PrivateKey;

/// Every field a minted obfuscation carries, at values the kernel module accepts.
pub fn minted_obfuscation() -> Obfuscation {
    Obfuscation {
        jc: Some(4),
        jmin: Some(10),
        jmax: Some(50),
        s1: Some(15),
        s2: Some(16),
        h1: Some(5.into()),
        h2: Some(6.into()),
        h3: Some(7.into()),
        h4: Some(8.into()),
        ..Obfuscation::default()
    }
}

/// What `mesh` reads as, with whatever it lacks filled in.
pub fn minted(mesh: &MeshConfig) -> ResolvedSecrets {
    let mut resolved = secrets::read(&crate::v1::lower(mesh).unwrap());
    for node in &mesh.nodes {
        resolved
            .mesh_private_keys
            .entry(node.name.clone())
            .or_insert_with(PrivateKey::generate);
    }
    for link in mesh.mesh.links.iter().filter(|link| !link.plain) {
        let obfuscation = resolved
            .mesh_link_obfuscation
            .entry(secrets::link_key(&link.pair))
            .or_default();
        *obfuscation = secrets::layer(obfuscation, &minted_obfuscation());
    }
    for pool in &mesh.roadwarriors {
        resolved
            .roadwarrior_private_keys
            .entry(pool.name.clone())
            .or_insert_with(PrivateKey::generate);
        if !pool.plain {
            let obfuscation = resolved
                .roadwarrior_obfuscation
                .entry(pool.name.clone())
                .or_default();
            *obfuscation = secrets::layer(obfuscation, &minted_obfuscation());
        }
    }
    resolved
}
