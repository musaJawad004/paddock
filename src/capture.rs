//! Log capture for servers the user starts: `paddock run` and the shell
//! integration from `paddock init`.
//!
//! A server's output goes only to the terminal that started it, so Paddock
//! cannot read the logs of a server that is already running. What it can do
//! is sit in front of the next start: `paddock run npm run dev` replaces
//! itself (exec, same pid) with the system's `script` tool, which runs the
//! command in a pseudo-terminal, shows everything in the user's terminal as
//! before, and writes a copy to a log file. The monitor recognises the
//! `script` parent of a server and reads that file.
//!
//! Log files live in `$XDG_STATE_HOME/paddock/logs` (default
//! `~/.local/state/paddock/logs`), in a folder only the user can read, and
//! are deleted after three days.
//!
//! `paddock init zsh` prints shell functions for npm, yarn, pnpm, bun, npx,
//! cargo and deno that call `paddock run --auto`. With `--auto`, only
//! commands that start a dev server (`npm run dev`, `yarn start`, `npx expo
//! start`, `cargo run`...) are captured; everything else (`npm install`)
//! runs exactly as if the function were not there.

use std::fs;
use std::io::{self, IsTerminal};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

/// Set for the command `paddock run` starts, so nested calls do not wrap
/// twice.
const NESTED: &str = "PADDOCK_RUN";
const KEEP: Duration = Duration::from_secs(3 * 24 * 60 * 60);

/// Programs the shell integration wraps.
pub const WRAPPED: &[&str] = &[
    "npm", "pnpm", "yarn", "bun", "npx", "pnpx", "bunx", "cargo", "deno",
];

const SERVER_SCRIPTS: &[&str] = &[
    "dev", "start", "serve", "server", "watch", "preview", "develop",
];

/// Tools that, run through npx or `npm exec`, start a server.
const SERVER_TOOLS: &[&str] = &[
    "expo",
    "vite",
    "next",
    "astro",
    "nuxt",
    "nuxi",
    "remix",
    "wrangler",
    "serve",
    "http-server",
    "live-server",
    "nodemon",
    "storybook",
    "webpack",
    "parcel",
    "gatsby",
    "docusaurus",
    "ng",
    "react-scripts",
    "turbo",
    "nx",
    "concurrently",
    "convex",
    "supabase",
    "firebase",
    "vercel",
    "netlify",
];

pub fn logs_dir() -> Option<PathBuf> {
    let state = std::env::var_os("XDG_STATE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| Path::new(&h).join(".local/state")))?;
    Some(state.join("paddock").join("logs"))
}

fn is_script_word(word: &str) -> bool {
    SERVER_SCRIPTS.contains(&word)
        || SERVER_SCRIPTS.iter().any(|s| {
            word.strip_prefix(s)
                .is_some_and(|rest| rest.starts_with(':'))
        })
}

/// True when `program args` starts a dev server rather than doing a quick
/// job like installing packages.
pub fn is_server_command(program: &str, args: &[String]) -> bool {
    let plain: Vec<&str> = args
        .iter()
        .map(String::as_str)
        .filter(|a| !a.starts_with('-'))
        .collect();
    let program = program.rsplit('/').next().unwrap_or(program);
    match program {
        "npm" => match plain.as_slice() {
            ["run" | "run-script", script, ..] => is_script_word(script),
            ["start", ..] => true,
            ["exec" | "x", tool, ..] => SERVER_TOOLS.contains(tool),
            _ => false,
        },
        "yarn" | "pnpm" | "bun" => match plain.as_slice() {
            ["run", script, ..] => is_script_word(script),
            ["dlx" | "exec" | "x", tool, ..] => SERVER_TOOLS.contains(tool),
            [script, ..] => is_script_word(script),
            [] => false,
        },
        "npx" | "pnpx" | "bunx" => plain
            .first()
            .is_some_and(|tool| SERVER_TOOLS.contains(tool)),
        "cargo" => matches!(
            plain.first(),
            Some(&("run" | "watch" | "leptos" | "shuttle"))
        ),
        "deno" => match plain.as_slice() {
            ["task", task, ..] => is_script_word(task),
            ["run" | "serve", ..] => true,
            _ => false,
        },
        _ => false,
    }
}

pub struct RunOptions {
    /// Capture only commands that start a server.
    pub auto: bool,
    /// Capture even without a terminal (used by tests).
    pub always: bool,
}

/// Replaces this process with the command, captured or not. Returns only if
/// the exec itself failed.
pub fn run(command: &[String], options: &RunOptions) -> io::Error {
    let Some((program, args)) = command.split_first() else {
        return io::Error::new(io::ErrorKind::InvalidInput, "no command given");
    };
    let interactive = io::stdin().is_terminal() && io::stdout().is_terminal();
    let capture = std::env::var_os(NESTED).is_none()
        && (!options.auto || is_server_command(program, args))
        && (interactive || options.always);
    if !capture {
        return Command::new(program).args(args).exec();
    }
    let log = match prepare_log(program) {
        Ok(log) => log,
        Err(err) => {
            eprintln!("paddock: not capturing logs ({err})");
            return Command::new(program).args(args).exec();
        }
    };
    let err = wrapped(command, &log).env(NESTED, "1").exec();
    // `script` is missing or failed to start: still run what the user asked.
    eprintln!("paddock: could not start script ({err}); running without log capture");
    Command::new(program).args(args).exec()
}

/// `script` with flags that flush every write and print nothing extra.
fn wrapped(command: &[String], log: &Path) -> Command {
    let mut script = Command::new("script");
    if cfg!(target_os = "macos") {
        script.arg("-q").arg("-F").arg(log).args(command);
    } else {
        // util-linux takes the command as one shell string.
        let quoted: Vec<String> = command.iter().map(|a| shell_quote(a)).collect();
        script
            .args(["-q", "-f", "-e", "-c"])
            .arg(quoted.join(" "))
            .arg(log);
    }
    script
}

/// Single-quotes `word` for a POSIX shell.
fn shell_quote(word: &str) -> String {
    format!("'{}'", word.replace('\'', r"'\''"))
}

fn prepare_log(program: &str) -> io::Result<PathBuf> {
    let dir = logs_dir().ok_or_else(|| io::Error::other("HOME is not set"))?;
    fs::create_dir_all(&dir)?;
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;
    // The monitor compares this path with the folder it resolved, so write
    // it resolved too (macOS: /var is /private/var).
    let dir = dir.canonicalize()?;
    remove_old_logs(&dir);
    let name: String = program
        .rsplit('/')
        .next()
        .unwrap_or("server")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    let stamp = crate::model::now_ms();
    Ok(dir.join(format!("{stamp}-{}-{name}.log", std::process::id())))
}

fn remove_old_logs(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let path = entry.path();
        let old = entry
            .metadata()
            .and_then(|m| m.modified())
            .is_ok_and(|modified| now.duration_since(modified).unwrap_or_default() > KEEP);
        if old && path.extension().is_some_and(|e| e == "log") {
            let _ = fs::remove_file(path);
        }
    }
}

