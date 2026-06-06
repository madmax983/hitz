fn main() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let zeros = [0u8; 1024];
    std::fs::write(tmp.path(), &zeros).unwrap();
    let _ro_file = std::fs::OpenOptions::new().read(true).open(tmp.path()).unwrap();
}
