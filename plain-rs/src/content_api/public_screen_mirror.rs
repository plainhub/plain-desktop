//! Public `/graphql` screen-mirror roots.
//!
//! Mirroring lives in the media pipeline, so every field here is a thin
//! projection of what the platform reports: nothing is gated, matching
//! plain-app, which only checks the RECORD_AUDIO runtime grant when a caller
//! asks to route audio.

use super::public_facts::{integer, text};
use crate::content_api::host::Host;
use async_graphql::{Context, Enum, Object, SimpleObject};
use serde_json::Value;
use std::sync::Arc;

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug, Default)]
pub enum ScreenMirrorMode {
    #[default]
    Hd,
    Smooth,
}

impl ScreenMirrorMode {
    fn parse(value: &str) -> Self {
        match value {
            "SMOOTH" => Self::Smooth,
            _ => Self::Hd,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Hd => "HD",
            Self::Smooth => "SMOOTH",
        }
    }
}

#[derive(SimpleObject, Clone, Debug)]
pub struct ScreenMirrorQuality {
    pub mode: ScreenMirrorMode,
    pub resolution: i32,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct ScreenMirrorVideoCodec {
    /// H.264 Annex-B codec configuration (SPS/PPS), base64-encoded.
    #[graphql(name = "annexB")]
    pub annex_b: String,
    /// Latest H.264 keyframe, base64-encoded; null until the pipeline emits one.
    #[graphql(name = "keyFrame")]
    pub key_frame: Option<String>,
}

#[derive(Default)]
pub struct ScreenMirrorQuery;

#[Object]
impl ScreenMirrorQuery {
    async fn is_screen_mirroring(&self, ctx: &Context<'_>) -> async_graphql::Result<bool> {
        Ok(state(ctx).await?["running"].as_bool().unwrap_or(false))
    }

    async fn screen_mirror_control_enabled(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<bool> {
        Ok(state(ctx).await?["controlEnabled"]
            .as_bool()
            .unwrap_or(false))
    }

    /// Null while the pipeline has not published its codec configuration.
    async fn screen_mirror_video_codec(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<Option<ScreenMirrorVideoCodec>> {
        let state = state(ctx).await?;
        let Some(published) = state["codec"].as_object() else {
            return Ok(None);
        };
        let published = Value::Object(published.clone());
        Ok(Some(codec(&published)))
    }

    async fn screen_mirror_quality(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<ScreenMirrorQuality> {
        let stored = ctx
            .data_unchecked::<Arc<Host>>()
            .call("systemScreenMirrorQuality", serde_json::json!({}))
            .await
            .map_err(|error| async_graphql::Error::new(error))?;
        Ok(quality(&stored))
    }
}

#[derive(Default)]
pub struct ScreenMirrorMutation;

#[Object]
impl ScreenMirrorMutation {
    async fn start_screen_mirror(
        &self,
        ctx: &Context<'_>,
        audio: bool,
    ) -> async_graphql::Result<bool> {
        dispatch(
            ctx,
            "systemStartScreenMirror",
            serde_json::json!({ "audio": audio }),
        )
        .await
    }

    async fn stop_screen_mirror(&self, ctx: &Context<'_>) -> async_graphql::Result<bool> {
        dispatch(ctx, "systemStopScreenMirror", serde_json::json!({})).await
    }

    /// Returns false when the device has not granted RECORD_AUDIO yet; the
    /// platform asks for it and the caller retries.
    async fn request_screen_mirror_audio(&self, ctx: &Context<'_>) -> async_graphql::Result<bool> {
        let granted = ctx
            .data_unchecked::<Arc<Host>>()
            .call("systemRequestScreenMirrorAudio", serde_json::json!({}))
            .await
            .map_err(|error| async_graphql::Error::new(error))?;
        Ok(granted.as_bool().unwrap_or(false))
    }

    async fn request_screen_mirror_key_frame(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<bool> {
        dispatch(
            ctx,
            "systemRequestScreenMirrorKeyFrame",
            serde_json::json!({}),
        )
        .await
    }

    async fn update_screen_mirror_quality(
        &self,
        ctx: &Context<'_>,
        mode: ScreenMirrorMode,
    ) -> async_graphql::Result<bool> {
        dispatch(
            ctx,
            "systemUpdateScreenMirrorQuality",
            serde_json::json!({ "mode": mode.name() }),
        )
        .await
    }
}

/// The two settings shortcuts the web console exposes. Both just raise the
/// matching app event, so the platform owns where the screen lands.
#[derive(Default)]
pub struct SettingsMutation;

#[Object]
impl SettingsMutation {
    async fn open_accessibility_settings(&self, ctx: &Context<'_>) -> async_graphql::Result<bool> {
        dispatch(
            ctx,
            "systemOpenAccessibilitySettings",
            serde_json::json!({}),
        )
        .await
    }

    async fn open_web_settings(
        &self,
        ctx: &Context<'_>,
        feature: Option<WebSettingsFeature>,
    ) -> async_graphql::Result<bool> {
        dispatch(
            ctx,
            "systemOpenWebSettings",
            serde_json::json!({
                "feature": feature.map(|feature| feature.as_str()),
            }),
        )
        .await
    }
}

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum WebSettingsFeature {
    Files,
    Contacts,
    Sms,
    CallLogs,
    CallPhone,
    PhoneNumber,
    Apps,
    Notifications,
    Clipboard,
}

impl WebSettingsFeature {
    /// The app event carries this name and opens the matching settings page.
    fn as_str(self) -> &'static str {
        match self {
            Self::Files => "FILES",
            Self::Contacts => "CONTACTS",
            Self::Sms => "SMS",
            Self::CallLogs => "CALL_LOGS",
            Self::CallPhone => "CALL_PHONE",
            Self::PhoneNumber => "PHONE_NUMBER",
            Self::Apps => "APPS",
            Self::Notifications => "NOTIFICATIONS",
            Self::Clipboard => "CLIPBOARD",
        }
    }
}

async fn state(ctx: &Context<'_>) -> async_graphql::Result<Value> {
    ctx.data_unchecked::<Arc<Host>>()
        .call("systemScreenMirrorState", serde_json::json!({}))
        .await
        .map_err(|error| async_graphql::Error::new(error))
}

async fn dispatch(ctx: &Context<'_>, method: &str, params: Value) -> async_graphql::Result<bool> {
    ctx.data_unchecked::<Arc<Host>>()
        .call(method, params)
        .await
        .map_err(|error| async_graphql::Error::new(error))?;
    Ok(true)
}

fn codec(value: &Value) -> ScreenMirrorVideoCodec {
    ScreenMirrorVideoCodec {
        annex_b: text(value, "annexB"),
        key_frame: value["keyFrame"]
            .as_str()
            .filter(|key_frame| !key_frame.is_empty())
            .map(str::to_owned),
    }
}

fn quality(value: &Value) -> ScreenMirrorQuality {
    ScreenMirrorQuality {
        mode: ScreenMirrorMode::parse(&text(value, "mode")),
        resolution: integer(value, "resolution") as i32,
    }
}

#[cfg(test)]
#[path = "../../tests/unit/content_api/public_screen_mirror.rs"]
mod tests;
