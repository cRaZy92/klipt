//! The global shortcuts: "save clip" and (optionally) "take screenshot".
//!
//! Settings store the shortcut as a canonical accelerator: modifiers in the order Ctrl, Alt,
//! Shift, Super, then exactly one key named by its *position* on the keyboard (W3C `code` names,
//! as tauri-plugin-global-shortcut uses), e.g. `Alt+Backquote` for Alt plus the key under Esc.
//!
//! Linux binds it through the XDG GlobalShortcuts portal ([`portal`]); Windows through
//! tauri-plugin-global-shortcut ([`windows`]). Either way the label shown to the user is the one
//! the system reports in [`BoundKey::display`], never the string Klipt asked for.

#[cfg(target_os = "linux")]
pub mod portal;
#[cfg(windows)]
pub mod windows;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use crate::errors::{AppError, Result};

/// The portal shortcut id (Linux).
pub const SHORTCUT_ID: &str = "save-clip";
/// What the system shortcut settings show next to the shortcut (Linux).
pub const SHORTCUT_DESCRIPTION: &str = "Klipt: save clip";
/// The portal id of the screenshot shortcut (Linux).
pub const SCREENSHOT_ID: &str = "screenshot";
pub const SCREENSHOT_DESCRIPTION: &str = "Klipt: take screenshot";

/// What a shortcut press asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    SaveClip,
    Screenshot,
}

/// The shortcuts to bind, as accelerators; no screenshot shortcut when `screenshot` is None.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keys {
    pub save: String,
    pub screenshot: Option<String>,
}

impl Keys {
    pub fn from_settings(s: &crate::settings::Settings) -> Keys {
        Keys { save: s.hotkey.clone(), screenshot: Some(s.screenshot_hotkey.clone()).filter(|k| !k.trim().is_empty()) }
    }
}

/// The shortcuts the system accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundKey {
    /// The save-clip binding as the system reports it, for showing to the user.
    pub display: String,
    /// The screenshot binding as the system reports it.
    pub screenshot: Option<String>,
    /// Why the screenshot shortcut couldn't be bound (saving clips still works).
    pub screenshot_error: Option<String>,
}

pub trait HotkeyService {
    /// Binds the shortcuts (any form [`normalize`] accepts), replacing earlier bindings. Fails
    /// only when the save-clip shortcut can't be bound; a screenshot shortcut that can't is
    /// reported in [`BoundKey::screenshot_error`]. May block while the system asks the user (KDE
    /// shows a dialog).
    fn bind(&mut self, keys: &Keys) -> Result<BoundKey>;
    /// A fresh stream of presses. Presses go to the receiver of the latest call. The stream
    /// ends if the binding is lost (e.g. the portal went away).
    fn events(&self) -> Receiver<Action>;
}

/// The hotkey service for this platform.
pub fn platform_service(app: &tauri::AppHandle) -> Box<dyn HotkeyService + Send> {
    #[cfg(target_os = "linux")]
    {
        let _ = app;
        Box::new(portal::PortalHotkey::new())
    }
    #[cfg(windows)]
    {
        Box::new(self::windows::PluginHotkey::new(app.clone()))
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        let _ = app;
        Box::new(Unsupported::default())
    }
}

/// Tauri event names the UI listens to.
pub mod events {
    /// Payload: `{ display }`, the binding as the system reports it.
    pub const HOTKEY_BOUND: &str = "hotkey-bound";
    /// Payload: `{ code, message }` with a friendly message (`hotkey_taken`, `hotkey_unavailable`).
    pub const HOTKEY_ERROR: &str = "hotkey-error";
}

/// The latest shortcut outcome, kept so a window that starts listening after the
/// `hotkey-bound`/`hotkey-error` events fired can still show it (`get_hotkey_status`).
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HotkeyStatus {
    /// The bound shortcut as the system reports it.
    pub display: Option<String>,
    /// The bound screenshot shortcut as the system reports it.
    pub screenshot_display: Option<String>,
    /// Why the screenshot shortcut isn't bound, if it should be.
    pub screenshot_error: Option<String>,
    /// A friendly message when binding failed or the binding was lost.
    pub error: Option<String>,
    /// The error's code (`hotkey_taken`, `hotkey_unavailable`), for the UI to branch on.
    pub error_code: Option<String>,
}

