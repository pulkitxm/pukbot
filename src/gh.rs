use std::process::Command;
use std::sync::OnceLock;

static TOKEN: OnceLock<String> = OnceLock::new();

pub fn command() -> Command {
    let mut command = Command::new("gh");
    authorize(&mut command);
    command
}

pub fn authorize(command: &mut Command) {
    if let Some(token) = TOKEN.get() {
        command.env_remove("GITHUB_TOKEN").env("GH_TOKEN", token);
    }
}
