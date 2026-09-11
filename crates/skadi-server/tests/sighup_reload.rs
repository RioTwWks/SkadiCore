//! SIGHUP reload: `UserStore::reload_from_protocol`.

use skadi_protocol::{VlessConfig, VlessUser};
use skadi_server::config::ProtocolConfig;
use skadi_server::store::UserStore;

const USER_A: &str = "b831381d-6324-4d53-ad4f-8cda48b30811";
const USER_B: &str = "a1b2c3d4-e5f6-7890-abcd-ef1234567890";

#[test]
fn reload_from_protocol_replaces_vless_users() {
    let initial = ProtocolConfig {
        socks5: Default::default(),
        vless: VlessConfig {
            enabled: true,
            users: vec![VlessUser {
                id: USER_A.to_string(),
                email: None,
                flow: None,
            }],
        },
    };
    let store = UserStore::from_protocol(&initial);
    assert_eq!(store.vless_user_count(), 1);

    let updated = ProtocolConfig {
        socks5: Default::default(),
        vless: VlessConfig {
            enabled: true,
            users: vec![VlessUser {
                id: USER_B.to_string(),
                email: Some("reloaded@example.com".into()),
                flow: None,
            }],
        },
    };
    store.reload_from_protocol(&updated).unwrap();

    let users = store.list_vless_users();
    assert_eq!(users.len(), 1);
    assert_eq!(users[0].id, USER_B);
    assert!(store
        .vless_config()
        .authenticate(
            &skadi_protocol::vless::Uuid::parse(USER_B)
                .unwrap()
                .as_bytes()
        )
        .is_some());
}

#[test]
fn reload_can_disable_vless() {
    let enabled = ProtocolConfig {
        socks5: Default::default(),
        vless: VlessConfig {
            enabled: true,
            users: vec![VlessUser {
                id: USER_A.to_string(),
                email: None,
                flow: None,
            }],
        },
    };
    let store = UserStore::from_protocol(&enabled);
    assert!(store.vless_enabled());

    let disabled = ProtocolConfig {
        socks5: Default::default(),
        vless: VlessConfig {
            enabled: false,
            users: vec![VlessUser {
                id: USER_A.to_string(),
                email: None,
                flow: None,
            }],
        },
    };
    store.reload_from_protocol(&disabled).unwrap();
    assert!(!store.vless_enabled());
}