/// Managed Tauri state holding the latest [`HotkeyStatus`].
#[derive(Default)]
pub struct HotkeyStatusState(pub Mutex<HotkeyStatus>);

impl HotkeyStatusState {
    pub fn get(&self) -> HotkeyStatus {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    fn set(&self, status: HotkeyStatus) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = status;
    }
}

/// The longest a rebind may take. KDE may show a dialog the user has to answer first.
pub const REBIND_TIMEOUT: Duration = Duration::from_secs(180);

enum Control {
    Rebind { keys: Keys, reply: Sender<Result<BoundKey>> },
}

/// Talks to the hotkey thread started by [`start_for_app`] (managed Tauri state).
#[derive(Clone)]
pub struct HotkeyHandle(Sender<Control>);

impl HotkeyHandle {
    /// Binds the save-clip shortcut to `accel` instead of the current one and returns the binding
    /// the system reports. On failure (e.g. [`AppError::HotkeyTaken`]) the previous shortcut is
    /// bound again when possible. Blocks, possibly while the user answers a system dialog.
    pub fn rebind(&self, keys: &Keys) -> Result<BoundKey> {
        let (reply, answer) = mpsc::channel();
        self.0
            .send(Control::Rebind { keys: keys.clone(), reply })
            .map_err(|_| AppError::HotkeyUnavailable("the hotkey thread stopped".into()))?;
        answer
            .recv_timeout(REBIND_TIMEOUT)
            .unwrap_or_else(|_| Err(AppError::HotkeyUnavailable("no answer from the shortcut service".into())))
    }
}

/// Binds the shortcut from Settings on a background thread (KDE may show a dialog first), saves
/// a clip on every press, and rebinds when Settings change it (through the returned handle).
/// A failure doesn't affect recording: it's reported as a `hotkey-error` event and a
/// notification, and "Save clip now" in the tray keeps working.
pub fn start_for_app(app: &tauri::AppHandle, keys: Keys) -> HotkeyHandle {
    let (tx, rx) = mpsc::channel();
    let app = app.clone();
    let spawned = std::thread::Builder::new().name("klipt-hotkey".into()).spawn(move || {
        let mut service = platform_service(&app);
        let presses = Presses::default();
        // The shortcuts currently bound, if any.
        let mut bound = match service.bind(&keys) {
            Ok(key) => {
                on_bound(&app, &*service, &presses, &key);
                Some(keys)
            }
            Err(e) => {
                eprintln!("klipt: can't bind the save-clip shortcut {:?}: {e}", keys.save);
                report_error(&app, &e);
                None
            }
        };
        // `service` lives as long as this loop (the app), which keeps the binding alive.
        for Control::Rebind { keys, reply } in rx {
            let result = service.bind(&keys);
            match &result {
                Ok(key) => {
                    on_bound(&app, &*service, &presses, key);
                    bound = Some(keys);
                }
                Err(e) => {
                    eprintln!("klipt: can't rebind the save-clip shortcut to {:?}: {e}", keys.save);
                    // Binding replaced the old shortcut; put it back so saving keeps working.
                    if let Some(old) = &bound {
                        match service.bind(old) {
                            Ok(key) => on_bound(&app, &*service, &presses, &key),
                            Err(e) => {
                                eprintln!("klipt: can't restore the save-clip shortcut {:?}: {e}", old.save);
                                bound = None;
                                report_error(&app, &e);
                            }
                        }
                    }
                }
            }
            let _ = reply.send(result);
        }
    });
    if let Err(e) = spawned {
        eprintln!("klipt: can't start the hotkey thread: {e}");
    }
    HotkeyHandle(tx)
}

