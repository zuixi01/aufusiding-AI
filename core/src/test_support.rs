use std::path::PathBuf;

use uuid::Uuid;

pub fn temp_path(prefix: &str) -> PathBuf {
    temp_file_path(prefix, "")
}

pub fn temp_file_path(prefix: &str, extension: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    let extension = extension.trim_start_matches('.');
    let file_name = if extension.is_empty() {
        format!("{prefix}-{}", Uuid::new_v4())
    } else {
        format!("{prefix}-{}.{}", Uuid::new_v4(), extension)
    };
    path.push(file_name);
    path
}

pub fn temp_db_path(prefix: &str) -> PathBuf {
    temp_file_path(prefix, "db")
}

pub fn temp_json_path(prefix: &str) -> PathBuf {
    temp_file_path(prefix, "json")
}

pub fn temp_dir_path(prefix: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!("{prefix}-{}", Uuid::new_v4()));
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temp_file_path_uses_system_temp_dir() {
        let path = temp_file_path("unit-test", "txt");
        assert!(path.starts_with(std::env::temp_dir()));
        assert_eq!(path.extension().unwrap(), "txt");
    }

    #[test]
    fn temp_dir_path_uses_system_temp_dir_without_extension() {
        let path = temp_dir_path("unit-test");
        assert!(path.starts_with(std::env::temp_dir()));
        assert!(path.extension().is_none());
    }
}
