use tokio::io::AsyncWriteExt;
use super::*;

#[napi]
pub struct Stream<'a>(pub(crate) &'a mut Connection);

#[napi]
impl<'a> Stream<'a> {
    #[napi]
    #[inline(always)]
    pub async unsafe fn fsend(&mut self, frame: Frame, body: &[u8], cid: u32) {
        let stream = &mut *self.0.stream.as_mut_ptr();
        
        stream.write_all(&frame.to_bytes()).await
              .expect("failed to write headers to stream");
            
    
        if body.len() != 0 {
            // Send body as DATA frame with END_STREAM flag
            let pframe = Frame::new(Kind::Data, flags::data::end_stream, cid, Box::from(body));
            stream.write_all(&pframe.to_bytes()).await
                  .expect("failed to write body to stream");
        }

        stream.flush().await.expect("failed to flush");

        let _guard = self.0.spin.lock();
        self.0.resps.insert(cid, Response::empty());
    }
    
    #[napi]
    pub async unsafe fn send(&mut self, method: &[u8], path: &[u8], body: &[u8], flags: Option<u8>) -> u32 {
        let cid  = self.0.next_id();
        let hdrs = Frame::head(self.0, method, path, cid, flags.unwrap_or(flags::hdrs::end_hdrs | flags::hdrs::end_stream));

        self.fsend(hdrs, body, cid).await;
        cid
    }
}