/// Counts press-forwarding threads, so only the newest one reports a lost shortcut.
#[derive(Clone, Default)]
struct Presses(Arc<AtomicU64>);

/// Records a successful binding (status, `hotkey-bound`) and forwards its presses as saves.
fn on_bound(app: &tauri::AppHandle, service: &dyn HotkeyService, presses: &Presses, key: &BoundKey) {
    use tauri::{Emitter, Manager};
    if let Some(status) = app.try_state::<HotkeyStatusState>() {
        status.set(HotkeyStatus {
            display: Some(key.display.clone()),
            screenshot_display: key.screenshot.clone(),
            screenshot_error: key.screenshot_error.clone(),
            error: None,
            error_code: None,
        });
    }
    let _ = app.emit(
        events::HOTKEY_BOUND,
        serde_json::json!({ "display": key.display, "screenshotDisplay": key.screenshot, "screenshotError": key.screenshot_error }),
    );
    if let Some(error) = &key.screenshot_error {
        crate::feedback::notify(app, "Klipt", Some(error));
    }
    // A new subscription ends the previous forwarder's stream; its generation is stale by then,
    // so it leaves quietly instead of reporting the shortcut as lost.
    let generation = presses.0.fetch_add(1, Ordering::SeqCst) + 1;
    let stream = service.events();
    let (app, presses) = (app.clone(), presses.clone());
    let spawned = std::thread::Builder::new().name("klipt-hotkey-presses".into()).spawn(move || {
        for action in stream {
            match action {
                Action::SaveClip => {
                    if let Some(recorder) = app.try_state::<crate::recorder::SupervisorHandle>() {
                        recorder.save();
                    }
                }
                Action::Screenshot => crate::screenshot::take_in_background(&app),
            }
        }
        // The press stream of the current binding only ends when it is lost (e.g. plasmashell
        // restarted).
        if presses.0.load(Ordering::SeqCst) == generation {
            report_error(&app, &AppError::HotkeyLost);
        }
    });
    if let Err(e) = spawned {
        eprintln!("klipt: can't start the hotkey press thread: {e}");
    }
}

/// Tells the UI (`hotkey-error`) and the user (a notification) about a shortcut problem.
fn report_error(app: &tauri::AppHandle, e: &AppError) {
    use tauri::{Emitter, Manager};
    eprintln!("klipt: save-clip shortcut: {e}");
    let message = e.user_message();
    if let Some(status) = app.try_state::<HotkeyStatusState>() {
        status.set(HotkeyStatus {
            display: None,
            screenshot_display: None,
            screenshot_error: None,
            error: Some(message.clone()),
            error_code: Some(e.code().into()),
        });
    }
    let _ = app.emit(events::HOTKEY_ERROR, serde_json::json!({ "code": e.code(), "message": message }));
    crate::feedback::notify(app, "Klipt", Some(&message));
}

#[cfg(not(any(target_os = "linux", windows)))]
#[derive(Default)]
struct Unsupported(PressSink);

#[cfg(not(any(target_os = "linux", windows)))]
impl HotkeyService for Unsupported {
    fn bind(&mut self, _keys: &Keys) -> Result<BoundKey> {
        Err(AppError::HotkeyUnavailable("no global shortcut support on this platform".into()))
    }
    fn events(&self) -> Receiver<Action> {
        self.0.subscribe()
    }
}

/// Where presses go: `subscribe` swaps in a new channel (same idea as the recorder's event sink).
#[derive(Clone, Default)]
pub(crate) struct PressSink(Arc<Mutex<Option<Sender<Action>>>>);

impl PressSink {
    pub(crate) fn subscribe(&self) -> Receiver<Action> {
        let (tx, rx) = mpsc::channel();
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = Some(tx);
        rx
    }

