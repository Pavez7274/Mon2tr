use webpki_roots::TLS_SERVER_ROOTS;
use rustls::{ClientConfig, RootCertStore};

use tokio_rustls::client::TlsStream;
use tokio_rustls::TlsConnector;
use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use std::collections::HashMap;
use std::mem::{MaybeUninit, transmute as t};
use std::sync::atomic::{Ordering::*, AtomicU32};
use std::sync::Arc;

use crate::ops::spin::Spin;
use crate::{BruteSend, bit_check};
use super::*;

use super::ACKNOWLEDGE_SETTINGS_PAYLOAD;
use super::ACKNOWLEDGE_SETTINGS_FRAME  ;
use super::Header;

#[napi]
pub struct Connection {
    pub(crate) resps : HashMap<u32, Response>,
    pub(crate) spin  : Spin,
    pub(crate) stream: MaybeUninit<TlsStream<TcpStream>>,
    pub(crate) auth  : Arc::<[u8]>,
    pub(crate) cur_id: AtomicU32, 
    
    decoder: hpack::Decoder<'static>,
    encoder: hpack::Encoder<'static>,
}

#[napi]
impl Connection {
    #[napi(constructor)]
    pub unsafe fn new(auth: &[u8]) -> Self {
        Self {
            decoder: hpack::Decoder::new(),
            encoder: hpack::Encoder::new(),
            stream : MaybeUninit::uninit(),
            cur_id : AtomicU32::new(1), 
            resps  : HashMap::new(),
            spin   : Spin::new(),
            auth   : auth.into(),
        }
    }

    #[napi]
    pub async unsafe fn connect(&mut self, ip: Option<&[u8]>, port: Option<u16>) {
        let mut cfg = ClientConfig::builder()
                .with_root_certificates(RootCertStore { roots: TLS_SERVER_ROOTS.to_vec() })
                .with_no_client_auth();

        cfg.alpn_protocols.push(b"h2".to_vec());

        let conn = TlsConnector::from(Arc::new(cfg));
        let addr = match (ip, port) {
            (Some(a), Some(p)) => (t::<_, &str>(a), p  ),
            (None   , Some(p)) => ("discord.com"  , p  ),
            _                  => ("discord.com"  , 443),
        };

        let tcp = TcpStream::connect(addr).await.expect("failed to connect to server");
        let tls = conn.connect(addr.0.try_into().expect("invalid domain"), tcp).await
                      .expect("failed to create TLS connection");

        self.stream = MaybeUninit::new(tls);
    }
    
    #[napi]
    pub async unsafe fn handshake(&mut self) {
        let stream = self.stream.assume_init_mut();

        // Preface by whirr starts playing
        stream.write_all(b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n").await
              .expect("failed to write client preface");

        // Send an initial SETTINGS frame (empty payload)
        // length: 0, type: 4 (SETTINGS), flags: 0, stream_id: 0
        stream.write_all(&ACKNOWLEDGE_SETTINGS_PAYLOAD).await
              .expect("failed to write SETTINGS frame");

        stream.flush().await
              .expect("failed to flush");

        // Checks that the ALPN negotiation resulted in HTTP/2
        if stream.get_ref().1.alpn_protocol() != Some(b"h2") {
            panic!(
                "ALPN negotiation failed: HTTP/2 not negotiated {}",
                String::from_utf8_lossy(stream.get_ref().1.alpn_protocol().unwrap_or(b"(none)"))
            );
        }

        // Wait for server's SETTINGS frame and ACK it
        loop {
            let mut header = [0; 9];
            if let Err(e) = stream.read_exact(&mut header).await {
                // Peer closed connection (often without TLS close_notify). Treat as EOF.
                if e.kind() == std::io::ErrorKind::UnexpectedEof { return; }
                panic!("failed to read frame header: {}", e);
            }

            let length = ((header[0] as usize) << 16) |
                         ((header[1] as usize) <<  8) |
                          (header[2] as usize)        ;

            let kind   =   header[3]; // type
            let flags  =   header[4];

            let mut payload = vec![0; length];
            if let Err(e) = stream.read_exact(&mut payload).await {
                if e.kind() == std::io::ErrorKind::UnexpectedEof {
                    return;
                }
                panic!("failed to read frame payload: {}", e);
            }

            if kind == 4 { // SETTINGS frame
                if flags & 0x1 != 0 { // ACK flag set
                    break; // Handshake complete
                }

                // Always ACK any SETTINGS frame from server
                stream.write_all(&ACKNOWLEDGE_SETTINGS_FRAME).await
                      .expect("failed to write SETTINGS ACK");

                stream.flush().await
                      .expect("failed to flush");
            }
        }
    }

    #[napi(js_name = "create_stream")]
    pub fn create_stream(&mut self) -> Stream<'_> {
        Stream(self)
    }

    #[napi]
    pub async unsafe fn recv(&mut self, id: u32) -> Option<Response> {
        loop {
            if let Some(_guard) = self.spin.try_lock() {
                if let Some(r) = self.resps.get(&id) && r.finalized(){
                    return self.resps.remove(&id);
                }
            }

            tokio::task::yield_now().await;
        }
    }

