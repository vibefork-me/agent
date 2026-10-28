//! The agent's questions to the person at the machine: Allow / Block, a text value, a secret.
//!
//! Windows and Linux: the agent's own window (egui, `vibefork-tray dialog <kind>` next to this
//! binary — the core itself has no GUI dependencies). When that window cannot be shown (no binary,
//! no display, no graphics driver), a system dialog: Windows — a form (PowerShell), Linux —
//! `zenity`. macOS: `osascript`.
//! Text goes through environment variables / arguments, never through a shell string; it is cleaned
//! of control characters and layout tricks, and zenity gets it as plain text (`--no-markup`).

use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use vf_core::{ConfirmDialog, Secret, dialog_text};

/// How long a dialog waits for the user before it counts as Block.
pub const DIALOG_TIMEOUT: Duration = Duration::from_secs(60);

/// How the agent asks its own dialog window (`vibefork-tray dialog <kind>`): the kind is the
/// argument, strings come in environment variables, the answer is the exit code (and stdout for a
/// value).
pub mod window {
    /// The binary that shows the window, next to the agent's own.
    pub const PROGRAM: &str = "vibefork-tray";
    /// First argument: show a dialog.
    pub const COMMAND: &str = "dialog";
    /// Kind: Allow / Block.
    pub const CONFIRM: &str = "confirm";
    /// Kind: a text value.
    pub const TEXT: &str = "text";
    /// Kind: a secret, not empty.
    pub const SECRET: &str = "secret";
    /// Kind: a secret, empty allowed.
    pub const SECRET_OPTIONAL: &str = "secret-optional";
    /// Title.
    pub const ENV_TITLE: &str = "VF_DIALOG_TITLE";
    /// The question.
    pub const ENV_TEXT: &str = "VF_DIALOG_TEXT";
    /// Accepting button.
    pub const ENV_ACCEPT: &str = "VF_DIALOG_ACCEPT";
    /// Rejecting button.
    pub const ENV_REJECT: &str = "VF_DIALOG_REJECT";
    /// Seconds the window waits.
    pub const ENV_TIMEOUT_SECS: &str = "VF_DIALOG_TIMEOUT_SECS";
    /// Exit code: accepted (a value, if any, is on stdout).
    pub const EXIT_ACCEPTED: i32 = 0;
    /// Exit code: rejected, closed or timed out.
    pub const EXIT_REJECTED: i32 = 1;
    /// Exit code: no window could be shown; the caller asks another way (as for any exit code
    /// other than [`EXIT_ACCEPTED`] and [`EXIT_REJECTED`], e.g. a crash).
    pub const EXIT_NO_WINDOW: i32 = 3;
    /// Longest value read back from the window, in bytes.
    pub const MAX_VALUE: u64 = 64 * 1024;
}

/// Extra time the agent gives its own window beyond [`DIALOG_TIMEOUT`] (the window closes itself
/// at the timeout; this covers its start-up).
const WINDOW_GRACE: Duration = Duration::from_secs(3);

/// What the agent's own window said.
enum Shown {
    /// The person answered (or the time ran out): accepted or not, and the value typed.
    Answered { accepted: bool, value: String },
    /// No window could be shown.
    Unavailable,
}

