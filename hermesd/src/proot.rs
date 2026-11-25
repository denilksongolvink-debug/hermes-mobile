use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ProotInvocation {
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

/// Pure builder for the proot command line. No I/O.
/// native_dir: directory containing libproot-xed.so etc.
/// files_dir: app filesDir (writable).
/// rootfs: rootfs directory.
/// inner_cmd: command to run inside rootfs.
pub fn build_proot_command(
    native_dir: &str,
    files_dir: &str,
    rootfs: &str,
    inner_cmd: &str,
    cwd: &str,
) -> ProotInvocation {
    let tmp_dir = format!("{}/tmp", files_dir);
    let lib_dir = format!("{}/lib", files_dir);

    let mut args: Vec<String> = Vec::new();
    args.push("--kill-on-exit".into());
    args.push("-b".into());
    args.push("/dev/urandom:/dev/random".into());
    args.push("-b".into());
    args.push("/dev".into());
    args.push("-b".into());
    args.push("/proc".into());
    args.push("-b".into());
    args.push("/sys".into());
    args.push("-r".into());
    args.push(rootfs.to_string());
    args.push("-w".into());
    args.push(cwd.to_string());
    args.push("/bin/sh".into());
    args.push("-c".into());
    args.push(inner_cmd.to_string());

    let env = vec![
        ("PROOT_LOADER".into(), format!("{}/libproot.so", native_dir)),
        ("PROOT_LOADER32".into(), format!("{}/libproot32.so", native_dir)),
        ("PROOT_TMP_DIR".into(), tmp_dir),
        ("LD_LIBRARY_PATH".into(), lib_dir),
    ];

    ProotInvocation {
        program: format!("{}/libproot-xed.so", native_dir),
        args,
        env,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary_is_xed() {
        let inv = build_proot_command("/nd", "/fd", "/rf", "uname -a", "/");
        assert!(inv.program.ends_with("libproot-xed.so"));
    }

    #[test]
    fn has_kill_on_exit() {
        let inv = build_proot_command("/nd", "/fd", "/rf", "uname -a", "/");
        assert!(inv.args.iter().any(|a| a == "--kill-on-exit"));
    }

    #[test]
    fn env_proot_loader() {
        let inv = build_proot_command("/nd", "/fd", "/rf", "uname -a", "/");
        let loader = inv.env.iter().find(|(k, _)| k == "PROOT_LOADER").unwrap();
        assert_eq!(loader.1, "/nd/libproot.so");
    }

    #[test]
    fn env_proot_tmp_dir() {
        let inv = build_proot_command("/nd", "/fd", "/rf", "uname -a", "/");
        let tmp = inv.env.iter().find(|(k, _)| k == "PROOT_TMP_DIR").unwrap();
        assert_eq!(tmp.1, "/fd/tmp");
    }

    #[test]
    fn env_ld_library_path() {
        let inv = build_proot_command("/nd", "/fd", "/rf", "uname -a", "/");
        let ld = inv.env.iter().find(|(k, _)| k == "LD_LIBRARY_PATH").unwrap();
        assert_eq!(ld.1, "/fd/lib");
    }

    #[test]
    fn binds_urandom() {
        let inv = build_proot_command("/nd", "/fd", "/rf", "uname -a", "/");
        assert!(inv.args.iter().any(|a| a == "/dev/urandom:/dev/random"));
    }

    #[test]
    fn no_sdcard_or_data_bind() {
        let inv = build_proot_command("/nd", "/fd", "/rf", "uname -a", "/");
        assert!(!inv.args.iter().any(|a| a == "/sdcard"));
        assert!(!inv.args.iter().any(|a| a == "/data"));
    }
}