    /// The shortcut is gone for good: ends the current receiver's stream.
    pub(crate) fn close(&self) {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).take();
    }

    pub(crate) fn press(&self, action: Action) {
        if let Some(tx) = self.0.lock().unwrap_or_else(PoisonError::into_inner).as_ref() {
            let _ = tx.send(action);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Modifier {
    Ctrl,
    Alt,
    Shift,
    Super,
}

impl Modifier {
    fn parse(token: &str) -> Option<Modifier> {
        match token {
            "ctrl" | "control" => Some(Modifier::Ctrl),
            "alt" | "option" => Some(Modifier::Alt),
            "shift" => Some(Modifier::Shift),
            "super" | "cmd" | "command" | "meta" | "win" | "logo" => Some(Modifier::Super),
            _ => None,
        }
    }

    fn canonical(self) -> &'static str {
        match self {
            Modifier::Ctrl => "Ctrl",
            Modifier::Alt => "Alt",
            Modifier::Shift => "Shift",
            Modifier::Super => "Super",
        }
    }

    /// XDG shortcuts spec modifier names.
    fn portal(self) -> &'static str {
        match self {
            Modifier::Ctrl => "CTRL",
            Modifier::Alt => "ALT",
            Modifier::Shift => "SHIFT",
            Modifier::Super => "LOGO",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Modifier::Super if cfg!(windows) => "Win",
            m => m.canonical(),
        }
    }
}

/// Named keys: (canonical W3C code name, extra lower-case aliases, XDG keysym name, label).
const NAMED_KEYS: &[(&str, &[&str], &str, &str)] = &[
    ("Backquote", &["`", "grave", "backtick"], "grave", "`"),
    ("Minus", &["-"], "minus", "-"),
    ("Equal", &["="], "equal", "="),
    ("BracketLeft", &["["], "bracketleft", "["),
    ("BracketRight", &["]"], "bracketright", "]"),
    ("Backslash", &["\\"], "backslash", "\\"),
    ("Semicolon", &[";"], "semicolon", ";"),
    ("Quote", &["'", "apostrophe"], "apostrophe", "'"),
    ("Comma", &[","], "comma", ","),
    ("Period", &["."], "period", "."),
    ("Slash", &["/"], "slash", "/"),
    ("Space", &[], "space", "Space"),
    ("Insert", &[], "Insert", "Insert"),
    ("Home", &[], "Home", "Home"),
    ("End", &[], "End", "End"),
    ("PageUp", &[], "Prior", "Page Up"),
    ("PageDown", &[], "Next", "Page Down"),
    ("Pause", &[], "Pause", "Pause"),
    ("PrintScreen", &["print"], "Print", "Print Screen"),
];

/// A parsed accelerator.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Accel {
    mods: Vec<Modifier>,
    /// Canonical key name.
    key: String,
}

fn invalid(accel: &str, why: &str) -> AppError {
    AppError::InvalidInput(format!("shortcut {accel:?}: {why}"))
}

/// The canonical key name for a lower-case token, if it's a key Klipt accepts.
fn canonical_key(token: &str) -> Option<String> {
    let bytes = token.as_bytes();
    if bytes.len() == 1 && (bytes[0].is_ascii_alphabetic() || bytes[0].is_ascii_digit()) {
        return Some(token.to_ascii_uppercase());
    }
    // W3C forms of letters and digits: KeyA, Digit1.
    if let Some(rest) = token.strip_prefix("key").filter(|r| r.len() == 1 && r.as_bytes()[0].is_ascii_alphabetic()) {
        return Some(rest.to_ascii_uppercase());
    }
    if let Some(rest) = token.strip_prefix("digit").filter(|r| r.len() == 1 && r.as_bytes()[0].is_ascii_digit()) {
        return Some(rest.into());
    }
    if let Some(n) = token.strip_prefix('f').and_then(|n| n.parse::<u8>().ok()) {
        if (1..=24).contains(&n) && !token[1..].starts_with('0') {
            return Some(format!("F{n}"));
        }
    }
    NAMED_KEYS
        .iter()
        .find(|(name, aliases, ..)| name.eq_ignore_ascii_case(token) || aliases.contains(&token))
        .map(|(name, ..)| (*name).into())
}

