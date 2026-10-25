//! Native Allow / Block dialog: Windows — MessageBox (PowerShell), macOS — `osascript`, Linux — `zenity`.
//! Text goes through environment variables / arguments, never through a shell string; it is cleaned
//! of control characters and layout tricks, and zenity gets it as plain text (`--no-markup`).

use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use vf_core::{ConfirmDialog, Secret, dialog_text};

/// How long a dialog waits for the user before it counts as Block.
pub const DIALOG_TIMEOUT: Duration = Duration::from_secs(60);

/// The OS dialog. Returns `false` on Block, timeout or when no dialog program is available.
#[derive(Debug, Default)]
pub struct NativeConfirmDialog;

impl ConfirmDialog for NativeConfirmDialog {
    fn confirm(&self, title: &str, text: &str) -> bool {
        // Defence in depth: callers already clean page values; the dialog cleans the whole text.
        match spawn_dialog(&dialog_text(title), &dialog_text(text)) {
            Ok(child) => wait_allowed(child),
            Err(e) => {
                tracing::warn!(error = %e, "cannot show a confirm dialog; treating as Block");
                false
            }
        }
    }

    fn ask_secret(&self, title: &str, text: &str) -> Option<Secret<String>> {
        ask_secret(title, text)
    }
}

/// Shows a secret entry dialog with the input masked.
/// Returns `None` on cancel, empty input, timeout or when no dialog can be shown.
pub fn ask_secret(title: &str, text: &str) -> Option<Secret<String>> {
    match spawn_secret_dialog(&dialog_text(title), &dialog_text(text)) {
        Ok(child) => wait_secret(child),
        Err(e) => {
            tracing::warn!(error = %e, "cannot show a secret dialog");
            None
        }
    }
}

#[cfg(windows)]
fn spawn_dialog(title: &str, text: &str) -> std::io::Result<Child> {
    use std::os::windows::process::CommandExt as _;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let script = "Add-Type -AssemblyName System.Windows.Forms; \
        $r=[System.Windows.Forms.MessageBox]::Show($env:VF_DIALOG_TEXT,$env:VF_DIALOG_TITLE,'YesNo','Question','Button2','DefaultDesktopOnly'); \
        if($r -eq 'Yes'){exit 0}else{exit 1}";
    Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("VF_DIALOG_TITLE", title)
        .env("VF_DIALOG_TEXT", text)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
}

#[cfg(windows)]
fn spawn_secret_dialog(title: &str, text: &str) -> std::io::Result<Child> {
    use std::os::windows::process::CommandExt as _;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let script = "Add-Type -AssemblyName System.Windows.Forms; \
        Add-Type -AssemblyName System.Drawing; \
        $form = New-Object System.Windows.Forms.Form; \
        $form.Text = $env:VF_DIALOG_TITLE; \
        $form.Size = New-Object System.Drawing.Size(420, 190); \
        $form.StartPosition = 'CenterScreen'; \
        $form.FormBorderStyle = 'FixedDialog'; \
        $form.MaximizeBox = $false; \
        $form.MinimizeBox = $false; \
        $form.TopMost = $true; \
        $label = New-Object System.Windows.Forms.Label; \
        $label.Text = $env:VF_DIALOG_TEXT; \
        $label.Location = New-Object System.Drawing.Point(16, 16); \
        $label.Size = New-Object System.Drawing.Size(370, 40); \
        $form.Controls.Add($label); \
        $text = New-Object System.Windows.Forms.TextBox; \
        $text.UseSystemPasswordChar = $true; \
        $text.Location = New-Object System.Drawing.Point(16, 64); \
        $text.Size = New-Object System.Drawing.Size(370, 24); \
        $form.Controls.Add($text); \
        $btnOk = New-Object System.Windows.Forms.Button; \
        $btnOk.Text = 'OK'; \
        $btnOk.DialogResult = [System.Windows.Forms.DialogResult]::OK; \
        $btnOk.Location = New-Object System.Drawing.Point(220, 104); \
        $form.Controls.Add($btnOk); \
        $form.AcceptButton = $btnOk; \
        $btnCancel = New-Object System.Windows.Forms.Button; \
        $btnCancel.Text = 'Cancel'; \
        $btnCancel.DialogResult = [System.Windows.Forms.DialogResult]::Cancel; \
        $btnCancel.Location = New-Object System.Drawing.Point(305, 104); \
        $form.Controls.Add($btnCancel); \
        $form.CancelButton = $btnCancel; \
        $form.Add_Shown({$form.Activate(); $text.Focus()}); \
        if ($form.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) { \
            [Console]::Out.Write($text.Text); \
            exit 0; \
        } else { \
            exit 1; \
        }";
    Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("VF_DIALOG_TITLE", title)
        .env("VF_DIALOG_TEXT", text)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
}

