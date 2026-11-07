use std::collections::HashSet;
use std::fs;
use std::os::unix::process::CommandExt;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::game::{Game, Platform};

fn dual_wow_clients(slug: &str) -> Option<(i64, &'static str, i64, &'static str)> {
    match slug {
        "world-of-warcraft-2" => Some((
            8,
            "/home/james/Games/World of Warcraft/_retail_",
            24,
            "/home/james/Games/World of Warcraft 2/_retail_",
        )),
        "world-of-warcraft-forever-2" => Some((
            18,
            "/home/james/Games/World of Warcraft/_forever_",
            23,
            "/home/james/Games/World of Warcraft 2/_forever_",
        )),
        _ => None,
    }
}

pub fn launch(game: &Game) {
    println!(
        "[launch] Launching '{}' (platform={}, via_lutris={}, lutris_id={:?})",
        game.name,
        game.platform.as_str(),
        game.launch_via_lutris,
        game.lutris_launch_id
    );

    let _ = Command::new("swaymsg")
        .arg("workspace --no-auto-back-and-forth 5")
        .output();

    if let Some((c1_id, c1_dir, c2_id, c2_dir)) = dual_wow_clients(&game.slug) {
        thread::spawn(move || {
            launch_dual_wow(c1_id, c1_dir, c2_id, c2_dir);
        });
        return;
    }

    if let Some(ref dir) = game.install_dir {
        let dir_str = dir.to_string_lossy();
        if dir_str.contains("World of Warcraft") {
            ensure_ready_or_running(&dir_str);
        }
    }

    if game.launch_via_lutris {
        if let Some(lid) = game.lutris_launch_id {
            launch_lutris_id(lid);
            return;
        }
    }
    if game.platform == Platform::Steam {
        if let Some(ref appid) = game.steam_appid {
            launch_steam(appid);
            return;
        }
    }
    if game.launch_via_lutris {
        if let Some(ref lslug) = game.lutris_slug {
            launch_lutris_slug(lslug);
            return;
        }
    }
    println!(
        "[launch] Don't know how to launch '{}' — no appid or lutris id.",
        game.name
    );
}

fn launch_dual_wow(c1_id: i64, c1_dir: &str, c2_id: i64, c2_dir: &str) {
    let _ = Command::new("/home/james/bin/wowf-dual-sync").status();

    let c1_running = ensure_ready_or_running(c1_dir);
    let c2_running = ensure_ready_or_running(c2_dir);

    if !c1_running {
        println!(
            "[launch] Starting WoW Client 1 (lutris id={}, dir={})",
            c1_id, c1_dir
        );
        launch_lutris_id(c1_id);
        if !c2_running {
            wait_for_wow_process(c1_dir, 8.0);
        }
    }

    if !c2_running {
        println!(
            "[launch] Starting WoW Client 2 (lutris id={}, dir={})",
            c2_id, c2_dir
        );
        launch_lutris_id(c2_id);
    }
}