/// Asks through the agent's own window. Never on macOS (system dialogs there).
fn ask_window(kind: &str, title: &str, text: &str, accept: &str, reject: &str) -> Shown {
    if cfg!(target_os = "macos") {
        return Shown::Unavailable;
    }
    let Some(program) = std::env::current_exe()
        .ok()
        .and_then(|exe| {
            exe.parent()
                .map(|d| d.join(crate::install::exe_name(window::PROGRAM)))
        })
        .filter(|p| p.is_file())
    else {
        return Shown::Unavailable;
    };
    let child = Command::new(program)
        .args([window::COMMAND, kind])
        .env(window::ENV_TITLE, title)
        .env(window::ENV_TEXT, text)
        .env(window::ENV_ACCEPT, accept)
        .env(window::ENV_REJECT, reject)
        .env(
            window::ENV_TIMEOUT_SECS,
            DIALOG_TIMEOUT.as_secs().to_string(),
        )
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = %e, "cannot start the dialog window; using a system dialog");
            return Shown::Unavailable;
        }
    };
    // The value is read while the window runs: a value larger than the pipe's buffer would block
    // the window's write, and the window would never exit.
    let reader = child.stdout.take().map(|out| {
        std::thread::spawn(move || {
            let mut value = String::new();
            let _ = std::io::Read::read_to_string(
                &mut std::io::Read::take(out, window::MAX_VALUE),
                &mut value,
            );
            value
        })
    });
    let deadline = Instant::now() + DIALOG_TIMEOUT + WINDOW_GRACE;
    loop {
        match child.try_wait() {
            // No window, or it crashed: not an answer.
            Ok(Some(status))
                if !matches!(
                    status.code(),
                    Some(window::EXIT_ACCEPTED | window::EXIT_REJECTED)
                ) =>
            {
                tracing::warn!(code = ?status.code(), "the dialog window cannot be shown; using a system dialog");
                return Shown::Unavailable;
            }
            Ok(Some(status)) => {
                let accepted = status.code() == Some(window::EXIT_ACCEPTED);
                // The window has exited: its end of the pipe is closed and the read ends.
                let value = match reader {
                    Some(r) if accepted => r.join().unwrap_or_default(),
                    _ => String::new(),
                };
                return Shown::Answered { accepted, value };
            }
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(100)),
            _ => {
                let _ = child.kill();
                return Shown::Answered {
                    accepted: false,
                    value: String::new(),
                };
            }
        }
    }
}

/// A value through the agent's own window: `None` when the window cannot be shown (ask another
/// way), `Some(None)` on cancel or timeout, `Some(Some(value))` (cleaned, possibly empty) on OK.
fn ask_value(
    kind: &str,
    title: &str,
    text: &str,
    lang: vf_core::Language,
) -> Option<Option<String>> {
    match ask_window(
        kind,
        title,
        text,
        &vf_core::catalog::btn_ok(lang),
        &vf_core::catalog::btn_cancel(lang),
    ) {
        Shown::Unavailable => None,
        Shown::Answered {
            accepted: false, ..
        } => Some(None),
        Shown::Answered {
            accepted: true,
            value,
        } => Some(Some(vf_core::clean_secret(&value))),
    }
}

/// The OS dialog. Returns `false` on Block, timeout or when no dialog program is available.
#[derive(Debug, Default)]
pub struct NativeConfirmDialog;

impl ConfirmDialog for NativeConfirmDialog {
    fn language(&self) -> vf_core::Language {
        crate::system_language()
    }

    fn confirm(&self, title: &str, text: &str) -> bool {
        let lang = self.language();
        // Defence in depth: callers already clean page values; the dialog cleans the whole text.
        let (title, text) = (dialog_text(title), dialog_text(text));
        if let Shown::Answered { accepted, .. } = ask_window(
            window::CONFIRM,
            &title,
            &text,
            &vf_core::catalog::btn_allow(lang),
            &vf_core::catalog::btn_block(lang),
        ) {
            return accepted;
        }
        match spawn_dialog(&title, &text, lang) {
            Ok(child) => wait_allowed(child),
            Err(e) => {
                tracing::warn!(error = %e, "cannot show a confirm dialog; treating as Block");
                false
            }
        }
    }

    fn ask_secret(&self, title: &str, text: &str) -> Option<Secret<String>> {
        ask_secret_with_lang(title, text, self.language())
    }

    fn ask_text(&self, title: &str, text: &str) -> Option<String> {
        ask_text_with_lang(title, text, self.language())
    }

    fn ask_secret_optional(&self, title: &str, text: &str) -> Option<Secret<String>> {
        ask_secret_optional_with_lang(title, text, self.language())
    }
}

/// Shows a text entry dialog with the input unmasked.
/// Returns `None` on cancel, empty input, timeout or when no dialog can be shown.
pub fn ask_text(title: &str, text: &str) -> Option<String> {
    ask_text_with_lang(title, text, crate::system_language())
}

