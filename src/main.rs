//! Awake - Ultra-lightweight macOS menu bar app to prevent sleep
//! Uses IOKit power assertions directly (no child processes)

use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, ClassBuilder, Sel};
use objc2::{msg_send, sel, ClassType, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSImage, NSMenu, NSMenuItem, NSStatusBar,
};
use objc2_foundation::NSString;

use core_foundation::base::TCFType;
use core_foundation::string::CFString;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

// IOKit power management bindings
#[link(name = "IOKit", kind = "framework")]
extern "C" {
    fn IOPMAssertionCreateWithName(
        assertion_type: core_foundation::string::CFStringRef,
        level: u32,
        name: core_foundation::string::CFStringRef,
        assertion_id: *mut u32,
    ) -> i32;
    fn IOPMAssertionRelease(assertion_id: u32) -> i32;
}

const IOPM_ASSERTION_LEVEL_ON: u32 = 255;
const LAUNCH_AGENT_LABEL: &str = "io.tmss.awake";

// Sleep prevention modes
const MODE_DISPLAY: u8 = 0;
const MODE_SYSTEM: u8 = 1;
const MODE_BOTH: u8 = 2;

// Global state
static ASSERTION_ID: AtomicU32 = AtomicU32::new(0);
static ASSERTION_ID_2: AtomicU32 = AtomicU32::new(0);
static TIMER_EXPIRY: AtomicU64 = AtomicU64::new(0);
static TIMER_CANCEL: Mutex<Option<Arc<AtomicBool>>> = Mutex::new(None);
static TIMER_THREAD: Mutex<Option<thread::JoinHandle<()>>> = Mutex::new(None);
static CURRENT_MODE: AtomicU8 = AtomicU8::new(MODE_BOTH);

// Wrapper for raw pointers to ObjC objects so they can be in statics
struct RawId(*mut AnyObject);
unsafe impl Send for RawId {}
unsafe impl Sync for RawId {}

static STATUS_ITEM: Mutex<RawId> = Mutex::new(RawId(std::ptr::null_mut()));
static STATUS_MENU: Mutex<RawId> = Mutex::new(RawId(std::ptr::null_mut()));
static LOGIN_ITEM: Mutex<RawId> = Mutex::new(RawId(std::ptr::null_mut()));
static MODE_ITEMS: Mutex<[RawId; 3]> = Mutex::new([
    RawId(std::ptr::null_mut()),
    RawId(std::ptr::null_mut()),
    RawId(std::ptr::null_mut()),
]);

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn is_awake() -> bool {
    ASSERTION_ID.load(Ordering::Relaxed) != 0
}

fn create_assertion(assertion_type: &str) -> u32 {
    let atype = CFString::new(assertion_type);
    let aname = CFString::new("Awake App");
    let mut aid: u32 = 0;

    let result = unsafe {
        IOPMAssertionCreateWithName(
            atype.as_concrete_TypeRef(),
            IOPM_ASSERTION_LEVEL_ON,
            aname.as_concrete_TypeRef(),
            &mut aid,
        )
    };

    if result == 0 { aid } else { 0 }
}

fn release_assertion(id: &AtomicU32) {
    let aid = id.swap(0, Ordering::Relaxed);
    if aid != 0 {
        unsafe { IOPMAssertionRelease(aid) };
    }
}

fn activate() {
    if is_awake() {
        return;
    }

    let mode = CURRENT_MODE.load(Ordering::Relaxed);

    match mode {
        MODE_DISPLAY => {
            let aid = create_assertion("PreventUserIdleDisplaySleep");
            if aid != 0 {
                ASSERTION_ID.store(aid, Ordering::Relaxed);
            }
        }
        MODE_SYSTEM => {
            let aid = create_assertion("PreventUserIdleSystemSleep");
            if aid != 0 {
                ASSERTION_ID.store(aid, Ordering::Relaxed);
            }
        }
        MODE_BOTH | _ => {
            let aid1 = create_assertion("PreventUserIdleDisplaySleep");
            let aid2 = create_assertion("PreventUserIdleSystemSleep");
            if aid1 != 0 {
                ASSERTION_ID.store(aid1, Ordering::Relaxed);
            }
            if aid2 != 0 {
                ASSERTION_ID_2.store(aid2, Ordering::Relaxed);
            }
        }
    }

    if is_awake() {
        update_icon("cup.and.saucer.fill");
    }
}

fn deactivate() {
    TIMER_EXPIRY.store(0, Ordering::Relaxed);
    release_assertion(&ASSERTION_ID);
    release_assertion(&ASSERTION_ID_2);
    update_icon("moon.zzz.fill");
}

fn toggle() {
    if is_awake() {
        deactivate();
    } else {
        activate();
    }
}

