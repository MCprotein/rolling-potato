fn snapshot_tree(root: &std::path::Path) -> BTreeMap<String, Vec<u8>> {
    fn visit(root: &std::path::Path, path: &std::path::Path, out: &mut BTreeMap<String, Vec<u8>>) {
        let entries = match std::fs::read_dir(path) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
            Err(error) => panic!("tree snapshot read failed: {error}"),
        };
        let mut entries = entries.map(Result::unwrap).collect::<Vec<_>>();
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            let path = entry.path();
            let relative = path.strip_prefix(root).unwrap().display().to_string();
            let metadata = std::fs::symlink_metadata(&path).unwrap();
            if metadata.file_type().is_symlink() {
                out.insert(
                    format!("symlink:{relative}"),
                    std::fs::read_link(&path)
                        .unwrap()
                        .display()
                        .to_string()
                        .into_bytes(),
                );
            } else if metadata.is_dir() {
                out.insert(format!("dir:{relative}"), Vec::new());
                visit(root, &path, out);
            } else {
                out.insert(format!("file:{relative}"), std::fs::read(&path).unwrap());
            }
        }
    }
    let mut out = BTreeMap::new();
    visit(root, root, &mut out);
    out
}
