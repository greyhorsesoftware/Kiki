//! "Open AI here…" and "Open Terminal here…": the AI the user has selected (Omarchy's choice unless
//! overridden) started as its own command-line tool, in conversation mode, in a terminal window in
//! the folder. kiki has no AI panel of its own (plan 31: the Jarvis panel of plan 19 was removed).

use crate::json::Value;
use crate::vfs::uri::Uri;
use crate::vfs::VfsError;

pub const PROVIDERS: &[&str] = &["anthropic", "openai", "gemini", "xai", "custom"];

fn jarvis_settings() -> Value {
    crate::config::settings().get("jarvis").cloned().unwrap_or(Value::Null)
}

/// Omarchy's chosen AI: the web app its AI keybinding launches. Heuristic; the override wins.
pub fn omarchy_provider() -> Option<&'static str> {
    let home = crate::config::home();
    for p in [home.join(".config/hypr/bindings.conf"), home.join(".local/share/omarchy/default/hypr/bindings.conf")] {
        let Ok(text) = std::fs::read_to_string(&p) else { continue };
        for line in text.lines() {
            let l = line.to_ascii_lowercase();
            let is_ai = l.contains("super, a,") || l.contains("super, a ,") || l.contains(", ai,") || l.contains("chatgpt") || l.contains("claude") || l.contains("grok") || l.contains("gemini");
            if !is_ai || !l.contains("webapp") && !l.contains("http") {
                continue;
            }
            if l.contains("claude.ai") || l.contains("anthropic") {
                return Some("anthropic");
            }
            if l.contains("chatgpt.com") || l.contains("openai") {
                return Some("openai");
            }
            if l.contains("gemini.google") {
                return Some("gemini");
            }
            if l.contains("grok.com") || l.contains("x.ai") {
                return Some("xai");
            }
        }
    }
    None
}

/// The effective provider and where the choice came from.
pub fn provider() -> (String, &'static str) {
    let s = jarvis_settings();
    match s.str_field("provider") {
        Some(p) if p != "omarchy" && PROVIDERS.contains(&p) => (p.to_string(), "settings"),
        _ => match omarchy_provider() {
            Some(p) => (p.to_string(), "omarchy"),
            None => ("anthropic".into(), "default"),
        },
    }
}

/// The provider's own tool started for a conversation — not print mode — with `prompt` as its
/// first message when there is one. A custom command is run as written: `{prompt}` stands for
/// that first message, and a word holding it is left out when there is nothing to say.
pub fn interactive_for(provider: &str, prompt: Option<&str>) -> Option<Vec<String>> {
    let js = jarvis_settings();
    if let Some(c) = js.str_field("cliCommand").filter(|c| !c.is_empty()) {
        return custom_argv(c, prompt);
    }
    let (bin, lead): (&str, &[&str]) = match provider {
        "anthropic" => ("claude", &[]),
        "openai" => ("codex", &[]),
        "gemini" => ("gemini", &["-i"]),
        "xai" => return Some(vec!["grok".to_string()]),
        _ => return None,
    };
    let mut argv = vec![bin.to_string()];
    if let Some(p) = prompt {
        argv.extend(lead.iter().map(|s| s.to_string()));
        argv.push(p.to_string());
    }
    Some(argv)
}

fn custom_argv(command: &str, prompt: Option<&str>) -> Option<Vec<String>> {
    let argv: Vec<String> = command
        .split_whitespace()
        .filter_map(|w| match (w.contains("{prompt}"), prompt) {
            (true, Some(p)) => Some(w.replace("{prompt}", p)),
            (true, None) => None,
            (false, _) => Some(w.to_string()),
        })
        .collect();
    (!argv.is_empty()).then_some(argv)
}

/// What the AI is told first: which files this is about, and to wait for the question.
pub fn opening_prompt(paths: &[String]) -> String {
    let files = paths.iter().map(|p| format!("`{p}`")).collect::<Vec<_>>().join(", ");
    format!("I have questions about {files}. Read {} and wait for my first question.", if paths.len() == 1 { "it" } else { "them" })
}

/// The terminal a program is run in: the desktop's chosen one (`xdg-terminal-exec`, which is
/// how Omarchy opens its own), else `$TERMINAL -e`, else alacritty.
/// It is started in `dir` either way; `xdg-terminal-exec` is told as well, as Omarchy's own
/// launcher tells it — a terminal that is one server with many windows takes its folder from
/// the option, not from whoever started the client.
pub fn terminal_argv(program: Vec<String>, dir: &std::path::Path, have_xdg: bool, terminal_env: Option<String>) -> Vec<String> {
    let mut argv = if have_xdg { vec!["xdg-terminal-exec".to_string(), format!("--dir={}", dir.display())] } else { vec![terminal_env.filter(|t| !t.is_empty()).unwrap_or_else(|| "alacritty".into()), "-e".to_string()] };
    argv.extend(program);
    argv
}

fn on_path(bin: &str) -> bool {
    std::env::var_os("PATH").map(|p| std::env::split_paths(&p).any(|d| d.join(bin).is_file())).unwrap_or(false)
}

