use std::{env, fs::{self, File, ReadDir}, io::{LineWriter, Write}, path::{Path, PathBuf}};
use fs_extra::dir::copy;
use fs_extra::dir::CopyOptions;
use regex::Regex;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let module_path = Path::new(".").join("src").join("modules");

    let input_directories = fs::read_dir(module_path.clone().join("inputs")).unwrap();
    let processing_directories = fs::read_dir(module_path.clone().join("processors")).unwrap();
    let output_directories = fs::read_dir(module_path.clone().join("outputs")).unwrap();

    let opts = CopyOptions::new().overwrite(true);
    copy(module_path.clone(), out_dir.clone(), &opts).unwrap();

    generate_mod_rs(Path::new(&out_dir).join("modules").join("inputs").join("mod.rs"), input_directories);
    generate_mod_rs(Path::new(&out_dir).join("modules").join("processors").join("mod.rs"), processing_directories);
    generate_mod_rs(Path::new(&out_dir).join("modules").join("outputs").join("mod.rs"), output_directories);
    
    let input_file = File::create(Path::new(&out_dir).join("modules").join("mod.rs")).unwrap();
    let mut input_file = LineWriter::new(input_file);
    input_file.write_all("pub mod inputs;\n".as_bytes()).unwrap();
    input_file.write_all("pub mod outputs;\n".as_bytes()).unwrap();
    input_file.write_all("pub mod processors;\n".as_bytes()).unwrap();
    input_file.flush().unwrap();
}

fn generate_mod_rs(out_dir: PathBuf, input_directories: ReadDir) {
    let mut mods = Vec::new();

    let input_file = File::create(Path::new(&out_dir)).unwrap();
    let mut input_file = LineWriter::new(input_file);
    input_file.write_all("use crate::Error;\n".as_bytes()).unwrap();

    for i in input_directories {
        let path = i.unwrap();
        let file_n = path.file_name();
        let file_name = file_n.to_str().unwrap();
        if file_name.ends_with(".rs") {
            continue;
        };
        if contains_registration_function(path.path()) {
            mods.push(format!("{}", file_name.to_string()));
            input_file.write_all(format!("pub mod {};\n", file_name).as_bytes()).unwrap();
        } ;
    };

    input_file.write_all("\n\npub fn register_plugins() -> Result<(), Error> {\n".as_bytes()).unwrap();

    for i in mods {
        input_file.write_all(format!("\t{}::fiddler_register_plugin()?;\n", i).as_bytes()).unwrap();
    };

    input_file.write_all("\tOk(())\n}\n".as_bytes()).unwrap();
    input_file.flush().unwrap();

}

fn contains_registration_function(module: PathBuf) -> bool {
    let listing = fs::read_dir(module.clone()).unwrap();
    let re = Regex::new(r"#\[fiddler_registration_func\]|fn fiddler_register_plugin\(\)").unwrap();

    for i in listing {
        let path = i.unwrap();
        let file_n = path.file_name();
        let file_name = file_n.to_str().unwrap();
        if !file_name.ends_with(".rs") {
            continue;
        };

        let contents = fs::read_to_string(path.path()).unwrap();
        if re.is_match(&contents) {
            return true
        } else {
            println!("cargo:warning=Skipping {} due to no presence of fiddler registration function.", module.clone().to_str().unwrap())
        };
    }

    false
}