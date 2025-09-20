#![feature(new_zeroed_alloc, maybe_uninit_slice)]

#[macro_use]
extern crate napi_derive;

// use std::io  ::{Read, Write, Result as IoResult};
// use std::net ::TcpStream;
// use std::sync::Arc;

// use webpki_roots::TLS_SERVER_ROOTS;
// use rustls::{ClientConfig, ClientConnection, RootCertStore, Stream};
// use hpack::{Decoder, Encoder};

pub mod core;

// const ACKNOWLEDGE_SETTINGS_PAYLOAD: [u8; 9] = [ 0, 0, 0, 4, 0, 0, 0, 0, 0 ];
// const ACKNOWLEDGE_SETTINGS_FRAME  : [u8; 9] = [ 0, 0, 0, 4, 1, 0, 0, 0, 0 ];

// pub fn main() {
//     let (mut tcp, mut conn) = set_up_tcp();

//     h2::client::Builder::new()
//         .initial_window_size(1024 * 1024) // 1 MB
//         .max_concurrent_streams(100)
//         .enable_push(false);

//     let mut tls  = Stream::new(&mut conn, &mut tcp);

//     // Preface by whirr starts playing
//     tls.write_all(b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n")
//        .expect("failed to write client preface");

//     // Send an initial SETTINGS frame (empty payload)
//     // length: 0, type: 4 (SETTINGS), flags: 0, stream_id: 0
//     tls.write_all(&ACKNOWLEDGE_SETTINGS_PAYLOAD)
//        .expect("failed to write SETTINGS frame");

//     tls.flush().expect("failed to flush");

//     // Checks that the ALPN negotiation resulted in HTTP/2
//     if tls.conn.alpn_protocol() != Some(b"h2") {
//         panic!(
//             "ALPN negotiation failed: HTTP/2 not negotiated {}",
//             String::from_utf8_lossy(tls.conn.alpn_protocol().unwrap_or(b"(none)"))
//         );
//     }

//     // Set up HPACK encoder and decoder
//     let mut encoder = Encoder::new();
//     let mut decoder = Decoder::new();

//     // Wait for server's SETTINGS frame and ACK it
//     loop {
//         let mut header = [0; 9];
//         tls.read_exact(&mut header).expect("failed to read frame header");

//         let length = ((header[0] as usize) << 16) |
//                      ((header[1] as usize) <<  8) |
//                       (header[2] as usize)        ;

//         let kind   =   header[3]; // type
//         let flags  =   header[4];

//         let mut payload = vec![0; length];
//         tls.read_exact(&mut payload).expect("failed to read frame payload");

//         if kind == 0x4 { // SETTINGS
//             if flags & 0x1 == 0 {
//                 // Send ACK for server's SETTINGS frame
//                 tls.write_all(&ACKNOWLEDGE_SETTINGS_FRAME)
//                     .expect("failed to send SETTINGS ACK");

//                 tls.flush().expect("failed to flush");
//             }

//             break;
//         }
//     }

//     // Send a GET request to "/api/v9/users/@me" for testing
//     send_request(&mut tls, &mut encoder, 1, "GET", "Bot NATE HIGGERS", "/api/v10/users/@me")
//          .expect("failed to send request");

//     let mut resp_body = Vec::with_capacity(2048);
//     let mut resp_hdrs = None;

//     loop {
//         // Read a frame header (9 bytes)
//         let mut header = [0; 9];
//         tls.read_exact(&mut header).expect("failed to read frame header");

//         let length = ((header[0] as usize) << 16) |
//                         ((header[1] as usize) <<  8) |
//                         (header[2] as usize)        ;

//         let kind   =   header[3]; // type
//         let flags  =   header[4];

//         // let stream_id = ((header[5] as u32 & 0x7F) << 24) | 
//         //                 ((header[6] as u32       ) << 16) | 
//         //                 ((header[7] as u32       ) <<  8) | 
//         //                  (header[8] as u32       )        ;

//         let mut payload = vec![0; length];
//         tls.read_exact(&mut payload).expect("failed to read frame payload");

//         match kind {
//             0x1 => { // HEADERS
//                 let headers = decoder.decode(&payload).expect("failed to decode headers");
//                 resp_hdrs = Some(headers);
//             },

//             0x0 => { // DATA
//                 resp_body.extend_from_slice(&payload);
                
//                 // END_STREAM
//                 if flags & 0x1 != 0 {  break; }
//             },

//             0x4 => { // SETTINGS
//                 if flags & 0x1 == 0 { 
//                     // Send ACK for server's SETTINGS frame
//                     tls.write_all(&ACKNOWLEDGE_SETTINGS_FRAME)
//                         .expect("failed to send SETTINGS ACK");
                    
//                     tls.flush().expect("failed to flush");
//                 }
//             },

//             _ => { /* Handle other frame types as needed */ }
//         }
//     }

//     if let Some(headers) = resp_hdrs {
//         println!("Response Headers:");
//         for (name, value) in headers {
//             println!("{}: {}", String::from_utf8_lossy(&name), String::from_utf8_lossy(&value));
//         }
//     }

//     println!("Body: {}", String::from_utf8_lossy(&resp_body));
// }

// pub fn send_request(
//     tls      : &mut Stream<ClientConnection, TcpStream>, 
//     encoder  : &mut Encoder, 
//     stream_id: u32 ,
//     method   : &str,
//     token    : &str, 
//     path     : &str,
// ) -> IoResult<()> {
//     let encoded = encoder.encode([
//         (b":method"      , unsafe { std::mem::transmute(method) }),
//         (b":scheme"      , b"https"                              ),
//         (b":authority"   , b"discord.com"                        ),
//         (b":path"        , unsafe { std::mem::transmute(path  ) }),
//         (b"authorization", unsafe { std::mem::transmute(token ) }),
//     ] as [(&[u8], &[u8]); 5]);
//     let len     = encoded.len() as u32   ;

//     let mut frame = Vec::with_capacity(9);
//     frame.extend_from_slice(&[
//         (len >> 16) as u8,
//         (len >>  8) as u8,
//          len        as u8,

//         0x1, // type  = HEADERS
//         0x5, // flags = END_HEADERS | END_STREAM
        
//         // stream_id (31 bits, highest bit must be zero)
//         ((stream_id >> 24) & 0x7F) as u8,
//          (stream_id >> 16)         as u8,
//          (stream_id >>  8)         as u8,
//           stream_id as u8               ,
//     ]);

//     frame.extend_from_slice(&encoded);

//     tls.write_all(&frame)?;
//     tls.flush()?;

//     Ok(())
// }

// pub fn set_up_tcp() -> (TcpStream, ClientConnection) {
//     let tcp = TcpStream::connect("discord.com:443")
//                         .expect("failed to connect to server");

//     let mut cfg = ClientConfig::builder()
//         .with_root_certificates(RootCertStore { roots: TLS_SERVER_ROOTS.to_vec() })
//         .with_no_client_auth();

//     // Set ALPN protocols to include "h2"
//     cfg.alpn_protocols.push(b"h2".to_vec());

//     let conn = ClientConnection::new(Arc::new(cfg), "discord.com".try_into().expect("invalid dnsname"))
//                                 .expect("failed to create client connection");

//     return (tcp, conn);
// }
