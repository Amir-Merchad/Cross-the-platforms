//! Windows as a BLE HID keyboard+mouse (HOGP server). Ported unchanged from the spike that passed
//! on the iPad and S24+ (docs/spikes/results.md). GATT layout mirrors Universal Bridge's
//! BleHidTransport (MIT). Notifications always target exactly one subscribed host.

use std::sync::{Arc, Mutex, atomic::{AtomicU64, Ordering}};

use bridge_core::hid::{KeyboardReport, MouseReport, REPORT_MAP};
use windows::{
    Devices::Bluetooth::{BluetoothAdapter, BluetoothError, BluetoothLEDevice, GenericAttributeProfile::*},
    Foundation::TypedEventHandler,
    Security::Cryptography::CryptographicBuffer,
    Storage::Streams::IBuffer,
    core::{Error, GUID, HSTRING, Result},
};

const DEVICE_RETRY: std::time::Duration = std::time::Duration::from_secs(10);

fn uuid16(id: u16) -> GUID {
    GUID::from_values(id as u32, 0, 0x1000, [0x80, 0x00, 0x00, 0x80, 0x5f, 0x9b, 0x34, 0xfb])
}

fn buf(bytes: &[u8]) -> Result<IBuffer> { CryptographicBuffer::CreateFromByteArray(bytes) }

fn ensure(e: BluetoothError, what: &str) -> Result<()> {
    if e == BluetoothError::Success { Ok(()) } else { Err(Error::new(windows::core::HRESULT(-1), format!("{what}: {e:?}"))) }
}

pub struct HidPeripheral {
    _provider: GattServiceProvider,
    keyboard: GattLocalCharacteristic,
    mouse: GattLocalCharacteristic,
    /// Every characteristic stays referenced for the provider's lifetime, as Universal Bridge does.
    _keep: Vec<GattLocalCharacteristic>,
    params: GattServiceProviderAdvertisingParameters,
    stats_n: AtomicU64,
    stats_us: AtomicU64,
    stats_max_us: AtomicU64,
    devices: Mutex<Vec<(String, Option<BluetoothLEDevice>, std::time::Instant)>>,
}

