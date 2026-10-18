//! Ambient Awareness: mirror the screen onto the lighting. Uses the desktop's screen-cast
//! portal (KDE asks once; the grant is remembered with a restore token) and a PipeWire video
//! stream requested at a tiny size, reduced to a coarse colour grid for AmbientLayers.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::{bail, ensure, Context, Result};
use pipewire as pw;
use pw::spa;
use tracing::{info, warn};
use tsuba_core::studio::AmbientFrame;
use zbus::blocking::{Connection, MessageIterator};
use zbus::message::Type;
use zbus::zvariant::{ObjectPath, OwnedValue, Value};
use zbus::MatchRule;

const DEST: &str = "org.freedesktop.portal.Desktop";
const PATH: &str = "/org/freedesktop/portal/desktop";
const IFACE: &str = "org.freedesktop.portal.ScreenCast";
pub const GRID_W: usize = 32;
pub const GRID_H: usize = 18;

fn token_path() -> std::path::PathBuf {
    dirs::config_dir().unwrap_or_default().join("tsuba").join("ambient-token")
}

/// Call a portal method that answers through a Request object's Response signal.
fn request<B>(conn: &Connection, method: &str, body: &B, token: &str) -> Result<HashMap<String, OwnedValue>>
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
{
    let sender = conn.unique_name().context("no bus name")?.trim_start_matches(':').replace('.', "_");
    let path = format!("/org/freedesktop/portal/desktop/request/{sender}/{token}");
    let rule = MatchRule::builder().msg_type(Type::Signal).interface("org.freedesktop.portal.Request")?.member("Response")?.path(path.as_str())?.build();
    let responses = MessageIterator::for_match_rule(rule, conn, Some(4))?;
    conn.call_method(Some(DEST), PATH, Some(IFACE), method, body)?;
    if let Some(msg) = responses.into_iter().next() {
        let (code, results): (u32, HashMap<String, OwnedValue>) = msg?.body().deserialize()?;
        ensure!(code == 0, "screen sharing was cancelled or refused");
        return Ok(results);
    }
    bail!("no answer from the screen-cast portal")
}

fn opts<'a>(pairs: &[(&'a str, Value<'a>)]) -> HashMap<&'a str, Value<'a>> {
    pairs.iter().map(|(k, v)| (*k, v.try_clone().unwrap())).collect()
}

/// Ask the portal for the screen; returns (PipeWire fd, stream node id).
fn open_screen() -> Result<(std::os::fd::OwnedFd, u32, Connection)> {
    let conn = Connection::session()?;
    let r = request(&conn, "CreateSession", &(opts(&[("handle_token", Value::from("tsuba1")), ("session_handle_token", Value::from("tsubaambient"))]),), "tsuba1")?;
    let session: String = r.get("session_handle").context("no session")?.try_clone()?.try_into()?;
    let session = ObjectPath::try_from(session.as_str())?.into_owned();
    let saved = std::fs::read_to_string(token_path()).unwrap_or_default();
    let mut select = vec![
        ("handle_token", Value::from("tsuba2")),
        ("types", Value::from(1u32)),       // monitor
        ("multiple", Value::from(false)),
        ("cursor_mode", Value::from(1u32)), // hidden
        ("persist_mode", Value::from(2u32)), // until revoked
    ];
    if !saved.trim().is_empty() {
        select.push(("restore_token", Value::from(saved.trim().to_string())));
    }
    request(&conn, "SelectSources", &(&session, opts(&select)), "tsuba2")?;
    let r = request(&conn, "Start", &(&session, "", opts(&[("handle_token", Value::from("tsuba3"))])), "tsuba3")?;
    if let Some(t) = r.get("restore_token") {
        let t: String = t.try_clone()?.try_into()?;
        let _ = std::fs::write(token_path(), t);
    }
    let streams: Vec<(u32, HashMap<String, OwnedValue>)> = r.get("streams").context("no stream")?.try_clone()?.try_into()?;
    let node = streams.first().context("no screen selected")?.0;
    let reply = conn.call_method(Some(DEST), PATH, Some(IFACE), "OpenPipeWireRemote", &(&session, HashMap::<&str, Value>::new()))?;
    let fd: zbus::zvariant::OwnedFd = reply.body().deserialize()?;
    Ok((fd.into(), node, conn))
}

pub struct AmbientCapture {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    pub frame: Arc<Mutex<AmbientFrame>>,
}

