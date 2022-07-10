#![allow(unused)]
use std::{env::args, ffi::CString, process::exit};

use x11rb::{
    connect,
    connection::Connection,
    properties::WmClass,
    protocol::{
        xproto::{
            AtomEnum, ChangeWindowAttributesAux, ConnectionExt, EventMask, GrabMode, KeyPressEvent,
            KeyReleaseEvent,
        },
        Event,
    },
    CURRENT_TIME,
};

use std::ffi::CStr;

use x11::xlib::{
    XCloseDisplay, XKeysymToKeycode, XKeysymToString, XStringToKeysym, XkbKeycodeToKeysym,
    XkbKeysymToModifiers, XkbOpenDisplay, _XDisplay,
};

fn usage() -> &'static str {
    "mmk(mimic)
  use a different keyboard layout for a given window.

  usage:
    -h         \tprints this help message
    -w <wid>   \ttry to run on a window with the given x11 id
        default: [needs to be specified]
    -c <class> \ttry to run on a window with the given x11 window class
        default: [needs to be specified]
    -p <pid>   \ttry to run on a client with the given process id
        default: [needs to be specified]
    -n <name>  \ttry to run on a window with a given WM_NAME or _NET_WM_NAME property
  how to use:
    1. set up two layouts you want to use using setxkbmap:
        $ setxkbmap -layout dvorak,us
    2. run with the specified window
        $ mmk -w 123456
    3. if the window id is correct, the correct events will be received
"
}

struct Dpy {
    dpy: *mut _XDisplay,
}

impl Drop for Dpy {
    fn drop(&mut self) {
        unsafe { XCloseDisplay(self.dpy) };
    }
}

impl Dpy {
    fn ptr(&self) -> *mut _XDisplay {
        self.dpy
    }
    fn new(dpy: *mut _XDisplay) -> Self {
        Self { dpy }
    }
}

#[derive(Debug, Clone, Default)]
struct Config {
    help: bool,
    wid: Option<u32>,
    class: Option<String>,
    pid: Option<u32>,
    name: Option<String>,
}

impl Config {
    fn from_args(input: Vec<String>) -> Result<Self, Box<dyn std::error::Error>> {
        let mut ret = Self::default();
        let mut iter = input.iter().peekable();

        while let Some(value) = iter.next() {
            match &value[..] {
                "-h" => ret = ret.with_help(),
                "-w" => {
                    if let Some(next) = iter.peek() {
                        if !next.starts_with('-') {
                            ret = ret.with_wid(next.parse()?);
                        }
                    }
                }
                "-c" => {
                    if let Some(next) = iter.peek() {
                        if !next.starts_with('-') {
                            ret = ret.with_class(next.to_string());
                        }
                    }
                }
                "-p" => {
                    if let Some(next) = iter.peek() {
                        if !next.starts_with('-') {
                            ret = ret.with_pid(next.parse()?);
                        }
                    }
                }
                "-n" => {
                    if let Some(next) = iter.peek() {
                        if !next.starts_with('-') {
                            ret = ret.with_name(next.to_string());
                        }
                    }
                }
                "-h" => ret = ret.with_help(),
                _ => (),
            }
        }

        Ok(ret)
    }

    fn with_wid(mut self, wid: u32) -> Self {
        self.wid = Some(wid);
        self
    }
    fn with_class(mut self, class: String) -> Self {
        self.class = Some(class);
        self
    }
    fn with_pid(mut self, pid: u32) -> Self {
        self.pid = Some(pid);
        self
    }
    fn with_name(mut self, name: String) -> Self {
        self.name = Some(name);
        self
    }
    fn with_help(mut self) -> Self {
        self.help = true;
        self
    }
}

enum KeyEvent {
    Press(KeyPressEvent),
    Release(KeyReleaseEvent),
}

