//! The "Copy link" button of a finished-upload notification on Windows.
//!
//! Windows tells an app about a toast button click only while the toast is on screen. Once it times
//! out into the notification centre, an ordinary button there does nothing (Klipt has no COM
//! activator). So on Windows the button opens `klipt://copy-link?url=<share link>`, a scheme Klipt
//! registers for the current user at start-up: Windows starts Klipt with that argument, a running
//! Klipt gets it through the single-instance plugin, and the link goes on the clipboard. It works
//! for as long as the toast is in the notification centre, even after Klipt was quit.
//!
//! Linux needs none of this: Plasma drops the buttons of a notification once it expires.

use tauri::{AppHandle, Manager, Url};

pub const SCHEME: &str = "klipt";
const COPY_HOST: &str = "copy-link";

/// `klipt://copy-link?url=<link>`, the link percent-encoded.
pub fn copy_uri(link: &str) -> String {
    let mut uri = Url::parse(&format!("{SCHEME}://{COPY_HOST}")).expect("a valid base URI");
    uri.query_pairs_mut().append_pair("url", link);
    uri.into()
}

/// The share link a `klipt://copy-link` argument asks to copy. Only a web link on the server the
/// app is logged in to counts, so another program opening a `klipt://` link can't put just any
/// text on the clipboard.
pub fn link_from_args<I, S>(args: I, server_url: Option<&str>) -> Option<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let server = Url::parse(server_url?).ok()?;
    args.into_iter().find_map(|arg| {
        let uri = Url::parse(arg.as_ref()).ok()?;
        if uri.scheme() != SCHEME || uri.host_str() != Some(COPY_HOST) {
            return None;
        }
        let (_, link) = uri.query_pairs().find(|(key, _)| key == "url")?;
        let parsed = Url::parse(&link).ok()?;
        let web = matches!(parsed.scheme(), "http" | "https");
        (web && parsed.origin() == server.origin()).then(|| link.into_owned())
    })
}

/// Copies the link a `klipt://copy-link` argument carries, if there is one, and says so.
/// Returns whether the arguments were such a request.
pub fn handle_args(app: &AppHandle, args: &[String]) -> bool {
    if !args.iter().any(|arg| arg.starts_with(&format!("{SCHEME}:"))) {
        return false;
    }
    let server = app
        .try_state::<crate::state::AppState>()
        .and_then(|state| crate::upload::auth::load(&state.data_dir))
        .map(|auth| auth.server_url);
    match link_from_args(args, server.as_deref()) {
        Some(link) => match crate::commands::copy_text(app, &link) {
            Ok(()) => crate::feedback::notify(app, "Link copied", None),
            Err(e) => crate::elog!("klipt: can't copy the share link: {e}"),
        },
        None => crate::elog!("klipt: ignored a klipt:// link that isn't a share link on this server"),
    }
    true
}

/// Text for toast XML.
pub fn xml_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

/// The toast: title, body and a "Copy link" button that opens [`copy_uri`].
pub fn toast_xml(title: &str, body: &str, link: &str) -> String {
    format!(
        "<toast><visual><binding template=\"ToastGeneric\"><text>{}</text><text>{}</text></binding></visual>\
         <actions><action content=\"Copy link\" activationType=\"protocol\" arguments=\"{}\"/></actions></toast>",
        xml_escape(title),
        xml_escape(body),
        xml_escape(&copy_uri(link)),
    )
}

