//! System tray (StatusNotifierItem + dbusmenu, written directly on D-Bus so no Qt Widgets is
//! needed) and the single-instance service (`org.freedesktop.Application`).
//!
//! Everything the tray wants the app to do goes through one [`Msg`] channel; the Qt side installs
//! the receiver with [`set_dispatcher`]. Messages sent before that are kept and delivered then.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use zbus::blocking::Connection;
use zbus::interface;
use zbus::zvariant::{ObjectPath, OwnedValue, Type, Value};

pub const APP_ID: &str = "io.github.Jenari_Dev.Tsuba";
const APP_PATH: &str = "/io/github/Jenari_Dev/Tsuba";

/// What the tray or another launch asks of the running app.
#[derive(Debug, Clone, PartialEq)]
pub enum Msg {
    /// Show and raise the main window.
    Activate,
    Quit,
    /// Switch the device's profile (device pid, profile id).
    Profile(String, String),
    /// Open the page of this name (e.g. "settings").
    OpenPage(String),
    /// A tray host appeared or went away.
    TrayAvailable(bool),
}

type Dispatcher = Box<dyn Fn(Msg) + Send + Sync>;
static DISPATCH: OnceLock<Dispatcher> = OnceLock::new();
static PENDING: Mutex<Vec<Msg>> = Mutex::new(Vec::new());

pub fn send(m: Msg) {
    match DISPATCH.get() {
        Some(d) => d(m),
        None => PENDING.lock().unwrap().push(m),
    }
}

/// Install the receiver (first call wins) and deliver what arrived meanwhile.
pub fn set_dispatcher(d: Dispatcher) {
    if DISPATCH.set(d).is_ok() {
        let pending: Vec<Msg> = std::mem::take(&mut *PENDING.lock().unwrap());
        for m in pending {
            send(m);
        }
    }
}

// ---- menu model ----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct TrayProfile {
    pub id: String,
    pub name: String,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TrayDevice {
    pub pid: String,
    pub name: String,
    pub profiles: Vec<TrayProfile>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ItemKind {
    Normal,
    Separator,
    Radio(bool),
    Submenu,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub id: i32,
    pub label: String,
    pub kind: ItemKind,
    pub enabled: bool,
    pub action: Option<Msg>,
    pub children: Vec<Item>,
}

/// Labels come in translated (the QML side calls `i18n`).
#[derive(Debug, Clone, PartialEq)]
pub struct Labels {
    pub open: String,
    pub settings: String,
    pub quit: String,
    pub profiles: String,
}

impl Default for Labels {
    fn default() -> Self {
        Self { open: "Open Tsuba".into(), settings: "Settings".into(), quit: "Quit".into(), profiles: "Profile".into() }
    }
}

/// The context menu: Open, one submenu of profiles per device, Settings, Quit.
pub fn build_menu(devices: &[TrayDevice], l: &Labels) -> Vec<Item> {
    let mut next = 1;
    let mut id = || {
        next += 1;
        next - 1
    };
    let item = |id: i32, label: &str, action: Msg| Item { id, label: label.into(), kind: ItemKind::Normal, enabled: true, action: Some(action), children: vec![] };
    let sep = |id: i32| Item { id, label: String::new(), kind: ItemKind::Separator, enabled: true, action: None, children: vec![] };
    let mut out = vec![item(id(), &l.open, Msg::Activate), sep(id())];
    for d in devices.iter().filter(|d| !d.profiles.is_empty()) {
        let children = d
            .profiles
            .iter()
            .map(|p| Item {
                id: id(),
                label: p.name.clone(),
                kind: ItemKind::Radio(p.active),
                enabled: true,
                action: Some(Msg::Profile(d.pid.clone(), p.id.clone())),
                children: vec![],
            })
            .collect();
        out.push(Item { id: id(), label: format!("{} — {}", d.name, l.profiles), kind: ItemKind::Submenu, enabled: true, action: None, children });
    }
    if out.len() > 2 {
        out.push(sep(id()));
    }
    out.push(item(id(), &l.settings, Msg::OpenPage("settings".into())));
    out.push(item(id(), &l.quit, Msg::Quit));
    out
}

fn find<'a>(items: &'a [Item], id: i32) -> Option<&'a Item> {
    items.iter().find_map(|i| if i.id == id { Some(i) } else { find(&i.children, id) })
}