fn parse(accel: &str) -> Result<Accel> {
    let trimmed = accel.trim();
    if trimmed.is_empty() {
        return Err(invalid(accel, "empty"));
    }
    let mut mods = vec![];
    let mut key = None;
    for raw in trimmed.split('+') {
        let token = raw.trim().to_ascii_lowercase();
        if token.is_empty() {
            return Err(invalid(accel, "empty part"));
        }
        if let Some(m) = Modifier::parse(&token) {
            if !mods.contains(&m) {
                mods.push(m);
            }
        } else if key.is_some() {
            return Err(invalid(accel, "more than one key"));
        } else {
            key = Some(canonical_key(&token).ok_or_else(|| invalid(accel, "unknown key"))?);
        }
    }
    let key = key.ok_or_else(|| invalid(accel, "no key, only modifiers"))?;
    mods.sort();
    Ok(Accel { mods, key })
}

/// Normalizes an accelerator to Klipt's canonical form, e.g. `"shift+ctrl+s"` → `"Ctrl+Shift+S"`.
pub fn normalize(accel: &str) -> Result<String> {
    let a = parse(accel)?;
    Ok(a.mods.iter().map(|m| m.canonical()).chain([a.key.as_str()]).collect::<Vec<_>>().join("+"))
}

/// The accelerator in the XDG shortcuts spec format the portal's `preferred_trigger` takes:
/// upper-case modifiers and an xkb keysym name, e.g. `"ALT+grave"`.
pub fn to_portal_trigger(accel: &str) -> Result<String> {
    let a = parse(accel)?;
    let key = match NAMED_KEYS.iter().find(|(name, ..)| *name == a.key) {
        Some((_, _, keysym, _)) => (*keysym).to_string(),
        // Letters are lower-case keysyms; digits and F-keys are already keysym names.
        None if a.key.len() == 1 => a.key.to_ascii_lowercase(),
        None => a.key.clone(),
    };
    Ok(a.mods.iter().map(|m| m.portal()).chain([key.as_str()]).collect::<Vec<_>>().join("+"))
}

/// The accelerator in tauri-plugin-global-shortcut's format. Our canonical form already is one
/// (W3C code names, `Ctrl`/`Alt`/`Shift`/`Super`).
pub fn to_plugin_accelerator(accel: &str) -> Result<String> {
    normalize(accel)
}

/// A human label for an accelerator, e.g. `"Alt+`"`. Used on Windows; it names the US-layout
/// symbol, which may differ from what the key types on other layouts.
pub fn label(accel: &str) -> Result<String> {
    let a = parse(accel)?;
    let key = NAMED_KEYS.iter().find(|(name, ..)| *name == a.key).map_or(a.key.as_str(), |(.., label)| *label);
    Ok(a.mods.iter().map(|m| m.label()).chain([key]).collect::<Vec<_>>().join("+"))
}

