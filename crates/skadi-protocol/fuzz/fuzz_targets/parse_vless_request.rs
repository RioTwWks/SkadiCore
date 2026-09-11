#![no_main]
use libfuzzer_sys::fuzz_target;
use skadi_protocol::vless::parse::parse_request;

fuzz_target!(|data: &[u8]| {
    // Парсер не должен паниковать ни на каких входных данных.
    let _ = parse_request(data);
});