// ---- dbusmenu ------------------------------------------------------------------------------

type Props = HashMap<String, OwnedValue>;

fn ov<'a, T: Into<Value<'a>>>(v: T) -> OwnedValue {
    OwnedValue::try_from(v.into()).expect("plain values hold no file descriptors")
}

fn item_props(i: &Item) -> Props {
    let mut p: Props = HashMap::new();
    match &i.kind {
        ItemKind::Separator => {
            p.insert("type".into(), ov("separator"));
        }
        ItemKind::Normal => {
            p.insert("label".into(), ov(i.label.as_str()));
        }
        ItemKind::Radio(on) => {
            p.insert("label".into(), ov(i.label.as_str()));
            p.insert("toggle-type".into(), ov("radio"));
            p.insert("toggle-state".into(), ov(if *on { 1i32 } else { 0i32 }));
        }
        ItemKind::Submenu => {
            p.insert("label".into(), ov(i.label.as_str()));
            p.insert("children-display".into(), ov("submenu"));
        }
    }
    p.insert("enabled".into(), ov(i.enabled));
    p
}

fn props_value(p: &Props) -> Value<'static> {
    let mut d = zbus::zvariant::Dict::new(&zbus::zvariant::Signature::Str, &zbus::zvariant::Signature::Variant);
    for (k, v) in p {
        d.append(Value::from(k.clone()), Value::Value(Box::new(Value::from(v.try_clone().expect("no fds"))))).expect("dict entry types match");
    }
    Value::Dict(d)
}

/// `(ia{sv}av)` as a variant, for nesting in the parent's children array.
fn layout_value(i: &Item, depth: i32) -> Value<'static> {
    let children: Vec<Value<'static>> = if depth != 0 { i.children.iter().map(|c| Value::Value(Box::new(layout_value(c, depth - 1)))).collect() } else { vec![] };
    let arr = zbus::zvariant::Array::from(children);
    Value::Structure(
        zbus::zvariant::StructureBuilder::new()
            .append_field(Value::I32(i.id))
            .append_field(props_value(&item_props(i)))
            .append_field(Value::Array(arr))
            .build()
            .expect("three fields"),
    )
}

#[derive(Type, serde::Serialize)]
struct Root {
    id: i32,
    props: Props,
    children: Vec<OwnedValue>,
}

struct Menu {
    items: Mutex<(Vec<Item>, u32)>,
}

#[interface(name = "com.canonical.dbusmenu")]
impl Menu {
    #[zbus(property)]
    fn version(&self) -> u32 {
        3
    }
    #[zbus(property)]
    fn text_direction(&self) -> String {
        "ltr".into()
    }
    #[zbus(property)]
    fn status(&self) -> String {
        "normal".into()
    }
    #[zbus(property)]
    fn icon_theme_path(&self) -> Vec<String> {
        vec![]
    }

    fn get_layout(&self, parent_id: i32, recursion_depth: i32, _property_names: Vec<String>) -> (u32, Root) {
        let g = self.items.lock().unwrap();
        let (items, rev) = (&g.0, g.1);
        let root_item = Item { id: 0, label: String::new(), kind: ItemKind::Submenu, enabled: true, action: None, children: items.clone() };
        let node = if parent_id == 0 { Some(&root_item) } else { find(items, parent_id) };
        let Some(node) = node else { return (rev, Root { id: parent_id, props: HashMap::new(), children: vec![] }) };
        let depth = if recursion_depth < 0 { -1 } else { recursion_depth };
        let children = if depth == 0 { vec![] } else { node.children.iter().map(|c| ov(layout_value(c, depth - 1))).collect() };
        (rev, Root { id: node.id, props: item_props(node), children })
    }

    fn get_group_properties(&self, ids: Vec<i32>, _names: Vec<String>) -> Vec<(i32, Props)> {
        let g = self.items.lock().unwrap();
        ids.into_iter().filter_map(|i| find(&g.0, i).map(|it| (i, item_props(it)))).collect()
    }

