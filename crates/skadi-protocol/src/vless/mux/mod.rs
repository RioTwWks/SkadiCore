//! VLESS Mux (Xray `common/mux` wire format).

pub mod frame;

pub use frame::{
    encode_data_frame, encode_end_frame, encode_meta, parse_frame, parse_meta_body, MuxError,
    MuxFrame, MuxMeta, MAX_CHUNK_SIZE, MAX_META_LEN, NETWORK_TCP, NETWORK_UDP, OPTION_DATA,
    OPTION_ERROR, SESSION_STATUS_END, SESSION_STATUS_KEEP, SESSION_STATUS_KEEP_ALIVE,
    SESSION_STATUS_NEW,
};