impl HidPeripheral {
    pub fn start() -> Result<Self> {
        let adapter = BluetoothAdapter::GetDefaultAsync()?.join()?;
        if !adapter.IsLowEnergySupported()? || !adapter.IsPeripheralRoleSupported()? {
            return Err(Error::new(windows::core::HRESULT(-1), "Bluetooth adapter has no BLE peripheral role"));
        }
        let created = GattServiceProvider::CreateAsync(uuid16(0x1812))?.join()?;
        ensure(created.Error()?, "create HID service 0x1812")?;
        let provider = created.ServiceProvider()?;
        let service = provider.Service()?;

        let make = |uuid: u16, props: GattCharacteristicProperties, value: Option<&[u8]>| -> Result<GattLocalCharacteristic> {
            let p = GattLocalCharacteristicParameters::new()?;
            p.SetCharacteristicProperties(props)?;
            p.SetReadProtectionLevel(GattProtectionLevel::EncryptionRequired)?;
            // Static (read-only) values get read protection only, exactly like Universal Bridge.
            match value {
                Some(v) => p.SetStaticValue(&buf(v)?)?,
                None => p.SetWriteProtectionLevel(GattProtectionLevel::EncryptionRequired)?,
            }
            let r = service.CreateCharacteristicAsync(uuid16(uuid), &p)?.join()?;
            ensure(r.Error()?, "create characteristic")?;
            r.Characteristic()
        };
        let reference = |c: &GattLocalCharacteristic, id: u8, kind: u8| -> Result<()> {
            let p = GattLocalDescriptorParameters::new()?;
            p.SetStaticValue(&buf(&[id, kind])?)?;
            p.SetReadProtectionLevel(GattProtectionLevel::EncryptionRequired)?;
            ensure(c.CreateDescriptorAsync(uuid16(0x2908), &p)?.join()?.Error()?, "report reference")
        };

        // bcdHID 1.11, country 0, NormallyConnectable
        let info = make(0x2A4A, GattCharacteristicProperties::Read, Some(&[0x11, 0x01, 0x00, 0x02]))?;
        let map = make(0x2A4B, GattCharacteristicProperties::Read, Some(REPORT_MAP))?;
        let control = make(0x2A4C, GattCharacteristicProperties::WriteWithoutResponse, None)?;
        control.WriteRequested(&TypedEventHandler::new(|_, args: windows::core::Ref<GattWriteRequestedEventArgs>| {
            if let Some(a) = args.as_ref() { respond_write(a, Some("control point")); }
            Ok(())
        }))?;

        let keyboard = make(0x2A4D, GattCharacteristicProperties::Read | GattCharacteristicProperties::Notify, None)?;
        log_subscriptions(&keyboard, "keyboard")?;
        reference(&keyboard, 1, 1)?;
        respond_reads(&keyboard, vec![0; 8])?;

        let leds = make(0x2A4D, GattCharacteristicProperties::Read | GattCharacteristicProperties::Write | GattCharacteristicProperties::WriteWithoutResponse, None)?;
        reference(&leds, 1, 2)?;
        respond_reads(&leds, vec![0])?;
        leds.WriteRequested(&TypedEventHandler::new(|_, args: windows::core::Ref<GattWriteRequestedEventArgs>| {
            if let Some(a) = args.as_ref() { respond_write(a, Some("LED report")); }
            Ok(())
        }))?;

        let mouse = make(0x2A4D, GattCharacteristicProperties::Read | GattCharacteristicProperties::Notify, None)?;
        log_subscriptions(&mouse, "mouse")?;
        reference(&mouse, 2, 1)?;
        respond_reads(&mouse, vec![0; 7])?;

        let ap = GattServiceProviderAdvertisingParameters::new()?;
        ap.SetIsConnectable(true)?;
        ap.SetIsDiscoverable(true)?;
        provider.StartAdvertisingWithParameters(&ap)?;
        // Wait up to 12 s for Started, like Universal Bridge. `Aborted` is transient on some adapters
        // (MediaTek MT7921 reports it first, then starts), so it is not treated as failure.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(12);
        loop {
            let status = provider.AdvertisementStatus()?;
            if status == GattServiceProviderAdvertisementStatus::Started { break }
            if std::time::Instant::now() > deadline {
                // Never leave a half-published HID service behind (it can wedge the adapter).
                let _ = provider.StopAdvertising();
                return Err(Error::new(windows::core::HRESULT(-1), format!("advertising not started after 12 s: {status:?}")));
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        eprintln!("advertising: Started");
        Ok(Self { _provider: provider, keyboard, mouse, _keep: vec![info, map, control, leds], params: ap, stats_n: AtomicU64::new(0), stats_us: AtomicU64::new(0), stats_max_us: AtomicU64::new(0), devices: Mutex::new(Vec::new()) })
    }

    /// True while advertising. After Bluetooth off/on or sleep the provider stops; this restarts it.
    pub fn ensure_advertising(&self, any_connected: bool) -> bool {
        // Some adapters stop advertising once a host is connected; restarting it then disturbs the link.
        if any_connected { return true }
        match self._provider.AdvertisementStatus() {
            Ok(GattServiceProviderAdvertisementStatus::Started | GattServiceProviderAdvertisementStatus::StartedWithoutAllAdvertisementData) => true,
            Ok(GattServiceProviderAdvertisementStatus::Created) => false, // still starting
            _ => { let _ = self._provider.StartAdvertisingWithParameters(&self.params); false }
        }
    }

    /// True once this host has subscribed to keyboard reports (it may do so seconds after the mouse).
    pub fn keyboard_ready(&self, host: &str) -> bool {
        let target = HSTRING::from(host);
        self.keyboard.SubscribedClients().is_ok_and(|v| v.into_iter().any(|c| {
            c.Session().and_then(|s| s.DeviceId()).and_then(|d| d.Id()).is_ok_and(|id| id == target)
        }))
    }

    /// Device ids of hosts subscribed to both keyboard and mouse reports.
    pub fn hosts(&self) -> Vec<String> {
        let ids = |c: &GattLocalCharacteristic| -> Vec<String> {
            c.SubscribedClients().map(|v| v.into_iter().filter_map(|s| Some(s.Session().ok()?.DeviceId().ok()?.Id().ok()?.to_string())).collect()).unwrap_or_default()
        };
        let kb = ids(&self.keyboard);
        let mut both: Vec<String> = ids(&self.mouse).into_iter().filter(|id| kb.contains(id)).collect();
        both.dedup();
        both
    }

    /// Pacing for motion reports: one per BLE connection event, like Universal Bridge (8–100 ms).
    pub fn report_interval_ms(&self, host: &str) -> u64 {
        let interval = self.device(host).ok_or(()).and_then(|d| d.GetConnectionParameters().and_then(|c| c.ConnectionInterval()).map_err(|_| ()));
        match interval { Ok(units) => ((units as f64 * 1.25).ceil() as u64).clamp(8, 100), Err(()) => 8 }
    }

    /// True while the host's link is up. Unlike `hosts()` (subscriptions linger after the host
    /// turns Bluetooth off), the connection status flips as soon as the link drops.
    pub fn connected(&self, host: &str) -> bool {
        self.device(host).is_some_and(|d| {
            d.ConnectionStatus().is_ok_and(|s| s == windows::Devices::Bluetooth::BluetoothConnectionStatus::Connected)
        })
    }

    /// One cached `BluetoothLEDevice` per host. Creating them is a blocking round trip to the Bluetooth
    /// stack, so never in the input loop: a failed lookup (device away/unpaired) is retried every 10 s.
    fn device(&self, host: &str) -> Option<BluetoothLEDevice> {
        let mut cache = self.devices.lock().unwrap();
        match cache.iter().position(|(id, _, _)| id == host) {
            Some(i) if cache[i].1.is_some() || cache[i].2.elapsed() < DEVICE_RETRY => return cache[i].1.clone(),
            Some(i) => { cache.remove(i); }
            None => {}
        }
        let d = BluetoothLEDevice::FromIdAsync(&HSTRING::from(host)).and_then(|op| op.join()).ok();
        cache.push((host.to_string(), d.clone(), std::time::Instant::now()));
        d
    }

    /// The host's Bluetooth name (e.g. "Amir's iPad"), if Windows knows it.
    pub fn host_name(&self, host: &str) -> Option<String> {
        self.device(host).and_then(|d| d.Name().ok()).map(|n| n.to_string()).filter(|n| !n.is_empty())
    }

    /// (reports sent, average ms, max ms) since the last call — how long each BLE notify blocked.
    pub fn take_stats(&self) -> (u64, f64, f64) {
        let n = self.stats_n.swap(0, Ordering::Relaxed);
        let total = self.stats_us.swap(0, Ordering::Relaxed);
        let max = self.stats_max_us.swap(0, Ordering::Relaxed);
        (n, if n == 0 { 0.0 } else { total as f64 / n as f64 / 1000.0 }, max as f64 / 1000.0)
    }

    fn notify(&self, c: &GattLocalCharacteristic, host: &str, bytes: &[u8]) -> bool {
        let t0 = std::time::Instant::now();
        let ok = self.notify_inner(c, host, bytes);
        let us = t0.elapsed().as_micros() as u64;
        let kind = if c == &self.keyboard { "keyboard" } else { "mouse" };
        if !ok { crate::diag::log(format_args!("{kind} send FAILED to {host} ({} ms)", us / 1000)); }
        else if kind == "keyboard" { crate::diag::log(format_args!("keyboard send ok to {host} ({} ms)", us / 1000)); }
        else if us > 30_000 { crate::diag::log(format_args!("mouse send slow to {host}: {} ms", us / 1000)); }
        self.stats_n.fetch_add(1, Ordering::Relaxed);
        self.stats_us.fetch_add(us, Ordering::Relaxed);
        self.stats_max_us.fetch_max(us, Ordering::Relaxed);
        ok
    }

    fn notify_inner(&self, c: &GattLocalCharacteristic, host: &str, bytes: &[u8]) -> bool {
        let Ok(clients) = c.SubscribedClients() else { return false };
        let target = HSTRING::from(host);
        for client in clients {
            let id = client.Session().and_then(|s| s.DeviceId()).and_then(|d| d.Id());
            if id.as_ref() == Ok(&target) {
                return buf(bytes)
                    .and_then(|b| c.NotifyValueForSubscribedClientAsync(&b, &client))
                    .and_then(|op| op.join())
                    .and_then(|r| r.Status())
                    .is_ok_and(|s| s == windows::Devices::Bluetooth::GenericAttributeProfile::GattCommunicationStatus::Success);
            }
        }
        false
    }

    pub fn send_mouse(&self, host: &str, r: MouseReport) -> bool { self.notify(&self.mouse, host, &r.to_bytes()) }
    pub fn send_keyboard(&self, host: &str, r: KeyboardReport) -> bool { self.notify(&self.keyboard, host, &r.to_bytes()) }
}

impl Drop for HidPeripheral {
    // Leaving a published service behind can wedge some Bluetooth drivers (MediaTek MT7921 did).
    fn drop(&mut self) { let _ = self._provider.StopAdvertising(); }
}

fn respond_reads(c: &GattLocalCharacteristic, value: Vec<u8>) -> Result<()> {
    let value = Arc::new(Mutex::new(value));
    c.ReadRequested(&TypedEventHandler::new(move |_, args: windows::core::Ref<GattReadRequestedEventArgs>| {
        if let Some(a) = args.as_ref() {
            let _d = a.GetDeferral()?;
            let req = a.GetRequestAsync()?.join()?;
            let v = value.lock().unwrap().clone();
            req.RespondWithValue(&buf(&v)?)?;
        }
        Ok(())
    }))?;
    Ok(())
}

fn log_subscriptions(c: &GattLocalCharacteristic, kind: &'static str) -> Result<()> {
    c.SubscribedClientsChanged(&TypedEventHandler::new(move |c: windows::core::Ref<GattLocalCharacteristic>, _| {
        if let Some(c) = c.as_ref() {
            let ids: Vec<String> = c.SubscribedClients().map(|v| v.into_iter().filter_map(|s| s.Session().ok()?.DeviceId().ok()?.Id().ok().map(|i| i.to_string())).collect()).unwrap_or_default();
            crate::diag::log(format_args!("{kind} subscribers now {ids:?}"));
        }
        Ok(())
    }))?;
    Ok(())
}

fn respond_write(a: &GattWriteRequestedEventArgs, what: Option<&str>) {
    let _ = (|| -> Result<()> {
        let _d = a.GetDeferral()?;
        let req = a.GetRequestAsync()?.join()?;
        if let Some(what) = what {
            let mut bytes = windows::core::Array::<u8>::new();
            CryptographicBuffer::CopyToByteArray(&req.Value()?, &mut bytes)?;
            let host = a.Session()?.DeviceId()?.Id()?;
            crate::diag::log(format_args!("{what} write from {host}: {:?}", &bytes[..]));
        }
        if req.Option()? == GattWriteOption::WriteWithResponse { req.Respond()?; }
        Ok(())
    })();
}
