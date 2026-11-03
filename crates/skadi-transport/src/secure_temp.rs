//! Безопасная запись конфигов дочерних процессов (hysteria2 / tuic / awg).
//!
//! Конфиги содержат plaintext-пароли, поэтому:
//! - базовый каталог — `$XDG_RUNTIME_DIR`, если он доступен (по спецификации
//!   он per-user с правами 0700), иначе `std::env::temp_dir()`;
//! - каталог создаётся/нормализуется с правами 0700 (unix);
//! - файлы пишутся с правами 0600 (unix) через `create_new` после удаления,
//!   чтобы ослабить symlink-гонки в общем `/tmp`.

use std::io;
use std::path::{Path, PathBuf};

/// Каталог для конфига дочернего процесса с ограниченными правами.
pub fn secure_conf_dir(name: &str) -> io::Result<PathBuf> {
    let base = runtime_base_dir();
    let dir = base.join(name);

    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        let mut builder = std::fs::DirBuilder::new();
        builder.recursive(true).mode(0o700);
        builder.create(&dir)?;
        // Каталог мог существовать ранее с ослабленными правами.
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
    }
    #[cfg(not(unix))]
    {
        std::fs::create_dir_all(&dir)?;
    }

    Ok(dir)
}

/// Записать конфиг с правами 0600 (unix). Содержит секреты — не для логов.
pub fn write_conf_file(path: &Path, contents: &str) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        // Ослабление symlink-гонки: сначала удалить, потом создать эксклюзивно.
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)?;
        file.write_all(contents.as_bytes())?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, contents)
    }
}

/// `$XDG_RUNTIME_DIR`, если задан и является каталогом; иначе `temp_dir()`.
fn runtime_base_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("XDG_RUNTIME_DIR") {
        let path = PathBuf::from(dir);
        if path.is_dir() {
            return path;
        }
    }
    std::env::temp_dir()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn conf_dir_and_file_have_restricted_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let name = format!("skadicore-secure-temp-test-{}", std::process::id());
        let dir = secure_conf_dir(&name).unwrap();
        let mode = std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);

        let file_path = dir.join("config.yaml");
        write_conf_file(&file_path, "password: secret").unwrap();
        let mode = std::fs::metadata(&file_path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        assert_eq!(std::fs::read_to_string(&file_path).unwrap(), "password: secret");

        // Перезапись существующего файла работает.
        write_conf_file(&file_path, "password: secret2").unwrap();
        assert_eq!(
            std::fs::read_to_string(&file_path).unwrap(),
            "password: secret2"
        );

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
