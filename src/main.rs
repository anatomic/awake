//! Awake - Ultra-lightweight macOS menu bar app to prevent sleep
//! Uses IOKit power assertions directly (no child processes)

#[macro_use]
extern crate objc;

use cocoa::appkit::{
    NSApp, NSApplication, NSApplicationActivationPolicy, NSMenu, NSMenuItem, NSStatusBar,
    NSStatusItem, NSVariableStatusItemLength,
};
use cocoa::base::{id, nil, selector, BOOL, NO, YES};
use cocoa::foundation::{NSAutoreleasePool, NSString};
use core_foundation::base::TCFType;
use core_foundation::string::CFString;
use objc::declare::ClassDecl;
use objc::runtime::{Class, Object, Sel};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicU8, Ordering};
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
const LAUNCH_AGENT_LABEL: &str = "com.awake.app";

// Sleep prevention modes
const MODE_DISPLAY: u8 = 0; // Prevent display sleep
const MODE_SYSTEM: u8 = 1; // Prevent system idle sleep
const MODE_BOTH: u8 = 2; // Prevent both (default)

// Global state
static ASSERTION_ID: AtomicU32 = AtomicU32::new(0);
static ASSERTION_ID_2: AtomicU32 = AtomicU32::new(0); // Second assertion for MODE_BOTH
static TIMER_EXPIRY: AtomicU64 = AtomicU64::new(0);
static CURRENT_MODE: AtomicU8 = AtomicU8::new(MODE_BOTH);

static mut STATUS_ITEM: Option<id> = None;
static mut LOGIN_ITEM: Option<id> = None;
static mut MODE_ITEMS: [Option<id>; 3] = [None, None, None];

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

    if result == 0 {
        aid
    } else {
        0
    }
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
        update_title("☕");
    }
}

fn deactivate() {
    TIMER_EXPIRY.store(0, Ordering::Relaxed);
    release_assertion(&ASSERTION_ID);
    release_assertion(&ASSERTION_ID_2);
    update_title("😴");
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
    unsafe {
        for (i, item) in MODE_ITEMS.iter().enumerate() {
            if let Some(menu_item) = *item {
                let state: BOOL = if i as u8 == current { YES } else { NO };
                let _: () = msg_send![menu_item, setState: state];
            }
        }
    }
}

fn activate_for_duration(minutes: u64) {
    deactivate();
    activate();

    if !is_awake() {
        return;
    }

    let expiry = now_secs() + (minutes * 60);
    TIMER_EXPIRY.store(expiry, Ordering::Relaxed);

    thread::spawn(move || {
        thread::sleep(Duration::from_secs(minutes * 60));
        if TIMER_EXPIRY.load(Ordering::Relaxed) == expiry {
            deactivate();
        }
    });
}

