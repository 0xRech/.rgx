use rgx::benchmark::{self, BenchmarkOptions};
use std::fs;
use tempfile::tempdir;

#[test]
fn benchmark_runs_rgx_and_zip_without_external_tools() {
    let temp = tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir_all(source.join("nested")).unwrap();

    let repeated = vec![b'A'; 512 * 1024];
    fs::write(source.join("alpha.bin"), &repeated).unwrap();
    fs::write(source.join("nested/beta.bin"), &repeated).unwrap();
    fs::write(source.join("note.txt"), b"RGX benchmark test\n").unwrap();

    let report = benchmark::run(
        &source,
        &BenchmarkOptions {
            level: 3,
            include_private: false,
            include_7zip: false,
        },
    )
    .unwrap();

    assert_eq!(report.files, 3);
    assert_eq!(
        report.input_bytes,
        (repeated.len() * 2 + b"RGX benchmark test\n".len()) as u64
    );
    assert_eq!(report.results.len(), 2);
    assert_eq!(report.results[0].name, "RGX");
    assert_eq!(report.results[1].name, "ZIP (Deflate)");
    assert!(report.results.iter().all(|result| result.archive_bytes > 0));
    assert!(report.rgx_deduplicated_bytes >= repeated.len() as u64);
    assert!(report.skipped.is_empty());
}
