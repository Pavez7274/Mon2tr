use napi::bindgen_prelude::*;
use napi_sys::*;

use std::mem::transmute as t;
use std::ptr::*;

use super::Connection;

#[derive(Clone, Copy)]
#[repr(u8)]
pub enum Kind {
    Data         = 0,
    Headers      = 1,
    Priority     = 2,
    RstStream    = 3,
    Settings     = 4,
    PushPromise  = 5,
    Ping         = 6,
    GoAway       = 7,
    WindowUpdate = 8,
    Continuation = 9,
}

pub mod flags {
    pub mod data {
        pub const allowed   : u8 = 0b1001;
        pub const end_stream: u8 = 0b0001;
        pub const padded    : u8 = 0b1000;
    }

    pub mod hdrs {
        pub const allowed   : u8 = 0b101101;
        pub const end_stream: u8 = 0b000001;
        pub const end_hdrs  : u8 = 0b000100;
        pub const padded    : u8 = 0b001000;
        pub const priority  : u8 = 0b100000;
    }

    pub mod settings { pub const allowed: u8 = 1; pub const ack: u8 = 1; }
    pub mod ping     { pub const allowed: u8 = 1; pub const ack: u8 = 1; }
}

pub struct Frame {
    pub payload   : Box<[u8]>,
    pub identifier: u32      ,
    pub length    : u32      ,
    pub kind      : Kind     ,
    pub flags     : u8       ,
}

impl Frame {
    #[inline(always)]
    pub fn head(conn: &mut Connection, method: &[u8], path: &[u8], identifier: u32, flags: u8) -> Frame {
        let auth = &*conn.auth.clone();
        let encoded = conn.encode(&[
            (b":method"      ,   method      ),
            (b":scheme"      , b"https"      ),
            (b":authority"   , b"discord.com"),
            (b":path"        ,   path        ),
            (b"authorization",   auth        ),
        ]);

        Self::new(Kind::Headers, flags, identifier, encoded.into_boxed_slice())
    }
    
    #[inline(always)]
    pub fn new(kind: Kind, flags: u8, identifier: u32, payload: Box<[u8]>) -> Self {
        let length = payload.len() as u32;

        Self { payload, identifier, length, kind, flags }
    }

    #[inline(always)]
    pub unsafe fn d_hdrs(conn: &mut Connection, method: &[u8], path: &[u8], identifier: u32, flags: u8) -> Frame {
        let auth = &*conn.auth.clone();
        let mut payload = Box::new_uninit_slice(auth.len() + method.len() + path.len() + 64);
        let     ptr = payload.assume_init_mut().as_mut_ptr();
        let mut off = 0;

        #[inline(always)]
        unsafe fn copy(dst: *mut u8, src: &[u8], off: &mut usize) {
            std::ptr::copy_nonoverlapping(src.as_ptr(), dst.add(*off), src.len());
            *off += src.len();
        }

        copy(ptr, b"\0\0:method"              , &mut off); copy(ptr, method, &mut off);
        copy(ptr, b"\0\0:schemehttps"         , &mut off);
        copy(ptr, b"\0\0:authoritydiscord.com", &mut off);
        copy(ptr, b"\0\0:path"                , &mut off); copy(ptr, path  , &mut off);
        copy(ptr, b"\0\0authorization"        , &mut off); copy(ptr, auth  , &mut off);

        Self::new(Kind::Headers, flags, identifier, payload.assume_init())
    }

    pub unsafe fn to_bytes(&self) -> Box<[u8]> {
        // This allocates a boxed slice for the full frame: 9 bytes for the HTTP/2 frame header + payload length.
        // We use MaybeUninit to avoid unnecessary zero-initialization, since we'll write every byte manually.

        // SAFETY: We'll initialize every byte before use.
        let mut boxed = Box::<[u8]>::new_uninit_slice(self.payload.len() + 9);
        let buf = boxed.assume_init_mut();

        // Write the 3-byte length field (big-endian, only lower 24 bits used)
        let len = self.length;
        buf[0] = (len >> 16) as u8;
        buf[1] = (len >>  8) as u8;
        buf[2] =  len        as u8;

        // Write the 1-byte type (kind) and 1-byte flags
        buf[3] = self.kind as u8;
        buf[4] = self.flags     ;

        // Write the 4-byte stream identifier (big-endian, highest bit must be zero)
        // The HTTP/2 spec says the highest bit is reserved and must be ignored on receipt.
        let id = self.identifier & 0x7FFF_FFFF;
        buf[5] = (id >> 24) as u8;
        buf[6] = (id >> 16) as u8;
        buf[7] = (id >>  8) as u8;
        buf[8] =  id        as u8;

        // Copy the payload bytes directly after the header
        buf[9..].copy_from_slice(&self.payload);

        // SAFETY: All bytes have been initialized above, so it's safe to assume_init.
        boxed.assume_init()
    }
}

impl FromNapiValue for Frame {
    unsafe fn from_napi_value(env: napi_env, object: napi_value) -> napi::Result<Self> {
        #[inline]
        unsafe fn property_cast<V: FromNapiValue>(env: napi_env, obj: napi_value, field: &[u8]) -> V {
            let mut key = null_mut();
            napi_create_string_utf8(env, field.as_ptr().cast(), field.len() as isize, &mut key);

            let mut ret = null_mut();
            napi_get_property(env, obj, key, &mut ret);

            V::from_napi_value(env, ret).unwrap()
        }

        let kind      : u8    = property_cast(env, object, b"kind\0"      );
        let flags     : u8    = property_cast(env, object, b"flags\0"     );
        let identifier: u32   = property_cast(env, object, b"identifier\0");
        let payload   : &[u8] = property_cast(env, object, b"payload\0"   );

        Ok(Frame::new(t(kind), t(flags), identifier, Box::from(payload)))
    }
}