fn get_sway_wow_pids() -> Option<HashSet<i32>> {
    let out = Command::new("swaymsg")
        .args(["-t", "get_tree"])
        .output()
        .ok()?;
    if !out.status.success() || out.stdout.is_empty() {
        return None;
    }
    let tree: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    let mut pids = HashSet::new();
    let mut stack = vec![&tree];
    while let Some(node) = stack.pop() {
        if let Some(pid) = node.get("pid").and_then(|v| v.as_i64()) {
            let name = node
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_lowercase();
            let app_id = node
                .get("app_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_lowercase();
            let cls = node
                .get("window_properties")
                .and_then(|p| p.get("class"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_lowercase();
            if name.contains("world of warcraft")
                || app_id.contains("wow.exe")
                || cls.contains("wow.exe")
            {
                pids.insert(pid as i32);
            }
        }
        if let Some(arr) = node.get("nodes").and_then(|v| v.as_array()) {
            stack.extend(arr.iter());
        }
        if let Some(arr) = node.get("floating_nodes").and_then(|v| v.as_array()) {
            stack.extend(arr.iter());
        }
    }
    Some(pids)
}

fn now_secs() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs_f64()
}

fn proc_age(pid: i32, now: f64) -> f64 {
    fs::metadata(format!("/proc/{}", pid))
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| now - d.as_secs_f64())
        .unwrap_or(999.0)
}

fn is_wow_exe_cmd(low: &str) -> bool {
    low.contains("wow.exe")
        && !low.contains("umu")
        && !low.contains("proton")
        && !low.contains("lutris")
        && !low.contains("pv-adverb")
        && !low.contains("srt-bwrap")
}

fn matches_install_dir(cmdline: &str, cwd: &str, install_dir: &str, win_dir_d: &str, win_dir_c: &str) -> bool {
    cmdline.contains(install_dir)
        || cmdline.contains(win_dir_d)
        || cmdline.contains(win_dir_c)
        || cwd == install_dir
}

fn has_live_wow(install_dir: &str) -> bool {
    let my_pid = std::process::id() as i32;
    let sway_pids = get_sway_wow_pids();
    let now = now_secs();
    let win_dir_d = install_dir
        .replace("/home/james/Games/", "D:\\")
        .replace('/', "\\");
    let win_dir_c = install_dir
        .replace("/home/james/Games/", "C:\\")
        .replace('/', "\\");

    let Ok(entries) = fs::read_dir("/proc") else {
        return false;
    };
    for entry in entries.flatten() {
        let fname = entry.file_name();
        let Some(s) = fname.to_str() else { continue };
        let Ok(pid) = s.parse::<i32>() else { continue };
        if pid == my_pid {
            continue;
        }
        let Ok(raw_cmd) = fs::read(format!("/proc/{}/cmdline", pid)) else {
            continue;
        };
        let cmdline: String = raw_cmd
            .into_iter()
            .map(|b| if b == 0 { b' ' } else { b })
            .map(|b| b as char)
            .collect();
        let low = cmdline.to_lowercase();
        if is_wow_exe_cmd(&low) {
            let cwd = fs::read_link(format!("/proc/{}/cwd", pid))
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            if matches_install_dir(&cmdline, &cwd, install_dir, &win_dir_d, &win_dir_c) {
                let age = proc_age(pid, now);
                if sway_pids.as_ref().map(|sp| sp.contains(&pid)).unwrap_or(true) || age < 20.0 {
                    return true;
                }
            }
        }
    }
    false
}

fn wait_for_wow_process(install_dir: &str, timeout: f64) {
    thread::sleep(Duration::from_millis(1500));
    let rem = (timeout - 1.5).max(0.0);
    let deadline = Instant::now() + Duration::from_secs_f64(rem);
    while Instant::now() < deadline {
        if has_live_wow(install_dir) {
            thread::sleep(Duration::from_millis(500));
            return;
        }
        thread::sleep(Duration::from_millis(250));
    }
}

fn bytes_contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

fn ensure_ready_or_running(install_dir: &str) -> bool {
    let mut has_live = false;
    let mut stale_pids = Vec::new();
    let mut had_lutris_wrapper = false;
    let my_pid = std::process::id() as i32;
    let sway_pids = get_sway_wow_pids();
    let now = now_secs();
    let win_dir_d = install_dir
        .replace("/home/james/Games/", "D:\\")
        .replace('/', "\\");
    let win_dir_c = install_dir
        .replace("/home/james/Games/", "C:\\")
        .replace('/', "\\");

    let pfx_tags: &[&[u8]] = if install_dir.contains("World of Warcraft 2") {
        &[
            b"WINEPREFIX=/home/james/Games/battlenet2\0",
            b"WINEPREFIX=/home/james/Games/battlenet2/pfx\0",
            b"WINEPREFIX=/home/james/Games/battlenet2/pfx/\0",
            b"STEAM_COMPAT_DATA_PATH=/home/james/Games/battlenet2\0",
        ]
    } else {
        &[
            b"WINEPREFIX=/home/james/Games/battlenet1\0",
            b"WINEPREFIX=/home/james/Games/battlenet1/pfx\0",
            b"WINEPREFIX=/home/james/Games/battlenet1/pfx/\0",
            b"STEAM_COMPAT_DATA_PATH=/home/james/Games/battlenet1\0",
        ]
    };

    if let Ok(entries) = fs::read_dir("/proc") {
        for entry in entries.flatten() {
            let fname = entry.file_name();
            let Some(s) = fname.to_str() else { continue };
            let Ok(pid) = s.parse::<i32>() else { continue };
            if pid == my_pid {
                continue;
            }
            let Ok(raw_cmd) = fs::read(format!("/proc/{}/cmdline", pid)) else {
                continue;
            };
            let cmdline: String = raw_cmd
                .into_iter()
                .map(|b| if b == 0 { b' ' } else { b })
                .map(|b| b as char)
                .collect();
            if cmdline.contains("/usr/bin/lutris") && !cmdline.contains("lutris-wrapper") {
                continue;
            }
            let cwd = fs::read_link(format!("/proc/{}/cwd", pid))
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            let env_bytes = fs::read(format!("/proc/{}/environ", pid)).unwrap_or_default();

            let dir_match = matches_install_dir(&cmdline, &cwd, install_dir, &win_dir_d, &win_dir_c);
            let matches = dir_match
                || pfx_tags.iter().any(|tag| bytes_contains(&env_bytes, tag));

            if matches {
                let low = cmdline.to_lowercase();
                if is_wow_exe_cmd(&low) {
                    let age = proc_age(pid, now);
                    let is_windowed_or_starting = sway_pids
                        .as_ref()
                        .map(|sp| sp.contains(&pid))
                        .unwrap_or(true)
                        || age < 20.0;
                    if dir_match && is_windowed_or_starting {
                        has_live = true;
                    } else {
                        stale_pids.push(pid);
                    }
                } else {
                    if low.contains("lutris-wrapper") {
                        had_lutris_wrapper = true;
                    }
                    stale_pids.push(pid);
                }
            }
        }
    }

    if !has_live && !stale_pids.is_empty() {
        for pid in stale_pids {
            unsafe {
                libc::kill(pid, libc::SIGKILL);
            }
        }
        if had_lutris_wrapper {
            thread::sleep(Duration::from_millis(2100));
        }
    }
    has_live
}

fn spawn_detached(prog: &str, arg: &str) {
    unsafe {
        let _ = Command::new(prog)
            .arg(arg)
            .pre_exec(|| {
                libc::setsid();
                Ok(())
            })
            .spawn();
    }
}

fn launch_steam(appid: &str) {
    spawn_detached("steam", &format!("steam://rungameid/{}", appid));
}

fn launch_lutris_id(lutris_id: i64) {
    spawn_detached("lutris", &format!("lutris:rungameid/{}", lutris_id));
}

fn launch_lutris_slug(slug: &str) {
    spawn_detached("lutris", &format!("lutris:rungame/{}", slug));
}