fn set_mode(mode: u8) {
    let was_awake = is_awake();
    if was_awake {
        deactivate();
    }

    CURRENT_MODE.store(mode, Ordering::Relaxed);
    update_mode_menu_state();

    if was_awake {
        activate();
    }
}

fn update_mode_menu_state() {
    let current = CURRENT_MODE.load(Ordering::Relaxed);
    let items = MODE_ITEMS.lock().unwrap();
    for (i, item) in items.iter().enumerate() {
        if !item.0.is_null() {
            let state: isize = if i as u8 == current { 1 } else { 0 };
            unsafe {
                let _: () = msg_send![item.0, setState: state];
            }
        }
    }
}

fn cancel_timer() {
    if let Some(cancel) = TIMER_CANCEL.lock().unwrap().take() {
        cancel.store(true, Ordering::Relaxed);
    }
    // Take and drop the old handle (don't join — it may be sleeping for a long time)
    TIMER_THREAD.lock().unwrap().take();
}

fn activate_for_duration(minutes: u64) {
    deactivate();
    activate();

    if !is_awake() {
        return;
    }

    cancel_timer();

    let expiry = now_secs() + (minutes * 60);
    TIMER_EXPIRY.store(expiry, Ordering::Relaxed);

    let cancelled = Arc::new(AtomicBool::new(false));
    *TIMER_CANCEL.lock().unwrap() = Some(Arc::clone(&cancelled));

    let handle = thread::spawn(move || {
        // Sleep in short intervals so we can check for cancellation
        let total_secs = minutes * 60;
        let mut elapsed = 0u64;
        while elapsed < total_secs {
            let chunk = (total_secs - elapsed).min(1);
            thread::sleep(Duration::from_secs(chunk));
            elapsed += chunk;
            if cancelled.load(Ordering::Relaxed) {
                return;
            }
        }
        if TIMER_EXPIRY.load(Ordering::Relaxed) == expiry {
            deactivate();
        }
    });

    *TIMER_THREAD.lock().unwrap() = Some(handle);
}

fn update_icon(symbol_name: &str) {
    let guard = STATUS_ITEM.lock().unwrap();
    let si = guard.0;
    if !si.is_null() {
        unsafe {
            let button: *mut AnyObject = msg_send![si, button];
            if !button.is_null() {
                let name = NSString::from_str(symbol_name);
                let desc: Option<&NSString> = None;
                let img: Option<Retained<NSImage>> =
                    msg_send![NSImage::class(), imageWithSystemSymbolName: &*name, accessibilityDescription: desc];
                if let Some(img) = img {
                    let _: () = msg_send![&*img, setTemplate: true];
                    let _: () = msg_send![button, setImage: &*img];
                }
            }
        }
    }
}

// Launch at login
fn launch_agent_path() -> PathBuf {
    let home = env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home)
        .join("Library/LaunchAgents")
        .join(format!("{}.plist", LAUNCH_AGENT_LABEL))
}

fn is_launch_at_login() -> bool {
    launch_agent_path().exists()
}

fn get_app_path() -> String {
    env::current_exe()
        .ok()
        .and_then(|p| p.to_str().map(String::from))
        .unwrap_or_default()
}

fn set_launch_at_login(enable: bool) {
    let path = launch_agent_path();

    if enable {
        let app_path = get_app_path();
        if app_path.is_empty() {
            return;
        }

        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let plist = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
</dict>
</plist>
"#,
            LAUNCH_AGENT_LABEL, app_path
        );

        let _ = fs::write(&path, plist);
    } else {
        let _ = fs::remove_file(&path);
    }

    update_login_item_state();
}

fn toggle_launch_at_login() {
    set_launch_at_login(!is_launch_at_login());
}

fn update_login_item_state() {
    let guard = LOGIN_ITEM.lock().unwrap();
    let item = guard.0;
    if !item.is_null() {
        let state: isize = if is_launch_at_login() { 1 } else { 0 };
        unsafe {
            let _: () = msg_send![item, setState: state];
        }
    }
}

// Action handlers
extern "C" fn toggle_action(_this: *mut AnyObject, _cmd: Sel, _sender: *mut AnyObject) {
    toggle();
}

extern "C" fn login_action(_this: *mut AnyObject, _cmd: Sel, _sender: *mut AnyObject) {
    toggle_launch_at_login();
}

extern "C" fn timer_15_action(_this: *mut AnyObject, _cmd: Sel, _sender: *mut AnyObject) {
    activate_for_duration(15);
}

extern "C" fn timer_30_action(_this: *mut AnyObject, _cmd: Sel, _sender: *mut AnyObject) {
    activate_for_duration(30);
}

