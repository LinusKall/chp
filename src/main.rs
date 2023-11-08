use std::fs::{create_dir_all, read_to_string, OpenOptions};
use std::io::Write;
use std::process::Command as TerminalCommand;
use std::{env::current_dir, path::PathBuf};

use clap::{Parser, Subcommand};
use color_eyre::eyre::{Report, Result};
use serde::Deserialize;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// This is the command
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Initialize a C++ project in the current directory
    /// with the name of the current directory.
    Init,

    /// Create a C++ project in a new directory
    /// called `name`.
    New {
        /// This is the name of the new C++ project.
        name: String,
    },

    /// Build and Run C++ project according to chp.toml.
    Run {
        /// The release flag enables the release profile (uses debug profile by default).
        #[arg(long)]
        release: bool,
    },

    /// Build C++ project according to chp.toml.
    Build {
        /// The release flag enables the release profile (uses debug profile by default).
        #[arg(long)]
        release: bool,
    },
}

fn main() -> Result<()> {
    color_eyre::install()?;

    let cli = Cli::parse();

    match cli.command {
        Command::Init => init_project(None),
        Command::New { name } => init_project(Some(name)),
        Command::Run { release } => build(release, true),
        Command::Build { release } => build(release, false),
    }
}

#[derive(Deserialize)]
struct Config {
    name: String,
    command: String,
    profiles: Profiles,
}

#[derive(Deserialize)]
struct Profiles {
    debug: Vec<String>,
    release: Vec<String>,
}

fn find_config() -> Result<Config> {
    for path in current_dir()?.ancestors() {
        let mut path_buf = path.to_path_buf();
        path_buf.push("chp.toml");

        if let Ok(content) = read_to_string(&path_buf) {
            return Ok(toml::from_str(&content)?);
        }
    }

    Err(Report::msg("Could not find chp.toml"))
}

fn build(release: bool, maybe_run: bool) -> Result<()> {
    let current_dir = current_dir()?;
    let config = find_config()?;
    let args = if release {
        config.profiles.release
    } else {
        config.profiles.debug
    };

    if maybe_run {
        println!("Running {:?}", &current_dir);
    } else {
        println!("Building {:?}", &current_dir);
    }

    let output = TerminalCommand::new(config.command).args(args).output()?;

    if !output.stderr.is_empty() {
        std::io::stderr().write_all(&output.stderr)?;
        return Ok(());
    }

    if maybe_run {
        run(release)?;
    }
    Ok(())
}

fn run(release: bool) -> Result<()> {
    let mut current_dir = current_dir()?;
    let config = find_config()?;

    let args = std::env::args().skip_while(|arg| arg != "--");

    current_dir.push("build");
    if release {
        current_dir.push("release");
    } else {
        current_dir.push("debug");
    }
    current_dir.push(format!("{}.exe", config.name));

    let output = TerminalCommand::new(current_dir).args(args).output()?;

    std::io::stdout().write_all(&output.stdout)?;
    std::io::stderr().write_all(&output.stderr)?;

    Ok(())
}

fn init_project(maybe_name: Option<String>) -> Result<()> {
    let mut current_dir = current_dir()?;

    if let Some(name) = maybe_name {
        current_dir.push(name)
    }

    println!("{:?}", &current_dir);
    if let Ok(mut read) = current_dir.read_dir() {
        if read.next().is_some() {
            return Err(Report::msg(format!(
                "Project folder is not empty: {current_dir:?}"
            )));
        }
    }

    write_project(current_dir)
}

fn write_project(mut path: PathBuf) -> Result<()> {
    // Create project path, if it does not exist.
    create_dir_all(&path)?;

    let project_name = path
        .file_name()
        .expect("Project directory should be defined")
        .to_str()
        .expect("Should be able to convert &OsStr to &str")
        .to_owned();

    let output = TerminalCommand::new("git")
        .arg("init")
        .current_dir(&path)
        .output()?;

    if !output.stderr.is_empty() {
        std::io::stderr().write_all(&output.stderr)?;
        return Err(Report::msg("Could not initialize git"));
    }

    // Create chp configuration TOML file.
    path.push("chp.toml");
    {
        let mut config_file = OpenOptions::new().create(true).write(true).open(&path)?;
        config_file.write_all(CONFIG_FILE_CONTENT.replace("{}", &project_name).as_bytes())?;
    }
    path.pop();

    // Create main cpp file.
    path.push("src");
    {
        create_dir_all(&path)?;
        path.push("main.cpp");
        {
            let mut main_file = OpenOptions::new().create(true).write(true).open(&path)?;
            main_file.write_all(MAIN_FILE_CONTENT.as_bytes())?;
        }
        path.pop();
    }
    path.pop();

    // Create build directories
    path.push("build");
    {
        path.push("debug");
        create_dir_all(&path)?;
        path.pop();

        path.push("release");
        create_dir_all(&path)?;
        path.pop();
    }
    path.pop();

    Ok(())
}

const CONFIG_FILE_CONTENT: &str = r#"name = "{}"
command = "g++"

[profiles]
debug = [
    "-fdiagnostics-color=always",
    "-std=c++20", 
    "-Wall", 
    "-Wextra", 
    "-pedantic", 
    "-Weffc++", 
    "-Wsuggest-attribute=const", 
    "-fconcepts", 
    "-Og", 
    "-g", 
    "src/main.cpp", 
    "-o", 
    "build/debug/{}.exe"
]
release = [
    "-fdiagnostics-color=always",
    "-std=c++20", 
    "-Wall", 
    "-Wextra", 
    "-pedantic", 
    "-Weffc++", 
    "-Wsuggest-attribute=const", 
    "-fconcepts", 
    "-O2", 
    "-g", 
    "src/main.cpp", 
    "-o", 
    "build/release/{}.exe"
]
"#;
const MAIN_FILE_CONTENT: &str = r#"#include <iostream>

int main() {
    std::cout << "Hello, World" << std::endl;
}
"#;
