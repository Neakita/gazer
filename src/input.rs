use evdev::{Device, EventType, KeyCode};
use snafu::{OptionExt, ResultExt, Snafu};
use std::{io, path::Path};
use tracing::{debug, info, trace};

pub(super) fn capture<P, F>(path: Option<P>, key: KeyCode, mut callback: F) -> Result<!, Error>
where
    P: AsRef<Path>,
    F: FnMut(bool),
{
    let mut device = if let Some(path) = path {
        Device::open(path).context(IoSnafu)?
    } else {
        find_device(key).context(DeviceNotFoundSnafu)?
    };
    loop {
        for event in device.fetch_events().context(IoSnafu)? {
            trace!(type = ?event.event_type(), "received event");
            if event.event_type() != EventType::KEY {
                continue;
            }
            if KeyCode::new(event.code()) != key {
                continue;
            }
            let pressed = event.value() != 0;
            debug!(?key, pressed, "key code event");
            callback(pressed);
        }
    }
}

fn find_device(key: KeyCode) -> Option<Device> {
    evdev::enumerate()
        .find(|(_, device)| supports_key(device, key))
        .inspect(|(path, device)| info!(?path, name = device.name(), "found device"))
        .map(|(_, device)| device)
}

fn supports_key(device: &Device, key: KeyCode) -> bool {
    device
        .supported_keys()
        .is_some_and(|keys| keys.contains(key))
}

#[derive(Debug, Snafu)]
pub(super) enum Error {
    Io { source: io::Error },
    DeviceNotFound,
}