extern "C" fn timer_60_action(_this: *mut AnyObject, _cmd: Sel, _sender: *mut AnyObject) {
    activate_for_duration(60);
}

extern "C" fn timer_120_action(_this: *mut AnyObject, _cmd: Sel, _sender: *mut AnyObject) {
    activate_for_duration(120);
}

extern "C" fn mode_display_action(_this: *mut AnyObject, _cmd: Sel, _sender: *mut AnyObject) {
    set_mode(MODE_DISPLAY);
}

extern "C" fn mode_system_action(_this: *mut AnyObject, _cmd: Sel, _sender: *mut AnyObject) {
    set_mode(MODE_SYSTEM);
}

extern "C" fn mode_both_action(_this: *mut AnyObject, _cmd: Sel, _sender: *mut AnyObject) {
    set_mode(MODE_BOTH);
}

extern "C" fn button_clicked(_this: *mut AnyObject, _cmd: Sel, _sender: *mut AnyObject) {
    unsafe {
        let mtm = MainThreadMarker::new_unchecked();
        let app = NSApplication::sharedApplication(mtm);
        let event: *mut AnyObject = msg_send![&app, currentEvent];
        if !event.is_null() {
            let event_type: u64 = msg_send![event, type];
            let modifier_flags: u64 = msg_send![event, modifierFlags];
            // Right mouse down (3) or right mouse up (4), or control+left click
            let is_right_click = event_type == 3
                || event_type == 4
                || (modifier_flags & 0x40000) != 0;
            if is_right_click {
                let status_item_ptr = STATUS_ITEM.lock().unwrap().0;
                let menu_ptr = STATUS_MENU.lock().unwrap().0;
                if !status_item_ptr.is_null() && !menu_ptr.is_null() {
                    let _: () = msg_send![status_item_ptr, setMenu: menu_ptr];
                    let button: *mut AnyObject = msg_send![status_item_ptr, button];
                    let _: () = msg_send![button, performClick: std::ptr::null::<AnyObject>()];
                    let _: () = msg_send![status_item_ptr, setMenu: std::ptr::null::<AnyObject>()];
                }
                return;
            }
        }
    }
    toggle();
}

extern "C" fn quit_action(_this: *mut AnyObject, _cmd: Sel, _sender: *mut AnyObject) {
    deactivate();
    cancel_timer();
    if let Some(handle) = TIMER_THREAD.lock().unwrap().take() {
        let _ = handle.join();
    }
    unsafe {
        let mtm = MainThreadMarker::new_unchecked();
        let app = NSApplication::sharedApplication(mtm);
        let _: () = msg_send![&app, terminate: std::ptr::null::<AnyObject>()];
    }
}

fn register_delegate_class() -> &'static AnyClass {
    static REGISTER: std::sync::Once = std::sync::Once::new();
    let mut cls_ptr: Option<&'static AnyClass> = None;

    REGISTER.call_once(|| {
        let superclass = objc2::class!(NSObject);
        let mut builder = ClassBuilder::new(c"AwakeDelegate", superclass).unwrap();

        type Fn3 = extern "C" fn(*mut AnyObject, Sel, *mut AnyObject);

        unsafe {
            builder.add_method(sel!(toggle:), toggle_action as Fn3);
            builder.add_method(sel!(toggleLogin:), login_action as Fn3);
            builder.add_method(sel!(timer15:), timer_15_action as Fn3);
            builder.add_method(sel!(timer30:), timer_30_action as Fn3);
            builder.add_method(sel!(timer60:), timer_60_action as Fn3);
            builder.add_method(sel!(timer120:), timer_120_action as Fn3);
            builder.add_method(sel!(modeDisplay:), mode_display_action as Fn3);
            builder.add_method(sel!(modeSystem:), mode_system_action as Fn3);
            builder.add_method(sel!(modeBoth:), mode_both_action as Fn3);
            builder.add_method(sel!(quit:), quit_action as Fn3);
            builder.add_method(sel!(buttonClicked:), button_clicked as Fn3);
        }

        cls_ptr = Some(builder.register());
    });

    if let Some(c) = cls_ptr {
        c
    } else {
        AnyClass::get(c"AwakeDelegate").unwrap()
    }
}

fn create_menu_item(
    title: &str,
    action: Sel,
    delegate: *mut AnyObject,
    mtm: MainThreadMarker,
) -> Retained<NSMenuItem> {
    unsafe {
        let title_str = NSString::from_str(title);
        let empty = NSString::from_str("");
        let item = NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &title_str,
            Some(action),
            &empty,
        );
        let _: () = msg_send![&item, setTarget: delegate];
        item
    }
}

