//! X25519MLKEM768 hybrid key exchange (RFC 10024).

use ml_kem::kem::{Decapsulate, DecapsulationKey, Encapsulate, EncapsulationKey};
use ml_kem::{Ciphertext, Encoded, EncodedSizeUser, KemCore, MlKem768, MlKem768Params};
use rand::rngs::OsRng;
use rustls::crypto::{ActiveKeyExchange, CompletedKeyExchange, SharedSecret, SupportedKxGroup};
use rustls::{Error, NamedGroup, PeerMisbehaved};
use x25519_dalek::{EphemeralSecret, PublicKey};

/// IANA code point for X25519MLKEM768 (RFC 10024).
pub const X25519_MLKEM768_NAMED_GROUP: NamedGroup = NamedGroup::X25519MLKEM768;

/// Client key share length: ML-KEM-768 encapsulation key + X25519 public key.
pub const CLIENT_SHARE_LEN: usize = 1184 + 32;
/// Server key share length: ML-KEM-768 ciphertext + X25519 public key.
pub const SERVER_SHARE_LEN: usize = 1088 + 32;
/// Combined shared secret length: ML-KEM + X25519.
pub const SHARED_SECRET_LEN: usize = 32 + 32;

const MLKEM_PK_LEN: usize = 1184;
const MLKEM_CT_LEN: usize = 1088;
const X25519_LEN: usize = 32;

/// Hybrid PQ/T group for rustls `CryptoProvider::kx_groups`.
#[derive(Debug)]
pub struct X25519MlKem768;

impl SupportedKxGroup for X25519MlKem768 {
    fn start(&self) -> Result<Box<dyn ActiveKeyExchange>, Error> {
        let mut rng = OsRng;
        let (mlkem_sk, mlkem_pk) = MlKem768::generate(&mut rng);
        let x25519_sk = EphemeralSecret::random_from_rng(&mut rng);
        let x25519_pk = PublicKey::from(&x25519_sk);

        let mut share = vec![0u8; CLIENT_SHARE_LEN];
        share[..MLKEM_PK_LEN].copy_from_slice(mlkem_pk.as_bytes().as_ref());
        share[MLKEM_PK_LEN..].copy_from_slice(x25519_pk.as_bytes());

        Ok(Box::new(ActiveHybridKex {
            mlkem_sk,
            x25519_sk,
            share,
        }))
    }

    fn name(&self) -> NamedGroup {
        X25519_MLKEM768_NAMED_GROUP
    }

    fn start_and_complete(
        &self,
        peer_pub_key: &[u8],
    ) -> Option<Result<CompletedKeyExchange, Error>> {
        Some(server_start_and_complete(peer_pub_key))
    }
}

struct ActiveHybridKex {
    mlkem_sk: DecapsulationKey<MlKem768Params>,
    x25519_sk: EphemeralSecret,
    share: Vec<u8>,
}

impl ActiveKeyExchange for ActiveHybridKex {
    fn complete(self: Box<Self>, peer_pub_key: &[u8]) -> Result<SharedSecret, Error> {
        client_complete(self.mlkem_sk, self.x25519_sk, peer_pub_key)
    }

    fn pub_key(&self) -> &[u8] {
        &self.share
    }

    fn group(&self) -> NamedGroup {
        X25519_MLKEM768_NAMED_GROUP
    }
}

fn server_start_and_complete(client_share: &[u8]) -> Result<CompletedKeyExchange, Error> {
    if client_share.len() != CLIENT_SHARE_LEN {
        return Err(invalid_key_share());
    }

    let (mlkem_pk_bytes, x25519_client_pk_bytes) = client_share.split_at(MLKEM_PK_LEN);
    let mlkem_pk_enc =
        Encoded::<EncapsulationKey<MlKem768Params>>::clone_from_slice(mlkem_pk_bytes);
    let mlkem_pk: EncapsulationKey<MlKem768Params> = EncapsulationKey::from_bytes(&mlkem_pk_enc);

    let mut rng = OsRng;
    let (mlkem_ct, mlkem_ss) = mlkem_pk
        .encapsulate(&mut rng)
        .map_err(|_| invalid_key_share())?;

    let x25519_sk = EphemeralSecret::random_from_rng(&mut rng);
    let x25519_server_pk = PublicKey::from(&x25519_sk);
    let x25519_client_pk = parse_x25519_pk(x25519_client_pk_bytes)?;
    let x25519_ss = x25519_sk
        .diffie_hellman(&x25519_client_pk)
        .as_bytes()
        .to_vec();

    let mut server_share = vec![0u8; SERVER_SHARE_LEN];
    server_share[..MLKEM_CT_LEN].copy_from_slice(mlkem_ct.as_ref());
    server_share[MLKEM_CT_LEN..].copy_from_slice(x25519_server_pk.as_bytes());

    let secret = concat_shared_secret(mlkem_ss.as_ref(), &x25519_ss)?;

    Ok(CompletedKeyExchange {
        group: X25519_MLKEM768_NAMED_GROUP,
        pub_key: server_share,
        secret,
    })
}

