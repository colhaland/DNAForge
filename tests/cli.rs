use std::path::PathBuf;
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

struct TestDir(PathBuf);

impl TestDir {
    fn new() -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("dnaforge-cli-test-{}-{unique}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

fn run(arguments: &[&std::ffi::OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_dnaforge"))
        .args(arguments)
        .output()
        .unwrap()
}

fn arg(value: &str) -> &std::ffi::OsStr {
    std::ffi::OsStr::new(value)
}

#[test]
fn encode_validate_decode_cli_round_trip() {
    let temp = TestDir::new();
    let source = temp.0.join("source.bin");
    let archive = temp.0.join("archive");
    let output = temp.0.join("decoded.bin");
    let input = vec![0x1b; 256];
    std::fs::write(&source, &input).unwrap();

    let encoded = run(&[
        arg("encode"),
        source.as_os_str(),
        arg("--output"),
        archive.as_os_str(),
        arg("--fragment-size"),
        arg("64"),
    ]);
    assert!(
        encoded.status.success(),
        "{}",
        String::from_utf8_lossy(&encoded.stderr)
    );

    let validated = run(&[arg("validate"), archive.as_os_str()]);
    assert!(
        validated.status.success(),
        "{}",
        String::from_utf8_lossy(&validated.stderr)
    );

    let decoded = run(&[
        arg("decode"),
        archive.as_os_str(),
        arg("--output"),
        output.as_os_str(),
    ]);
    assert!(
        decoded.status.success(),
        "{}",
        String::from_utf8_lossy(&decoded.stderr)
    );
    assert_eq!(std::fs::read(output).unwrap(), input);
}

#[test]
fn simulate_and_recover_cli_preserve_duplicate_identity() {
    let temp = TestDir::new();
    let source = temp.0.join("source.bin");
    let archive = temp.0.join("archive");
    let damaged = temp.0.join("observations");
    let output = temp.0.join("recovered.bin");
    std::fs::write(&source, (0..=127).collect::<Vec<u8>>()).unwrap();

    let encoded = run(&[
        arg("encode"),
        source.as_os_str(),
        arg("--output"),
        archive.as_os_str(),
    ]);
    assert!(encoded.status.success());

    let simulated = run(&[
        arg("simulate"),
        archive.as_os_str(),
        arg("--output"),
        damaged.as_os_str(),
        arg("--dup"),
        arg("1"),
        arg("--seed"),
        arg("5"),
    ]);
    assert!(
        simulated.status.success(),
        "{}",
        String::from_utf8_lossy(&simulated.stderr)
    );

    let recovered = run(&[
        arg("recover"),
        damaged.as_os_str(),
        arg("--output"),
        output.as_os_str(),
        arg("--original"),
        source.as_os_str(),
    ]);
    assert!(
        recovered.status.success(),
        "{}",
        String::from_utf8_lossy(&recovered.stderr)
    );
    assert_eq!(
        std::fs::read(output).unwrap(),
        std::fs::read(source).unwrap()
    );
}

#[test]
fn benchmark_uses_file_by_default_and_allows_explicit_synthetic_input() {
    let temp = TestDir::new();
    let source = temp.0.join("source.bin");
    let report = temp.0.join("actual.csv");
    let synthetic_report = temp.0.join("synthetic.json");
    std::fs::write(&source, vec![0x5a; 128]).unwrap();

    let actual = run(&[
        arg("benchmark"),
        source.as_os_str(),
        arg("--report"),
        report.as_os_str(),
        arg("--seeds"),
        arg("7"),
    ]);
    assert!(
        actual.status.success(),
        "{}",
        String::from_utf8_lossy(&actual.stderr)
    );
    let csv = std::fs::read_to_string(report).unwrap();
    assert!(csv.contains(",7,128,512,"));
    assert!(!csv.contains(source.to_str().unwrap()));

    let synthetic = run(&[
        arg("benchmark"),
        temp.0.join("not-read.bin").as_os_str(),
        arg("--report"),
        synthetic_report.as_os_str(),
        arg("--synthetic-sizes"),
        arg("16"),
        arg("--seeds"),
        arg("7"),
    ]);
    assert!(
        synthetic.status.success(),
        "{}",
        String::from_utf8_lossy(&synthetic.stderr)
    );
    let json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(synthetic_report).unwrap()).unwrap();
    assert_eq!(json["rows"][0]["input_bytes"], 16);
    assert_eq!(json["rows"][0]["seed"], 7);
}
