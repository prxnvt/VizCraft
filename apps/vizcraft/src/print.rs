//! Hands a print-ready PDF from `file.print` to the operating system, without unsafe code: only
//! child processes started with an argument list (no shell, so a path cannot inject commands).
//!
//! | Platform | What happens |
//! |---|---|
//! | Apple macOS | `lp` sends the PDF to the default printer (CUPS). With no default printer, or when `lp` fails, the PDF opens in the default viewer (`open`, usually Preview), where it can be printed with the system dialog. |
//! | Linux and the BSDs | `lp` to the default printer; failing that, `xdg-open` opens the PDF in the default viewer. |
//! | Microsoft Windows | `explorer` opens the PDF in the default viewer, which prints it with its own dialog. |

use std::path::Path;
use std::process::Command;

/// Send the PDF at `path` to the printer, or open it for printing. Returns a status-bar message.
pub fn send(path: &str) -> Result<String, String> {
    let file = Path::new(path);
    if !file.is_file() {
        return Err(format!("{path} does not exist"));
    }
    // An absolute path, so a file name starting with a dash is never read as an option.
    let abs = file.canonicalize().map_err(|e| format!("{path}: {e}"))?;
    if cfg!(windows) {
        return open_in_viewer(&abs);
    }
    match Command::new("lp").arg(&abs).output() {
        Ok(out) if out.status.success() => {
            let job = String::from_utf8_lossy(&out.stdout).trim().to_string();
            Ok(if job.is_empty() { "Sent to the default printer".into() } else { format!("Sent to the default printer ({job})") })
        }
        _ => open_in_viewer(&abs),
    }
}

/// Open the PDF in the system's default viewer.
fn open_in_viewer(abs: &Path) -> Result<String, String> {
    let program = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(windows) {
        "explorer"
    } else {
        "xdg-open"
    };
    // Explorer returns a non-zero code even when it opened the file, so only a failure to start
    // counts.
    Command::new(program)
        .arg(abs)
        .spawn()
        .map(|_| "Opened the print file in the default viewer; print it from there".to_string())
        .map_err(|e| format!("could not start {program}: {e}; the print file is {}", abs.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_file_is_an_error_and_starts_nothing() {
        assert!(send("/no/such/vizcraft-print.pdf").is_err());
        assert!(send("").is_err());
    }
}
