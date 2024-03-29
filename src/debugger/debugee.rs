use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;

use super::breakpoint::{Breakpoint, HardwareBreakpoint, SoftwareBreakpoint};

pub struct Debugee {
    pub stopped: bool,
    pid: u32,
    _waitpid_thread: JoinHandle<()>,
    pub waitpid_communication: (Sender<i32>, Receiver<i32>),
    breakpoints: Vec<Box<dyn Breakpoint>>,
    context: libc::user_regs_struct,
    hardware_breakpoints: usize,
}

impl Debugee {
    pub fn new(pid: u32) -> Result<Self, Box<dyn std::error::Error>> {
        //TODO: error check probably
        unsafe {
            libc::ptrace(libc::PTRACE_SEIZE, pid, 0, 0);
        }

        let (tx, rx) = mpsc::channel::<i32>();

        let sender = tx.clone();

        let _waitpid_thread = std::thread::spawn(move || waitpid_thread(pid, sender));

        Ok(Self {
            stopped: false,
            pid,
            _waitpid_thread,
            waitpid_communication: (tx, rx),
            breakpoints: Vec::new(),
            context: unsafe { std::mem::zeroed() }, //this is safe trust me :)
            hardware_breakpoints: 0,
        })
    }

    pub fn detach(&mut self) {
        unsafe {
            libc::ptrace(libc::PTRACE_DETACH, self.pid);
        }
    }

    /// Kills the process if it's already stopped (default ptrace behaviour)
    pub fn stop(&mut self) {
        unsafe {
            libc::ptrace(libc::PTRACE_INTERRUPT, self.pid, 0, 0);
        }
        self.update_context();
        self.stopped = true;
    }

    pub fn r#continue(&mut self) {
        self.update_context();
        unsafe {
            //yes, two calls are required
            libc::ptrace(libc::PTRACE_CONT, self.pid, 0, 0);
            libc::ptrace(libc::PTRACE_CONT, self.pid, 0, 0);
        }
        self.stopped = false;
    }

    pub fn single_step(&mut self) {
        unsafe {
            libc::ptrace(libc::PTRACE_SINGLESTEP, self.pid);
        }
        self.update_context();
    }

    //TODO: use /proc/<pid>/mem for io!!!

    pub fn write_memory(&self, address: usize, data: &[u8]) {
        for i in 0..(data.len() as f32 / 8.0).floor() as usize {
            unsafe {
                libc::ptrace(
                    libc::PTRACE_POKEDATA,
                    self.pid,
                    address + i * 8,
                    u64::from_le_bytes(data[i..i + 8].try_into().unwrap()),
                )
            };
        }

        let left_over = data.len() % 8;

        let mut original = self.read_memory(address - left_over, 8);
        original
            .iter_mut()
            .take(left_over)
            .enumerate()
            .for_each(|(i, x)| *x = data[data.len() - left_over + i]);

        unsafe {
            libc::ptrace(
                libc::PTRACE_POKEDATA,
                self.pid,
                address - left_over,
                u64::from_le_bytes(original.try_into().unwrap()),
            )
        };
    }

    pub fn read_memory(&self, address: usize, size: usize) -> Vec<u8> {
        let mut read = Vec::new();

        while read.len() < size {
            unsafe {
                read.extend_from_slice(
                    &libc::ptrace(libc::PTRACE_PEEKDATA, self.pid, address + read.len(), 0)
                        .to_le_bytes(),
                );
            }
        }

        read.into_iter().take(size).collect()
    }

    pub fn kill(&mut self) {
        unsafe {
            libc::kill(self.pid as i32, libc::SIGKILL);
            libc::ptrace(libc::PTRACE_DETACH, self.pid);
        }
    }

    pub fn update_context(&mut self) -> &libc::user_regs_struct {
        unsafe {
            libc::ptrace(
                libc::PTRACE_GETREGS,
                self.pid,
                0,
                &mut self.context as *mut _ as usize,
            );
        }
        &self.context
    }

    pub const fn context(&self) -> &libc::user_regs_struct {
        &self.context
    }

    pub fn write_user(&self, offset: usize, value: u64) {
        unsafe {
            libc::ptrace(libc::PTRACE_POKEUSER, self.pid, offset, value);
        }
    }

    pub fn read_user(&self, offset: usize) -> u64 {
        unsafe { libc::ptrace(libc::PTRACE_PEEKUSER, self.pid, offset, 0) as u64 }
    }

    pub fn breakpoints(&self) -> &Vec<Box<dyn Breakpoint>> {
        &self.breakpoints
    }

    pub fn breakpoint_at_address(&mut self, addr: u64) -> Option<&mut Box<dyn Breakpoint>> {
        self.breakpoints.iter_mut().find(|bp| bp.address() == addr)
    }

    pub fn add_software_breakpoint(&mut self, addr: u64 /*hardware: bool*/, size: u64) {
        let mut breakpoint = SoftwareBreakpoint::new(addr, size);
        breakpoint.enable(self);
        self.breakpoints.push(Box::new(breakpoint));
    }

    pub fn add_hardware_breakpoint(&mut self, addr: u64) {
        if self.hardware_breakpoints >= 4 {
            return;
        }

        let mut breakpoint = HardwareBreakpoint::new(addr, self.hardware_breakpoints).unwrap();
        breakpoint.enable(self);
        self.breakpoints.push(Box::new(breakpoint));
        self.hardware_breakpoints += 1;
    }

    pub fn try_remove_breakpoint(&mut self, addr: u64) {
        let mut breakpoints = std::mem::replace(&mut self.breakpoints, Vec::new());

        if let Some(breakpoint_index) = breakpoints.iter().position(|bp| bp.address() == addr) {
            breakpoints[breakpoint_index].disable(self);

            if breakpoints[breakpoint_index].hardware() {
                self.hardware_breakpoints -= 1;
            }

            breakpoints.remove(breakpoint_index);
        }

        self.breakpoints = breakpoints;
    }

    pub fn set_rip(&mut self, rip: u64) {
        self.write_user(
            std::mem::offset_of!(libc::user, regs)
                + std::mem::offset_of!(libc::user_regs_struct, rip),
            rip,
        );
        self.context.rip = rip;
    }
}

fn waitpid_thread(pid: u32, tx: Sender<i32>) {
    let mut status = 0i32;
    while unsafe { libc::waitpid(pid as i32, &mut status as _, libc::__WALL) != -1 } {
        let _ = tx.send(status);
    }
}
