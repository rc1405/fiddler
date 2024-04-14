use fs_extra::dir::copy;
use fs_extra::dir::CopyOptions;
use regex::Regex;
use std::{
    env,
    fs::{self, File, ReadDir},
    io::{LineWriter, Write},
    path::{Path, PathBuf},
};

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let module_path = Path::new(".").join("src").join("modules");

    let input_directories = fs::read_dir(module_path.clone().join("inputs")).unwrap();
    let processing_directories = fs::read_dir(module_path.clone().join("processors")).unwrap();
    let output_directories = fs::read_dir(module_path.clone().join("outputs")).unwrap();

    let opts = CopyOptions::new().overwrite(true);
    copy(module_path.clone(), out_dir.clone(), &opts).unwrap();

    generate_mod_rs(
        Path::new(&out_dir)
            .join("modules")
            .join("inputs")
            .join("mod.rs"),
        input_directories,
    );
    generate_mod_rs(
        Path::new(&out_dir)
            .join("modules")
            .join("processors")
            .join("mod.rs"),
        processing_directories,
    );
    generate_mod_rs(
        Path::new(&out_dir)
            .join("modules")
            .join("outputs")
            .join("mod.rs"),
        output_directories,
    );

    let input_file = File::create(Path::new(&out_dir).join("modules").join("mod.rs")).unwrap();
    let mut input_file = LineWriter::new(input_file);
    input_file
        .write_all("pub mod inputs;\n".as_bytes())
        .unwrap();
    input_file
        .write_all("pub mod outputs;\n".as_bytes())
        .unwrap();
    input_file
        .write_all("pub mod processors;\n".as_bytes())
        .unwrap();
    input_file.flush().unwrap();

    // check for certain dependencies
    let target = env::var("TARGET").unwrap();
    let windows = target.contains("windows");

    if env::var("CARGO_FEATURE_PYTHON").is_ok() {
        if windows {
            println!(
                "cargo:warning=Skipping check for python as pkg-config isn't available on windows",
            )
        } else {
            find_python_pkg()
        }
    }
}

fn generate_mod_rs(out_dir: PathBuf, input_directories: ReadDir) {
    let mut mods = Vec::new();

    let input_file = File::create(Path::new(&out_dir)).unwrap();
    let mut input_file = LineWriter::new(input_file);
    input_file
        .write_all("use crate::Error;\n".as_bytes())
        .unwrap();

    for i in input_directories {
        let path = i.unwrap();
        let file_n = path.file_name();
        let file_name = file_n.to_str().unwrap();
        if file_name.ends_with(".rs") {
            continue;
        };
        if contains_registration_function(path.path()) {
            mods.push(file_name.to_string());
            input_file
                .write_all(format!("pub mod {};\n", file_name).as_bytes())
                .unwrap();
        };
    }

    input_file
        .write_all("\n\npub fn register_plugins() -> Result<(), Error> {\n".as_bytes())
        .unwrap();

    for i in mods {
        input_file
            .write_all(format!("\t{}::fiddler_register_plugin()?;\n", i).as_bytes())
            .unwrap();
    }

    input_file.write_all("\tOk(())\n}\n".as_bytes()).unwrap();
    input_file.flush().unwrap();
}

fn contains_registration_function(module: PathBuf) -> bool {
    let listing = fs::read_dir(module.clone()).unwrap();
    let re = Regex::new(r"#\[fiddler_registration_func\]|fn fiddler_register_plugin\(\)").unwrap();
    let feature_re =
        Regex::new(r#"#\[cfg_attr\(feature = "(\w+)", fiddler_registration_func\)\]"#).unwrap();

    for i in listing {
        let path = i.unwrap();
        let file_n = path.file_name();
        let file_name = file_n.to_str().unwrap();
        if !file_name.ends_with(".rs") {
            continue;
        };
        println!("cargo:rerun-if-changed={}", path.path().to_str().unwrap());

        let contents = fs::read_to_string(path.path()).unwrap();
        if re.is_match(&contents) {
            return true;
        } else if feature_re.is_match(&contents) {
            let captures = feature_re.captures(&contents).unwrap();
            let feature = captures.extract::<1>().1[0];
            if env::var(format!(
                "CARGO_FEATURE_{}",
                feature.to_uppercase().replace('-', "_")
            ))
            .is_ok()
            {
                return true;
            } else {
                println!(
                    "cargo:warning=Skipping {} due to feature not being enabled.",
                    module.clone().to_str().unwrap()
                )
            }
        } else {
            println!(
                "cargo:warning=Skipping {} due to no presence of fiddler registration function.",
                module.clone().to_str().unwrap()
            )
        };
    }

    false
}

fn find_python_pkg() {
    match pkg_config::Config::new()
        .atleast_version("3.7.0")
        .probe("python3")
    {
        Ok(_lib) => {}
        Err(e) => {
            panic!(
                "
Failed to run pkg-config:
{:?}

You can try fixing this by installing pkg-config:

    # On Ubuntu
    sudo apt install pkg-config
    # On Arch Linux
    sudo pacman -S pkgconf
    # On Fedora
    sudo dnf install pkgconf-pkg-config

Installing Python:
    # On Ubuntu
    sudo apt install python3
    # On Arch Linux
    sudo pacman -S python3
    # On Fedora
    sudo dnf install python3
    # On Windows
    https://www.python.org/downloads/
",
                e
            );
        }
    }
}
