use tokio::io::{AsyncReadExt, AsyncWriteExt};

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::RwLock;

use super::*;

#[napi]
pub struct Stream {
    connection: Connection,
    current_id: AtomicU32 ,

    responses: RwLock<HashMap<u32, Response>>,
}

#[napi]
impl Stream {
    #[napi(constructor)]
    pub fn new(auth: &[u8]) -> Self {
        let connection = Connection::new(auth);
        Self::from_conn(connection)
    }

    pub fn from_conn(connection: Connection) -> Self {
        Self { connection, current_id: AtomicU32::new(1), responses: RwLock::new(HashMap::new()) }
    }
    
    #[napi]
    pub async unsafe fn send(&mut self, flags: u8, method: &[u8], path: &[u8], body: Option<&[u8]>) -> u32 {
        let current_id = self.next_id();
        
        let auth = self.connection.auth.clone();

        let headers = Frame::header(&mut self.connection, flags, current_id, method, &*auth, path);

        let stream = unsafe { &mut *self.connection.stream.as_mut_ptr() };
        stream.write_all(&headers.to_bytes()).await
              .expect("failed to write headers to stream");

        if let Some(body) = body {
            let body_frame = Frame::new(Kind::Data, ACK, current_id, Box::from(body));
            stream.write_all(&body_frame.to_bytes()).await
                  .expect("failed to write body to stream");
        }

        stream.flush().await.expect("failed to flush");

        let mut responses = self.responses.write().unwrap();
        responses.insert(current_id, Response::empty());

        current_id
    }

    #[napi]
    pub async unsafe fn recv(&self, id: u32) -> Option<Response> {
        loop {
            if let Ok  (mut responses) = self.responses.try_write() && 
               let Some(    response )  = responses.get(&id) {
                if response.state.load(Ordering::Acquire) & STREAM_ENDED != 0 {
                    return responses.remove(&id);
                }
            } 
            
            tokio::task::yield_now().await;
        }
    }

    #[napi]
    pub async unsafe fn recv_loop(&mut self) {
        use std::alloc::{Layout, dealloc, alloc};
        use std::slice::from_raw_parts_mut;

        loop {
            let tls = unsafe { &mut *self.connection.stream.as_mut_ptr() };

            let mut header = [0; 9];
            tls.read_exact(&mut header).await.expect("failed to read frame header");

            let length = ((header[0] as usize) << 16) |
                         ((header[1] as usize) <<  8) |
                         ( header[2] as usize)        ;

            let kind   =   header[3]; // type
            let flags  =   header[4];

            let id     = ((header[5] as u32 & 0x7F) << 24) | 
                         ((header[6] as u32       ) << 16) | 
                         ((header[7] as u32       ) <<  8) | 
                          (header[8] as u32       )        ;

            let slice_layout = Layout::from_size_align(length, 1).unwrap();
            let payload_ptr  = unsafe { BruteSend(alloc(slice_layout))          };
            let mut payload  = unsafe { from_raw_parts_mut(payload_ptr.0, length) };

            tls.read_exact(&mut payload).await.expect("failed to read frame payload");

            match unsafe { std::mem::transmute(kind) } {
                Kind::Data => {
                    let mut responses = self.responses.write().unwrap();
                    let     response  = responses.get_mut(&id).expect("response not found");

                    if flags & END_STREAM == 0 {
                        response.write_body(payload);
                        response.state.fetch_or(STREAM_ENDED, Ordering::Release);
                        break;
                    }

                    response.write_body_chunk(&payload);
                },

                Kind::Headers => {
                    let headers = self.connection.decode(&payload)
                        .expect("failed to decode headers");
                    
                    let mut responses = self.responses.write().unwrap();
                    let     response  = responses.get_mut(&id).expect("response not found");

                    response.write_headers(headers);

                    if flags & END_STREAM != 0 {
                        response.state.fetch_or(STREAM_ENDED, Ordering::Release);
                    }
                },

                Kind::Priority  => { /* Ignore for now */ },
                Kind::RstStream => { /* Ignore for now */ },

                Kind::Settings => {
                    if flags & 0x1 == 0 { 
                        // Send ACK for server's SETTINGS frame
                        tls.write_all(&super::ACKNOWLEDGE_SETTINGS_FRAME).await
                            .expect("failed to send SETTINGS ACK");
                        
                        tls.flush().await.expect("failed to flush");
                    }
                },

                Kind::PushPromise  => { /* Ignore for now                 */ },

                Kind::Ping         => {
                    // Send PING ACK
                    let ping_ack = Frame::new(Kind::Ping, 0x1, 0, Box::from(&payload[..8]));
                    tls.write_all(&ping_ack.to_bytes()).await
                        .expect("failed to send PING ACK");
                    
                    tls.flush().await.expect("failed to flush");
                },

                Kind::GoAway        => { /* Handle GOAWAY if needed        */ },
                Kind::WindowUpdate  => { /* Handle WINDOW_UPDATE if needed */ },

                Kind::Continuation  => { /* Handle CONTINUATION if needed  */ },
            }

            unsafe { dealloc(payload_ptr.0, slice_layout); }
        }
    }

    fn next_id(&self) -> u32 {
        self.current_id.fetch_add(2, Ordering::SeqCst)
    }
}

pub struct BruteSend<T>(T);

unsafe impl<T> Send for BruteSend<T> {}