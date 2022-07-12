use std::{collections::HashMap, env::args, process::exit};

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

use x11::xlib::{
    XCloseDisplay, XKeysymToKeycode, XkbKeycodeToKeysym, XkbKeysymToModifiers, XkbOpenDisplay,
    _XDisplay,
};

fn usage() -> &'static str {
    "mmk(mimic)
  use a different keyboard layout for a given window.

  options:
    -h | --help                    \tprints this help message
    -l | --layout                  \tspecify which layout to use, starts from 0
        default: 0, meaning use the current layout
    -w | --window <wid>            \ttry to run on a window with the given x11 id
        default: [needs to be specified]
    -c | --class <class>.<instance>\ttry to run on a window with the given x11 window class and instance
        default: [needs to be specified]
    -p | --pid <pid>               \ttry to run on a client with the given process id
        default: [needs to be specified]
    -n | --name <name>             \ttry to run on a window with a given WM_NAME or _NET_WM_NAME property
    -a | --all                     \ttry to run on all windows matching the specified criteria
  how to use:
    1. set up two layouts you want to use using setxkbmap:
        $ setxkbmap -layout dvorak,us
    2. run with something specified
        $ mmk --layout 1 --name MyWindow
    3. the window should now receive the mimiced layout keys
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
    all_windows: bool,
    layout: usize,
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
                "-w" | "--window" => {
                    if let Some(next) = iter.peek() {
                        if !next.starts_with('-') {
                            ret = ret.with_wid(next.parse()?);
                        }
                    }
                }
                "-c" | "--class" => {
                    if let Some(next) = iter.peek() {
                        if !next.starts_with('-') {
                            ret = ret.with_class(next.to_string());
                        }
                    }
                }
                "-p" | "--pid" => {
                    if let Some(next) = iter.peek() {
                        if !next.starts_with('-') {
                            ret = ret.with_pid(next.parse()?);
                        }
                    }
                }
                "-n" | "--name" => {
                    if let Some(next) = iter.peek() {
                        if !next.starts_with('-') {
                            ret = ret.with_name(next.to_string());
                        }
                    }
                }
                "-h" | "--help" => ret = ret.with_help(),
                "-l" | "--layout" => {
                    if let Some(next) = iter.peek() {
                        if !next.starts_with('-') {
                            ret = ret.with_layout(next.parse()?)
                        }
                    }
                }
                "-a" | "--all" => ret = ret.with_all_windows(),
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
    fn with_layout(mut self, layout: usize) -> Self {
        self.layout = layout;
        self
    }
    fn with_all_windows(mut self) -> Self {
        self.all_windows = true;
        self
    }
}

enum KeyEvent {
    Press(KeyPressEvent),
    Release(KeyReleaseEvent),
}

fn translate(
    dpy: *mut _XDisplay,
    ev: KeyEvent,
    layout_index: usize,
) -> Result<(u8, u32), Box<dyn std::error::Error>> {
    let event = match ev {
        KeyEvent::Press(e) => e,
        KeyEvent::Release(e) => e,
    };
    let layout_keysym =
        unsafe { XkbKeycodeToKeysym(dpy, event.detail, layout_index as _, event.state as _) };

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

fn rec_query_tree(
    conn: &impl Connection,
    win: u32,
    vec: &mut Vec<u32>,
) -> Result<(), Box<dyn std::error::Error>> {
    if !vec.contains(&win) {
        let reply = conn.query_tree(win)?.reply()?;
        if !reply.children.is_empty() {
            for child in reply.children.iter() {
                rec_query_tree(conn, *child, vec)?;
                vec.push(*child);
            }
        }
    }

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = args().collect();
    // parse command line args
    let config = Config::from_args(args)?;
    let mut windows: Vec<u32> = vec![];
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

    let mut clients = Vec::new();
    rec_query_tree(&conn, root, &mut clients)?;

    // try to get the x11 window id
    if let Some(wid) = config.wid {
        windows.push(wid)
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
                    2048,
                )?
                .reply()?;
            if class_reply.format != 8 || class_reply.type_ != AtomEnum::STRING.into() {
                continue;
            }

            let class_struct = WmClass::from_reply(class_reply)?;

            let class_string = String::from_utf8(class_struct.class().to_vec())?;
            let instance_string = String::from_utf8(class_struct.instance().to_vec())?;
            let class_string = format!("{class_string}.{instance_string}");
            if class == class_string {
                windows.push(*client);
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
                windows.push(*client);
            }
        }
    }

    // check for window name
    if let Some(name) = config.name {
        for client in clients.iter() {
            let client_net_name_reply = conn
                .get_property(false, *client, net_wm_name, AtomEnum::STRING, 0, 1024)?
                .reply()?;
            let client_net_name = String::from_utf8(client_net_name_reply.value)?;

            let client_name_reply = conn
                .get_property(false, *client, AtomEnum::WM_NAME, AtomEnum::STRING, 0, 1024)?
                .reply()?;
            let client_name = String::from_utf8(client_name_reply.value)?;
            if client_net_name == name || client_name == name {
                windows.push(*client);
            }
        }
    }

    let mut masks: HashMap<u32, u32> = HashMap::new();

    if !windows.is_empty() {
        let mut wins = vec![windows[0]];
        if config.all_windows {
            wins = windows.clone()
        }
        for window in wins {
            let m = conn.get_window_attributes(window)?.reply()?.your_event_mask;
            conn.change_window_attributes(
                window,
                &ChangeWindowAttributesAux::new().event_mask(Some(
                    (m | EventMask::KEY_PRESS | EventMask::KEY_RELEASE).into(),
                )),
            )?;
            conn.grab_key(false, window, 32768u16, 0, GrabMode::ASYNC, GrabMode::ASYNC)?;

            conn.flush()?;
            masks.insert(
                window,
                conn.get_window_attributes(window)?.reply()?.your_event_mask,
            );
        }
    } else {
        eprintln!("error: No window for the given specifications found.");
        exit(1);
    }

    loop {
        let event = conn.wait_for_event()?;
        let mut event_opt = Some(event);
        while let Some(event) = event_opt {
            match event {
                Event::KeyPress(mut e) => {
                    if !event.sent_event() {
                        let (detail, state) =
                            translate(dpy.ptr(), KeyEvent::Press(e), config.layout)?;
                        e.detail = detail;
                        e.state = state as _;
                        e.time = CURRENT_TIME;
                        conn.send_event(true, e.event, masks[&e.event], e)?;
                        conn.flush()?;
                    }
                }
                Event::KeyRelease(mut e) => {
                    if !event.sent_event() {
                        let (detail, state) =
                            translate(dpy.ptr(), KeyEvent::Release(e), config.layout)?;
                        e.detail = detail;
                        e.state = state as _;
                        e.time = CURRENT_TIME;
                        conn.send_event(true, e.event, masks[&e.event], e)?;
                        conn.flush()?;
                    }
                }
                _ => (),
            };
            event_opt = conn.poll_for_event()?;
        }
    }
}