/// Shows a text entry dialog with a specified language.
pub fn ask_text_with_lang(title: &str, text: &str, lang: vf_core::Language) -> Option<String> {
    let (title, text) = (dialog_text(title), dialog_text(text));
    if let Some(answer) = ask_value(window::TEXT, &title, &text, lang) {
        return answer.filter(|v| !v.is_empty());
    }
    match spawn_text_dialog(&title, &text, lang) {
        Ok(child) => wait_text(child),
        Err(e) => {
            tracing::warn!(error = %e, "cannot show a text dialog");
            None
        }
    }
}

/// Shows a secret entry dialog allowing empty input.
pub fn ask_secret_optional_with_lang(
    title: &str,
    text: &str,
    lang: vf_core::Language,
) -> Option<Secret<String>> {
    let (title, text) = (dialog_text(title), dialog_text(text));
    if let Some(answer) = ask_value(window::SECRET_OPTIONAL, &title, &text, lang) {
        return answer.map(Secret::new);
    }
    match spawn_secret_dialog(&title, &text, lang) {
        Ok(child) => wait_secret_optional(child),
        Err(e) => {
            tracing::warn!(error = %e, "cannot show a secret dialog");
            None
        }
    }
}

/// Shows a secret entry dialog with the input masked.
/// Returns `None` on cancel, empty input, timeout or when no dialog can be shown.
pub fn ask_secret(title: &str, text: &str) -> Option<Secret<String>> {
    ask_secret_with_lang(title, text, crate::system_language())
}

/// Shows a secret entry dialog with a specified language.
pub fn ask_secret_with_lang(
    title: &str,
    text: &str,
    lang: vf_core::Language,
) -> Option<Secret<String>> {
    let (title, text) = (dialog_text(title), dialog_text(text));
    if let Some(answer) = ask_value(window::SECRET, &title, &text, lang) {
        return answer.filter(|v| !v.is_empty()).map(Secret::new);
    }
    match spawn_secret_dialog(&title, &text, lang) {
        Ok(child) => wait_secret(child),
        Err(e) => {
            tracing::warn!(error = %e, "cannot show a secret dialog");
            None
        }
    }
}

