use std::env;

fn main() {
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
