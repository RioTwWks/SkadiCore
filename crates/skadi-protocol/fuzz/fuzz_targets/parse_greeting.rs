#![no_main]
use libfuzzer_sys::fuzz_target;
use skadi_protocol::socks5::parse::parse_greeting;

fuzz_target!(|data: &[u8]| {
    // Парсер не должен паниковать ни на каких входных данных.
    // Ошибки — это нормально, паники — нет.
    let _ = parse_greeting(data);
});
