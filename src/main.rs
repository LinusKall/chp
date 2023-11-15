use std::ffi::OsStr;
use std::fs::{create_dir_all, read_dir, read_to_string, OpenOptions};
use std::io::Write;
use std::path::Path;
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

        /// Any argument passed after this flag is passed to your program.
        #[arg(value_parser, short, num_args = 1.., value_delimiter = ' ')]
        args: Vec<String>,
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
        Command::Run { release, args } => run(release, args),
        Command::Build { release } => build(release),
    }
}

#[derive(Deserialize)]
struct Config {
    name: String,
    command: String,
    compile_cpp_in_dirs: Option<Vec<String>>,
    profiles: Profiles,
}

#[derive(Deserialize)]
struct Profiles {
    debug: Vec<String>,
    release: Vec<String>,
}

fn find_root() -> Result<PathBuf> {
    for path in current_dir()?.ancestors() {
        let chp_exists = read_dir(path)?.any(|item| {
            if let Ok(entry) = item {
                entry.file_name().as_os_str() == "chp.toml"
            } else {
                false
            }
        });

        if chp_exists {
            return Ok(path.to_path_buf());
        }
    }

    Err(Report::msg("Could not find root (chp.toml not found)"))
}

fn read_config() -> Result<Config> {
    let mut chp_path = find_root()?;
    chp_path.push("chp.toml");

    if let Ok(content) = read_to_string(chp_path) {
        return Ok(toml::from_str(&content)?);
    }

    Err(Report::msg("Could not read chp.toml"))
}

fn find_cpp_files_in_dirs(maybe_dirs: Option<Vec<String>>) -> Result<Vec<PathBuf>> {
    let root = find_root()?;

    let mut cpp_files = Vec::new();

    for directory in maybe_dirs.unwrap_or_default() {
        let mut src_path = root.clone();
        src_path.push(directory);

        find_cpp_files_in_dirs_helper(&mut cpp_files, &src_path, &root)?;
    }

    Ok(cpp_files)
}

fn find_cpp_files_in_dirs_helper(
    cpp_files: &mut Vec<PathBuf>,
    dir: &Path,
    root: &Path,
) -> Result<()> {
    for entry in read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            find_cpp_files_in_dirs_helper(cpp_files, &path, root)?;
        }

        if Some(OsStr::new("cpp")) == path.extension() {
            cpp_files.push(path.strip_prefix(root)?.to_path_buf());
        }
    }
    Ok(())
}

fn build(release: bool) -> Result<()> {
    let current_dir = current_dir()?;
    let Config {
        command,
        compile_cpp_in_dirs,
        profiles,
        ..
    } = read_config()?;
    let args = if release {
        profiles.release
    } else {
        profiles.debug
    };

    println!("Building {:?}", &current_dir);

    let output = TerminalCommand::new(command)
        .args(find_cpp_files_in_dirs(compile_cpp_in_dirs)?)
        .args(args)
        .output()?;

    if !output.stderr.is_empty() {
        std::io::stderr().write_all(&output.stderr)?;
        return Ok(());
    }

    Ok(())
}

fn run(release: bool, args: Vec<String>) -> Result<()> {
    build(release)?;

    let mut current_dir = current_dir()?;
    let config = read_config()?;

    current_dir.push("build");
    if release {
        current_dir.push("release");
    } else {
        current_dir.push("debug");
    }
    current_dir.push(format!("{}.exe", config.name));

    println!("Running {:?}", &current_dir);

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
        .output()
        .map_err(|_| Report::msg("Git is not installed"))?;

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

# chp will recursively look for cpp files in these directories.
# This variable is optional, *if* you provide the files you want
# to compile in the debug and release profiles.
compile_cpp_in_dirs = [
    "src"
]

[profiles]
debug = [
    # All cpp files found in the directories provided in the 
    # `compile_cpp_in_dirs` list, will be inserted here.
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
    "-o", 
    "build/debug/{}.exe",
]
release = [
    # All cpp files found in the directories provided in the 
    # `compile_cpp_in_dirs` list, will be inserted here.
    "-fdiagnostics-color=always",
    "-std=c++20", 
    "-Wall", 
    "-Wextra", 
    "-pedantic", 
    "-Weffc++", 
    "-Wsuggest-attribute=const", 
    "-fconcepts", 
    "-O2", 
    "-o", 
    "build/release/{}.exe",
]
"#;
const MAIN_FILE_CONTENT: &str = r#"#include <iostream>

int main() {
    std::cout << "Hello, World" << std::endl;
}
"#;
