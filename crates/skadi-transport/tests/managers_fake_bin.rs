//! Тесты process manager'ов с поддельными бинарниками (без реального AWG/Hysteria/TUIC).

use skadi_transport::{
    AwgClientConfig, AwgManager, AwgNatConfig, AwgObfuscationConfig, AwgPeerConfig,
    AwgServerConfig, Hysteria2Manager, Hysteria2ServerConfig, TuicManager, TuicServerConfig,
};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use tempfile::TempDir;
use tokio::sync::watch;

static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

struct EnvGuard {
    saved: Vec<(String, Option<String>)>,
    _lock: std::sync::MutexGuard<'static, ()>,
}

impl EnvGuard {
    fn set(vars: &[(&str, Option<&str>)]) -> Self {
        let lock = ENV_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let saved = vars
            .iter()
            .map(|(k, _)| (k.to_string(), std::env::var(k).ok()))
            .collect();
        for (key, value) in vars {
            match value {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
        Self { saved, _lock: lock }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (key, prev) in &self.saved {
            match prev {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
    }
}

fn write_executable(path: &Path, content: &str) -> PathBuf {
    std::fs::write(path, content).expect("write fake binary");
    let mut perms = std::fs::metadata(path).expect("metadata").permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms).expect("chmod");
    path.to_path_buf()
}

fn sample_obfuscation() -> AwgObfuscationConfig {
    AwgObfuscationConfig {
        jc: 8,
        jmin: 64,
        jmax: 1024,
        s1: 32,
        s2: 32,
        s3: 16,
        s4: 16,
        h1: "1-10000000".into(),
        h2: "10000001-20000000".into(),
        h3: "20000001-30000000".into(),
        h4: "30000001-40000000".into(),
    }
}

fn sample_awg_server_config(iface: &str) -> AwgServerConfig {
    AwgServerConfig {
        listen: "127.0.0.1:51820".parse().unwrap(),
        interface_name: iface.into(),
        private_key: "sJkP2oorqrq49P6Ln25MWo3X04PxhB8k+RnJJnZ4gEo=".into(),
        address: "10.8.0.1/24".into(),
        mtu: Some(1420),
        obfuscation: sample_obfuscation(),
        peers: vec![AwgPeerConfig {
            public_key: "kHkjzj1KeQjR/82vXYRdQPA113MAzNRkDsedH5kZLi4=".into(),
            allowed_ips: vec!["10.8.0.2/32".into()],
            preshared_key: None,
            endpoint: None,
            persistent_keepalive: Some(25),
        }],
    }
}

fn fake_awg_go() -> &'static str {
    r#"#!/bin/sh
dir="${AWG_UAPI_DIR:-/var/run/amneziawg}"
mkdir -p "$dir"
iface=""
while [ $# -gt 0 ]; do
  case "$1" in
    -f) iface="$2"; shift 2 ;;
    *) shift ;;
  esac
done
touch "${dir}/${iface}.sock"
exec sleep 300
"#
}

fn fake_awg_tools() -> &'static str {
    "#!/bin/sh\nexit 0\n"
}

fn fake_sleep_binary() -> &'static str {
    "#!/bin/sh\nexec sleep 300\n"
}

fn fake_exit_binary() -> &'static str {
    "#!/bin/sh\nexit 1\n"
}

#[tokio::test]
async fn awg_manager_start_and_shutdown() {
    let dir = TempDir::new().expect("tempdir");
    let socket_dir = dir.path().join("awg-sockets");
    std::fs::create_dir_all(&socket_dir).expect("socket dir");
    let go_bin = write_executable(&dir.path().join("amneziawg-go"), fake_awg_go());
    let tools_bin = write_executable(&dir.path().join("awg"), fake_awg_tools());
    let iface = format!("skadiwg{}", std::process::id());
    let config = sample_awg_server_config(&iface);
    let nat = AwgNatConfig {
        enabled: false,
        egress_interface: None,
        subnet: "10.8.0.0/24".into(),
    };

    let _env = EnvGuard::set(&[
        ("AWG_GO_BINARY", Some(go_bin.to_str().unwrap())),
        ("AWG_TOOLS_BINARY", Some(tools_bin.to_str().unwrap())),
        ("AWG_UAPI_DIR", Some(socket_dir.to_str().unwrap())),
    ]);

    let manager = AwgManager::start(&config, Some(&nat))
        .await
        .expect("awg manager start");
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let handle = tokio::spawn(async move { manager.run_until_shutdown(shutdown_rx).await });
    shutdown_tx.send(true).expect("shutdown");
    handle.await.expect("join").expect("clean shutdown");
}