fn client_complete(
    mlkem_sk: DecapsulationKey<MlKem768Params>,
    x25519_sk: EphemeralSecret,
    server_share: &[u8],
) -> Result<SharedSecret, Error> {
    if server_share.len() != SERVER_SHARE_LEN {
        return Err(invalid_key_share());
    }

    let (mlkem_ct_bytes, x25519_server_pk_bytes) = server_share.split_at(MLKEM_CT_LEN);
    let mlkem_ct = Ciphertext::<MlKem768>::clone_from_slice(mlkem_ct_bytes);
    let mlkem_ss = mlkem_sk
        .decapsulate(&mlkem_ct)
        .map_err(|_| invalid_key_share())?;

    let x25519_server_pk = parse_x25519_pk(x25519_server_pk_bytes)?;
    let x25519_ss = x25519_sk
        .diffie_hellman(&x25519_server_pk)
        .as_bytes()
        .to_vec();

    concat_shared_secret(mlkem_ss.as_ref(), &x25519_ss)
}

fn parse_x25519_pk(bytes: &[u8]) -> Result<PublicKey, Error> {
    if bytes.len() != X25519_LEN {
        return Err(invalid_key_share());
    }
    let mut arr = [0u8; X25519_LEN];
    arr.copy_from_slice(bytes);
    Ok(PublicKey::from(arr))
}

fn concat_shared_secret(mlkem_ss: &[u8], x25519_ss: &[u8]) -> Result<SharedSecret, Error> {
    if mlkem_ss.len() != 32 || x25519_ss.len() != 32 {
        return Err(invalid_key_share());
    }
    let mut out = [0u8; SHARED_SECRET_LEN];
    out[..32].copy_from_slice(mlkem_ss);
    out[32..].copy_from_slice(x25519_ss);
    Ok(SharedSecret::from(out.as_slice()))
}

fn invalid_key_share() -> Error {
    Error::PeerMisbehaved(PeerMisbehaved::InvalidKeyShare)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hybrid_share_sizes() {
        let client_kx = X25519MlKem768.start().expect("start");
        assert_eq!(client_kx.pub_key().len(), CLIENT_SHARE_LEN);

        let completed = X25519MlKem768
            .start_and_complete(client_kx.pub_key())
            .expect("supported")
            .expect("start_and_complete");
        assert_eq!(completed.pub_key.len(), SERVER_SHARE_LEN);
        assert_eq!(completed.secret.secret_bytes().len(), SHARED_SECRET_LEN);
    }

    #[test]
    fn hybrid_group_metadata() {
        assert_eq!(X25519MlKem768.name(), X25519_MLKEM768_NAMED_GROUP);
        let kx = X25519MlKem768.start().expect("start");
        assert_eq!(kx.group(), X25519_MLKEM768_NAMED_GROUP);
    }

    #[test]
    fn rejects_invalid_share_lengths() {
        let short_client = vec![0u8; 64];
        assert!(
            X25519MlKem768
                .start_and_complete(&short_client)
                .expect("supported")
                .is_err()
        );

        let client_kx = X25519MlKem768.start().expect("start");
        assert!(client_kx.complete(&[0u8; 16]).is_err());
    }

    #[test]
    fn parse_x25519_pk_rejects_wrong_length() {
        assert!(parse_x25519_pk(&[0u8; 16]).is_err());
    }

    #[test]
    fn concat_shared_secret_rejects_bad_lengths() {
        assert!(concat_shared_secret(&[0u8; 16], &[0u8; 32]).is_err());
        assert!(concat_shared_secret(&[0u8; 32], &[0u8; 16]).is_err());
    }

    #[test]
    fn hybrid_kex_secrets_match() {
        let client_kx = X25519MlKem768.start().expect("client start");
        let client_pub = client_kx.pub_key().to_vec();

        let server = X25519MlKem768
            .start_and_complete(&client_pub)
            .expect("supported")
            .expect("server start_and_complete");

        let client_secret = client_kx
            .complete(&server.pub_key)
            .expect("client complete");

        assert_eq!(
            client_secret.secret_bytes(),
            server.secret.secret_bytes(),
            "hybrid shared secrets must match"
        );
    }
}