#[cfg(windows)]
fn spawn_dialog(title: &str, text: &str, lang: vf_core::Language) -> std::io::Result<Child> {
    use std::os::windows::process::CommandExt as _;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let script = "Add-Type -AssemblyName System.Windows.Forms; \
        Add-Type -AssemblyName System.Drawing; \
        $sig = @'
[DllImport(\"user32.dll\")]
public static extern bool SetForegroundWindow(IntPtr hWnd);
'@; \
        $u = Add-Type -MemberDefinition $sig -Name \"NativeUtil\" -Namespace \"Win32\" -PassThru; \
        $form = New-Object System.Windows.Forms.Form; \
        $form.Text = $env:VF_DIALOG_TITLE; \
        $form.FormBorderStyle = 'FixedDialog'; \
        $form.MaximizeBox = $false; \
        $form.MinimizeBox = $false; \
        $form.HelpButton = $false; \
        $form.StartPosition = 'CenterScreen'; \
        $form.TopMost = $true; \
        $label = New-Object System.Windows.Forms.Label; \
        $label.AutoSize = $true; \
        $label.MaximumSize = New-Object System.Drawing.Size(430, 0); \
        $label.Location = New-Object System.Drawing.Point(24, 20); \
        $label.Text = $env:VF_DIALOG_TEXT; \
        $form.Controls.Add($label); \
        $btnY = [Math]::Max($label.Bottom + 20, 100); \
        $btnAllow = New-Object System.Windows.Forms.Button; \
        $btnAllow.Text = $env:VF_BTN_ALLOW; \
        $btnAllow.DialogResult = [System.Windows.Forms.DialogResult]::Yes; \
        $btnAllow.Location = New-Object System.Drawing.Point(230, $btnY); \
        $btnAllow.Size = New-Object System.Drawing.Size(100, 28); \
        $form.Controls.Add($btnAllow); \
        $btnBlock = New-Object System.Windows.Forms.Button; \
        $btnBlock.Text = $env:VF_BTN_BLOCK; \
        $btnBlock.DialogResult = [System.Windows.Forms.DialogResult]::No; \
        $btnBlock.Location = New-Object System.Drawing.Point(344, $btnY); \
        $btnBlock.Size = New-Object System.Drawing.Size(110, 28); \
        $form.Controls.Add($btnBlock); \
        $form.AcceptButton = $btnBlock; \
        $form.CancelButton = $btnBlock; \
        $form.ClientSize = New-Object System.Drawing.Size(480, ($btnY + 48)); \
        $form.Add_Shown({ [Win32.NativeUtil]::SetForegroundWindow($form.Handle); $form.Activate(); $form.BringToFront(); $btnBlock.Focus() }); \
        if ($form.ShowDialog() -eq [System.Windows.Forms.DialogResult]::Yes) { \
            exit 0; \
        } else { \
            exit 1; \
        }";
    Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("VF_DIALOG_TITLE", title)
        .env("VF_DIALOG_TEXT", text)
        .env("VF_BTN_ALLOW", vf_core::catalog::btn_allow(lang))
        .env("VF_BTN_BLOCK", vf_core::catalog::btn_block(lang))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
}

#[cfg(windows)]
fn spawn_text_dialog(title: &str, text: &str, lang: vf_core::Language) -> std::io::Result<Child> {
    use std::os::windows::process::CommandExt as _;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let script = "Add-Type -AssemblyName System.Windows.Forms; \
        Add-Type -AssemblyName System.Drawing; \
        $sig = @'
[DllImport(\"user32.dll\")]
public static extern bool SetForegroundWindow(IntPtr hWnd);
'@; \
        $u = Add-Type -MemberDefinition $sig -Name \"NativeUtilText\" -Namespace \"Win32\" -PassThru; \
        $form = New-Object System.Windows.Forms.Form; \
        $form.Text = $env:VF_DIALOG_TITLE; \
        $form.FormBorderStyle = 'FixedDialog'; \
        $form.MaximizeBox = $false; \
        $form.MinimizeBox = $false; \
        $form.HelpButton = $false; \
        $form.StartPosition = 'CenterScreen'; \
        $form.TopMost = $true; \
        $label = New-Object System.Windows.Forms.Label; \
        $label.AutoSize = $true; \
        $label.MaximumSize = New-Object System.Drawing.Size(430, 0); \
        $label.Location = New-Object System.Drawing.Point(24, 20); \
        $label.Text = $env:VF_DIALOG_TEXT; \
        $form.Controls.Add($label); \
        $textY = $label.Bottom + 12; \
        $text = New-Object System.Windows.Forms.TextBox; \
        $text.Location = New-Object System.Drawing.Point(24, $textY); \
        $text.Size = New-Object System.Drawing.Size(430, 24); \
        $form.Controls.Add($text); \
        $btnY = $text.Bottom + 16; \
        $btnOk = New-Object System.Windows.Forms.Button; \
        $btnOk.Text = $env:VF_BTN_OK; \
        $btnOk.DialogResult = [System.Windows.Forms.DialogResult]::OK; \
        $btnOk.Location = New-Object System.Drawing.Point(230, $btnY); \
        $btnOk.Size = New-Object System.Drawing.Size(100, 28); \
        $form.Controls.Add($btnOk); \
        $form.AcceptButton = $btnOk; \
        $btnCancel = New-Object System.Windows.Forms.Button; \
        $btnCancel.Text = $env:VF_BTN_CANCEL; \
        $btnCancel.DialogResult = [System.Windows.Forms.DialogResult]::Cancel; \
        $btnCancel.Location = New-Object System.Drawing.Point(344, $btnY); \
        $btnCancel.Size = New-Object System.Drawing.Size(110, 28); \
        $form.Controls.Add($btnCancel); \
        $form.CancelButton = $btnCancel; \
        $form.ClientSize = New-Object System.Drawing.Size(480, ($btnY + 48)); \
        $form.Add_Shown({ [Win32.NativeUtilText]::SetForegroundWindow($form.Handle); $form.Activate(); $form.BringToFront(); $text.Focus() }); \
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
        .env("VF_BTN_OK", vf_core::catalog::btn_ok(lang))
        .env("VF_BTN_CANCEL", vf_core::catalog::btn_cancel(lang))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
}

#[cfg(windows)]
fn spawn_secret_dialog(title: &str, text: &str, lang: vf_core::Language) -> std::io::Result<Child> {
    use std::os::windows::process::CommandExt as _;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let script = "Add-Type -AssemblyName System.Windows.Forms; \
        Add-Type -AssemblyName System.Drawing; \
        $sig = @'
[DllImport(\"user32.dll\")]
public static extern bool SetForegroundWindow(IntPtr hWnd);
'@; \
        $u = Add-Type -MemberDefinition $sig -Name \"NativeUtilSecret\" -Namespace \"Win32\" -PassThru; \
        $form = New-Object System.Windows.Forms.Form; \
        $form.Text = $env:VF_DIALOG_TITLE; \
        $form.FormBorderStyle = 'FixedDialog'; \
        $form.MaximizeBox = $false; \
        $form.MinimizeBox = $false; \
        $form.HelpButton = $false; \
        $form.StartPosition = 'CenterScreen'; \
        $form.TopMost = $true; \
        $label = New-Object System.Windows.Forms.Label; \
        $label.AutoSize = $true; \
        $label.MaximumSize = New-Object System.Drawing.Size(430, 0); \
        $label.Location = New-Object System.Drawing.Point(24, 20); \
        $label.Text = $env:VF_DIALOG_TEXT; \
        $form.Controls.Add($label); \
        $textY = $label.Bottom + 12; \
        $text = New-Object System.Windows.Forms.TextBox; \
        $text.UseSystemPasswordChar = $true; \
        $text.Location = New-Object System.Drawing.Point(24, $textY); \
        $text.Size = New-Object System.Drawing.Size(430, 24); \
        $form.Controls.Add($text); \
        $btnY = $text.Bottom + 16; \
        $btnOk = New-Object System.Windows.Forms.Button; \
        $btnOk.Text = $env:VF_BTN_OK; \
        $btnOk.DialogResult = [System.Windows.Forms.DialogResult]::OK; \
        $btnOk.Location = New-Object System.Drawing.Point(230, $btnY); \
        $btnOk.Size = New-Object System.Drawing.Size(100, 28); \
        $form.Controls.Add($btnOk); \
        $form.AcceptButton = $btnOk; \
        $btnCancel = New-Object System.Windows.Forms.Button; \
        $btnCancel.Text = $env:VF_BTN_CANCEL; \
        $btnCancel.DialogResult = [System.Windows.Forms.DialogResult]::Cancel; \
        $btnCancel.Location = New-Object System.Drawing.Point(344, $btnY); \
        $btnCancel.Size = New-Object System.Drawing.Size(110, 28); \
        $form.Controls.Add($btnCancel); \
        $form.CancelButton = $btnCancel; \
        $form.ClientSize = New-Object System.Drawing.Size(480, ($btnY + 48)); \
        $form.Add_Shown({ [Win32.NativeUtilSecret]::SetForegroundWindow($form.Handle); $form.Activate(); $form.BringToFront(); $text.Focus() }); \
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
        .env("VF_BTN_OK", vf_core::catalog::btn_ok(lang))
        .env("VF_BTN_CANCEL", vf_core::catalog::btn_cancel(lang))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
}

#[cfg(target_os = "macos")]
fn spawn_dialog(title: &str, text: &str, lang: vf_core::Language) -> std::io::Result<Child> {
    let btn_b = vf_core::catalog::btn_block(lang);
    let btn_a = vf_core::catalog::btn_allow(lang);
    let script = format!(
        "on run argv\n\
         display dialog (item 2 of argv) with title (item 1 of argv) buttons {{\"{btn_b}\", \"{btn_a}\"}} default button \"{btn_b}\" cancel button \"{btn_b}\" with icon caution\n\
         end run"
    );
    let mut cmd = Command::new("osascript");
    for line in script.lines() {
        cmd.args(["-e", line]);
    }
    cmd.args([title, text])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
}

#[cfg(target_os = "macos")]
fn spawn_text_dialog(title: &str, text: &str, lang: vf_core::Language) -> std::io::Result<Child> {
    let btn_c = vf_core::catalog::btn_cancel(lang);
    let btn_o = vf_core::catalog::btn_ok(lang);
    let script = format!(
        "on run argv\n\
         set dialogResult to display dialog (item 2 of argv) with title (item 1 of argv) default answer \"\" buttons {{\"{btn_c}\", \"{btn_o}\"}} default button \"{btn_o}\" cancel button \"{btn_c}\"\n\
         return text returned of dialogResult\n\
         end run"
    );
    let mut cmd = Command::new("osascript");
    for line in script.lines() {
        cmd.args(["-e", line]);
    }
    cmd.args([title, text])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
}

#[cfg(target_os = "macos")]
fn spawn_secret_dialog(title: &str, text: &str, lang: vf_core::Language) -> std::io::Result<Child> {
    let btn_c = vf_core::catalog::btn_cancel(lang);
    let btn_o = vf_core::catalog::btn_ok(lang);
    let script = format!(
        "on run argv\n\
         set dialogResult to display dialog (item 2 of argv) with title (item 1 of argv) default answer \"\" with hidden answer buttons {{\"{btn_c}\", \"{btn_o}\"}} default button \"{btn_o}\" cancel button \"{btn_c}\"\n\
         return text returned of dialogResult\n\
         end run"
    );
    let mut cmd = Command::new("osascript");
    for line in script.lines() {
        cmd.args(["-e", line]);
    }
    cmd.args([title, text])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
}

#[cfg(all(unix, not(target_os = "macos")))]
fn spawn_dialog(title: &str, text: &str, lang: vf_core::Language) -> std::io::Result<Child> {
    let ok_label = format!("--ok-label={}", vf_core::catalog::btn_allow(lang));
    let cancel_label = format!("--cancel-label={}", vf_core::catalog::btn_block(lang));
    Command::new("zenity")
        .args(["--question", "--no-markup", &ok_label, &cancel_label])
        .arg(format!("--title={title}"))
        .arg(format!("--text={text}"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
}

#[cfg(all(unix, not(target_os = "macos")))]
fn spawn_text_dialog(title: &str, text: &str, lang: vf_core::Language) -> std::io::Result<Child> {
    let ok_label = format!("--ok-label={}", vf_core::catalog::btn_ok(lang));
    let cancel_label = format!("--cancel-label={}", vf_core::catalog::btn_cancel(lang));
    Command::new("zenity")
        .args(["--entry", &ok_label, &cancel_label])
        .arg(format!("--title={title}"))
        .arg(format!("--text={text}"))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
}

#[cfg(all(unix, not(target_os = "macos")))]
fn spawn_secret_dialog(title: &str, text: &str, lang: vf_core::Language) -> std::io::Result<Child> {
    let ok_label = format!("--ok-label={}", vf_core::catalog::btn_ok(lang));
    let cancel_label = format!("--cancel-label={}", vf_core::catalog::btn_cancel(lang));
    Command::new("zenity")
        .args(["--entry", "--hide-text", &ok_label, &cancel_label])
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
                // Windows PowerShell may prefix its output with a byte order mark.
                let trimmed = vf_core::clean_secret(&buf);
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

/// Reads unmasked text from the child process's stdout. Kills the dialog after [`DIALOG_TIMEOUT`].
fn wait_text(mut child: Child) -> Option<String> {
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
                let trimmed = vf_core::clean_secret(&buf);
                if trimmed.is_empty() {
                    return None;
                }
                return Some(trimmed);
            }
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(100)),
            _ => {
                let _ = child.kill();
                return None;
            }
        }
    }
}

/// Reads the secret from the child process's stdout, allowing empty string on OK. Kills the dialog after [`DIALOG_TIMEOUT`].
fn wait_secret_optional(mut child: Child) -> Option<Secret<String>> {
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
                let trimmed = vf_core::clean_secret(&buf);
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
    fn language(&self) -> vf_core::Language {
        crate::system_language()
    }

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

    fn ask_text(&self, title: &str, _text: &str) -> Option<String> {
        tracing::warn!(%title, "auto-approved text (insecure-auto-approve build)");
        std::env::var("VF_AUTO_APPROVE_TEXT")
            .ok()
            .filter(|s| !s.trim().is_empty())
    }

    fn ask_secret_optional(&self, title: &str, _text: &str) -> Option<Secret<String>> {
        tracing::warn!(%title, "auto-approved optional secret (insecure-auto-approve build)");
        std::env::var("VF_AUTO_APPROVE_SECRET")
            .ok()
            .map(|s| Secret::new(s.trim().to_owned()))
            .or_else(|| Some(Secret::new(String::new())))
    }
}
