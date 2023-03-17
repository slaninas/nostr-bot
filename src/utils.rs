use bech32::FromBase32;

/// Returns unix timestamp
pub fn unix_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// Returns keypair parsed from string (hex or bech32)
pub fn keypair_from_secret(secret: &str) -> secp256k1::KeyPair {
    let secp = secp256k1::Secp256k1::new();

    if secret.starts_with("nsec1") {
        let (_hrp, secret_hex_base32, _variant) = bech32::decode(secret).unwrap();

        let secret_hex_base256 = Vec::<u8>::from_base32(&secret_hex_base32).unwrap();
        let secret_hex = hex::encode(secret_hex_base256);

        secp256k1::KeyPair::from_seckey_str(&secp, &secret_hex).unwrap()
    } else {
        secp256k1::KeyPair::from_seckey_str(&secp, secret).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keypair_from_hexkey_secret() {
        let (hex_pubkey, _parity) =
            keypair_from_secret("e0944e59d05a7eaa8d4fa02d6c7a681044bc740a052ca1d24721c5d6dc893997")
                .x_only_public_key();
        let expected_hex_pubkey =
            "7529ed8c74dc803a669b7d531623898f571b7c21985d6b0829c1832eca2588ab";
        assert_eq!(format!("{:x}", hex_pubkey), expected_hex_pubkey);

        let (hex_pubkey, _parity) =
            keypair_from_secret("b2b4c98561d4f9676f2eaf56239fa45f40925f5ef25443c09507211248dd9e4a")
                .x_only_public_key();
        let expected_hex_pubkey =
            "7c53ac4a0cc3352e6c8bdfc085397fcf3e87ae46a2184653afa9986ffc54766c";
        assert_eq!(format!("{:x}", hex_pubkey), expected_hex_pubkey);

        let (hex_pubkey, _parity) =
            keypair_from_secret("911fb7c678815e8f4d408f37cdb9813c104b8f85a1fe71959e38c57e571e953f")
                .x_only_public_key();
        let expected_hex_pubkey =
            "eb6e769c864e46416345bce135e9e890e20753e50529b4c0aa811e96135f61e4";
        assert_eq!(format!("{:x}", hex_pubkey), expected_hex_pubkey);
    }

    #[test]
    fn test_keypair_from_bech32_secret() {
        // Same input keys as for test_keypair_from_hexkey_secret, just in bech32
        let (hex_pubkey, _parity) =
            keypair_from_secret("nsec1uz2yukwstfl24r205qkkc7ngzpztcaq2q5k2r5j8y8zadhyf8xtsne5h7m")
                .x_only_public_key();
        let expected_hex_pubkey =
            "7529ed8c74dc803a669b7d531623898f571b7c21985d6b0829c1832eca2588ab";
        assert_eq!(format!("{:x}", hex_pubkey), expected_hex_pubkey);

        let (hex_pubkey, _parity) =
            keypair_from_secret("nsec1k26vnptp6nukwmew4atz88aytaqfyh677f2y8sy4qus3yjxane9qpqkm82")
                .x_only_public_key();
        let expected_hex_pubkey =
            "7c53ac4a0cc3352e6c8bdfc085397fcf3e87ae46a2184653afa9986ffc54766c";
        assert_eq!(format!("{:x}", hex_pubkey), expected_hex_pubkey);

        let (hex_pubkey, _parity) =
            keypair_from_secret("nsec1jy0m03ncs90g7n2q3umumwvp8sgyhru958l8r9v78rzhu4c7j5lsvutefu")
                .x_only_public_key();
        let expected_hex_pubkey =
            "eb6e769c864e46416345bce135e9e890e20753e50529b4c0aa811e96135f61e4";
        assert_eq!(format!("{:x}", hex_pubkey), expected_hex_pubkey);
    }
}
