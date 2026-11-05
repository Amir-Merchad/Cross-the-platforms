//! Does the iPad's pasteboard service take ONE item carrying public.html and the text types together?
//! idevice's `set_text` already sends three text UTIs in one item, so the limit is its helper, not the
//! service. This sends a fixed sample (no real clipboard content) and prints which types come back.
//! Run: `ipad-input htmlset [--usb]`, then paste into Notes (bold + italic?) and a plain field (text?).

use super::*;
use idevice::{
    IdeviceError,
    core_device::{GENERAL_PASTEBOARD, PasteboardServiceClient},
    xpc,
    xpc::Dictionary,
};

const TEXT_UTIS: [&str; 3] = ["public.utf8-plain-text", "public.plain-text", "public.text"];
const SAMPLE_TEXT: &str = "Bridge html spike";
const SAMPLE_HTML: &str = "<b>Bridge</b> html <i>spike</i>";

/// Our own handle on the service: the same XPC channel, but we send the SET ourselves.
struct RawPasteboard(RemoteXpcClient<Io>);

impl RsdService for RawPasteboard {
    fn rsd_service_name() -> std::borrow::Cow<'static, str> { "com.apple.coredevice.pasteboardservice".into() }
    async fn from_stream(stream: Io) -> Result<Self, IdeviceError> {
        let mut inner = RemoteXpcClient::new(stream).await?;
        inner.do_handshake().await?;
        Ok(Self(inner))
    }
}

impl RawPasteboard {
    async fn set_html_and_text(&mut self, html: &str, text: &str) -> Result<(), IdeviceError> {
        let mut data = Dictionary::new();
        let mut types = vec!["public.html"];
        data.insert("public.html".into(), xpc!({ "data": html.as_bytes() }));
        for uti in TEXT_UTIS {
            types.push(uti);
            data.insert(uti.into(), xpc!({ "data": text.as_bytes() }));
        }
        let msg = xpc!({
            "command": "SET",
            "pasteboardName": GENERAL_PASTEBOARD,
            "items": [{ "types": types, "data": XPCObject::Dictionary(data) }],
            "sourceMetadata": XPCObject::Null,
        });
        self.0.send_object(msg, true).await?;
        self.0.recv().await?;
        Ok(())
    }
}

pub async fn htmlset(dir: &Path, usb: bool) -> Result<(), String> {
    let (mut adapter, mut handshake) = if usb { connect_usb().await? } else { connect_wifi(&dir.join(if dir.join(SPIKE_RECORD).exists() { SPIKE_RECORD } else { RECORD })).await? };
    let mut raw = RawPasteboard::connect_rsd(&mut adapter, &mut handshake).await.map_err(|e| format!("pasteboard (set): {e:?}"))?;
    raw.set_html_and_text(SAMPLE_HTML, SAMPLE_TEXT).await.map_err(|e| format!("SET with html and text: {e:?}"))?;
    println!("SET accepted: one item, public.html plus {} text types", TEXT_UTIS.len());
    let mut reader = PasteboardServiceClient::connect_rsd(&mut adapter, &mut handshake).await.map_err(|e| format!("pasteboard (get): {e:?}"))?;
    let snap = reader.get(GENERAL_PASTEBOARD).await.map_err(|e| format!("get: {e:?}"))?;
    for item in &snap.items { println!("item {} types: {:?}", item.index, item.types); }
    println!("now paste into Notes (expect bold and italic) and into a plain text field (expect the sample text)");
    Ok(())
}
