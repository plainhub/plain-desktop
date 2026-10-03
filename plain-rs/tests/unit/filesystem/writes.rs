use super::*;

#[test]
fn writes_preserve_existing_files_and_report_real_failures() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().join("parent/child");
    Operation::CreateDirectory {
        path: dir.to_str().unwrap().into(),
    }
    .apply()
    .unwrap();
    assert!(dir.is_dir());
    let path = dir.join("中文.txt").to_str().unwrap().to_owned();
    Operation::CreateFile { path: path.clone() }
        .apply()
        .unwrap();
    let original = "synthetic 中文\n";
    Operation::WriteText {
        path: path.clone(),
        content: original.into(),
        overwrite: true,
    }
    .apply()
    .unwrap();
    Operation::CreateFile { path: path.clone() }
        .apply()
        .unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), original);
    assert_eq!(
        Operation::WriteText {
            path: path.clone(),
            content: "rejected".into(),
            overwrite: false
        }
        .apply()
        .unwrap_err()
        .kind(),
        io::ErrorKind::AlreadyExists
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), original);
    Operation::WriteText {
        path: path.clone(),
        content: "short".into(),
        overwrite: true,
    }
    .apply()
    .unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "short");
    assert!(Operation::CreateDirectory { path }.apply().is_err());
    assert!(
        Operation::CreateFile {
            path: dir.to_str().unwrap().into()
        }
        .apply()
        .is_err()
    );
    assert!(
        Operation::CreateFile {
            path: "relative".into()
        }
        .apply()
        .is_err()
    );
    assert!(
        Operation::CreateDirectory { path: "/".into() }
            .apply()
            .is_err()
    );
}
