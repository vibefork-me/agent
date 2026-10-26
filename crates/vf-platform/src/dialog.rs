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
    fn language(&self) -> vf_core::Language {
        crate::system_language()
    }

    fn confirm(&self, title: &str, text: &str) -> bool {
        let lang = self.language();
        // Defence in depth: callers already clean page values; the dialog cleans the whole text.
        match spawn_dialog(&dialog_text(title), &dialog_text(text), lang) {
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
    match spawn_secret_dialog(&dialog_text(title), &dialog_text(text), lang) {
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
}
