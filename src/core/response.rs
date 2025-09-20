use std::sync::atomic::{AtomicU8, Ordering};
use std::mem::MaybeUninit;

use super::Headers;

pub const HEADERS_RECEIVED: u8 =  1;
pub const BODY_RECEIVED   : u8 =  2;
pub const STREAM_ENDED    : u8 =  4;
pub const WITHOUT_BODY    : u8 =  8;
pub const FAILED          : u8 = 16;
pub const CANCELLED       : u8 = 32;
pub const INITIALIZING    : u8 = 64;

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

    #[napi(ts_return_type = "{ name: string; value: string; }[]", getter)]
    pub fn headers(&self) -> &Headers {
        unsafe { self.headers.assume_init_ref() }
    }

    #[napi(getter)]
    pub fn body(&self) -> &Vec<u8> {
        unsafe { self.body.assume_init_ref() }
    }

    #[napi]
    pub fn write_headers(&mut self, headers: Headers) {
        unsafe { self.headers.as_mut_ptr().write(headers); }
    }

    #[napi]
    pub fn write_body(&mut self, body: &[u8]) {
        let mut vec = Vec::with_capacity(body.len());
        vec.extend_from_slice(body);

        unsafe { self.body.as_mut_ptr().write(vec); }

        self.state.fetch_or(BODY_RECEIVED | STREAM_ENDED, Ordering::Release);
    }

    #[napi]
    pub fn write_body_chunk(&mut self, chunk: &[u8]) {
        // This bullshit was slow as fuck
        // let body = unsafe {
        //     if self.state.load(Ordering::Acquire) & BODY_RECEIVED == 0 {
        //         self.body.as_mut_ptr().write(Vec::new());
        //     }

        //     &mut *self.body.as_mut_ptr()
        // };

        self.body_mut().extend_from_slice(chunk);
        self.state.fetch_or(BODY_RECEIVED, Ordering::Release);
    }

    #[napi]
    pub fn body_mut(&mut self) -> &mut Vec<u8> {
        // Fast path: body already initialized
        if self.state.load(Ordering::Acquire) & BODY_RECEIVED == 0 {
            return unsafe { &mut *self.body.as_mut_ptr() };
        }

        // Tries to mark as INITIALIZING (only one thread can succeed)
        loop {
            let current = self.state.load(Ordering::Acquire);
            if current & BODY_RECEIVED != 0 { // other thread already initialized the body
                return unsafe { &mut *self.body.as_mut_ptr() };
            }

            // Another thread is initializing the body
            // Short spin-wait and retry
            if current & INITIALIZING != 0 {
                std::hint::spin_loop();
                continue;
            }

            // Try to mark as INITIALIZING
            let want = current | INITIALIZING;
            match self.state.compare_exchange(current, want, Ordering::AcqRel, Ordering::Acquire) {
                Ok (_) => break   , // We got the "reservation" to initialize
                Err(_) => continue, // Failed, retry
            }
        }

        // We are the thread that will initialize the body
        unsafe { self.body.as_mut_ptr().write(Vec::new()); }

        // Publish the body as initialized
        // We make an atomic update to set BODY_RECEIVED and clear INITIALIZING
        // Here we don't care about bit changes, we use fetch_update to ensure safety
        let _ = self.state.fetch_update(Ordering::AcqRel, Ordering::Acquire, |mut state| {
            state &= !INITIALIZING;
            state |= BODY_RECEIVED;

            Some(state)
        });

        unsafe { &mut *self.body.as_mut_ptr() }
    }
    
    #[napi]
    pub fn drop(&mut self) {
        let state = self.state.load(Ordering::Acquire);

        if state & HEADERS_RECEIVED != 0 {
            unsafe { std::ptr::drop_in_place(self.headers.as_mut_ptr()); }
        }

        if state & BODY_RECEIVED    != 0 {
            unsafe { std::ptr::drop_in_place(self.body.as_mut_ptr()); }
        }
    }
}