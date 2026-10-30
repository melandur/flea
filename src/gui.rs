use crate::paths;
use crate::thp;
use crate::vulkan;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

// exec rather than spawn, so the shell replaces this process and no pid is orphaned.
pub fn exec_qs(ui: &Path, start: Option<&str>, select: Option<&str>) -> i32 {
    if let Some(chosen) = iced_requested() {
        let Some(bin) = chosen else { return iced_missing() };
        let mut cmd = iced_command(bin);
        cmd.args(start);
        if let Some(target) = select {
            cmd.env("FLEA_SELECT", target);
        }
        return exec(cmd);
    }
    let mut cmd = qs_command(ui.to_path_buf());
    if let Some(path) = start {
        cmd.env("FLEA_PATH", path);
    }
    if let Some(target) = select {
        cmd.env("FLEA_SELECT", target);
    }
    exec(cmd)
}

// flea --pick <reply>: the picker window tools/flea-portal opens for one portal request. Same shell
// and the same renderer choice, on a second entry point, so a chooser is not a second application.
pub fn pick(reply: &str) -> i32 {
    // Empty is absent, the rule paths::has_display() applies: a wrapper's unset variable is not a request.
    if !std::env::var_os("FLEA_PICKER").is_some_and(|value| !value.is_empty()) {
        eprintln!("flea: --pick needs FLEA_PICKER, the portal request tools/flea-portal puts in the environment");
        return 2;
    }
    if reply.is_empty() {
        eprintln!("flea: --pick needs the reply file tools/flea-portal names, and it was empty");
        return 2;
    }
    if !paths::has_display() {
        eprintln!("flea: there is no graphical session to open a file chooser in");
        return 2;
    }
    if let Some(chosen) = iced_requested() {
        let Some(bin) = chosen else { return iced_missing() };
        let mut cmd = iced_command(bin);
        cmd.arg("--pick").env("FLEA_PICKER_REPLY", reply);
        return exec(cmd);
    }
    let Some(ui) = paths::ui_dir() else {
        eprintln!("flea: the shell config is missing, set FLEA_UI or install /usr/share/flea/ui");
        return 2;
    };
    let mut cmd = qs_command(ui.join("picker.qml"));
    cmd.env("FLEA_PICKER_REPLY", reply);
    exec(cmd)
}

// FLEA_FRONTEND=iced opts into the iced window, crates/flea-iced, for both front doors; anything
// else, empty included, is the shipped qs window. None is not asked; Some(None) is asked for and not
// found, which refuses rather than falling back, because the choice was the operator's.
fn iced_requested() -> Option<Option<PathBuf>> {
    if std::env::var("FLEA_FRONTEND").ok().as_deref() != Some("iced") {
        return None;
    }
    let runnable = |p: &PathBuf| p.is_file() && std::fs::metadata(p).is_ok_and(|m| {
        use std::os::unix::fs::PermissionsExt;
        m.permissions().mode() & 0o111 != 0
    });
    if let Some(explicit) = std::env::var_os("FLEA_ICED_BIN").filter(|v| !v.is_empty()) {
        return Some(Some(PathBuf::from(explicit)).filter(runnable));
    }
    // Beside this binary when packaged, else the dev tree's own build beside target/.
    let exe = std::env::current_exe().ok()?;
    let mut candidates = vec![exe.with_file_name("flea-iced")];
    if let Some(root) = exe.parent().and_then(Path::parent).and_then(Path::parent) {
        for profile in ["release", "debug"] {
            candidates.push(root.join("crates/flea-iced/target").join(profile).join("flea-iced"));
        }
    }
    Some(candidates.into_iter().find(runnable))
}

fn iced_missing() -> i32 {
    eprintln!("flea: FLEA_FRONTEND asks for the iced window and no flea-iced binary was found, set FLEA_ICED_BIN or build crates/flea-iced");
    2
}

// The iced binary is handed the backend this launcher is, the same seam qs_command gives qs. Its
// renderer is wgpu's own choice: QSG_RHI_BACKEND is Qt's and means nothing to it.
fn iced_command(bin: PathBuf) -> Command {
    let mut cmd = Command::new(bin);
    if std::env::var_os("FLEA_BIN").map_or(true, |value| value.is_empty()) {
        if let Ok(binary) = std::env::current_exe() {
            cmd.env("FLEA_BIN", binary);
        }
    }
    cmd
}

// The qs invocation both entry points share: the target, the binary the shell calls back into, and
// the renderer, which is chosen here because this is the last point that can hand it to qs.
fn qs_command(target: PathBuf) -> Command {
    let mut cmd = Command::new("qs");
    cmd.arg("-p").arg(target);
    // An explicit choice is the operator's, the same rule FLEA_UI and QSG_RHI_BACKEND follow here.
    // map_or, not is_none_or: that method landed in 1.82 and Cargo.toml declares a 1.77 floor.
    if std::env::var_os("FLEA_BIN").map_or(true, |value| value.is_empty()) {
        if let Ok(binary) = std::env::current_exe() {
            cmd.env("FLEA_BIN", binary);
        }
    }
    // Empty is absent, the rule paths::has_display() applies: a wrapper's unset variable is not a choice.
    if std::env::var_os("QSG_RHI_BACKEND").is_some_and(|value| !value.is_empty()) {
        // An explicit choice is the operator's, so it is neither replaced nor offered a retry.
        cmd.env_remove("FLEA_RENDERER_AUTOMATIC");
    } else if let Err(reason) = vulkan::usable() {
        // A silent downgrade hides a 2.4x memory regression, so the reason the probe found is said once.
        eprintln!("flea: Vulkan is unusable, {reason}, so the shell starts on OpenGL");
        cmd.env("QSG_RHI_BACKEND", "opengl");
        cmd.env_remove("FLEA_RENDERER_AUTOMATIC");
    } else {
        // Vulkan is the measured fast path, and the marker is what permits the QML arm its one retry.
        cmd.env("QSG_RHI_BACKEND", "vulkan");
        cmd.env("FLEA_RENDERER_AUTOMATIC", "1");
    }
    cmd
}

fn exec(mut cmd: Command) -> i32 {
    // The setting is preserved across exec, so this is the last point that can hand it to qs.
    thp::disable();
    // exec() only returns on failure; the reason is elided, never shown raw.
    let _ = cmd.exec();
    eprintln!("flea: could not start the shell, qs is not on PATH or failed to run");
    1
}
