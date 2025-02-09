use std::{
    fs, io,
    path::{Path, PathBuf},
};

fn get_proc_exe(proc: &Path) -> Option<String> {
    let exe = proc.join("exe");

    let exe_path = fs::read_link(exe);

    if let Ok(path) = exe_path {
        if let Some(name) = path.file_name() {
            return Some(name.to_str().unwrap_or("Unknown").into());
        }
    }
    None
}

fn format_size(size: u64) -> String {
    const UNITS: &[&str] = &["B", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB"];

    let mut rounded_size: f64 = size as f64;

    let mut i = 0;
    while rounded_size >= 1024. && i < UNITS.len() {
        rounded_size /= 1024.;
        i += 1;
    }

    format!("{:.2}{}", rounded_size, UNITS[i])
}

#[derive(Debug)]
struct FD {
    _id: usize,
    name: Option<String>,
    size: u64,
    pos: u64,
    flags: FDFlags,
}

#[derive(Debug, PartialEq, Eq)]
enum FDFlags {
    ReadOnly,
    WriteOnly,
    ReadWrite,
}

impl FDFlags {
    fn from(flags: u64) -> Self {
        const O_ACCMODE: u64 = 0b11;
        const O_RDONLY: u64 = 0b0;
        const O_WRONLY: u64 = 0b1;
        const O_RDWR: u64 = 0b10;

        match flags & O_ACCMODE {
            O_RDONLY => Self::ReadOnly,
            O_WRONLY => Self::WriteOnly,
            O_RDWR => Self::ReadWrite,
            _ => Self::ReadOnly,
        }
    }
}

impl FD {
    pub fn new(proc: &Path, id: usize) -> Option<Self> {
        let fd = proc.join("fd").join(id.to_string());
        let fd_info = proc.join("fdinfo").join(id.to_string());

        let fd_link = if let Ok(link) = fs::read_link(fd.clone()) {
            link.to_str().map(|s| s.to_string())
        } else {
            return None;
        };

        let (pos, flags_u64) = match Self::read_fdinfo(fd_info) {
            Ok(v) => v,
            Err(_) => return None,
        };

        let flags = FDFlags::from(flags_u64);

        let fd_size = fs::metadata(fd).unwrap().len();

        Some(FD {
            _id: id,
            name: fd_link,
            pos,
            size: fd_size,
            flags,
        })
    }

    fn read_fdinfo(path: PathBuf) -> io::Result<(u64, u64)> {
        let infos = fs::read_to_string(path)?;

        let infos_split = infos.split("\n").filter_map(|line| line.split_once(":"));

        let pos = infos_split
            .clone()
            .filter(|line_split| line_split.0 == "pos")
            .map(|line_split| line_split.1.trim())
            .next();
        let flags = infos_split
            .filter(|line_split| line_split.0 == "flags")
            .map(|line_split| line_split.1.trim())
            .next();

        if let (Some(pos_value), Some(flags_value)) = (pos, flags) {
            Ok((
                pos_value.parse::<u64>().unwrap(),
                flags_value.parse::<u64>().unwrap(),
            ))
        } else {
            Err(io::Error::new(io::ErrorKind::Other, "something is broken"))
        }
    }

    fn progress(&self) -> f32 {
        (self.pos as f32) / self.size as f32
    }
}

#[derive(Debug)]
struct Proc {
    path: PathBuf,
    pid: usize,
    exe: String,
    fd: Vec<FD>,
}

impl Proc {
    fn new(exe: String, path: PathBuf) -> Self {
        let pid = path
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .parse::<usize>()
            .unwrap();
        let mut p = Proc {
            exe,
            path,
            fd: vec![],
            pid,
        };
        p.get_file_descriptors();
        p
    }

    fn get_file_descriptors(&mut self) {
        let fd_dir = self.path.join("fd");
        let fd = fs::read_dir(fd_dir)
            .unwrap()
            .filter_map(|x| x.ok())
            .filter_map(|x| x.file_name().into_string().unwrap().parse::<usize>().ok())
            .filter_map(|id| FD::new(&self.path, id))
            .collect::<Vec<_>>();
        self.fd = fd;
    }

    fn find_biggest_fd(&self, flag_type: FDFlags) -> Option<&FD> {
        self.fd
            .iter()
            .filter(|x| x.flags == flag_type)
            .max_by_key(|x| x.size)
    }

    fn print(&self) {
        let fd_read = self.find_biggest_fd(FDFlags::ReadOnly);
        let fd_write = self.find_biggest_fd(FDFlags::WriteOnly);
        println!(
            "[{}] {} {} > {}",
            self.pid,
            self.exe,
            fd_read.unwrap().name.as_ref().unwrap(),
            match fd_write {
                Some(fd) => fd.name.as_ref().unwrap(),
                None => "",
            }
        );

        if let Some(fd) = fd_read {
            if fd.size > 0 {
                println!(
                    "\t{:.2}% ({} / {})",
                    fd_read.unwrap().progress() * 100.,
                    format_size(fd.pos),
                    format_size(fd.size)
                );
            } else {
                println!("\tUnknown progress")
            }
        }

        println!();
    }
}

const PROGS: &[&str] = &["cp", "mv", "dd", "cat"];

fn main() -> io::Result<()> {
    let procs = fs::read_dir("/proc")
        .expect("procfs is not accessible")
        .filter_map(|x| x.ok())
        .filter(|x| {
            x.file_name()
                .into_string()
                .unwrap()
                .parse::<usize>()
                .is_ok()
        })
        .collect::<Vec<_>>();

    let filtered_procs = procs
        .iter()
        .map(|x| (x.path(), get_proc_exe(&x.path())))
        .filter(|x| x.1.is_some())
        .filter(|x| PROGS.iter().any(|p| *p == x.1.as_ref().unwrap()))
        .map(|x| Proc::new(x.1.unwrap(), x.0))
        .collect::<Vec<_>>();

    for p in filtered_procs {
        p.print();
    }

    Ok(())
}