impl Drop for AmbientCapture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

pub fn start() -> Result<AmbientCapture> {
    let (fd, node, conn) = open_screen()?;
    let frame = Arc::new(Mutex::new(AmbientFrame { w: GRID_W, h: GRID_H, px: vec![[0; 3]; GRID_W * GRID_H] }));
    let stop = Arc::new(AtomicBool::new(false));
    let (out, flag) = (frame.clone(), stop.clone());
    let thread = std::thread::Builder::new().name("ambient".into()).spawn(move || {
        let _portal = conn; // keep the portal session alive with the stream
        if let Err(e) = run_stream(fd, node, out, flag) {
            warn!("ambient: {e:#}");
        }
    })?;
    info!("ambient: screen capture running (node {node})");
    Ok(AmbientCapture { stop, thread: Some(thread), frame })
}

struct UserData {
    format: spa::param::video::VideoInfoRaw,
    out: Arc<Mutex<AmbientFrame>>,
    frames: u64,
}

fn run_stream(fd: std::os::fd::OwnedFd, node: u32, out: Arc<Mutex<AmbientFrame>>, stop: Arc<AtomicBool>) -> Result<()> {
    pw::init();
    let mainloop = pw::main_loop::MainLoopRc::new(None)?;
    let context = pw::context::ContextRc::new(&mainloop, None)?;
    let core = context.connect_fd_rc(fd, None)?;
    let stream = pw::stream::StreamBox::new(
        &core,
        "tsuba-ambient",
        pw::properties::properties! { *pw::keys::MEDIA_TYPE => "Video", *pw::keys::MEDIA_CATEGORY => "Capture", *pw::keys::MEDIA_ROLE => "Screen" },
    )?;
    let _listener = stream
        .add_local_listener_with_user_data(UserData { format: Default::default(), out, frames: 0 })
        .param_changed(|_, ud, id, param| {
            let Some(param) = param else { return };
            if id == spa::param::ParamType::Format.as_raw() {
                let _ = ud.format.parse(param);
                info!("ambient: stream format {:?} {}x{}", ud.format.format(), ud.format.size().width, ud.format.size().height);
            }
        })
        .process(|stream, ud| {
            let Some(mut buffer) = stream.dequeue_buffer() else { return };
            let (w, h) = (ud.format.size().width as usize, ud.format.size().height as usize);
            let fmt = ud.format.format();
            let datas = buffer.datas_mut();
            let Some(d) = datas.first_mut() else { return };
            ud.frames += 1;
            let stride = d.chunk().stride().max(0) as usize;
            let offset = d.chunk().offset() as usize;
            if ud.frames == 1 || ud.frames % 200 == 0 {
                info!("ambient: frame {} type {:?} chunk size {} stride {stride} mapped {}", ud.frames, d.type_(), d.chunk().size(), d.as_raw().data.is_null() == false);
            }
            let Some(bytes) = d.data() else { return };
            if w == 0 || h == 0 || stride < w * 4 || bytes.len() < offset + stride * h {
                if ud.frames == 1 {
                    warn!("ambient: frame doesn't fit ({}x{} stride {stride} offset {offset}, {} bytes mapped)", w, h, bytes.len());
                }
                return;
            }
            let bgr = matches!(fmt, spa::param::video::VideoFormat::BGRx | spa::param::video::VideoFormat::BGRA);
            let px = &bytes[offset..];
            let mut grid = vec![[0u8; 3]; GRID_W * GRID_H];
            for gy in 0..GRID_H {
                for gx in 0..GRID_W {
                    // Average a few samples inside each grid cell.
                    let (mut r, mut g, mut b, mut n) = (0u32, 0u32, 0u32, 0u32);
                    for sy in 0..3 {
                        for sx in 0..3 {
                            let x = ((gx * 3 + sx) * w / (GRID_W * 3)).min(w - 1);
                            let y = ((gy * 3 + sy) * h / (GRID_H * 3)).min(h - 1);
                            let p = &px[y * stride + x * 4..y * stride + x * 4 + 3];
                            let (pr, pg, pb) = if bgr { (p[2], p[1], p[0]) } else { (p[0], p[1], p[2]) };
                            r += pr as u32;
                            g += pg as u32;
                            b += pb as u32;
                            n += 1;
                        }
                    }
                    grid[gy * GRID_W + gx] = [(r / n) as u8, (g / n) as u8, (b / n) as u8];
                }
            }
            if ud.frames == 1 || ud.frames % 200 == 0 {
                let avg = grid.iter().fold([0u32; 3], |a, p| [a[0] + p[0] as u32, a[1] + p[1] as u32, a[2] + p[2] as u32]).map(|v| v / grid.len() as u32);
                info!("ambient: average screen colour {avg:?}");
            }
            if let Ok(mut f) = ud.out.lock() {
                f.px = grid;
            }
        })
        .register()?;

    let obj = spa::pod::object!(
        spa::utils::SpaTypes::ObjectParamFormat,
        spa::param::ParamType::EnumFormat,
        spa::pod::property!(spa::param::format::FormatProperties::MediaType, Id, spa::param::format::MediaType::Video),
        spa::pod::property!(spa::param::format::FormatProperties::MediaSubtype, Id, spa::param::format::MediaSubtype::Raw),
        spa::pod::property!(
            spa::param::format::FormatProperties::VideoFormat,
            Choice,
            Enum,
            Id,
            spa::param::video::VideoFormat::BGRx,
            spa::param::video::VideoFormat::BGRx,
            spa::param::video::VideoFormat::BGRA,
            spa::param::video::VideoFormat::RGBx,
            spa::param::video::VideoFormat::RGBA
        ),
        spa::pod::property!(
            spa::param::format::FormatProperties::VideoSize,
            Choice,
            Range,
            Rectangle,
            spa::utils::Rectangle { width: 64, height: 36 },
            spa::utils::Rectangle { width: 1, height: 1 },
            spa::utils::Rectangle { width: 8192, height: 8192 }
        ),
        // Screen casts use a variable rate (0/1); cap it with the max framerate, as OBS does.
        spa::pod::property!(
            spa::param::format::FormatProperties::VideoFramerate,
            Choice,
            Range,
            Fraction,
            spa::utils::Fraction { num: 0, denom: 1 },
            spa::utils::Fraction { num: 0, denom: 1 },
            spa::utils::Fraction { num: 1000, denom: 1 }
        ),
        spa::pod::property!(
            spa::param::format::FormatProperties::VideoMaxFramerate,
            Choice,
            Range,
            Fraction,
            spa::utils::Fraction { num: 15, denom: 1 },
            spa::utils::Fraction { num: 1, denom: 1 },
            spa::utils::Fraction { num: 15, denom: 1 }
        ),
    );
    let values: Vec<u8> = spa::pod::serialize::PodSerializer::serialize(std::io::Cursor::new(Vec::new()), &spa::pod::Value::Object(obj))
        .map_err(|e| anyhow::anyhow!("pod: {e:?}"))?
        .0
        .into_inner();
    let mut params = [spa::pod::Pod::from_bytes(&values).context("pod")?];
    stream.connect(spa::utils::Direction::Input, Some(node), pw::stream::StreamFlags::AUTOCONNECT | pw::stream::StreamFlags::MAP_BUFFERS, &mut params)?;

    // Quit when the owner drops us.
    let weak = mainloop.downgrade();
    let timer = mainloop.loop_().add_timer(move |_| {
        if stop.load(Ordering::Relaxed) {
            if let Some(m) = weak.upgrade() {
                m.quit();
            }
        }
    });
    timer.update_timer(Some(Duration::from_millis(200)), Some(Duration::from_millis(200))).into_result()?;
    mainloop.run();
    Ok(())
}

/// One capture shared by every device that needs it: the first caller starts it, later
/// callers reuse it, and it stops when the last holder drops its handle.
static SHARED: Mutex<Weak<AmbientCapture>> = Mutex::new(Weak::new());

pub fn shared() -> Result<Arc<AmbientCapture>> {
    let mut slot = SHARED.lock().unwrap();
    if let Some(c) = slot.upgrade() {
        return Ok(c);
    }
    let c = Arc::new(start()?);
    *slot = Arc::downgrade(&c);
    Ok(c)
}

/// The live coarse screen grid as "WxH:rrggbb…", or "" when nothing is capturing.
pub fn preview() -> String {
    let Some(c) = SHARED.lock().unwrap().upgrade() else { return String::new() };
    let f = c.frame.lock().unwrap();
    let mut out = format!("{}x{}:", f.w, f.h);
    for p in &f.px {
        out.push_str(&format!("{:02x}{:02x}{:02x}", p[0], p[1], p[2]));
    }
    out
}
