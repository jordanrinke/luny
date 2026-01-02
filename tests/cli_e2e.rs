use std::process::Command;

use tempfile::TempDir;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_luny"))
}

#[test]
fn e2e_generate_creates_toon_files() {
    let temp_dir = TempDir::new().expect("temp dir");
    std::fs::write(
        temp_dir.path().join("main.ts"),
        "export const x = 1;\nexport function foo() { return 42; }\n",
    )
    .expect("write source");

    let status = bin()
        .args([
            "--root",
            temp_dir.path().to_string_lossy().as_ref(),
            "generate",
        ])
        .status()
        .expect("run luny");

    assert!(status.success());
    assert!(temp_dir.path().join(".ai/main.ts.toon").exists());
}

#[test]
fn e2e_generate_is_deterministic_for_same_inputs() {
    let temp_dir = TempDir::new().expect("temp dir");
    std::fs::create_dir_all(temp_dir.path().join("src")).expect("mkdir src");
    std::fs::write(
        temp_dir.path().join("src/a.ts"),
        "export const a = 1;\n",
    )
    .expect("write a.ts");
    std::fs::write(
        temp_dir.path().join("src/b.ts"),
        "export const b = 2;\n",
    )
    .expect("write b.ts");

    let root = temp_dir.path().to_string_lossy();

    let status1 = bin()
        .args(["--root", root.as_ref(), "generate", "src"])
        .status()
        .expect("run luny (1)");
    assert!(status1.success());

    let a1 = std::fs::read_to_string(temp_dir.path().join(".ai/src/a.ts.toon")).expect("read a1");
    let b1 = std::fs::read_to_string(temp_dir.path().join(".ai/src/b.ts.toon")).expect("read b1");

    // Remove outputs and re-run
    std::fs::remove_dir_all(temp_dir.path().join(".ai")).expect("rm .ai");

    let status2 = bin()
        .args(["--root", root.as_ref(), "generate", "src"])
        .status()
        .expect("run luny (2)");
    assert!(status2.success());

    let a2 = std::fs::read_to_string(temp_dir.path().join(".ai/src/a.ts.toon")).expect("read a2");
    let b2 = std::fs::read_to_string(temp_dir.path().join(".ai/src/b.ts.toon")).expect("read b2");

    assert_eq!(a1, a2);
    assert_eq!(b1, b2);
}

#[test]
fn e2e_validate_fix_regenerates_invalid_toon_file() {
    let temp_dir = TempDir::new().expect("temp dir");
    std::fs::write(temp_dir.path().join("main.ts"), "export const x = 1;\n")
        .expect("write source");

    // Create an invalid TOON file (missing purpose)
    std::fs::create_dir_all(temp_dir.path().join(".ai")).expect("mkdir .ai");
    std::fs::write(
        temp_dir.path().join(".ai/main.ts.toon"),
        "tokens: ~50\nexports[1]: x(const)\n",
    )
    .expect("write invalid toon");

    let root = temp_dir.path().to_string_lossy();

    let fix_status = bin()
        .args(["--root", root.as_ref(), "validate", "--fix"])
        .status()
        .expect("run validate --fix");
    assert!(fix_status.success());

    let fixed = std::fs::read_to_string(temp_dir.path().join(".ai/main.ts.toon")).expect("read fixed");
    assert!(fixed.contains("purpose:"), "expected regenerated TOON to include purpose");

    // Strict validate should pass after fix.
    let strict_status = bin()
        .args(["--root", root.as_ref(), "validate", "--strict"])
        .status()
        .expect("run validate --strict");
    assert!(strict_status.success());
}


