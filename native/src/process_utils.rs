#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[cfg(target_os = "linux")]
fn configure_parent_death_signal_std(command: &mut std::process::Command) {
    use std::os::unix::process::CommandExt;

    let parent_pid = std::process::id();
    unsafe {
        command.pre_exec(move || {
            if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            if libc::getppid() != parent_pid as libc::pid_t {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Interrupted,
                    "parent process exited before exec",
                ));
            }
            Ok(())
        });
    }
}

#[cfg(not(target_os = "linux"))]
fn configure_parent_death_signal_std(_command: &mut std::process::Command) {}

#[cfg(target_os = "windows")]
pub fn configure_std_command(command: &mut std::process::Command) {
    use std::os::windows::process::CommandExt;

    command.creation_flags(CREATE_NO_WINDOW);
    configure_parent_death_signal_std(command);
}

#[cfg(not(target_os = "windows"))]
pub fn configure_std_command(command: &mut std::process::Command) {
    configure_parent_death_signal_std(command);
}

#[cfg(target_os = "linux")]
fn configure_parent_death_signal_tokio(command: &mut tokio::process::Command) {
    let parent_pid = std::process::id();
    unsafe {
        command.pre_exec(move || {
            if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            if libc::getppid() != parent_pid as libc::pid_t {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Interrupted,
                    "parent process exited before exec",
                ));
            }
            Ok(())
        });
    }
}

#[cfg(not(target_os = "linux"))]
fn configure_parent_death_signal_tokio(_command: &mut tokio::process::Command) {}

#[cfg(target_os = "windows")]
pub fn configure_tokio_command(command: &mut tokio::process::Command) {
    use std::os::windows::process::CommandExt;

    command.creation_flags(CREATE_NO_WINDOW);
    configure_parent_death_signal_tokio(command);
}

#[cfg(not(target_os = "windows"))]
pub fn configure_tokio_command(command: &mut tokio::process::Command) {
    configure_parent_death_signal_tokio(command);
}