    fn get_property(&self, id: i32, name: String) -> zbus::fdo::Result<OwnedValue> {
        let g = self.items.lock().unwrap();
        find(&g.0, id).and_then(|it| item_props(it).remove(&name)).ok_or_else(|| zbus::fdo::Error::InvalidArgs("no such property".into()))
    }

    fn event(&self, id: i32, event_id: String, _data: Value<'_>, _timestamp: u32) {
        if event_id != "clicked" {
            return;
        }
        let action = find(&self.items.lock().unwrap().0, id).and_then(|i| i.action.clone());
        if let Some(a) = action {
            send(a);
        }
    }

    fn event_group(&self, events: Vec<(i32, String, Value<'_>, u32)>) -> Vec<i32> {
        for (id, ev, data, ts) in events {
            self.event(id, ev, data, ts);
        }
        vec![]
    }

    fn about_to_show(&self, _id: i32) -> bool {
        false
    }

    fn about_to_show_group(&self, _ids: Vec<i32>) -> (Vec<i32>, Vec<i32>) {
        (vec![], vec![])
    }

    #[zbus(signal)]
    async fn layout_updated(emitter: &zbus::object_server::SignalEmitter<'_>, revision: u32, parent: i32) -> zbus::Result<()>;
}

// ---- StatusNotifierItem ----------------------------------------------------------------------

struct Sni {
    tooltip: Mutex<String>,
}

#[interface(name = "org.kde.StatusNotifierItem")]
impl Sni {
    #[zbus(property)]
    fn category(&self) -> String {
        "ApplicationStatus".into()
    }
    #[zbus(property)]
    fn id(&self) -> String {
        "tsuba".into()
    }
    #[zbus(property)]
    fn title(&self) -> String {
        "Tsuba".into()
    }
    #[zbus(property)]
    fn status(&self) -> String {
        "Active".into()
    }
    #[zbus(property)]
    fn window_id(&self) -> u32 {
        0
    }
    #[zbus(property)]
    fn icon_name(&self) -> String {
        APP_ID.into()
    }
    #[zbus(property)]
    fn icon_pixmap(&self) -> Vec<(i32, i32, Vec<u8>)> {
        vec![]
    }
    #[zbus(property)]
    fn overlay_icon_name(&self) -> String {
        String::new()
    }
    #[zbus(property)]
    fn overlay_icon_pixmap(&self) -> Vec<(i32, i32, Vec<u8>)> {
        vec![]
    }
    #[zbus(property)]
    fn attention_icon_name(&self) -> String {
        String::new()
    }
    #[zbus(property)]
    fn attention_icon_pixmap(&self) -> Vec<(i32, i32, Vec<u8>)> {
        vec![]
    }
    #[zbus(property)]
    fn attention_movie_name(&self) -> String {
        String::new()
    }
    #[zbus(property)]
    fn tool_tip(&self) -> (String, Vec<(i32, i32, Vec<u8>)>, String, String) {
        (APP_ID.into(), vec![], "Tsuba".into(), self.tooltip.lock().unwrap().clone())
    }
    #[zbus(property)]
    fn item_is_menu(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn menu(&self) -> ObjectPath<'static> {
        ObjectPath::from_static_str_unchecked("/MenuBar")
    }

    fn activate(&self, _x: i32, _y: i32) {
        send(Msg::Activate);
    }
    fn secondary_activate(&self, _x: i32, _y: i32) {
        send(Msg::Activate);
    }
    fn context_menu(&self, _x: i32, _y: i32) {}
    fn scroll(&self, _delta: i32, _orientation: String) {}

    #[zbus(signal)]
    async fn new_tool_tip(emitter: &zbus::object_server::SignalEmitter<'_>) -> zbus::Result<()>;
}

// ---- single instance (org.freedesktop.Application) ----------------------------------------------

struct Application;

#[interface(name = "org.freedesktop.Application")]
impl Application {
    fn activate(&self, _platform_data: HashMap<String, OwnedValue>) {
        send(Msg::Activate);
    }
    fn open(&self, _uris: Vec<String>, _platform_data: HashMap<String, OwnedValue>) {
        send(Msg::Activate);
    }
    fn activate_action(&self, name: String, _parameter: Vec<OwnedValue>, _platform_data: HashMap<String, OwnedValue>) {
        if name == "settings" {
            send(Msg::OpenPage("settings".into()));
        } else {
            send(Msg::Activate);
        }
    }
}

static CONN: OnceLock<Connection> = OnceLock::new();

/// Become the one running instance. Returns `false` when another instance already runs (it has
/// been asked to show its window) and this process should exit. If the session bus is not
/// reachable this returns `true`: better two windows than none.
pub fn claim_instance() -> bool {
    let build = || -> zbus::Result<Connection> {
        let c = zbus::blocking::connection::Builder::session()?.serve_at(APP_PATH, Application)?.build()?;
        // Not through Builder::name(): that asks to replace an existing owner, which would steal the running instance's name.
        c.request_name_with_flags(APP_ID, zbus::fdo::RequestNameFlags::DoNotQueue.into())?;
        Ok(c)
    };
    match build() {
        Ok(c) => {
            let _ = CONN.set(c);
            true
        }
        Err(zbus::Error::NameTaken) => {
            if let Ok(c) = Connection::session() {
                let args: HashMap<String, Value> = HashMap::new();
                let _ = c.call_method(Some(APP_ID), APP_PATH, Some("org.freedesktop.Application"), "Activate", &(args,));
            }
            false
        }
        Err(_) => true,
    }
}

// ---- tray ------------------------------------------------------------------------------------

struct Tray {
    conn: Connection,
    menu: zbus::blocking::object_server::InterfaceRef<Menu>,
    sni: zbus::blocking::object_server::InterfaceRef<Sni>,
}

static TRAY: OnceLock<Tray> = OnceLock::new();

const WATCHER: &str = "org.kde.StatusNotifierWatcher";
const WATCHER_PATH: &str = "/StatusNotifierWatcher";

/// Whether a tray host is listening, and the watcher's unique owner (to notice restarts).
fn watcher_state(c: &Connection) -> Option<String> {
    let dbus = zbus::blocking::fdo::DBusProxy::new(c).ok()?;
    let owner = dbus.get_name_owner(WATCHER.try_into().ok()?).ok()?;
    let host: bool = c
        .call_method(Some(WATCHER), WATCHER_PATH, Some("org.freedesktop.DBus.Properties"), "Get", &(WATCHER, "IsStatusNotifierHostRegistered"))
        .ok()
        .and_then(|m| m.body().deserialize::<OwnedValue>().ok())
        .and_then(|v| bool::try_from(v).ok())
        .unwrap_or(false);
    host.then(|| owner.to_string())
}

/// Create the tray icon and keep it registered (Plasma restarts re-register it). Idempotent.
pub fn start() {
    if TRAY.get().is_some() {
        return;
    }
    let svc = format!("org.kde.StatusNotifierItem-{}-1", std::process::id());
    let built = zbus::blocking::connection::Builder::session()
        .and_then(|b| b.name(svc.as_str()))
        .and_then(|b| b.serve_at("/StatusNotifierItem", Sni { tooltip: Mutex::new(String::new()) }))
        .and_then(|b| b.serve_at("/MenuBar", Menu { items: Mutex::new((vec![], 1)) }))
        .and_then(|b| b.build());
    let Ok(conn) = built else {
        send(Msg::TrayAvailable(false));
        return;
    };
    let (Ok(menu), Ok(sni)) = (conn.object_server().interface::<_, Menu>("/MenuBar"), conn.object_server().interface::<_, Sni>("/StatusNotifierItem")) else {
        send(Msg::TrayAvailable(false));
        return;
    };
    let tray = Tray { conn: conn.clone(), menu, sni };
    if TRAY.set(tray).is_err() {
        return;
    }
    std::thread::Builder::new()
        .name("tray-watch".into())
        .spawn(move || {
            let mut registered_with: Option<String> = None;
            let mut available = None;
            loop {
                let now = watcher_state(&conn);
                if now != registered_with {
                    if let Some(owner) = &now {
                        let ok = conn.call_method(Some(WATCHER), WATCHER_PATH, Some(WATCHER), "RegisterStatusNotifierItem", &(svc.as_str(),)).is_ok();
                        registered_with = ok.then(|| owner.clone());
                    } else {
                        registered_with = None;
                    }
                }
                let avail = now.is_some() && registered_with.is_some();
                if available != Some(avail) {
                    available = Some(avail);
                    send(Msg::TrayAvailable(avail));
                }
                std::thread::sleep(std::time::Duration::from_secs(3));
            }
        })
        .ok();
}

/// Replace the menu and tooltip.
pub fn update(devices: &[TrayDevice], labels: &Labels, tooltip: &str) {
    let Some(t) = TRAY.get() else { return };
    let items = build_menu(devices, labels);
    let rev = {
        let iface = t.menu.get();
        let mut g = iface.items.lock().unwrap();
        g.1 += 1;
        g.0 = items;
        g.1
    };
    let _ = zbus::block_on(Menu::layout_updated(t.menu.signal_emitter(), rev, 0));
    *t.sni.get().tooltip.lock().unwrap() = tooltip.to_string();
    let _ = zbus::block_on(Sni::new_tool_tip(t.sni.signal_emitter()));
    let _ = &t.conn;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dev(pid: &str, profiles: &[(&str, &str, bool)]) -> TrayDevice {
        TrayDevice {
            pid: pid.into(),
            name: format!("Device {pid}"),
            profiles: profiles.iter().map(|(id, n, a)| TrayProfile { id: id.to_string(), name: n.to_string(), active: *a }).collect(),
        }
    }

    #[test]
    fn menu_without_devices_has_open_settings_quit() {
        let m = build_menu(&[], &Labels::default());
        let labels: Vec<_> = m.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels, ["Open Tsuba", "", "Settings", "Quit"]);
        assert_eq!(m[1].kind, ItemKind::Separator);
        assert_eq!(m[3].action, Some(Msg::Quit));
    }

    #[test]
    fn profile_items_switch_that_devices_profile() {
        let m = build_menu(&[dev("028d", &[("a", "Work", true), ("b", "Games", false)]), dev("00aa", &[])], &Labels::default());
        let sub = m.iter().find(|i| i.kind == ItemKind::Submenu).expect("one submenu (devices without profiles are skipped)");
        assert_eq!(sub.children.len(), 2);
        assert_eq!(sub.children[0].kind, ItemKind::Radio(true));
        assert_eq!(sub.children[1].action, Some(Msg::Profile("028d".into(), "b".into())));
        let mut ids = vec![];
        fn collect(items: &[Item], ids: &mut Vec<i32>) {
            for i in items {
                ids.push(i.id);
                collect(&i.children, ids);
            }
        }
        collect(&m, &mut ids);
        let mut u = ids.clone();
        u.sort();
        u.dedup();
        assert_eq!(u.len(), ids.len(), "ids are unique");
        assert!(!ids.contains(&0), "0 is the root");
        assert_eq!(find(&m, sub.children[1].id).unwrap().label, "Games");
    }

    #[test]
    fn layout_serialises_with_the_dbusmenu_signature() {
        let m = build_menu(&[dev("028d", &[("a", "Work", true)])], &Labels::default());
        let root = Item { id: 0, label: String::new(), kind: ItemKind::Submenu, enabled: true, action: None, children: m };
        let v = layout_value(&root, -1);
        assert_eq!(v.value_signature().to_string(), "(ia{sv}av)");
        let r = Root { id: 0, props: item_props(&root), children: root.children.iter().map(|c| ov(layout_value(c, -1))).collect() };
        assert_eq!(Root::SIGNATURE.to_string(), "(ia{sv}av)");
        assert_eq!(r.children.len(), root.children.len());
    }

    #[test]
    fn messages_wait_for_the_dispatcher() {
        // Only this test installs the dispatcher (it is process-wide).
        send(Msg::OpenPage("settings".into()));
        let seen = std::sync::Arc::new(Mutex::new(Vec::new()));
        let s = seen.clone();
        set_dispatcher(Box::new(move |m| s.lock().unwrap().push(m)));
        send(Msg::Quit);
        let got = seen.lock().unwrap().clone();
        assert_eq!(got, vec![Msg::OpenPage("settings".into()), Msg::Quit]);
    }
}