#[cfg(windows)]
mod platform {
    use windows::core::{w, HSTRING, PCWSTR};
    use windows::Data::Xml::Dom::XmlDocument;
    use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};
    use windows::Win32::System::Registry::{RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ};

    /// Points `klipt://` at this Klipt.exe for the current user; every start, so the path follows
    /// the install.
    pub fn register_scheme() -> Result<(), String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let command = format!("\"{}\" \"%1\"", exe.display());
        set(w!("Software\\Classes\\klipt"), PCWSTR::null(), "URL:Klipt")?;
        set(w!("Software\\Classes\\klipt"), w!("URL Protocol"), "")?;
        set(w!("Software\\Classes\\klipt\\shell\\open\\command"), PCWSTR::null(), &command)
    }

    fn set(key: PCWSTR, value: PCWSTR, data: &str) -> Result<(), String> {
        let wide: Vec<u16> = data.encode_utf16().chain(std::iter::once(0)).collect();
        let status = unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                key,
                value,
                REG_SZ.0,
                Some(wide.as_ptr().cast()),
                (wide.len() * 2) as u32,
            )
        };
        status.ok().map_err(|e| e.to_string())
    }

    pub fn show_toast(app_id: &str, xml: &str) -> windows::core::Result<()> {
        let doc = XmlDocument::new()?;
        doc.LoadXml(&HSTRING::from(xml))?;
        let toast = ToastNotification::CreateToastNotification(&doc)?;
        ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(app_id))?.Show(&toast)
    }
}

/// Registers `klipt://` (Windows, installed builds; elsewhere nothing to do).
pub fn register_scheme() {
    #[cfg(windows)]
    if !cfg!(debug_assertions) {
        if let Err(e) = platform::register_scheme() {
            crate::elog!("klipt: can't register klipt:// links: {e}");
        }
    }
}

/// Shows the finished-upload toast with a Copy link button that keeps working from the
/// notification centre. False when it couldn't (not Windows, a development build, or an error):
/// the caller shows its ordinary notification instead.
#[allow(unused_variables)]
pub fn show_windows_toast(app: &AppHandle, title: &str, body: &str, link: &str) -> bool {
    #[cfg(windows)]
    if !cfg!(debug_assertions) {
        // A development build has no Start menu entry under Klipt's id, so Windows wouldn't show it.
        return match platform::show_toast(&app.config().identifier, &toast_xml(title, body, link)) {
            Ok(()) => true,
            Err(e) => {
                crate::elog!("klipt: can't show the upload toast ({e}); showing a plain one");
                false
            }
        };
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    const SERVER: Option<&str> = Some("https://klipt.example.com");

    #[test]
    fn a_copy_uri_carries_the_link_back() {
        let link = "https://klipt.example.com/c/abc?x=1&y=2";
        let uri = copy_uri(link);
        assert!(uri.starts_with("klipt://copy-link?url="), "{uri}");
        assert_eq!(link_from_args(["Klipt.exe".to_string(), uri], SERVER), Some(link.to_string()));
    }

    #[test]
    fn only_links_on_the_logged_in_server_are_copied() {
        let other = copy_uri("https://evil.example.com/c/abc");
        assert_eq!(link_from_args([other], SERVER), None);
        let not_web = copy_uri("javascript:alert(1)");
        assert_eq!(link_from_args([not_web], SERVER), None);
        let ours = copy_uri("https://klipt.example.com/c/abc");
        assert_eq!(link_from_args([ours.clone()], None), None, "logged out: nothing to compare with");
        assert_eq!(link_from_args(["klipt://something-else?url=https://klipt.example.com/c/a"], SERVER), None);
        assert_eq!(link_from_args(["--autostart"], SERVER), None);
    }

    #[test]
    fn the_server_url_may_have_a_trailing_slash() {
        let uri = copy_uri("https://klipt.example.com/c/abc");
        assert!(link_from_args([uri], Some("https://klipt.example.com/")).is_some());
    }

    #[test]
    fn toast_xml_escapes_titles_and_the_button_link() {
        let xml = toast_xml("Clip uploaded", "Tom & Jerry <3", "https://k.example/c/a?b=1&c=2");
        assert!(xml.contains("<text>Tom &amp; Jerry &lt;3</text>"), "{xml}");
        assert!(xml.contains("activationType=\"protocol\""));
        assert!(xml.contains("arguments=\"klipt://copy-link?url=https%3A%2F%2Fk.example%2Fc%2Fa%3Fb%3D1%26c%3D2\""), "{xml}");
    }
}