/// A terminal window in `dir`.
pub fn open_terminal(dir: &Uri) -> Result<(), VfsError> {
    if !dir.is_local() {
        return Err(VfsError::said(1251, &[], "a terminal opens in a folder on this machine"));
    }
    let argv = terminal_argv(Vec::new(), &dir.to_path(), on_path("xdg-terminal-exec"), std::env::var("TERMINAL").ok());
    // `$TERMINAL -e` with nothing after it is not a command line: the bare terminal, then.
    let argv: Vec<String> = if argv.last().map(|a| a == "-e").unwrap_or(false) { argv[..argv.len() - 1].to_vec() } else { argv };
    crate::desktop::spawn_detached_in(&argv, Some(&dir.to_path())).map_err(VfsError::Io)
}

/// "Open AI here…": the chosen AI in a terminal window of its own, started in `dir` and, when
/// files were selected, told which ones this is about. The conversation is the user's, in the
/// tool they chose, with everything that tool can do — not a panel of ours in front of it.
pub fn open_external(dir: &Uri, uris: &[Uri]) -> Result<String, VfsError> {
    let (p, _) = provider();
    if !dir.is_local() || uris.iter().any(|u| !u.is_local()) {
        return Err(VfsError::said(1252, &[], "the AI works on files on this machine; copy these here first"));
    }
    let paths: Vec<String> = uris.iter().map(|u| u.to_path().to_string_lossy().into_owned()).collect();
    let prompt = (!paths.is_empty()).then(|| opening_prompt(&paths));
    let program = interactive_for(&p, prompt.as_deref()).ok_or_else(|| VfsError::said(1253, &[], "no command-line tool is set for this AI (Settings → Jarvis)"))?;
    if !on_path(&program[0]) {
        return Err(VfsError::said(1250, &[("name", &program[0])], format!("{} is not installed", program[0])));
    }
    let bin = program[0].clone();
    let argv = terminal_argv(program, &dir.to_path(), on_path("xdg-terminal-exec"), std::env::var("TERMINAL").ok());
    crate::desktop::spawn_detached_in(&argv, Some(&dir.to_path())).map_err(VfsError::Io)?;
    Ok(bin)
}

pub fn status() -> Value {
    let (p, chosen_by) = provider();
    let cli = interactive_for(&p, None).and_then(|argv| argv.into_iter().next());
    let available = cli.as_deref().map(crate::openin::on_path).unwrap_or(false);
    Value::obj()
        .b("configured", available)
        .s("provider", p.clone())
        .s("chosenBy", chosen_by)
        .opt_s("omarchyProvider", omarchy_provider())
        .opt_s("cli", cli.as_deref())
        .b("cliAvailable", available)
        .v("providers", Value::Arr(PROVIDERS.iter().map(|x| Value::Str(x.to_string())).collect()))
        .done()
}

/// Settings → AI: provider ("omarchy" to follow Omarchy) and an optional custom command.
pub fn configure(provider_choice: Option<&str>, cli_command: Option<&str>) -> Result<(), VfsError> {
    let mut patch = Value::obj();
    if let Some(p) = provider_choice {
        patch = patch.s("provider", p);
    }
    if let Some(c) = cli_command {
        patch = patch.s("cliCommand", c);
    }
    crate::config::set_settings(&Value::obj().v("jarvis", patch.done()).done()).map_err(|e| VfsError::Io(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn the_conversation_is_started_with_the_files_and_not_in_print_mode() {
        let prompt = opening_prompt(&["/w/footer.js".into()]);
        assert_eq!(prompt, "I have questions about `/w/footer.js`. Read it and wait for my first question.");
        assert!(opening_prompt(&["/a".into(), "/b".into()]).contains("`/a`, `/b`. Read them"));
        assert_eq!(interactive_for("anthropic", Some(&prompt)), Some(vec!["claude".to_string(), prompt.clone()]));
        assert_eq!(interactive_for("gemini", Some("x")), Some(vec!["gemini".to_string(), "-i".to_string(), "x".to_string()]));
        assert!(!interactive_for("openai", Some("x")).unwrap().contains(&"exec".to_string()), "exec is codex's print mode");
        // Nothing selected: the tool alone, in the folder.
        assert_eq!(interactive_for("anthropic", None), Some(vec!["claude".to_string()]));
        assert_eq!(interactive_for("gemini", None), Some(vec!["gemini".to_string()]));
        assert_eq!(interactive_for("nobody", Some("x")), None);
    }

    #[test]
    fn it_runs_in_the_desktops_terminal() {
        let prog = vec!["claude".to_string(), "hi".to_string()];
        let d = std::path::Path::new("/w/site");
        assert_eq!(terminal_argv(prog.clone(), d, true, Some("kitty".into())), ["xdg-terminal-exec", "--dir=/w/site", "claude", "hi"]);
        assert_eq!(terminal_argv(prog.clone(), d, false, Some("kitty".into())), ["kitty", "-e", "claude", "hi"]);
        assert_eq!(terminal_argv(prog, d, false, None), ["alacritty", "-e", "claude", "hi"]);
    }

    #[test]
    fn a_custom_command_is_run_as_written() {
        assert_eq!(custom_argv("mytool --chat {prompt}", Some("hi there")), Some(vec!["mytool".to_string(), "--chat".to_string(), "hi there".to_string()]));
        // Nothing selected: the word that would have held the prompt goes, the rest stays.
        assert_eq!(custom_argv("mytool --chat {prompt}", None), Some(vec!["mytool".to_string(), "--chat".to_string()]));
        assert_eq!(custom_argv("mytool", Some("hi")), Some(vec!["mytool".to_string()]));
        assert_eq!(custom_argv("  ", Some("hi")), None);
    }
}
