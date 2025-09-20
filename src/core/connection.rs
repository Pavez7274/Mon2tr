use webpki_roots::TLS_SERVER_ROOTS;

use rustls::{ClientConfig, RootCertStore};
use tokio_rustls::client::TlsStream;
use tokio_rustls::TlsConnector;
use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use napi::bindgen_prelude::block_on;

use std::mem::MaybeUninit;
use std::sync::Arc;

use crate::core::Headers;

use super::ACKNOWLEDGE_SETTINGS_PAYLOAD;
use super::ACKNOWLEDGE_SETTINGS_FRAME  ;
use super::Header;

#[napi]
pub struct Connection {
    pub(crate) stream: MaybeUninit<TlsStream<TcpStream>>,
    pub(crate) auth  : Arc::<[u8]>,
    
    decoder: hpack::Decoder<'static>,
    encoder: hpack::Encoder<'static>,
}

#[napi]
impl Connection {
    #[napi(constructor)]
    pub fn new(auth: &[u8]) -> Self {
        let (mut conn, connector) = Self::__new__(auth);

        block_on(conn.establish_connection(&connector));
        block_on(unsafe { conn.handshake() });

        conn
    }

    fn __new__(auth: &[u8]) -> (Self, TlsConnector) {
        let mut cfg = ClientConfig::builder()
            .with_root_certificates(RootCertStore { roots: TLS_SERVER_ROOTS.to_vec() })
            .with_no_client_auth();

        cfg.alpn_protocols.push(vec![b'h', b'2']);

        (
            Self {
                decoder: hpack::Decoder::new(),
                encoder: hpack::Encoder::new(),
                stream : MaybeUninit::uninit(),
                auth   : auth.into()
            },

            TlsConnector::from(Arc::new(cfg))
        )
    }

    #[inline]
    pub async fn establish_connection(&mut self, connector: &TlsConnector) {
        let tcp = TcpStream::connect("discord.com:443").await
                            .expect("failed to connect to server");

        let conn = connector.connect("discord.com".try_into().expect("invalid domain"), tcp).await
                            .expect("failed to create TLS connection");

        self.stream = MaybeUninit::new(conn);
    }

    #[napi]
    pub async unsafe fn handshake(&mut self) {
        let stream = unsafe { &mut *self.stream.as_mut_ptr() };

        // Preface by whirr starts playing
        stream.write_all(b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n").await
              .expect("failed to write client preface");

        // Send an initial SETTINGS frame (empty payload)
        // length: 0, type: 4 (SETTINGS), flags: 0, stream_id: 0
        stream.write_all(&ACKNOWLEDGE_SETTINGS_PAYLOAD).await
              .expect("failed to write SETTINGS frame");

        stream.flush().await.expect("failed to flush");

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
            stream.read_exact(&mut header).await.expect("failed to read frame header");

            let length = ((header[0] as usize) << 16) |
                         ((header[1] as usize) <<  8) |
                          (header[2] as usize)        ;

            let kind   =   header[3]; // type
            let flags  =   header[4];

            let mut payload = vec![0; length];
            stream.read_exact(&mut payload).await.expect("failed to read frame payload");

            if kind == 4 { // SETTINGS frame
                if flags & 0x1 != 0 { // ACK flag set
                    break; // Handshake complete
                }

                // Always ACK any SETTINGS frame from server
                stream.write_all(&ACKNOWLEDGE_SETTINGS_FRAME).await
                      .expect("failed to write SETTINGS ACK");

                stream.flush().await.expect("failed to flush");
            }
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
}
