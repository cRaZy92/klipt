//! Display enumeration for `listDisplays`.

use crate::protocol::DisplayInfo;

/// OBS labels monitors as "<name>: <w>x<h> @ <x>,<y>[ (Primary Monitor)]". Keeps "<name>".
pub fn name_from_obs_label(label: &str) -> Option<String> {
    let (name, _) = label.rsplit_once(": ")?;
    let name = name.trim();
    (!name.is_empty()).then(|| name.to_string())
}

/// Windows: every monitor, with the id that libobs' `monitor_capture` matches on
/// (the device interface name, or `\\.\DISPLAYn` when that isn't available).
#[cfg(windows)]
pub fn list() -> Vec<DisplayInfo> {
    use std::mem::{size_of, zeroed};
    use windows_sys::Win32::Foundation::{LPARAM, RECT, TRUE};
    use windows_sys::Win32::Graphics::Gdi::{
        EnumDisplayDevicesW, EnumDisplayMonitors, GetMonitorInfoW, DISPLAY_DEVICEW, HDC, HMONITOR, MONITORINFO,
        MONITORINFOEXW,
    };
    const MONITORINFOF_PRIMARY: u32 = 1;
    const EDD_GET_DEVICE_INTERFACE_NAME: u32 = 1;

    unsafe extern "system" fn collect(monitor: HMONITOR, _dc: HDC, _rect: *mut RECT, data: LPARAM) -> i32 {
        (*(data as *mut Vec<HMONITOR>)).push(monitor);
        TRUE
    }

    fn wide_to_string(w: &[u16]) -> String {
        let end = w.iter().position(|&c| c == 0).unwrap_or(w.len());
        String::from_utf16_lossy(&w[..end])
    }

    let mut monitors: Vec<HMONITOR> = vec![];
    unsafe {
        EnumDisplayMonitors(std::ptr::null_mut(), std::ptr::null(), Some(collect), &mut monitors as *mut _ as LPARAM);
    }

    let obs_labels = crate::obs::list_property_items("monitor_capture", "monitor_id");
    let mut out = vec![];
    for (index, monitor) in monitors.into_iter().enumerate() {
        unsafe {
            let mut info: MONITORINFOEXW = zeroed();
            info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
            if GetMonitorInfoW(monitor, &mut info as *mut MONITORINFOEXW as *mut MONITORINFO) == 0 {
                continue;
            }
            let device_name = wide_to_string(&info.szDevice);
            let mut device: DISPLAY_DEVICEW = zeroed();
            device.cb = size_of::<DISPLAY_DEVICEW>() as u32;
            let id = if EnumDisplayDevicesW(info.szDevice.as_ptr(), 0, &mut device, EDD_GET_DEVICE_INTERFACE_NAME) != 0
            {
                wide_to_string(&device.DeviceID)
            } else {
                device_name.clone()
            };
            let rect = info.monitorInfo.rcMonitor;
            let name = obs_labels
                .iter()
                .find(|(_, value)| *value == id)
                .and_then(|(label, _)| name_from_obs_label(label))
                .unwrap_or_else(|| format!("Display {}", index + 1));
            out.push(DisplayInfo {
                id,
                name,
                width: (rect.right - rect.left).max(0) as u32,
                height: (rect.bottom - rect.top).max(0) as u32,
                primary: info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
            });
        }
    }
    out
}

/// Linux: the screen-cast portal chooses the screen, so there is nothing to list.
#[cfg(not(windows))]
pub fn list() -> Vec<DisplayInfo> {
    vec![]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn obs_labels_are_reduced_to_the_monitor_name() {
        assert_eq!(
            name_from_obs_label("LG ULTRAGEAR: 2560x1440 @ 0,0 (Primary Monitor)").as_deref(),
            Some("LG ULTRAGEAR")
        );
        assert_eq!(name_from_obs_label("Display 2: 1920x1080 @ 2560,0").as_deref(), Some("Display 2"));
        assert_eq!(name_from_obs_label("no separator"), None);
    }
}