fn translate(dpy: *mut _XDisplay, ev: KeyEvent) -> Result<(u8, u32), Box<dyn std::error::Error>> {
    let event = match ev {
        KeyEvent::Press(e) => e,
        KeyEvent::Release(e) => e,
    };
    let layout_keysym = unsafe { XkbKeycodeToKeysym(dpy, event.detail, 1, event.state as _) };
    unsafe {
        let ptr = XKeysymToString(layout_keysym);
        if !ptr.is_null() {
            // println!("{}", CStr::from_ptr(ptr).to_str()?)
        }
    };

    let ret = unsafe {
        (
            XKeysymToKeycode(dpy, layout_keysym),
            XkbKeysymToModifiers(dpy, layout_keysym),
        )
    };

    if ret.0 >= 204 || ret.1 >= 204 {
        return Ok((event.detail, event.state as _));
    }

    Ok(ret)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = args().collect();
    // parse command line args
    let config = Config::from_args(args)?;
    let mut window: Option<u32> = None;
    let (conn, screen) = connect(None)?;
    let setup = &conn.setup();
    let screen = &setup.roots[screen];
    let root = screen.root;
    let dpy = unsafe {
        XkbOpenDisplay(
            std::ptr::null_mut() as _,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            1 as _,
            0 as _,
            std::ptr::null_mut(),
        )
    };
    let dpy = Dpy::new(dpy);
    assert!(!dpy.ptr().is_null());

    // atoms
    let net_wm_pid = conn.intern_atom(false, b"_NET_WM_PID")?.reply()?.atom;
    let net_wm_name = conn.intern_atom(false, b"_NET_WM_NAME")?.reply()?.atom;

    config.help.then(|| {
        print!("{}", usage());
        exit(0)
    });

    let clients = conn.query_tree(root)?.reply()?.children;

    // try to get the x11 window id
    if let Some(wid) = config.wid {
        window = Some(wid);
    }

    // check for class
    if let Some(class) = config.class {
        for client in clients.iter() {
            let class_reply = conn
                .get_property(
                    false,
                    *client,
                    AtomEnum::WM_CLASS,
                    AtomEnum::STRING,
                    0,
                    1024,
                )?
                .reply()?;
            let class_string = String::from_utf8(class_reply.value.to_vec())?;
            if class == class_string {
                window = Some(*client);
                break;
            }
        }
    }

    // check for pid
    if let Some(pid) = config.pid {
        for client in clients.iter() {
            let pid_reply = conn
                .get_property(false, *client, net_wm_pid, AtomEnum::CARDINAL, 0, 4)?
                .reply()?;
            let client_pid = pid_reply
                .value32()
                .map(|iter| iter.collect::<Vec<u32>>())
                .unwrap_or_else(|| vec![0])[0];
            if client_pid == pid {
                window = Some(*client);
                break;
            }
        }
    }

    // check for window name
    if let Some(name) = config.name {
        for client in clients.iter() {
            let client_name_reply = conn
                .get_property(false, *client, net_wm_name, AtomEnum::STRING, 0, 1024)?
                .reply()?;
            let client_name = String::from_utf8(client_name_reply.value)?;
            if client_name == name {
                window = Some(*client);
                break;
            }
        }
    }

    let mut mask = 0u32;

    if let Some(window) = window {
        let m = conn.get_window_attributes(window)?.reply()?.your_event_mask;
        conn.change_window_attributes(
            window,
            &ChangeWindowAttributesAux::new().event_mask(Some(
                (m | EventMask::KEY_PRESS | EventMask::KEY_RELEASE).into(),
            )),
        )?;
        conn.grab_key(true, window, 32768u16, 0, GrabMode::ASYNC, GrabMode::ASYNC)?;
        conn.flush()?;
        mask = conn.get_window_attributes(window)?.reply()?.your_event_mask;
    } else {
        return Ok(());
    }

    loop {
        let event = conn.wait_for_event()?;
        let mut event_opt = Some(event);
        while let Some(event) = event_opt {
            match event {
                Event::KeyPress(mut e) => {
                    if !event.sent_event() {
                        let (detail, state) = translate(dpy.ptr(), KeyEvent::Press(e))?;
                        e.detail = detail;
                        e.state = state as _;
                        conn.send_event(false, window.unwrap(), mask, e)?;
                        conn.flush()?;
                    }
                }
                Event::KeyRelease(mut e) => {
                    if !event.sent_event() {
                        let (detail, state) = translate(dpy.ptr(), KeyEvent::Release(e))?;
                        e.detail = detail;
                        e.state = state as _;
                        conn.send_event(false, window.unwrap(), mask, e)?;
                        conn.flush()?;
                    }
                }
                _ => (),
            };
            event_opt = conn.poll_for_event()?;
        }
    }

    Ok(())
}
