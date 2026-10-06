use std::path::PathBuf;

/// Whether a command names something that can actually be run.
pub fn exists(command: &str) -> bool {
    match program_of(command) {
        Some(program) => which(&program).is_some(),
        None => false,
    }
}

/// The full path of the program a command names, if this machine has it.
///
/// The command may be a bare program name to look for on PATH, an absolute path, or a
/// quoted path. Arguments after the program are ignored, so `"C:\Program
/// Files\agent.exe" acp` and `agent acp` both resolve to the program itself.
pub fn resolve(command: &str) -> Option<PathBuf> {
    let program = program_of(command)?;
    let candidate = PathBuf::from(&program);

    if candidate.components().count() > 1 || program.contains(['/', '\\']) {
        return candidate.is_file().then_some(candidate);
    }

    which(&program)
}

/// The program part of a command, ignoring any leading quotes and arguments.
fn program_of(command: &str) -> Option<String> {
    let trimmed = command.trim();
    let unquoted = trimmed.strip_prefix('"').unwrap_or(trimmed);
    let end = unquoted.find('"').unwrap_or(unquoted.len());
    unquoted[..end]
        .split_whitespace()
        .next()
        .filter(|program| !program.is_empty())
        .map(str::to_string)
}

fn which(program: &str) -> Option<PathBuf> {
    let extensions = executable_extensions();

    for directory in std::env::split_paths(&std::env::var_os("PATH")?) {
        let candidate = directory.join(program);
        if candidate.is_file() {
            return Some(candidate);
        }
        for extension in &extensions {
            let candidate = directory.join(format!("{program}.{extension}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// The extensions Windows treats as runnable, taken from the machine itself so this
/// does not hard-code a list that drifts.
fn executable_extensions() -> Vec<String> {
    if !cfg!(target_os = "windows") {
        return Vec::new();
    }
    std::env::var_os("PATHEXT")
        .map(|value| {
            std::env::split_paths(&value)
                .filter_map(|extension| {
                    let extension = extension.to_string_lossy().into_owned();
                    let extension = extension.trim_start_matches('.');
                    (!extension.is_empty()).then(|| extension.to_string())
                })
                .collect()
        })
        .unwrap_or_default()
}
