//! E2E: AmneziaWG L3 tunnel через два netns (server ↔ client) + TCP echo.
//!
//! AWG — не SOCKS5: на одном хосте без netns трафик к tunnel-IP уходит в `lo`.
//! Пропускается без `amneziawg-go`/`awg`, без `sudo`/`CAP_NET_ADMIN`, или без `ip`.

use skadi_transport::{
    generate_keypair, render_client_conf, render_server_conf, strip_wgquick_fields,
    AwgClientExport, AwgObfuscationConfig, AwgPeerConfig, AwgServerConfig,
};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

fn which(name: &str) -> Option<PathBuf> {
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {name}"))
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
}

fn bin_available(env_name: &str, default: &str) -> Option<PathBuf> {
    std::env::var(env_name)
        .ok()
        .map(PathBuf::from)
        .filter(|p| p.is_file())
        .or_else(|| which(default))
}

fn awg_ready() -> Option<(PathBuf, PathBuf)> {
    let go = bin_available("AWG_GO_BINARY", "amneziawg-go")?;
    let tools = bin_available("AWG_TOOLS_BINARY", "awg").or_else(|| which("wg"))?;
    Some((go, tools))
}

fn have_root() -> bool {
    Command::new("sudo")
        .args(["-n", "true"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn sudo(args: &[&str]) -> std::process::Output {
    Command::new("sudo")
        .args(["-n"])
        .args(args)
        .output()
        .expect("sudo")
}

fn sudo_ok(args: &[&str]) {
    let out = sudo(args);
    assert!(
        out.status.success(),
        "sudo {:?} failed: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
}

fn sample_obf() -> AwgObfuscationConfig {
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

struct NetnsGuard {
    names: Vec<String>,
}

impl NetnsGuard {
    fn new(names: &[&str]) -> Self {
        for n in names {
            let _ = sudo(&["ip", "netns", "del", n]);
            sudo_ok(&["ip", "netns", "add", n]);
        }
        Self {
            names: names.iter().map(|s| (*s).to_string()).collect(),
        }
    }
}

impl Drop for NetnsGuard {
    fn drop(&mut self) {
        let _ = sudo(&["killall", "amneziawg-go"]);
        thread::sleep(Duration::from_millis(200));
        for n in &self.names {
            let _ = sudo(&["ip", "netns", "del", n]);
        }
    }
}

fn write_setconf(path: &Path, full_conf: &str) {
    std::fs::write(path, strip_wgquick_fields(full_conf)).expect("write conf");
}

#[test]
fn awg_netns_tcp_echo() {
    let Some((go_bin, tools_bin)) = awg_ready() else {
        eprintln!("SKIP awg_traffic_e2e: amneziawg-go / awg not found");
        return;
    };
    if which("ip").is_none() {
        eprintln!("SKIP awg_traffic_e2e: ip (iproute2) not found");
        return;
    }
    if !have_root() {
        eprintln!("SKIP awg_traffic_e2e: sudo -n / CAP_NET_ADMIN required");
        return;
    }

    let pid = std::process::id();
    let ns_s = format!("skadiawgs{pid}");
    let ns_c = format!("skadiawgc{pid}");
    let iface_s = format!("awgs{}", pid % 10000);
    let iface_c = format!("awgc{}", pid % 10000);
    let veth_s = format!("veths{}", pid % 10000);
    let veth_c = format!("vethc{}", pid % 10000);

    let _ = sudo(&["killall", "amneziawg-go"]);
    thread::sleep(Duration::from_millis(300));
    let _ = std::fs::remove_file(format!("/var/run/amneziawg/{iface_s}.sock"));
    let _ = std::fs::remove_file(format!("/var/run/amneziawg/{iface_c}.sock"));

    let _guard = NetnsGuard::new(&[&ns_s, &ns_c]);

    sudo_ok(&[
        "ip", "link", "add", &veth_s, "type", "veth", "peer", "name", &veth_c,
    ]);
    sudo_ok(&["ip", "link", "set", &veth_s, "netns", &ns_s]);
    sudo_ok(&["ip", "link", "set", &veth_c, "netns", &ns_c]);
    sudo_ok(&[
        "ip",
        "-n",
        &ns_s,
        "addr",
        "add",
        "192.168.77.1/24",
        "dev",
        &veth_s,
    ]);
    sudo_ok(&[
        "ip",
        "-n",
        &ns_c,
        "addr",
        "add",
        "192.168.77.2/24",
        "dev",
        &veth_c,
    ]);
    sudo_ok(&["ip", "-n", &ns_s, "link", "set", &veth_s, "up"]);
    sudo_ok(&["ip", "-n", &ns_c, "link", "set", &veth_c, "up"]);
    sudo_ok(&["ip", "-n", &ns_s, "link", "set", "lo", "up"]);
    sudo_ok(&["ip", "-n", &ns_c, "link", "set", "lo", "up"]);

    let (srv_priv, srv_pub) = generate_keypair();
    let (cli_priv, cli_pub) = generate_keypair();
    let obf = sample_obf();

    let server_cfg = AwgServerConfig {
        listen: "0.0.0.0:51820".parse().unwrap(),
        interface_name: iface_s.clone(),
        private_key: srv_priv,
        address: "10.66.0.1/24".into(),
        mtu: Some(1420),
        obfuscation: obf.clone(),
        peers: vec![AwgPeerConfig {
            public_key: cli_pub,
            allowed_ips: vec!["10.66.0.2/32".into()],
            preshared_key: None,
            endpoint: None,
            persistent_keepalive: None,
        }],
    };
    let client_export = AwgClientExport {
        private_key: cli_priv,
        address: "10.66.0.2/24".into(),
        dns: None,
        allowed_ips: vec!["10.66.0.1/32".into()],
        persistent_keepalive: Some(25),
    };
    let client_conf = render_client_conf(
        &srv_pub,
        "192.168.77.1:51820",
        &client_export,
        &obf,
        Some(1420),
    )
    .expect("client conf");
    let server_conf = render_server_conf(&server_cfg).expect("server conf");

    let dir = tempfile::tempdir().unwrap();
    let srv_path = dir.path().join("server.conf");
    let cli_path = dir.path().join("client.conf");
    write_setconf(&srv_path, &server_conf);
    write_setconf(&cli_path, &client_conf);

    let mut child_s = Command::new("sudo")
        .args(["-n", "ip", "netns", "exec", &ns_s])
        .arg(&go_bin)
        .args(["-f", &iface_s])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn server go");
    let mut child_c = Command::new("sudo")
        .args(["-n", "ip", "netns", "exec", &ns_c])
        .arg(&go_bin)
        .args(["-f", &iface_c])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn client go");

    thread::sleep(Duration::from_millis(800));
    if let Some(status) = child_s.try_wait().ok().flatten() {
        let mut err = String::new();
        if let Some(mut stderr) = child_s.stderr.take() {
            let _ = stderr.read_to_string(&mut err);
        }
        panic!("amneziawg-go server exited early ({status}): {err}");
    }
    if let Some(status) = child_c.try_wait().ok().flatten() {
        let mut err = String::new();
        if let Some(mut stderr) = child_c.stderr.take() {
            let _ = stderr.read_to_string(&mut err);
        }
        panic!("amneziawg-go client exited early ({status}): {err}");
    }

    let tools = tools_bin.to_str().unwrap();
    sudo_ok(&[
        "ip",
        "netns",
        "exec",
        &ns_s,
        tools,
        "setconf",
        &iface_s,
        srv_path.to_str().unwrap(),
    ]);
    sudo_ok(&[
        "ip",
        "netns",
        "exec",
        &ns_c,
        tools,
        "setconf",
        &iface_c,
        cli_path.to_str().unwrap(),
    ]);
    sudo_ok(&[
        "ip",
        "-n",
        &ns_s,
        "addr",
        "replace",
        "10.66.0.1/24",
        "dev",
        &iface_s,
    ]);
    sudo_ok(&[
        "ip",
        "-n",
        &ns_c,
        "addr",
        "replace",
        "10.66.0.2/24",
        "dev",
        &iface_c,
    ]);
    sudo_ok(&["ip", "-n", &ns_s, "link", "set", &iface_s, "up"]);
    sudo_ok(&["ip", "-n", &ns_c, "link", "set", &iface_c, "up"]);

    thread::sleep(Duration::from_millis(500));

    let ns_s_echo = ns_s.clone();
    let listener = thread::spawn(move || {
        let out = Command::new("sudo")
            .args([
                "-n",
                "ip",
                "netns",
                "exec",
                &ns_s_echo,
                "python3",
                "-c",
                r#"
import socket
s=socket.socket(); s.setsockopt(socket.SOL_SOCKET,socket.SO_REUSEADDR,1)
s.bind(('10.66.0.1', 19999)); s.listen(1); s.settimeout(8)
c,_=s.accept(); d=c.recv(64); c.sendall(d); c.close()
"#,
            ])
            .output()
            .expect("echo server");
        assert!(
            out.status.success(),
            "echo server failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    });

    thread::sleep(Duration::from_millis(400));

    let client_out = Command::new("sudo")
        .args([
            "-n",
            "ip",
            "netns",
            "exec",
            &ns_c,
            "python3",
            "-c",
            r#"
import socket
c=socket.create_connection(('10.66.0.1',19999), timeout=5)
payload=b'hello awg traffic e2e'
c.sendall(payload)
got=c.recv(len(payload))
assert got==payload, got
print('OK', got.decode())
"#,
        ])
        .output()
        .expect("echo client");

    assert!(
        client_out.status.success(),
        "echo client failed: stdout={} stderr={}",
        String::from_utf8_lossy(&client_out.stdout),
        String::from_utf8_lossy(&client_out.stderr)
    );
    listener.join().expect("listener join");

    let _ = child_s.kill();
    let _ = child_c.kill();
    let _ = child_s.wait();
    let _ = child_c.wait();
}
