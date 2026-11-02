//! Windows as a BLE HID keyboard+mouse (HOGP server). GATT layout mirrors Universal Bridge's
//! BleHidTransport (MIT). Notifications always target exactly one subscribed host.

use std::sync::{Arc, Mutex};

use bridge_core::hid::{KeyboardReport, MouseReport, REPORT_MAP};
use windows::{
    Devices::Bluetooth::{BluetoothAdapter, BluetoothError, GenericAttributeProfile::*},
    Foundation::TypedEventHandler,
    Security::Cryptography::CryptographicBuffer,
    Storage::Streams::IBuffer,
    core::{Error, GUID, HSTRING, Result},
};

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
            p.SetWriteProtectionLevel(GattProtectionLevel::EncryptionRequired)?;
            if let Some(v) = value { p.SetStaticValue(&buf(v)?)?; }
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
        make(0x2A4A, GattCharacteristicProperties::Read, Some(&[0x11, 0x01, 0x00, 0x02]))?;
        make(0x2A4B, GattCharacteristicProperties::Read, Some(REPORT_MAP))?;
        let control = make(0x2A4C, GattCharacteristicProperties::WriteWithoutResponse, None)?;
        control.WriteRequested(&TypedEventHandler::new(|_, args: windows::core::Ref<GattWriteRequestedEventArgs>| {
            if let Some(a) = args.as_ref() { respond_write(a); }
            Ok(())
        }))?;

        let keyboard = make(0x2A4D, GattCharacteristicProperties::Read | GattCharacteristicProperties::Notify, None)?;
        reference(&keyboard, 1, 1)?;
        respond_reads(&keyboard, vec![0; 8])?;

        let leds = make(0x2A4D, GattCharacteristicProperties::Read | GattCharacteristicProperties::Write | GattCharacteristicProperties::WriteWithoutResponse, None)?;
        reference(&leds, 1, 2)?;
        respond_reads(&leds, vec![0])?;
        leds.WriteRequested(&TypedEventHandler::new(|_, args: windows::core::Ref<GattWriteRequestedEventArgs>| {
            if let Some(a) = args.as_ref() { respond_write(a); }
            Ok(())
        }))?;

        let mouse = make(0x2A4D, GattCharacteristicProperties::Read | GattCharacteristicProperties::Notify, None)?;
        reference(&mouse, 2, 1)?;
        respond_reads(&mouse, vec![0; 7])?;

        let ap = GattServiceProviderAdvertisingParameters::new()?;
        ap.SetIsConnectable(true)?;
        ap.SetIsDiscoverable(true)?;
        provider.StartAdvertisingWithParameters(&ap)?;
        Ok(Self { _provider: provider, keyboard, mouse })
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

    fn notify(&self, c: &GattLocalCharacteristic, host: &str, bytes: &[u8]) -> bool {
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

fn respond_write(a: &GattWriteRequestedEventArgs) {
    let _ = (|| -> Result<()> {
        let _d = a.GetDeferral()?;
        let req = a.GetRequestAsync()?.join()?;
        if req.Option()? == GattWriteOption::WriteWithResponse { req.Respond()?; }
        Ok(())
    })();
}
