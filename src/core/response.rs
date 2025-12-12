use napi::bindgen_prelude::Uint8Array;

use std::sync::atomic::{AtomicU8, Ordering::*};
use std::mem::MaybeUninit;

use super::Headers;

pub const HEADERS_RECEIVED: u8 =  1;
pub const BODY_RECEIVED   : u8 =  2;
pub const STREAM_ENDED    : u8 =  4;
pub const WITHOUT_BODY    : u8 =  8;
pub const FAILED          : u8 = 16;
pub const CANCELLED       : u8 = 32;
pub const INITIALIZING    : u8 = 64;

#[macro_export]
macro_rules! bit_check {
    ($state:expr, $flag:expr) => {
        $state & ($flag) != 0
    };
}

#[napi]
pub struct Response {
    pub(crate) headers: MaybeUninit<Headers>,
    pub(crate) body   : MaybeUninit<Vec<u8>>,
    pub(crate) state  : AtomicU8,
}

#[napi]
impl Response {
    #[napi]
    pub fn empty() -> Self {
        Self {
            headers: MaybeUninit::uninit(),
            body   : MaybeUninit::uninit(),
            state  : AtomicU8::new(0),
        }
    }

    pub fn finalized(&self) -> bool {
        let state = self.state.load(Acquire);

        // Finalized if we have headers AND either received body OR stream ended (no body case)
        bit_check!(state, STREAM_ENDED)
            && (bit_check!(state, BODY_RECEIVED) || bit_check!(state, WITHOUT_BODY))
            && bit_check!(state, HEADERS_RECEIVED)
    }

    #[napi(getter)]
    pub unsafe fn headers(&self) -> &Headers {
        self.headers.assume_init_ref()
    }

    #[napi(getter)]
    pub unsafe fn body(&mut self) -> Uint8Array {
        Uint8Array::with_data_copied(self.body.assume_init_ref())
    }

    #[napi(js_name = "headers_write")]
    pub unsafe fn headers_write(&mut self, headers: Headers) {
        self.headers.as_mut_ptr().write(headers);
        self.state.fetch_or(HEADERS_RECEIVED, Release);
    }

    #[napi(js_name = "body_write")]
    pub unsafe fn body_write(&mut self, body: &[u8]) {
        self.body_mut().extend_from_slice(body);
        self.state.fetch_or(BODY_RECEIVED, Release);
    }

    #[napi(js_name = "body_write_chunk")]
    pub unsafe fn body_write_chunk(&mut self, chunk: &[u8]) {
        self.body_mut().extend_from_slice(chunk);
        self.state.fetch_or(BODY_RECEIVED, Release);
    }

    #[napi(js_name = "body_mut")]
    pub unsafe fn body_mut(&mut self) -> &mut Vec<u8> {
        // Fast path: body already initialized
        if bit_check!(self.state.load(Relaxed), BODY_RECEIVED) {
            return self.body.assume_init_mut();
        }

        // Tries to mark as INITIALIZING (only one thread can succeed)
        loop {
            let current = self.state.load(Acquire);
            if bit_check!(current, BODY_RECEIVED) { // other thread already initialized the body
                return self.body.assume_init_mut();
            }

            // Another thread is initializing the body
            // Short spin-wait and retry
            if current & INITIALIZING != 0 {
                std::hint::spin_loop();
                continue;
            }

            // Try to mark as INITIALIZING
            match self.state.compare_exchange(current, current | INITIALIZING, AcqRel, Acquire) {
                Ok (_) => break   , // We got the "reservation" to initialize
                Err(_) => continue, // Failed, retry
            }
        }

        // We are the thread that will initialize the body
        self.body.as_mut_ptr().write(Vec::new());

        // Publish the body as initialized
        // We make an atomic update to set BODY_RECEIVED and clear INITIALIZING
        // Here we don't care about bit changes, we use fetch_update to ensure safety
        let _ = self.state.fetch_update(AcqRel, Acquire, |mut state| {
            state &= !INITIALIZING;
            state |= BODY_RECEIVED;

            Some(state)
        });

        self.body.assume_init_mut()
    }
    
    #[napi]
    pub unsafe fn drop(&mut self) {
        use std::ptr::drop_in_place;

        let state = self.state.load(Acquire);

        if state & HEADERS_RECEIVED != 0 
            { drop_in_place(self.headers.as_mut_ptr()); }

        if state & BODY_RECEIVED    != 0 
            { drop_in_place(self.body.as_mut_ptr()); }
    }
}