#[cfg(target_os = "macos")]
fn spawn_dialog(title: &str, text: &str) -> std::io::Result<Child> {
    let script = [
        "on run argv",
        "display dialog (item 2 of argv) with title (item 1 of argv) buttons {\"Block\", \"Allow\"} default button \"Block\" cancel button \"Block\" with icon caution",
        "end run",
    ];
    let mut cmd = Command::new("osascript");
    for line in script {
        cmd.args(["-e", line]);
    }
    cmd.args([title, text])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
}

#[cfg(target_os = "macos")]
fn spawn_secret_dialog(title: &str, text: &str) -> std::io::Result<Child> {
    let script = [
        "on run argv",
        "set dialogResult to display dialog (item 2 of argv) with title (item 1 of argv) default answer \"\" with hidden answer buttons {\"Cancel\", \"OK\"} default button \"OK\" cancel button \"Cancel\"",
        "return text returned of dialogResult",
        "end run",
    ];
    let mut cmd = Command::new("osascript");
    for line in script {
        cmd.args(["-e", line]);
    }
    cmd.args([title, text])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
}

#[cfg(all(unix, not(target_os = "macos")))]
fn spawn_dialog(title: &str, text: &str) -> std::io::Result<Child> {
    Command::new("zenity")
        .args([
            "--question",
            "--no-markup",
            "--ok-label=Allow",
            "--cancel-label=Block",
        ])
        .arg(format!("--title={title}"))
        .arg(format!("--text={text}"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
}

#[cfg(all(unix, not(target_os = "macos")))]
fn spawn_secret_dialog(title: &str, text: &str) -> std::io::Result<Child> {
    Command::new("zenity")
        .args(["--entry", "--hide-text"])
        .arg(format!("--title={title}"))
        .arg(format!("--text={text}"))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
}

/// Exit code 0 = Allow. Kills the dialog after [`DIALOG_TIMEOUT`].
fn wait_allowed(mut child: Child) -> bool {
    let deadline = Instant::now() + DIALOG_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(100)),
            _ => {
                let _ = child.kill();
                return false;
            }
        }
    }
}

/// Reads the secret from the child process's stdout. Kills the dialog after [`DIALOG_TIMEOUT`].
fn wait_secret(mut child: Child) -> Option<Secret<String>> {
    let deadline = Instant::now() + DIALOG_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    return None;
                }
                let mut stdout = child.stdout.take()?;
                let mut buf = String::new();
                std::io::Read::read_to_string(&mut stdout, &mut buf).ok()?;
                let trimmed = buf.trim().to_owned();
                if trimmed.is_empty() {
                    return None;
                }
                return Some(Secret::new(trimmed));
            }
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(100)),
            _ => {
                let _ = child.kill();
                return None;
            }
        }
    }
}

/// Always Allow, without showing anything. **Only for automated tests**; compiled in only with the
/// `insecure-auto-approve` feature.
#[cfg(feature = "insecure-auto-approve")]
#[derive(Debug, Default)]
pub struct AutoApproveDialog;

#[cfg(feature = "insecure-auto-approve")]
impl ConfirmDialog for AutoApproveDialog {
    fn confirm(&self, title: &str, _text: &str) -> bool {
        tracing::warn!(%title, "auto-approved (insecure-auto-approve build)");
        true
    }

    fn ask_secret(&self, title: &str, _text: &str) -> Option<Secret<String>> {
        tracing::warn!(%title, "auto-approved secret (insecure-auto-approve build)");
        std::env::var("VF_AUTO_APPROVE_SECRET")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .map(Secret::new)
    }
}
