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

pub const END_HEADERS: u8 = 0x_4;
pub const END_STREAM : u8 = 0x_1;
pub const PING_ACK   : u8 = 0x_1;
pub const ACK        : u8 = 0x_1;
pub const PADDED     : u8 = 0x_8;
pub const PRIORITY   : u8 = 0x20; 

pub struct Frame {
    pub payload   : Box<[u8]>,
    pub identifier: u32      ,
    pub length    : u32      ,
    pub kind      : Kind     ,
    pub flags     : u8       ,
}

impl Frame {
    pub fn header(conn: &mut super::Connection, flags: u8, identifier: u32, method: &[u8], auth: &[u8], path: &[u8]) -> Frame {
        let encoded = conn.encode(&[
            (b":method"      ,   method      ),
            (b":scheme"      , b"https"      ),
            (b":authority"   , b"discord.com"),
            (b":path"        ,   path        ),
            (b"authorization",   auth        ),
        ]);

        Self::new(Kind::Headers, flags, identifier, encoded.into_boxed_slice())
    }
    
    #[inline]
    pub fn new(kind: Kind, flags: u8, identifier: u32, payload: Box<[u8]>) -> Self {
        let length = payload.len() as u32;

        Self { payload, identifier, length, kind, flags }
    }

    pub fn to_bytes(&self) -> Box<[u8]> {
        use std::mem::MaybeUninit;

        // This allocates a boxed slice for the full frame: 9 bytes for the HTTP/2 frame header + payload length.
        // We use MaybeUninit to avoid unnecessary zero-initialization, since we'll write every byte manually.

        // SAFETY: We'll initialize every byte before use.
        let mut boxed = Box::<[MaybeUninit<u8>]>::new_uninit_slice(9 + self.payload.len());
        let slice = unsafe { boxed.assume_init_mut() };

        // Write the 3-byte length field (big-endian, only lower 24 bits used)
        slice[0] = MaybeUninit::new((self.length >> 16) as u8);
        slice[1] = MaybeUninit::new((self.length >>  8) as u8);
        slice[2] = MaybeUninit::new((self.length      ) as u8);

        // Write the 1-byte type (kind) and 1-byte flags
        slice[3] = MaybeUninit::new(self.kind as u8);
        slice[4] = MaybeUninit::new(self.flags     );

        // Write the 4-byte stream identifier (big-endian, highest bit must be zero)
        // The HTTP/2 spec says the highest bit is reserved and must be ignored on receipt.
        slice[5] = MaybeUninit::new(((self.identifier >> 24) & 0x7F) as u8);
        slice[6] = MaybeUninit::new(( self.identifier >> 16        ) as u8);
        slice[7] = MaybeUninit::new(( self.identifier >>  8        ) as u8);
        slice[8] = MaybeUninit::new(( self.identifier              ) as u8);

        // Copy the payload bytes directly after the header
        for (i, byte) in self.payload.iter().enumerate() {
            slice[9 + i] = MaybeUninit::new(*byte);
        }

        // SAFETY: All bytes have been initialized above, so it's safe to assume_init.
        unsafe { boxed.assume_init().assume_init() }
    }
}