use super::Debugee;

//could've just used an enum..
//TODO?
pub trait Breakpoint {
    fn enabled(&self) -> bool;
    fn hardware(&self) -> bool;
    fn address(&self) -> u64;
    fn size(&self) -> usize;
    fn original_bytes<'a>(&'a self) -> Option<&'a [u8]>;

    fn enable(&mut self, debugee: &Debugee);
    fn disable(&mut self, debugee: &Debugee);
}
pub struct SoftwareBreakpoint {
    enabled: bool,
    address: u64,
    size: usize,
    original_bytes: Vec<u8>,
}

impl SoftwareBreakpoint {
    pub const fn new(address: u64, instruction_size: u64) -> Self {
        Self {
            enabled: false,
            address,
            size: instruction_size as usize,
            original_bytes: Vec::new(),
        }
    }
}

impl Breakpoint for SoftwareBreakpoint {
    #[allow(unreachable_code, unused)]
    fn enable(&mut self, debugee: &Debugee) {
        if self.enabled {
            return;
        }

        self.original_bytes = debugee.read_memory(self.address as usize, 1);
        debugee.write_memory(self.address as usize + 1, &vec![0xCCu8]);

        println!("enabled");

        self.enabled = true;
    }

    fn disable(&mut self, debugee: &Debugee) {
        debugee.write_memory(self.address as usize + 1, &self.original_bytes);

        println!("disabled");

        self.enabled = false;
    }

    fn address(&self) -> u64 {
        self.address
    }

    fn enabled(&self) -> bool {
        self.enabled
    }

    fn hardware(&self) -> bool {
        false
    }

    fn size(&self) -> usize {
        self.size
    }

    fn original_bytes<'a>(&'a self) -> Option<&'a [u8]> {
        Some(&self.original_bytes)
    }
}

pub struct HardwareBreakpoint {
    enabled: bool,
    address: u64,
    register_index: usize,
}

impl HardwareBreakpoint {
    pub const fn new(address: u64, register_index: usize) -> Result<Self, ()> {
        if register_index >= 4 {
            Err(())
        } else {
            Ok(Self {
                enabled: false,
                address,
                register_index,
            })
        }
    }
}

impl Breakpoint for HardwareBreakpoint {
    fn address(&self) -> u64 {
        self.address
    }

    fn enabled(&self) -> bool {
        self.enabled
    }

    fn hardware(&self) -> bool {
        true
    }

    fn enable(&mut self, debugee: &Debugee) {
        println!("actual dr7 {:#b}", read_dr(debugee, 7));

        //drX = addr
        write_dr(debugee, self.register_index, self.address);

        //dr6 = 0
        write_dr(debugee, 6, 0);

        //https://en.wikipedia.org/wiki/X86_debug_register
        //tmp = dr7 | LX | GX | LE | GE | RESERVED10
        //GX | LX is global and local enable for breakpoint X
        let new_dr7 = read_dr(debugee, 7)
            | (1 << (self.register_index * 2))
            | (1 << (self.register_index * 2 + 1))
            | (1 << 8)
            | (1 << 9)
            | (1 << 10);

        //dr7 = tmp
        write_dr(debugee, 7, new_dr7);

        println!("new_dr7 = {new_dr7:#b}");
    }

    fn disable(&mut self, debugee: &Debugee) {
        println!("actual dr7 {:#b}", read_dr(debugee, 7));

        //drX = 0
        write_dr(debugee, self.register_index, 0);

        //tmp = dr7 & ~(LX | GX)
        let new_dr7 = read_dr(debugee, 7)
            & !((1 << (self.register_index * 2)) | (1 << (self.register_index * 2 + 1)));

        //dr7 = tmp
        write_dr(debugee, 7, new_dr7);

        println!("new dr7 {:#b}", read_dr(debugee, 7));
        self.enabled = false;
    }

    fn size(&self) -> usize {
        0
    }

    fn original_bytes<'a>(&'a self) -> Option<&'a [u8]> {
        None
    }
}

fn read_dr(debugee: &Debugee, idx: usize) -> u64 {
    debugee.read_user(std::mem::offset_of!(libc::user, u_debugreg) + idx * 8)
}

fn write_dr(debugee: &Debugee, idx: usize, data: u64) {
    debugee.write_user(std::mem::offset_of!(libc::user, u_debugreg) + idx * 8, data);
}
