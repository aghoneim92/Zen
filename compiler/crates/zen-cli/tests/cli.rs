use std::{fs, path::PathBuf, process::Command};
fn binary() -> Command {
    Command::new(env!("CARGO_BIN_EXE_zen"))
}
#[test]
fn exits_and_diagnostics() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../tests/fixtures");
    let valid = binary()
        .arg("check")
        .arg(root.join("valid/generic_identity.zen"))
        .output()
        .unwrap();
    assert!(
        valid.status.success(),
        "{}",
        String::from_utf8_lossy(&valid.stderr)
    );
    let invalid = binary()
        .arg("check")
        .arg(root.join("invalid/immutable_assignment.zen"))
        .output()
        .unwrap();
    assert_eq!(invalid.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("ZEN-TYPE-0004"));
    for flag in ["--help", "--version"] {
        assert!(binary().arg(flag).status().unwrap().success());
    }
    assert_eq!(
        binary().arg("build").output().unwrap().status.code(),
        Some(2)
    );
    assert_eq!(
        binary()
            .args(["check", "/does/not/exist.zen"])
            .output()
            .unwrap()
            .status
            .code(),
        Some(1)
    );
}
#[test]
fn module_loading() {
    let dir = std::env::temp_dir().join(format!("zen-cli-{}", std::process::id()));
    fs::create_dir_all(dir.join("src/models")).unwrap();
    fs::write(dir.join("src/models/user.zen"),"public struct User { public name: String; } public fn make() -> User { return User { name: \"Zen\" }; }").unwrap();
    fs::write(dir.join("src/main.zen"),"import models.user.User as Person; import models.user.make; fn main() -> Unit { let p: Person = make(); let s = p.name; }").unwrap();
    let result = binary()
        .arg("check")
        .arg(dir.join("src/main.zen"))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    fs::write(
        dir.join("src/main.zen"),
        "import models.user.User; impl User { fn injected(self) -> Unit {} }",
    )
    .unwrap();
    let result = binary()
        .arg("check")
        .arg(dir.join("src/main.zen"))
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("ZEN-IMPL-0003"));
    fs::write(dir.join("src/main.zen"), [0xff, 0xfe]).unwrap();
    let result = binary()
        .arg("check")
        .arg(dir.join("src/main.zen"))
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&result.stderr).contains("ZEN-IO-0001"));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn formatter_files_directories_and_check() {
    let dir = std::env::temp_dir().join(format!("zen-fmt-cli-{}", std::process::id()));
    fs::create_dir_all(dir.join("src")).unwrap();
    let file = dir.join("src/main.zen");
    let ugly = "fn f()->Unit{let x=1; // keep مرحبا 😀\n}";
    fs::write(&file, ugly).unwrap();
    for ignored in [".hidden", "target", "vendor", "generated", "node_modules"] {
        fs::create_dir_all(dir.join(ignored)).unwrap();
        fs::write(dir.join(ignored).join("bad.zen"), "fn invalid").unwrap();
    }
    let check = binary()
        .args(["fmt", "--check"])
        .arg(&dir)
        .output()
        .unwrap();
    assert_eq!(check.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&check.stdout).contains("main.zen"));
    assert_eq!(fs::read_to_string(&file).unwrap(), ugly);
    assert!(
        binary()
            .arg("fmt")
            .arg(&dir)
            .arg(&file)
            .status()
            .unwrap()
            .success()
    );
    let expected = "fn f() -> Unit {\n    let x = 1; // keep مرحبا 😀\n}\n";
    assert_eq!(fs::read_to_string(&file).unwrap(), expected);
    let modified = fs::metadata(&file).unwrap().modified().unwrap();
    assert!(binary().arg("fmt").arg(&file).status().unwrap().success());
    assert_eq!(modified, fs::metadata(&file).unwrap().modified().unwrap());
    assert!(
        binary()
            .args(["fmt", "--check"])
            .arg(&dir)
            .status()
            .unwrap()
            .success()
    );
    fs::write(&file, "fn broken {").unwrap();
    let failed = binary().arg("fmt").arg(&file).output().unwrap();
    assert_eq!(failed.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&failed.stderr).contains("ZEN-PARSE"));
    assert_eq!(fs::read_to_string(&file).unwrap(), "fn broken {");
    assert_eq!(
        binary()
            .args(["fmt", "--check"])
            .arg(&file)
            .output()
            .unwrap()
            .status
            .code(),
        Some(2)
    );
    assert_eq!(fs::read_dir(dir.join("src")).unwrap().count(), 1);
    fs::remove_dir_all(dir).unwrap();
}
#[test]
fn formatter_stdin() {
    use std::{io::Write, process::Stdio};
    for (input, expected, code) in [
        ("fn f()->Unit{}", "fn f() -> Unit {\n}\n", 0),
        ("fn broken", "", 2),
    ] {
        let mut child = binary()
            .args(["fmt", "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert_eq!(output.status.code(), Some(code));
        assert_eq!(String::from_utf8(output.stdout).unwrap(), expected);
        assert_eq!(output.stderr.is_empty(), code == 0);
    }
}

#[test]
fn reference_run_pipeline_and_exit_behavior() {
    let dir = std::env::temp_dir().join(format!("zen-run-cli-{}", std::process::id()));
    fs::create_dir_all(dir.join("src")).unwrap();
    let entry = dir.join("src/entry.zen");
    fs::write(
        dir.join("src/library.zen"),
        "public fn value() -> Int { return 42; } public fn main() -> Unit {}",
    )
    .unwrap();
    fs::write(
        &entry,
        "import library.value; fn main() -> Int { return value(); }",
    )
    .unwrap();
    let result = binary().arg("run").arg(&entry).output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(result.stdout.is_empty());
    for source in [
        "fn main() -> Bool { return false; }",
        "fn main() -> String { return \"zen\"; }",
        "fn main() -> Unit {}",
        "async fn main() -> Unit {}",
        "async fn answer() -> Int { return 42; } async fn main() -> Int { return await answer(); }",
        "async fn main() -> Bool { return false; }",
        "async fn main() -> String { return \"zen\"; }",
    ] {
        fs::write(&entry, source).unwrap();
        assert!(
            binary()
                .arg("run")
                .arg(&entry)
                .output()
                .unwrap()
                .status
                .success()
        );
    }
    for source in ["fn main() -> Int { return true; }", "fn main( {"] {
        fs::write(&entry, source).unwrap();
        let check = binary().arg("check").arg(&entry).output().unwrap();
        let run = binary().arg("run").arg(&entry).output().unwrap();
        assert_eq!(run.status.code(), Some(1));
        assert_eq!(check.stderr, run.stderr);
    }
    for (source, expected) in [
        (
            "import library.value; fn other() -> Int { return value(); }",
            "no main",
        ),
        ("fn main(x: Int) -> Unit {}", "no parameters"),
        (
            "native fn missing() -> Unit; fn main() -> Unit { missing(); }",
            "no reference-interpreter implementation",
        ),
        (
            "native async fn missing() -> Unit; async fn main() -> Unit { await missing(); }",
            "no reference-interpreter implementation",
        ),
    ] {
        fs::write(&entry, source).unwrap();
        let run = binary().arg("run").arg(&entry).output().unwrap();
        assert_eq!(run.status.code(), Some(1));
        assert!(
            String::from_utf8_lossy(&run.stderr).contains(expected),
            "{}",
            String::from_utf8_lossy(&run.stderr)
        );
    }
    assert_eq!(binary().arg("run").output().unwrap().status.code(), Some(2));
    assert!(
        binary()
            .args(["run", "--help"])
            .output()
            .unwrap()
            .status
            .success()
    );
    assert_eq!(
        binary()
            .args(["run", "/not/a/program.zen"])
            .output()
            .unwrap()
            .status
            .code(),
        Some(1)
    );
    fs::remove_dir_all(dir).unwrap();
}
