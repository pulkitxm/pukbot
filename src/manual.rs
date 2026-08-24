use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use clap::{Command, CommandFactory};

use crate::Cli;

pub fn render() -> Result<String> {
    let mut output = Vec::new();
    clap_mangen::Man::new(Cli::command()).render(&mut output)?;
    String::from_utf8(output).context("manual page was not valid UTF-8")
}

pub fn write_all(directory: &Path) -> Result<Vec<String>> {
    fs::create_dir_all(directory)
        .with_context(|| format!("failed to create {}", directory.display()))?;
    let mut paths = Vec::new();
    write_command(Cli::command(), "pukbot", directory, &mut paths)?;
    Ok(paths)
}

fn write_command(
    mut command: Command,
    name: &str,
    directory: &Path,
    paths: &mut Vec<String>,
) -> Result<()> {
    let children = command.get_subcommands().cloned().collect::<Vec<_>>();
    command = command.name(name.to_owned()).bin_name(name.to_owned());
    let path = directory.join(format!("{}.1", name.replace(' ', "-")));
    let mut output = Vec::new();
    clap_mangen::Man::new(command).render(&mut output)?;
    fs::write(&path, output).with_context(|| format!("failed to write {}", path.display()))?;
    paths.push(path.display().to_string());
    for child in children {
        let child_name = format!("{name} {}", child.get_name());
        write_command(child, &child_name, directory, paths)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::write_all;

    #[test]
    fn writes_nested_manual_pages() {
        let directory = tempfile::tempdir().expect("temporary directory should be created");
        let paths = write_all(directory.path()).expect("manuals should render");
        assert!(paths.iter().any(|path| path.ends_with("pukbot.1")));
        assert!(
            paths
                .iter()
                .any(|path| path.ends_with("pukbot-pr-review.1"))
        );
    }
}
