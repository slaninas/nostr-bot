pub fn unix_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

pub fn keypair_from_secret(secret: &str) -> secp256k1::KeyPair {
    let secp = secp256k1::Secp256k1::new();
    secp256k1::KeyPair::from_seckey_str(&secp, secret).unwrap()
}