    #[napi(js_name = "recv_loop")]
    pub async unsafe fn recv_loop(&mut self) {
        use std::ptr::from_raw_parts_mut;

        loop {
            let stream = self.stream.assume_init_mut();

            let mut header = [0; 9];
            if let Err(e) = stream.read_exact(&mut header).await {
                if e.kind() == std::io::ErrorKind::UnexpectedEof {
                    return;
                }
                
                panic!("failed to read frame header: {}", e);
            }

            let length = usize::from_be_bytes([0, 0, 0, 0, 0, header[0], header[1], header[2]]);
            let id     = u32  ::from_be_bytes([header[5], header[6], header[7], header[8]]) & 0x7FFF_FFFF;
            let kind   = header[3]; // type
            let flags  = header[4];

            let payload_ptr = BruteSend(crate::__rust_alloc(length, 1));
            let mut payload = &mut *from_raw_parts_mut(payload_ptr.0, length);

            if let Err(e) = stream.read_exact(&mut payload).await {
                if e.kind() == std::io::ErrorKind::UnexpectedEof {
                    crate::__rust_dealloc(payload_ptr.0, length, 1);
                    return;
                }
                crate::__rust_dealloc(payload_ptr.0, length, 1);
                panic!("failed to read frame payload: {}", e);
            }

            match t(kind) {
                Kind::Data => {
                    let _guard   = self.spin.lock();
                    let response = self.resps.get_mut(&id).expect("response not found");

                    // Append the received data chunk. If the END_STREAM flag is set,
                    // mark the stream as ended. Note: `flags::data::end_stream` is
                    // non-zero when this is the final DATA frame for the stream.
                    response.body_write_chunk(&payload);

                    if bit_check!(flags, flags::hdrs::end_stream) {
                        response.state.fetch_or(STREAM_ENDED, Release);
                    }
                },

                Kind::Headers => {
                    let headers = self.decode(&payload).expect("failed to decode headers");
                    
                    let _guard   = self.spin.lock();
                    let response = self.resps.get_mut(&id).expect("response not found");

                    response.headers_write(headers);

                    if bit_check!(flags, flags::hdrs::end_stream) {
                        response.state.fetch_or(STREAM_ENDED, Release);
                    }
                },

                Kind::Priority  => { /* Ignore for now */ },
                Kind::RstStream => { /* Ignore for now */ },

                Kind::Settings => {
                    if flags & flags::settings::ack == 0 { 
                        stream.write_all(&ACKNOWLEDGE_SETTINGS_FRAME).await
                              .expect("failed to send SETTINGS ACK");
                        
                        stream.flush().await.expect("failed to flush");
                    }
                },

                Kind::PushPromise  => { /* Ignore for now */ },

                Kind::Ping         => {
                    let ping_ack = Frame::new(Kind::Ping, flags::ping::ack, 0, Box::from(&payload[..8]));
                    stream.write_all(&ping_ack.to_bytes()).await
                          .expect("failed to send PING ACK");
                    
                    stream.flush().await.expect("failed to flush");
                },

                Kind::GoAway        => {
                    // Parse GOAWAY payload: first 4 bytes = last stream ID, next 4 bytes = error code
                    if payload.len() >= 8 {
                        let last_stream = u32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]]);
                        let error_code = u32::from_be_bytes([payload[4], payload[5], payload[6], payload[7]]);
                        eprintln!("Received GOAWAY: last_stream={}, error_code={}", last_stream, error_code);
                        if payload.len() > 8 {
                            eprintln!("  debug_data: {:?}", String::from_utf8_lossy(&payload[8..]));
                        }
                    }
                    return; // Exit recv_loop on GOAWAY
                },
                Kind::WindowUpdate  => { /* Handle WINDOW_UPDATE if needed */ },
                Kind::Continuation  => { /* Handle CONTINUATION if needed  */ },
            }

            crate::__rust_dealloc(payload_ptr.0, length, 1);
        }
    }

    #[inline]
    pub fn decode(&mut self, data: &[u8]) -> Result<Headers, hpack::decoder::DecoderError> {
        let mut headers = Vec::with_capacity(24);
        
        self.decoder.decode_with_cb(data, |name, value| headers.push(Header::from((name, value))))?;

        Ok(Headers(headers.into_boxed_slice()))
    }

    #[inline]
    pub fn encode(&mut self, headers: &[(&[u8], &[u8])]) -> Vec<u8> {
        self.encoder.encode(headers.iter().copied())
    }

    #[inline]
    pub fn encode_non_copy(&mut self, headers: &[(&[u8], &[u8])]) -> Vec<u8> {
        // SAFETY: We are transmuting &[(&[u8], &[u8])] to an iterator of (&[u8], &[u8]).
        // This is safe because we are not changing lifetimes or mutability, just avoiding .copied().
        // However, we must ensure that the encoder does not outlive the headers slice.
        // If the encoder requires 'static, this is UB. Otherwise, it's fine.
        self.encoder.encode(headers.iter().map(|&(k, v)| (k, v)))
    }

    #[napi(js_name = "current_id")]
    pub fn current_id(&self) -> u32 {
        self.cur_id.load(Relaxed)
    }

    #[napi(js_name = "next_id")]
    #[inline(always)]
    pub fn next_id(&self) -> u32 {
        self.cur_id.fetch_add(2, SeqCst)
    }
}