fn main() {
    let mtm = MainThreadMarker::new().expect("must run on main thread");

    unsafe {
        let app = NSApplication::sharedApplication(mtm);
        app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

        let delegate_class = register_delegate_class();
        let delegate: *mut AnyObject = msg_send![delegate_class, new];

        let status_bar = NSStatusBar::systemStatusBar();
        let status_item = status_bar.statusItemWithLength(-1.0); // NSVariableStatusItemLength

        // Set initial icon
        {
            let button: *mut AnyObject = msg_send![&status_item, button];
            if !button.is_null() {
                let name = NSString::from_str("moon.zzz.fill");
                let desc: Option<&NSString> = None;
                let img: Option<Retained<NSImage>> =
                    msg_send![NSImage::class(), imageWithSystemSymbolName: &*name, accessibilityDescription: desc];
                if let Some(img) = img {
                    let _: () = msg_send![&*img, setTemplate: true];
                    let _: () = msg_send![button, setImage: &*img];
                }
            }
        }

        STATUS_ITEM.lock().unwrap().0 = Retained::as_ptr(&status_item) as *mut _;

        let menu = NSMenu::new(mtm);

        // Toggle
        let toggle_item = create_menu_item("Toggle", sel!(toggle:), delegate, mtm);
        menu.addItem(&toggle_item);

        // Separator
        let sep = NSMenuItem::separatorItem(mtm);
        menu.addItem(&sep);

        // Timer submenu
        let timer_title = NSString::from_str("Awake For...");
        let empty = NSString::from_str("");
        let timer_menu_item = NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &timer_title,
            None,
            &empty,
        );
        let timer_submenu = NSMenu::new(mtm);
        timer_submenu.addItem(&create_menu_item("15 minutes", sel!(timer15:), delegate, mtm));
        timer_submenu.addItem(&create_menu_item("30 minutes", sel!(timer30:), delegate, mtm));
        timer_submenu.addItem(&create_menu_item("1 hour", sel!(timer60:), delegate, mtm));
        timer_submenu.addItem(&create_menu_item("2 hours", sel!(timer120:), delegate, mtm));
        timer_menu_item.setSubmenu(Some(&timer_submenu));
        menu.addItem(&timer_menu_item);

        // Mode submenu
        let mode_title = NSString::from_str("Mode");
        let mode_menu_item = NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &mode_title,
            None,
            &empty,
        );
        let mode_submenu = NSMenu::new(mtm);

        let mode_display = create_menu_item("Display Only", sel!(modeDisplay:), delegate, mtm);
        let mode_system = create_menu_item("System Only", sel!(modeSystem:), delegate, mtm);
        let mode_both = create_menu_item("Display + System", sel!(modeBoth:), delegate, mtm);

        {
            let mut items = MODE_ITEMS.lock().unwrap();
            items[MODE_DISPLAY as usize].0 = Retained::as_ptr(&mode_display) as *mut _;
            items[MODE_SYSTEM as usize].0 = Retained::as_ptr(&mode_system) as *mut _;
            items[MODE_BOTH as usize].0 = Retained::as_ptr(&mode_both) as *mut _;
        }

        mode_submenu.addItem(&mode_display);
        mode_submenu.addItem(&mode_system);
        mode_submenu.addItem(&mode_both);

        mode_menu_item.setSubmenu(Some(&mode_submenu));
        menu.addItem(&mode_menu_item);
        update_mode_menu_state();

        // Separator
        let sep2 = NSMenuItem::separatorItem(mtm);
        menu.addItem(&sep2);

        // Launch at Login
        let login_item = create_menu_item("Launch at Login", sel!(toggleLogin:), delegate, mtm);
        LOGIN_ITEM.lock().unwrap().0 = Retained::as_ptr(&login_item) as *mut _;
        menu.addItem(&login_item);
        update_login_item_state();

        // Separator
        let sep3 = NSMenuItem::separatorItem(mtm);
        menu.addItem(&sep3);

        // Quit
        let quit_item = create_menu_item("Quit", sel!(quit:), delegate, mtm);
        menu.addItem(&quit_item);

        // Store menu for right-click access (don't set it on status item —
        // left click toggles, right click shows menu)
        STATUS_MENU.lock().unwrap().0 = Retained::as_ptr(&menu) as *mut _;

        // Set button action for left-click toggle
        {
            let button: *mut AnyObject = msg_send![&status_item, button];
            if !button.is_null() {
                let _: () = msg_send![button, setAction: sel!(buttonClicked:)];
                let _: () = msg_send![button, setTarget: delegate];
            }
        }

        // Send right-click events to our button handler
        // Fire action on left mouse up and right mouse down/up
        let mask: i64 = (1 << 2) | (1 << 3) | (1 << 4);
        let _: () = msg_send![&status_item, sendActionOn: mask];

        app.run();
    }
}
