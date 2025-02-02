#[macro_use]
extern crate bitflags;

use libc::close;
use std::io::Error;
use std::mem::size_of_val;
use std::os::unix::io::{AsRawFd, RawFd};

mod uapi;

bitflags! {
    pub struct AccessFs: u64 {
        const EXECUTE = uapi::LANDLOCK_ACCESS_FS_EXECUTE as u64;
        const WRITE_FILE = uapi::LANDLOCK_ACCESS_FS_WRITE_FILE as u64;
        const READ_FILE = uapi::LANDLOCK_ACCESS_FS_READ_FILE as u64;
        const READ_DIR = uapi::LANDLOCK_ACCESS_FS_READ_DIR as u64;
        const REMOVE_DIR = uapi::LANDLOCK_ACCESS_FS_REMOVE_DIR as u64;
        const REMOVE_FILE = uapi::LANDLOCK_ACCESS_FS_REMOVE_FILE as u64;
        const MAKE_CHAR = uapi::LANDLOCK_ACCESS_FS_MAKE_CHAR as u64;
        const MAKE_DIR = uapi::LANDLOCK_ACCESS_FS_MAKE_DIR as u64;
        const MAKE_REG = uapi::LANDLOCK_ACCESS_FS_MAKE_REG as u64;
        const MAKE_SOCK = uapi::LANDLOCK_ACCESS_FS_MAKE_SOCK as u64;
        const MAKE_FIFO = uapi::LANDLOCK_ACCESS_FS_MAKE_FIFO as u64;
        const MAKE_BLOCK = uapi::LANDLOCK_ACCESS_FS_MAKE_BLOCK as u64;
        const MAKE_SYM = uapi::LANDLOCK_ACCESS_FS_MAKE_SYM as u64;
    }
}

enum Rule {
    PathBeneath(uapi::landlock_path_beneath_attr),
}

impl Rule {
    fn as_ptr(&self) -> *const libc::c_void {
        match self {
            Rule::PathBeneath(attr) => attr as *const _ as _,
        }
    }
}

impl Into<uapi::landlock_rule_type> for &Rule {
    fn into(self) -> uapi::landlock_rule_type {
        match self {
            Rule::PathBeneath(_) => uapi::landlock_rule_type_LANDLOCK_RULE_PATH_BENEATH,
        }
    }
}

fn prctl_set_no_new_privs() -> Result<(), Error> {
    match unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) } {
        0 => Ok(()),
        _ => Err(Error::last_os_error()),
    }
}

pub struct RulesetAttr {
    handled_fs: AccessFs,
}

impl RulesetAttr {
    pub fn new() -> Self {
        // The API should be future-proof: one Rust program or library should have the same
        // behavior if builded with an old or a newer crate (e.g. with an extended ruleset_attr
        // enum).  It should then not be possible to give an "all-possible-handled-accesses" to the
        // Ruleset builder because this value would be relative to the running kernel.
        RulesetAttr {
            handled_fs: AccessFs::empty(),
        }
    }

    pub fn handle_fs(&mut self, access: AccessFs) -> &mut Self {
        self.handled_fs = access;
        self
    }

    pub fn create(&self) -> Result<Ruleset, Error> {
        // Without any handle_fs() call, will return -ENOMSG.
        Ruleset::new(self)
    }
}

pub struct Ruleset {
    fd: RawFd,
    no_new_privs: bool,
}

impl Ruleset {
    fn new(attribute: &RulesetAttr) -> Result<Self, Error> {
        let attr = uapi::landlock_ruleset_attr {
            handled_access_fs: attribute.handled_fs.bits,
        };

        match unsafe { uapi::landlock_create_ruleset(&attr, size_of_val(&attr), 0) } {
            fd if fd >= 0 => Ok(Ruleset {
                fd: fd,
                no_new_privs: true,
            }),
            _ => Err(Error::last_os_error()),
        }
    }

    fn add_rule(&mut self, rule: &Rule) -> Result<(), Error> {
        match unsafe { uapi::landlock_add_rule(self.fd, rule.into(), rule.as_ptr(), 0) } {
            0 => Ok(()),
            _ => Err(Error::last_os_error()),
        }
    }

    // Directly checks and uses the FD.
    pub fn add_path_beneath_rule<T>(mut self, parent: T, allowed: AccessFs) -> Result<Self, Error>
    where
        T: AsRawFd,
    {
        self.add_rule(&Rule::PathBeneath(uapi::landlock_path_beneath_attr {
            allowed_access: allowed.bits,
            parent_fd: parent.as_raw_fd(),
        }))?;
        Ok(self)
    }

    pub fn set_no_new_privs(mut self, no_new_privs: bool) -> Self {
        self.no_new_privs = no_new_privs;
        self
    }

    // Eager method, may not fit with all use-cases though.
    pub fn restrict_self(self) -> Result<(), Error> {
        if self.no_new_privs {
            prctl_set_no_new_privs()?;
        }
        match unsafe { uapi::landlock_restrict_self(self.fd, 0) } {
            0 => Ok(()),
            _ => Err(Error::last_os_error()),
        }
    }
}

impl Drop for Ruleset {
    fn drop(&mut self) {
        unsafe {
            close(self.fd);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;

    fn ruleset_root() -> Result<(), Error> {
        RulesetAttr::new()
            // FIXME: Make it impossible to use AccessFs::all()
            .handle_fs(AccessFs::all())
            .create()?
            .set_no_new_privs(true)
            .add_path_beneath_rule(File::open("/")?, AccessFs::all())?
            .restrict_self()
    }

    #[test]
    fn allow_root() {
        ruleset_root().unwrap()
    }
}