fn update_title(title: &str) {
    unsafe {
        if let Some(status_item) = STATUS_ITEM {
            let button: id = msg_send![status_item, button];
            let title_str = NSString::alloc(nil).init_str(title);
            let _: () = msg_send![button, setTitle: title_str];
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
    unsafe {
        if let Some(item) = LOGIN_ITEM {
            let state: BOOL = if is_launch_at_login() { YES } else { NO };
            let _: () = msg_send![item, setState: state];
        }
    }
}

// Action handlers
extern "C" fn toggle_action(_this: &Object, _cmd: Sel, _sender: id) {
    toggle();
}

extern "C" fn login_action(_this: &Object, _cmd: Sel, _sender: id) {
    toggle_launch_at_login();
}

extern "C" fn timer_15_action(_this: &Object, _cmd: Sel, _sender: id) {
    activate_for_duration(15);
}

extern "C" fn timer_30_action(_this: &Object, _cmd: Sel, _sender: id) {
    activate_for_duration(30);
}

extern "C" fn timer_60_action(_this: &Object, _cmd: Sel, _sender: id) {
    activate_for_duration(60);
}

extern "C" fn timer_120_action(_this: &Object, _cmd: Sel, _sender: id) {
    activate_for_duration(120);
}

extern "C" fn mode_display_action(_this: &Object, _cmd: Sel, _sender: id) {
    set_mode(MODE_DISPLAY);
}

extern "C" fn mode_system_action(_this: &Object, _cmd: Sel, _sender: id) {
    set_mode(MODE_SYSTEM);
}

extern "C" fn mode_both_action(_this: &Object, _cmd: Sel, _sender: id) {
    set_mode(MODE_BOTH);
}

extern "C" fn quit_action(_this: &Object, _cmd: Sel, _sender: id) {
    deactivate();
    unsafe {
        let app = NSApp();
        let _: () = msg_send![app, terminate: nil];
    }
}

fn register_delegate_class() -> *const Class {
    let superclass = class!(NSObject);
    let mut decl = ClassDecl::new("AwakeDelegate", superclass).unwrap();

    unsafe {
        decl.add_method(
            selector("toggle:"),
            toggle_action as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            selector("toggleLogin:"),
            login_action as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            selector("timer15:"),
            timer_15_action as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            selector("timer30:"),
            timer_30_action as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            selector("timer60:"),
            timer_60_action as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            selector("timer120:"),
            timer_120_action as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            selector("modeDisplay:"),
            mode_display_action as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            selector("modeSystem:"),
            mode_system_action as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            selector("modeBoth:"),
            mode_both_action as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            selector("quit:"),
            quit_action as extern "C" fn(&Object, Sel, id),
        );
    }

    decl.register()
}

fn create_menu_item(title: &str, action: Sel, delegate: id) -> id {
    unsafe {
        let title_str = NSString::alloc(nil).init_str(title);
        let item = NSMenuItem::alloc(nil).initWithTitle_action_keyEquivalent_(
            title_str,
            action,
            NSString::alloc(nil).init_str(""),
        );
        let _: () = msg_send![item, setTarget: delegate];
        item
    }
}

fn main() {
    unsafe {
        let _pool = NSAutoreleasePool::new(nil);

        let app = NSApp();
        app.setActivationPolicy_(
            NSApplicationActivationPolicy::NSApplicationActivationPolicyAccessory,
        );

        let delegate_class = register_delegate_class();
        let delegate: id = msg_send![delegate_class, new];

        let status_bar = NSStatusBar::systemStatusBar(nil);
        let status_item = status_bar.statusItemWithLength_(NSVariableStatusItemLength);
        STATUS_ITEM = Some(status_item);

        let button: id = msg_send![status_item, button];
        let title = NSString::alloc(nil).init_str("😴");
        let _: () = msg_send![button, setTitle: title];

        let menu = NSMenu::new(nil).autorelease();

        // Toggle
        menu.addItem_(create_menu_item("Toggle", selector("toggle:"), delegate));

        // Separator
        let sep: id = msg_send![class!(NSMenuItem), separatorItem];
        menu.addItem_(sep);

        // Timer submenu
        let timer_title = NSString::alloc(nil).init_str("Awake For...");
        let timer_menu_item = NSMenuItem::alloc(nil).initWithTitle_action_keyEquivalent_(
            timer_title,
            selector(""),
            NSString::alloc(nil).init_str(""),
        );
        let timer_submenu = NSMenu::new(nil).autorelease();
        timer_submenu.addItem_(create_menu_item("15 minutes", selector("timer15:"), delegate));
        timer_submenu.addItem_(create_menu_item("30 minutes", selector("timer30:"), delegate));
        timer_submenu.addItem_(create_menu_item("1 hour", selector("timer60:"), delegate));
        timer_submenu.addItem_(create_menu_item("2 hours", selector("timer120:"), delegate));
        let _: () = msg_send![timer_menu_item, setSubmenu: timer_submenu];
        menu.addItem_(timer_menu_item);

        // Mode submenu
        let mode_title = NSString::alloc(nil).init_str("Mode");
        let mode_menu_item = NSMenuItem::alloc(nil).initWithTitle_action_keyEquivalent_(
            mode_title,
            selector(""),
            NSString::alloc(nil).init_str(""),
        );
        let mode_submenu = NSMenu::new(nil).autorelease();

        let mode_display = create_menu_item("Display Only", selector("modeDisplay:"), delegate);
        let mode_system = create_menu_item("System Only", selector("modeSystem:"), delegate);
        let mode_both = create_menu_item("Display + System", selector("modeBoth:"), delegate);

        MODE_ITEMS[MODE_DISPLAY as usize] = Some(mode_display);
        MODE_ITEMS[MODE_SYSTEM as usize] = Some(mode_system);
        MODE_ITEMS[MODE_BOTH as usize] = Some(mode_both);

        mode_submenu.addItem_(mode_display);
        mode_submenu.addItem_(mode_system);
        mode_submenu.addItem_(mode_both);

        let _: () = msg_send![mode_menu_item, setSubmenu: mode_submenu];
        menu.addItem_(mode_menu_item);
        update_mode_menu_state();

        // Separator
        let sep2: id = msg_send![class!(NSMenuItem), separatorItem];
        menu.addItem_(sep2);

        // Launch at Login
        let login_item = create_menu_item("Launch at Login", selector("toggleLogin:"), delegate);
        LOGIN_ITEM = Some(login_item);
        menu.addItem_(login_item);
        update_login_item_state();

        // Separator
        let sep3: id = msg_send![class!(NSMenuItem), separatorItem];
        menu.addItem_(sep3);

        // Quit
        menu.addItem_(create_menu_item("Quit", selector("quit:"), delegate));

        status_item.setMenu_(menu);
        app.run();
    }
}
