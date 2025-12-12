use napi::{bindgen_prelude::{JsValuesTuple, Object}, JsValue};
use napi_sys::*;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::ptr::*;
use std::ffi::*;

use crate::{no_construct, c_str};

#[napi(module_exports)]
pub unsafe fn init(export: Object) {
    let env = export.env();
    let exp = export.raw();
    
    let properties: [napi_property_descriptor; 3] = [
        napi_property_descriptor {
            utf8name: c_str!("warmup"),
            name: null_mut(),
            method: Some(warmup_cb),
            getter: None, setter: None, value: null_mut(),
            attributes: PropertyAttributes::static_,
            data: null_mut(),
        },
        napi_property_descriptor {
            utf8name: c_str!("encode"),
            name: null_mut(),
            method: Some(encode_cb),
            getter: None, setter: None, value: null_mut(),
            attributes: PropertyAttributes::static_,
            data: null_mut(),
        },
        napi_property_descriptor {
            utf8name: c_str!("decode"),
            name: null_mut(),
            method: Some(decode_cb),
            getter: None, setter: None, value: null_mut(),
            attributes: PropertyAttributes::static_,
            data: null_mut(),
        },
    ];

    let mut class = null_mut();
    napi_define_class(env, c_str!("Codec"), 6, Some(no_construct), null_mut(), properties.len(), properties.as_ptr(), &mut class);
    napi_set_named_property(env, exp, c_str!("Codec"), class);
}

// tune this
const POOL_SIZE : usize = 8;
static NEXT_SLOT: AtomicUsize = AtomicUsize::new(0);

// Pool state
static mut POOL_PTR: [*mut c_void; POOL_SIZE] = [null_mut(); POOL_SIZE];
static mut POOL_ARR: [napi_value ; POOL_SIZE] = [null_mut(); POOL_SIZE];
static mut POOL_REF: [napi_ref   ; POOL_SIZE] = [null_mut(); POOL_SIZE];
static mut POOL_LEN: [usize      ; POOL_SIZE] = [0usize; POOL_SIZE];


/// Allocate a buffer into slot `i` with length `len`.
unsafe fn pool_alloc_slot(env: napi_env, i: usize, len: usize) {
    // free previous if exists
    if !POOL_REF[i].is_null() {
        napi_reference_unref(env, POOL_REF[i], null_mut());
        POOL_PTR[i] = null_mut();
        POOL_ARR[i] = null_mut();
        POOL_REF[i] = null_mut();
        POOL_LEN[i] = 0;
    }

    let mut ptr: *mut c_void = null_mut();
    let mut arr: napi_value  = null_mut();

    // create arraybuffer and store handle + pointer
    napi_create_arraybuffer(env, len, &mut ptr, &mut arr);

    // persistent reference so GC won't free it between uses
    let mut ref_: napi_ref = null_mut();
    napi_create_reference(env, arr, 1, &mut ref_);

    POOL_PTR[i] = ptr;
    POOL_ARR[i] = arr;
    POOL_REF[i] = ref_;
    POOL_LEN[i] = len;
}

/// Warm up pool with buffers of size `size`.
/// Intended JS API: codec_warmup(size)
pub unsafe extern "C" fn warmup_cb(env: napi_env, info: napi_callback_info) -> napi_value {
    let mut argc = 1;
    let mut argv = [null_mut(); 1];
    napi_get_cb_info(env, info, &mut argc, argv.as_mut_ptr(), null_mut(), null_mut());

    let mut size = 0;
    napi_get_value_uint32(env, argv[0], &mut size);

    let size = size as usize;
    for i in 0..POOL_SIZE {
        pool_alloc_slot(env, i, size);
    }
    
    null_mut()
}

/// Extremely fast encode. Accepts Uint8Array or string.
/// Returns a Uint8Array which points to a pool slot (round-robin).
pub unsafe extern "C" fn encode_cb(env: napi_env, info: napi_callback_info) -> napi_value {
    let mut argc = 1;
    let mut argv = [null_mut(); 1];
    napi_get_cb_info(env, info, &mut argc, argv.as_mut_ptr(), null_mut(), null_mut());

    // fast zero-copy path for Uint8Array/Buffer input
    let mut is_typedarray: bool = false;
    napi_is_typedarray(env, argv[0], &mut is_typedarray);
    if is_typedarray {
        let mut len = 0;
        napi_get_buffer_info(env, argv[0], null_mut(), &mut len);

        let mut out = null_mut();
        napi_create_typedarray(env, TypedarrayType::uint8_array, len, argv[0], 0, &mut out);

        return out;
    }

    // measure utf8 len
    let mut len: usize = 0;
    napi_get_value_string_utf8(env, argv[0], null_mut(), 0, &mut len);

    // pick next slot (round-robin) and ensure slot capacity
    let slot = NEXT_SLOT.fetch_add(1, Ordering::Relaxed) % POOL_SIZE;
    if POOL_LEN[slot] < len || POOL_ARR[slot].is_null() {
        pool_alloc_slot(env, slot, len);
    }

    // get pointer (we stored it; could call napi_get_arraybuffer_info each time if paranoid)
    let ptr = POOL_PTR[slot];

    // encode directly into pool memory
    let slice = slice_from_raw_parts_mut(ptr as *mut u8, len);
    let mut written: usize = 0;
    napi_get_value_string_utf8(env, argv[0], slice as *mut _, len + 1, &mut written);

    // create typedarray view into this slot's ArrayBuffer (no copy)
    let mut typed = null_mut();
    napi_create_typedarray(env, TypedarrayType::uint8_array, len, POOL_ARR[slot], 0, &mut typed);

    typed
}

pub unsafe extern "C" fn decode_cb(env: napi_env, info: napi_callback_info) -> napi_value {
    let mut argc = 1;
    let mut argv = [null_mut(); 1];
    napi_get_cb_info(env, info, &mut argc, argv.as_mut_ptr(), null_mut(), null_mut());

    let mut ptr = null_mut();
    let mut len = 0;

    napi_get_typedarray_info(env, argv[0], null_mut(), &mut len, &mut ptr, null_mut(), null_mut());

    let mut string = null_mut();
    napi_create_string_utf8(env, ptr.cast(), len as isize, &mut string);
    
    string
}