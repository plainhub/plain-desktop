use crate::discovery::advertisement;
use crate::discovery::nearby_wire;
use crate::prefs;
use crate::prefs::Prefs;
use crate::utils;
use serde::Deserialize;
use std::ffi::CStr;
use std::ffi::c_char;
use std::ffi::CString;
use std::sync::Arc;

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "camelCase", deny_unknown_fields)]
enum Request {
    Reply,
    Ble,
    Mdns,
    BleDecode { payload: Option<Vec<u8>> },
    BleShortId { id: String },
}

fn c_string(value: String) -> *mut c_char {
    CString::new(value).expect("string contains NUL").into_raw()
}

fn input(pointer: *const c_char) -> Result<String, String> {
    if pointer.is_null() {
        return Err("null string passed to discovery FFI".to_string());
    }
    // SAFETY: callers pass a null-terminated string valid for this call.
    Ok(unsafe { CStr::from_ptr(pointer) }
        .to_str()
        .map_err(|e| e.to_string())?
        .to_owned())
}

fn facts_from_kotlin(facts_json: &str) -> Result<advertisement::Facts, String> {
    serde_json::from_str(facts_json).map_err(|e| format!("Invalid facts JSON: {e}"))
}

fn current_prefs() -> Result<Arc<Prefs>, String> {
    prefs::try_get_default().ok_or_else(|| "Rust preferences are not initialized".to_string())
}

fn client_id() -> Result<String, String> {
    let prefs = current_prefs()?;
    prefs
        .get::<String>(prefs::system::keys::CLIENT_ID)
        .map_err(|e| e.to_string())?
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "Missing client identity".to_string())
}

fn user_device_name() -> Result<String, String> {
    let prefs = current_prefs()?;
    prefs::user::snapshot(&prefs)
        .map(|s| s.device_name)
        .map_err(|e| e.to_string())
}

fn user_https_port() -> Result<u16, String> {
    let prefs = current_prefs()?;
    prefs::user::snapshot(&prefs)
        .map(|s| s.https_port as u16)
        .map_err(|e| e.to_string())
}

fn mdns_hostname() -> Result<String, String> {
    let prefs = current_prefs()?;
    Ok(prefs::ensure_mdns_hostname(&prefs))
}

fn execute(facts_json: &str, request: Request) -> Result<serde_json::Value, String> {
    if matches!(request, Request::BleDecode { .. } | Request::BleShortId { .. }) {
        return match request {
            Request::BleDecode { payload } => {
                let bytes = payload.unwrap_or_default();
                if bytes.len() > 1650 {
                    return Err("BLE payload exceeds advertising limit".to_string());
                }
                serde_json::to_value(advertisement::decode_ble(&bytes))
                    .map_err(|e| e.to_string())
            }
            Request::BleShortId { id } => {
                if id.is_empty() || id.len() > 1024 {
                    return Err("Invalid peer identity".to_string());
                }
                Ok(serde_json::Value::String(nearby_wire::short_id(&id)))
            }
            _ => unreachable!(),
        };
    }
    let facts = facts_from_kotlin(facts_json)?;
    let id = client_id()?;
    match request {
        Request::Reply => {
            let name = user_device_name()?;
            let port = user_https_port()?;
            let reply = advertisement::reply(&id, &name, port, facts)
                .map_err(|e| format!("{e}"))?;
            Ok(serde_json::to_value(reply).map_err(|e| e.to_string())?)
        }
        Request::Ble => {
            let bytes = advertisement::ble(&id, facts.aware_supported, facts.aware_running)
                .map_err(|e| format!("{e}"))?;
            Ok(serde_json::Value::String(utils::base64::base64_encode(&bytes)))
        }
        Request::Mdns => {
            let name = user_device_name()?;
            let port = user_https_port()?;
            let reply = advertisement::reply(&id, &name, port, facts)
                .map_err(|e| format!("{e}"))?;
            let info = advertisement::mdns(&reply, &mdns_hostname()?)
                .map_err(|e| format!("{e}"))?;
            Ok(serde_json::to_value(info).map_err(|e| e.to_string())?)
        }
        _ => unreachable!(),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn plain_discovery_action(
    facts_json: *const c_char,
    request_json: *const c_char,
) -> *mut c_char {
    c_string(match input(facts_json).and_then(|facts| {
        input(request_json).and_then(|request| {
            serde_json::from_str::<Request>(&request)
                .map_err(|e| format!("Invalid request JSON: {e}"))
                .and_then(|r| execute(&facts, r))
        })
    }) {
        Ok(value) => value.to_string(),
        Err(error) => format!("ERROR:{error}"),
    })
}

#[cfg(target_os = "android")]
mod android {
    use super::*;
    use jni::EnvUnowned;
    use jni::errors::ThrowRuntimeExAndDefault;
    use jni::objects::{JObject, JString};
    use jni::sys::jstring;

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_com_ismartcoding_plain_rust_bridge_RustDiscoveryBridge_actionNative(
        mut env: EnvUnowned<'_>,
        _: JObject<'_>,
        facts_json: JString<'_>,
        request_json: JString<'_>,
    ) -> jstring {
        env.with_env(|env| -> jni::errors::Result<_> {
            let facts = facts_json.to_string();
            let request = request_json.to_string();
            let result = input(facts.as_ptr() as *const c_char)
                .and_then(|f| {
                    input(request.as_ptr() as *const c_char).and_then(|r| {
                        serde_json::from_str::<Request>(&r)
                            .map_err(|e| format!("Invalid request JSON: {e}"))
                            .and_then(|req| execute(&f, req))
                    })
                })
                .unwrap_or_else(|error| format!("ERROR:{error}"));
            env.new_string(result)
        })
        .resolve::<ThrowRuntimeExAndDefault>()
        .into_raw()
    }
}