#[tokio::test]
async fn awg_client_manager_start_and_stop() {
    let dir = TempDir::new().expect("tempdir");
    let socket_dir = dir.path().join("awg-sockets");
    std::fs::create_dir_all(&socket_dir).expect("socket dir");
    let go_bin = write_executable(&dir.path().join("amneziawg-go"), fake_awg_go());
    let tools_bin = write_executable(&dir.path().join("awg"), fake_awg_tools());
    let iface = format!("skadiawg{}", std::process::id());
    let config = AwgClientConfig {
        interface_name: iface,
        private_key: "yAnz5TF+lXXJte14tji3zlMNq+hd2rYUIgJBgB3fBmk=".into(),
        address: "10.8.0.2/24".into(),
        server_public_key: "BKVtmgSy1V3vWdvrZ8NWdxgPACG6OBVH3pH8ptdNZFA=".into(),
        endpoint: "127.0.0.1:51820".into(),
        mtu: Some(1420),
        dns: Some("1.1.1.1".into()),
        allowed_ips: vec!["0.0.0.0/0".into()],
        persistent_keepalive: Some(25),
        obfuscation: sample_obfuscation(),
    };

    let _env = EnvGuard::set(&[
        ("AWG_GO_BINARY", Some(go_bin.to_str().unwrap())),
        ("AWG_TOOLS_BINARY", Some(tools_bin.to_str().unwrap())),
        ("AWG_UAPI_DIR", Some(socket_dir.to_str().unwrap())),
    ]);

    let mut manager = skadi_transport::AwgClientManager::start(&config)
        .await
        .expect("awg client manager start");
    manager.stop();
}

#[tokio::test]
async fn hysteria2_manager_lifecycle() {
    let dir = TempDir::new().expect("tempdir");
    let bin = write_executable(&dir.path().join("hysteria"), fake_sleep_binary());
    let config = Hysteria2ServerConfig {
        listen: "127.0.0.1:18443".parse().unwrap(),
        password: "secret".into(),
        cert_path: "/tmp/cert.pem".into(),
        key_path: "/tmp/key.pem".into(),
        masquerade_url: None,
    };

    let _env = EnvGuard::set(&[("HYSTERIA2_BINARY", Some(bin.to_str().unwrap()))]);

    let manager = Hysteria2Manager::start(&config)
        .await
        .expect("hysteria2 start");
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let handle = tokio::spawn(async move { manager.run_until_shutdown(shutdown_rx).await });
    shutdown_tx.send(true).expect("shutdown");
    handle.await.expect("join").expect("clean shutdown");
}

#[tokio::test]
async fn tuic_manager_lifecycle() {
    let dir = TempDir::new().expect("tempdir");
    let bin = write_executable(&dir.path().join("tuic-server"), fake_sleep_binary());
    let config = TuicServerConfig {
        listen: "127.0.0.1:18444".parse().unwrap(),
        uuid: "550e8400-e29b-41d4-a716-446655440000".into(),
        password: "secret".into(),
        cert_path: "/tmp/cert.pem".into(),
        key_path: "/tmp/key.pem".into(),
        congestion_control: "cubic".into(),
        alpn: vec!["h3".into()],
    };

    let _env = EnvGuard::set(&[("TUIC_SERVER_BINARY", Some(bin.to_str().unwrap()))]);

    let manager = TuicManager::start(&config).await.expect("tuic start");
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let handle = tokio::spawn(async move { manager.run_until_shutdown(shutdown_rx).await });
    shutdown_tx.send(true).expect("shutdown");
    handle.await.expect("join").expect("clean shutdown");
}

#[test]
fn manager_binary_not_found_errors() {
    {
        let _env = EnvGuard::set(&[("AWG_GO_BINARY", Some("/no/such/amneziawg-go"))]);
        let config = sample_awg_server_config("skadiwgfail");
        let err = match tokio::runtime::Runtime::new()
            .expect("runtime")
            .block_on(AwgManager::start(&config, None))
        {
            Err(err) => err,
            Ok(_) => panic!("expected awg binary not found"),
        };
        assert!(err.to_string().contains("not found"));
    }

    {
        let _env = EnvGuard::set(&[("HYSTERIA2_BINARY", Some("/no/such/hysteria"))]);
        let config = Hysteria2ServerConfig {
            listen: "127.0.0.1:1".parse().unwrap(),
            password: "x".into(),
            cert_path: "/tmp/cert.pem".into(),
            key_path: "/tmp/key.pem".into(),
            masquerade_url: None,
        };
        let err = match tokio::runtime::Runtime::new()
            .expect("runtime")
            .block_on(Hysteria2Manager::start(&config))
        {
            Err(err) => err,
            Ok(_) => panic!("expected hysteria binary not found"),
        };
        assert!(err.to_string().contains("not found"));
    }
}

#[tokio::test]
async fn hysteria2_early_exit_is_reported() {
    let dir = TempDir::new().expect("tempdir");
    let bin = write_executable(&dir.path().join("hysteria"), fake_exit_binary());
    let config = Hysteria2ServerConfig {
        listen: "127.0.0.1:18445".parse().unwrap(),
        password: "secret".into(),
        cert_path: "/tmp/cert.pem".into(),
        key_path: "/tmp/key.pem".into(),
        masquerade_url: None,
    };

    let _env = EnvGuard::set(&[("HYSTERIA2_BINARY", Some(bin.to_str().unwrap()))]);
    let err = match Hysteria2Manager::start(&config).await {
        Err(err) => err,
        Ok(_) => panic!("expected hysteria early exit"),
    };
    assert!(err.to_string().contains("exited"));
}
