#![no_main]
use libfuzzer_sys::fuzz_target;
use skadi_protocol::socks5::parse::parse_auth;

fuzz_target!(|data: &[u8]| {
    let _ = parse_auth(data);
});