/// Maps a tauri-plugin-global-shortcut registration error message to an app error.
pub fn classify_register_error(message: &str) -> AppError {
    let lower = message.to_ascii_lowercase();
    if lower.contains("already registered") || lower.contains("unable to register") {
        AppError::HotkeyTaken
    } else {
        AppError::HotkeyUnavailable(message.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_case_and_modifier_order() {
        assert_eq!(normalize("alt+f9").unwrap(), "Alt+F9");
        assert_eq!(normalize("alt+`").unwrap(), "Alt+Backquote");
        assert_eq!(normalize("ALT+BACKQUOTE").unwrap(), "Alt+Backquote");
        assert_eq!(normalize("Alt+grave").unwrap(), "Alt+Backquote");
        assert_eq!(normalize("shift+ctrl+s").unwrap(), "Ctrl+Shift+S");
        assert_eq!(normalize("win+control+meta+1").unwrap(), "Ctrl+Super+1");
        assert_eq!(normalize("cmd+shift+F24").unwrap(), "Shift+Super+F24");
        assert_eq!(normalize(" Ctrl + PageUp ").unwrap(), "Ctrl+PageUp");
        assert_eq!(normalize("alt+KeyZ").unwrap(), "Alt+Z");
        assert_eq!(normalize("alt+semicolon").unwrap(), "Alt+Semicolon");
        assert_eq!(normalize("F10").unwrap(), "F10");
    }

    #[test]
    fn the_default_setting_is_canonical() {
        let default = crate::settings::DEFAULT_HOTKEY;
        assert_eq!(normalize(default).unwrap(), default);
    }

    #[test]
    fn rejects_invalid_accelerators() {
        for bad in ["", "  ", "Alt", "Ctrl+Shift", "Alt+F9+F10", "Alt+Nope", "Alt++F9", "Alt+F25", "Alt+F0", "Alt+F09"] {
            let err = normalize(bad).expect_err(bad);
            assert_eq!(err.code(), "invalid_input", "{bad}");
        }
    }

    #[test]
    fn converts_to_the_portal_trigger_format() {
        assert_eq!(to_portal_trigger("Alt+Backquote").unwrap(), "ALT+grave");
        assert_eq!(to_portal_trigger("alt+f9").unwrap(), "ALT+F9");
        assert_eq!(to_portal_trigger("shift+ctrl+s").unwrap(), "CTRL+SHIFT+s");
        assert_eq!(to_portal_trigger("Super+PageDown").unwrap(), "LOGO+Next");
        assert_eq!(to_portal_trigger("Alt+Quote").unwrap(), "ALT+apostrophe");
        assert_eq!(to_portal_trigger("Ctrl+5").unwrap(), "CTRL+5");
        assert!(to_portal_trigger("Alt").is_err());
    }

    #[test]
    fn converts_to_the_plugin_format() {
        assert_eq!(to_plugin_accelerator("alt+`").unwrap(), "Alt+Backquote");
        assert_eq!(to_plugin_accelerator("shift+ctrl+s").unwrap(), "Ctrl+Shift+S");
        assert_eq!(to_plugin_accelerator("ctrl+printscreen").unwrap(), "Ctrl+PrintScreen");
        assert!(to_plugin_accelerator("Alt+Nope").is_err());
    }

    #[test]
    fn labels_are_readable() {
        assert_eq!(label("Alt+Backquote").unwrap(), "Alt+`");
        assert_eq!(label("ctrl+pagedown").unwrap(), "Ctrl+Page Down");
        assert_eq!(label("alt+f9").unwrap(), "Alt+F9");
    }

    #[test]
    fn a_taken_shortcut_is_a_dedicated_error() {
        let e = classify_register_error("HotKey already registered: HotKey { mods: ALT, key: F9, id: 1 }");
        assert_eq!(e.code(), "hotkey_taken");
        assert_eq!(e.user_message(), "That shortcut is already used by another app. Pick a different one in Settings.");
        assert_eq!(classify_register_error("Unable to register hotkey: x").code(), "hotkey_taken");
        assert_eq!(classify_register_error("something else").code(), "hotkey_unavailable");
    }

    #[test]
    fn presses_go_to_the_latest_subscriber() {
        let sink = PressSink::default();
        sink.press(Action::SaveClip); // nobody listens: fine
        let old = sink.subscribe();
        let new = sink.subscribe();
        sink.press(Action::Screenshot);
        assert_eq!(new.try_recv(), Ok(Action::Screenshot));
        assert!(old.try_recv().is_err());
    }

    #[test]
    fn closing_the_sink_ends_the_press_stream() {
        let sink = PressSink::default();
        let presses = sink.subscribe();
        sink.press(Action::SaveClip);
        sink.close();
        sink.press(Action::SaveClip); // after close: dropped, not a panic
        assert_eq!(presses.iter().count(), 1, "the stream ends after the press that came first");
    }

    #[test]
    fn a_lost_shortcut_is_reported_as_unavailable_with_a_friendly_message() {
        let e = AppError::HotkeyLost;
        assert_eq!(e.code(), "hotkey_unavailable");
        assert_eq!(e.user_message(), "The save shortcut stopped working. Use the tray menu to save, or restart Klipt.");
    }
}
