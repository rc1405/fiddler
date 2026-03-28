use std::process::Command;

fn fiddler_script_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_fiddler-script"))
}

#[test]
fn run_script_file() {
    let output = fiddler_script_bin()
        .arg("tests/scripts/hello.fs")
        .output()
        .expect("failed to execute");

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "Hello, World!"
    );
}

#[test]
fn run_script_file_not_found() {
    let output = fiddler_script_bin()
        .arg("nonexistent.fs")
        .output()
        .expect("failed to execute");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Error reading"));
}

#[test]
fn run_script_with_error() {
    let output = fiddler_script_bin()
        .arg("tests/scripts/error.fs")
        .output()
        .expect("failed to execute");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("undefined_var"));
}

#[test]
fn eval_flag_prints_output() {
    let output = fiddler_script_bin()
        .args(["-e", "print(1 + 2);"])
        .output()
        .expect("failed to execute");

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "3");
}

#[test]
fn eval_flag_reports_errors() {
    let output = fiddler_script_bin()
        .args(["-e", "let x = ;"])
        .output()
        .expect("failed to execute");

    assert!(!output.status.success());
}

#[test]
fn help_flag() {
    let output = fiddler_script_bin()
        .arg("--help")
        .output()
        .expect("failed to execute");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("FiddlerScript"));
}

#[test]
fn version_flag() {
    let output = fiddler_script_bin()
        .arg("--version")
        .output()
        .expect("failed to execute");

    assert!(output.status.success());
}

#[test]
fn run_from_stdin_pipe() {
    use std::io::Write;
    use std::process::Stdio;

    let mut child = fiddler_script_bin()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn");

    child
        .stdin
        .take()
        .expect("no stdin")
        .write_all(b"print(\"from stdin\");")
        .expect("write failed");

    let output = child.wait_with_output().expect("failed to wait");
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "from stdin");
}
