use std::thread::sleep;
use std::time::{Duration, Instant};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Cli {
    /// Monitor a specific PID
    #[arg(short, long, value_delimiter = ',')]
    pid: Option<Vec<usize>>,

    /// Monitor a specific command
    #[arg(short, long, value_delimiter = ',')]
    command: Option<Vec<String>>,

    /// Add a command to watch
    #[arg(short, long, value_delimiter = ',')]
    additional_command: Option<Vec<String>>,

    /// Wait to estimate throughput
    #[arg(short, long)]
    wait: bool,

    /// Wait a specified delay to estimate throughput
    #[arg(short = 'W', long)]
    wait_delay: Option<f64>,
}

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

#[derive(Clone, Debug)]
struct FD {
    _id: usize,
    fd_info: PathBuf,
    name: Option<String>,
    size: u64,
    pos: u64,
    flags: FDFlags,
    speed: Option<u64>,
    last_scan: Instant,
}

#[derive(Clone, Debug, PartialEq, Eq)]
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

        let (pos, flags_u64) = match Self::read_fdinfo(fd_info.clone()) {
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
            speed: None,
            last_scan: Instant::now(),
            fd_info,
        })
    }

    pub fn update(&mut self) -> bool {
        let (pos, _) = match Self::read_fdinfo(self.fd_info.clone()) {
            Ok(v) => v,
            Err(_) => return false,
        };

        let elapsed = self.last_scan.elapsed();
        let diff = pos - self.pos;
        self.speed = Some((diff as f64 / elapsed.as_secs_f64()) as u64);

        self.pos = pos;
        self.last_scan = Instant::now();

        true
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

    pub fn progress(&self) -> f32 {
        (self.pos as f32) / self.size as f32
    }

    pub fn speed(&self) -> Option<u64> {
        self.speed
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

    fn update(&mut self) {
        self.fd.retain_mut(|x| x.update());
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

        let speed = match fd_read.unwrap().speed() {
            Some(s) => format!("{}/s", format_size(s)),
            None => String::new(),
        };

        if let Some(fd) = fd_read {
            if fd.size > 0 {
                println!(
                    "\t{:.2}% ({} / {}) {}",
                    fd_read.unwrap().progress() * 100.,
                    format_size(fd.pos),
                    format_size(fd.size),
                    speed
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
    let cli = Cli::parse();

    let mut progs_to_watch: Vec<&str> = if let Some(prog_list) = &cli.command {
        Vec::from_iter(prog_list.iter().map(|x| x as &str))
    } else {
        PROGS.into()
    };

    if let Some(additional_commands) = &cli.additional_command {
        progs_to_watch.extend(additional_commands.iter().map(|x| x as &str));
    }

    let procs: Vec<usize> = if let Some(pids) = cli.pid {
        pids
    } else {
        fs::read_dir("/proc")
            .expect("procfs is not accessible")
            .filter_map(|x| x.ok())
            .filter_map(|x| x.file_name().into_string().unwrap().parse::<usize>().ok())
            .collect::<Vec<_>>()
    };

    let mut filtered_procs = procs
        .iter()
        .map(|pid| PathBuf::from("/proc").join(format!("{}", pid)))
        .map(|x| (x.clone(), get_proc_exe(&x)))
        .filter(|x| x.1.is_some())
        .filter(|x| progs_to_watch.iter().any(|p| *p == x.1.as_ref().unwrap()))
        .map(|x| Proc::new(x.1.unwrap(), x.0))
        .collect::<Vec<_>>();

    if cli.wait || cli.wait_delay.is_some() {
        let duration = match cli.wait_delay {
            Some(v) => v,
            None => 1.0,
        };

        sleep(Duration::from_secs_f64(duration));

        for p in &mut filtered_procs {
            p.update();
        }
    }

    for p in filtered_procs {
        p.print();
    }

    Ok(())
}
