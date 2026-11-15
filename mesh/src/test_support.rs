//! Secrets for the fixtures, which set none of their own: keys generated here, and an
//! obfuscation with every minted field, so that a fixture renders the way a minted file does.

use sliplib_config::Obfuscation;

/// Every field of `secrets::MINTED_OBFUSCATION`, at values the kernel module accepts.
pub fn minted_obfuscation() -> Obfuscation {
    Obfuscation {
        jc: Some(4),
        jmin: Some(10),
        jmax: Some(50),
        s1: Some(15),
        s2: Some(16),
        h1: Some(5),
        h2: Some(6),
        h3: Some(7),
        h4: Some(8),
        ..Obfuscation::default()
    }
}
