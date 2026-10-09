use super::{self as ble_wire, Assembler};
use std::{
    collections::HashMap,
    sync::{
        LazyLock, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};
static HANDLES: LazyLock<Mutex<HashMap<u64, Assembler>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
struct Encoder {
    data: Vec<u8>,
    id: u32,
    response: bool,
    sequence: u32,
    limit: usize,
}
static ENCODERS: LazyLock<Mutex<HashMap<u64, Encoder>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static NEXT: AtomicU64 = AtomicU64::new(1);
#[unsafe(no_mangle)]
pub extern "C" fn plain_ble_new() -> u64 {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    HANDLES.lock().unwrap().insert(id, Assembler::default());
    id
}
#[unsafe(no_mangle)]
pub extern "C" fn plain_ble_free(id: u64) {
    HANDLES.lock().unwrap().remove(&id);
    ENCODERS.lock().unwrap().remove(&id);
}
fn encoder(id: u32, response: bool, limit: u32, data: &[u8]) -> Result<u64, String> {
    ble_wire::frame(data, id, response, 0, limit as usize).map_err(|e| e.to_string())?;
    let mut encoders = ENCODERS.lock().unwrap();
    if encoders.len() >= 32 {
        return Err("BLE encoder capacity exceeded".into());
    }
    let handle = NEXT.fetch_add(1, Ordering::Relaxed);
    encoders.insert(
        handle,
        Encoder {
            data: data.to_vec(),
            id,
            response,
            sequence: 0,
            limit: limit as usize,
        },
    );
    Ok(handle)
}
/// # Safety
/// `data` must reference `len` readable bytes for the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn plain_ble_encoder(
    id: u32,
    response: bool,
    limit: u32,
    data: *const u8,
    len: usize,
) -> u64 {
    if data.is_null() || len > ble_wire::MAX_MESSAGE {
        return 0;
    }
    encoder(id, response, limit, unsafe {
        std::slice::from_raw_parts(data, len)
    })
    .unwrap_or(0)
}
#[unsafe(no_mangle)]
pub extern "C" fn plain_ble_info(id: u64) -> u64 {
    HANDLES
        .lock()
        .unwrap()
        .get(&id)
        .map(Assembler::info)
        .unwrap_or(0)
}
fn call(
    action: i32,
    handle: u64,
    id: u32,
    sequence: u32,
    limit: u32,
    response: bool,
    data: &[u8],
) -> Result<Option<Vec<u8>>, String> {
    let result = match action {
        0 => ble_wire::message(&[], data).map(Some),
        1 => ble_wire::nearby_body(data).map(|b| Some(b.to_vec())),
        2 => ble_wire::frame(data, id, response, sequence, limit as usize),
        3 => {
            return HANDLES
                .lock()
                .unwrap()
                .get_mut(&handle)
                .ok_or("BLE assembler released".to_string())?
                .push(data)
                .map_err(|e| e.to_string());
        }
        4 => {
            let mut encoders = ENCODERS.lock().unwrap();
            let e = encoders
                .get_mut(&handle)
                .ok_or("BLE encoder released".to_string())?;
            let frame = ble_wire::frame(&e.data, e.id, e.response, e.sequence, e.limit)
                .map_err(|e| e.to_string())?;
            if frame.is_some() {
                e.sequence = e.sequence.checked_add(1).ok_or("BLE sequence overflow")?;
            }
            return Ok(frame);
        }
        5 => {
            let args: AuthArgs = serde_json::from_slice(data).map_err(|e| e.to_string())?;
            ble_wire::GatewayRequest::Auth {
                client_id: args.client_id,
                client_name: args.client_name,
                password: crate::crypto::sha512_hex(&args.password),
            }
            .encode()
            .map(Some)
        }
        6 => {
            let args: GraphqlArgs = serde_json::from_slice(data).map_err(|e| e.to_string())?;
            let body = crate::crypto::xchacha_encrypt(&args.token, args.body.as_bytes())
                .ok_or("Invalid BLE encryption key")?;
            ble_wire::GatewayRequest::Graphql {
                client_id: args.client_id,
                body,
            }
            .encode()
            .map(Some)
        }
        7 => ble_wire::decode_response(data).and_then(|(status, body)| {
            anyhow::ensure!(status == 200, "BLE HTTP status {status}");
            Ok(Some(body.to_vec()))
        }),
        8 => {
            if data.len() < 32 {
                return Err("Invalid BLE encryption key".into());
            }
            let (status, body) =
                ble_wire::decode_response(&data[32..]).map_err(|e| e.to_string())?;
            if status != 200 {
                return Err(format!("BLE HTTP status {status}"));
            }
            return crate::crypto::xchacha_decrypt_raw(&data[..32], body)
                .map(Some)
                .ok_or("Invalid BLE encrypted response".into());
        }
        _ => return Err("Invalid BLE codec action".into()),
    };
    result.map_err(|e| e.to_string())
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AuthArgs {
    client_id: String,
    client_name: String,
    password: String,
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GraphqlArgs {
    client_id: String,
    token: String,
    body: String,
}
#[repr(C)]
pub struct Buffer {
    data: *mut u8,
    len: usize,
    status: i32,
}
/// # Safety
/// A non-null `data` must reference `len` readable bytes for the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn plain_ble_call(
    action: i32,
    handle: u64,
    id: u32,
    sequence: u32,
    limit: u32,
    response: bool,
    data: *const u8,
    len: usize,
) -> Buffer {
    let result = if len
        > if matches!(action, 5 | 6) {
            ble_wire::MAX_MESSAGE * 6 + 1024
        } else {
            ble_wire::MAX_MESSAGE + 32
        }
        || (data.is_null() && len != 0)
    {
        Err("Invalid BLE input".into())
    } else {
        call(
            action,
            handle,
            id,
            sequence,
            limit,
            response,
            if len == 0 {
                &[]
            } else {
                unsafe { std::slice::from_raw_parts(data, len) }
            },
        )
    };
    let (value, status) = match result {
        Ok(Some(v)) => (v, 1),
        Ok(None) => {
            return Buffer {
                data: std::ptr::null_mut(),
                len: 0,
                status: 0,
            };
        }
        Err(e) => (e.into_bytes(), -1),
    };
    let mut value = value.into_boxed_slice();
    let out = Buffer {
        data: value.as_mut_ptr(),
        len: value.len(),
        status,
    };
    std::mem::forget(value);
    out
}
/// # Safety
/// `buffer` must be an unmodified, unfreed result of `plain_ble_call`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn plain_ble_buffer_free(buffer: Buffer) {
    if !buffer.data.is_null() {
        drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(buffer.data, buffer.len)) });
    }
}
#[cfg(target_os = "android")]
mod android {
    use super::*;
    use jni::{
        EnvUnowned,
        errors::ThrowRuntimeExAndDefault,
        objects::{JByteArray, JObject},
        sys::{jboolean, jbyteArray, jint, jlong},
    };
    #[unsafe(no_mangle)]
    pub extern "system" fn Java_com_ismartcoding_plain_ble_RustBleBridge_encoderNative(
        mut env: EnvUnowned<'_>,
        _: JObject<'_>,
        data: JByteArray<'_>,
        id: jint,
        response: jboolean,
        limit: jint,
    ) -> jlong {
        env.with_env(|env| -> jni::errors::Result<jlong> {
            let bytes = env.convert_byte_array(&data)?;
            match encoder(id as u32, response, limit as u32, &bytes) {
                Ok(handle) => Ok(handle as jlong),
                Err(e) => {
                    env.throw_new(
                        jni::jni_str!("java/lang/IllegalStateException"),
                        jni::strings::JNIString::from(e),
                    )?;
                    Ok(0)
                }
            }
        })
        .resolve::<ThrowRuntimeExAndDefault>()
    }
    #[unsafe(no_mangle)]
    pub extern "system" fn Java_com_ismartcoding_plain_ble_RustBleBridge_newNative(
        _: EnvUnowned<'_>,
        _: JObject<'_>,
    ) -> jlong {
        plain_ble_new() as jlong
    }
    #[unsafe(no_mangle)]
    pub extern "system" fn Java_com_ismartcoding_plain_ble_RustBleBridge_freeNative(
        _: EnvUnowned<'_>,
        _: JObject<'_>,
        handle: jlong,
    ) {
        plain_ble_free(handle as u64);
    }
    #[unsafe(no_mangle)]
    pub extern "system" fn Java_com_ismartcoding_plain_ble_RustBleBridge_infoNative(
        _: EnvUnowned<'_>,
        _: JObject<'_>,
        handle: jlong,
    ) -> jlong {
        plain_ble_info(handle as u64) as jlong
    }
    #[unsafe(no_mangle)]
    pub extern "system" fn Java_com_ismartcoding_plain_ble_RustBleBridge_callNative(
        mut env: EnvUnowned<'_>,
        _: JObject<'_>,
        action: jint,
        handle: jlong,
        id: jint,
        sequence: jint,
        limit: jint,
        response: jboolean,
        data: JByteArray<'_>,
    ) -> jbyteArray {
        env.with_env(|env| -> jni::errors::Result<_> {
            let data = env.convert_byte_array(&data)?;
            match call(
                action,
                handle as u64,
                id as u32,
                sequence as u32,
                limit as u32,
                response,
                &data,
            ) {
                Ok(Some(bytes)) => env.byte_array_from_slice(&bytes),
                Ok(None) => Ok(JByteArray::null()),
                Err(error) => {
                    env.throw_new(
                        jni::jni_str!("java/lang/IllegalStateException"),
                        jni::strings::JNIString::from(error),
                    )?;
                    Ok(JByteArray::null())
                }
            }
        })
        .resolve::<ThrowRuntimeExAndDefault>()
        .into_raw()
    }
}

#[cfg(test)]
#[path = "../../tests/unit/ble/ffi.rs"]
mod tests;