/// Shell code for `eval "$(paddock init zsh)"`.
pub fn init_script(shell: &str) -> Option<String> {
    let mut out = String::from(
        "# Paddock: dev servers started from this shell show their logs in Paddock.\n\
         # Only server commands (npm run dev, yarn start, npx expo start, cargo run...)\n\
         # are captured; everything else runs untouched. Remove this line to stop.\n",
    );
    match shell {
        "zsh" | "bash" => {
            out.push_str("if command -v paddock >/dev/null 2>&1; then\n");
            for program in WRAPPED {
                out.push_str(&format!(
                    "  {program}() {{ paddock run --auto -- {program} \"$@\"; }}\n"
                ));
            }
            out.push_str("fi\n");
        }
        "fish" => {
            out.push_str("if type -q paddock\n");
            for program in WRAPPED {
                out.push_str(&format!(
                    "  function {program} --wraps {program}; paddock run --auto -- {program} $argv; end\n"
                ));
            }
            out.push_str("end\n");
        }
        _ => return None,
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server(command: &str) -> bool {
        let mut words = command.split_whitespace().map(String::from);
        let program = words.next().unwrap_or_default();
        is_server_command(&program, &words.collect::<Vec<_>>())
    }

    #[test]
    fn server_commands_are_captured() {
        for command in [
            "npm run dev",
            "npm start",
            "npm run dev:api",
            "yarn dev",
            "yarn run start",
            "pnpm dev",
            "bun run dev",
            "npx expo start",
            "npx vite --port 3000",
            "npm exec expo start",
            "cargo run",
            "cargo watch -x run",
            "deno task dev",
        ] {
            assert!(server(command), "{command} should be captured");
        }
    }

    #[test]
    fn everything_else_runs_untouched() {
        for command in [
            "npm install",
            "npm run build",
            "npm test",
            "yarn",
            "yarn add react",
            "pnpm lint",
            "npx prettier --write .",
            "cargo build",
            "cargo test",
            "deno fmt",
        ] {
            assert!(!server(command), "{command} should not be captured");
        }
    }

    #[test]
    fn quoting_survives_awkward_arguments() {
        assert_eq!(shell_quote("plain"), "'plain'");
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
    }

    #[test]
    fn init_scripts_wrap_every_program() {
        let zsh = init_script("zsh").unwrap();
        assert!(zsh.contains("npm() { paddock run --auto -- npm \"$@\"; }"));
        assert!(
            init_script("fish")
                .unwrap()
                .contains("function cargo --wraps cargo")
        );
        assert!(init_script("powershell").is_none());
    }
}
