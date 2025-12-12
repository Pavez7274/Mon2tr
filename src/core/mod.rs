pub mod connection;
pub mod response  ;
pub mod stream    ;
pub mod frame     ;

pub use connection::*;
pub use response  ::*;
pub use stream    ::*;
pub use frame     ::*;

pub(crate) static ACKNOWLEDGE_SETTINGS_PAYLOAD: [u8; 9] = [ 0, 0, 0, 4, 0, 0, 0, 0, 0 ];
pub(crate) static ACKNOWLEDGE_SETTINGS_FRAME  : [u8; 9] = [ 0, 0, 0, 4, 1, 0, 0, 0, 0 ];

use std::borrow::Cow;
use std::ptr::null_mut;
use napi_sys::*;

pub struct Header {
    name : Box<[u8]>,
    value: Box<[u8]>,
}

impl From<(Cow<'_, [u8]>, Cow<'_, [u8]>)> for Header {
    fn from((name, value): (Cow<'_, [u8]>, Cow<'_, [u8]>)) -> Self {
        Self { name: Box::from(name), value: Box::from(value) }
    }
}

pub struct Headers(Box<[Header]>);

impl napi::bindgen_prelude::ToNapiValue for &Headers {
    unsafe fn to_napi_value(env: napi_env, hdrs: Self) -> napi::Result<napi_value> {
        let mut object = null_mut();

        // Create an array with the same length as the Vec
        napi_create_array_with_length(env, hdrs.0.len(), &mut object);

        // Populate the array with the headers
        for (i, header) in hdrs.0.iter().enumerate() {
            let mut element = null_mut();
            napi_create_object(env, &mut element);

            let mut name = null_mut();
            napi_create_string_utf8(env, header.name.as_ptr().cast(), header.name.len() as isize, &mut name);
            napi_set_named_property(env, element, b"name\0".as_ptr().cast(), name);

            let mut value = null_mut();
            napi_create_string_utf8(env, header.value.as_ptr().cast(), header.value.len() as isize, &mut value);
            napi_set_named_property(env, element, b"value\0".as_ptr().cast(), value);

            napi_set_element(env, object, i as u32, element);
        }

        Ok(object)
    }
}

impl napi::bindgen_prelude::FromNapiValue for Headers {
    unsafe fn from_napi_value(env: napi_env, value: napi_value) -> napi::Result<Self> {
        let mut length = 0;
        napi_get_array_length(env, value, &mut length);

        // Preallocate the vector with the correct length
        let mut headers = Box::<[Header]>::new_uninit_slice(length as usize);

        for i in 0..length {
            let mut element = null_mut();
            napi_get_element(env, value, i, &mut element);

            let mut name_value = null_mut();
            napi_get_named_property(env, element, b"name\0".as_ptr().cast(), &mut name_value);

            // Get the length of the string value of the "name" property
            let mut name_len = 0;
            napi_get_value_string_utf8(env, name_value, null_mut(), 0, &mut name_len);

            // Allocate a buffer to hold the string value (including null terminator)
            // and then (uhrmm) actually get the string value
            // I prefer Box over Vec here because it makes more sense for fixed-size data
            // and also its faster (see benches/fuck_vec.rs)
            let mut name_buf = Box::new_uninit_slice(name_len + 1);
            napi_get_value_string_utf8(env, name_value, name_buf.as_mut_ptr().cast(), name_len + 1, &mut name_len);

            // Ah, yeah, the classical value_value called variable
            // I really should have named it something else...
            let mut value_value = null_mut();
            napi_get_named_property(env, element, b"value\0".as_ptr().cast(), &mut value_value);

            // Get the length of the string value of the "value" property
             let mut value_len = 0;
            napi_get_value_string_utf8(env, value_value, null_mut(), 0, &mut value_len);

            // Same as above, allocate buffer and get the string value
            let mut value_buf = Box::new_uninit_slice(value_len + 1);
            napi_get_value_string_utf8(env, value_value, value_buf.as_mut_ptr().cast(), value_len + 1, &mut value_len);

            headers[i as usize].as_mut_ptr().write(Header {
                name : name_buf .assume_init(),
                value: value_buf.assume_init(),
            });
        }

        unsafe { Ok(Headers(headers.assume_init())) }
    }
}
