#[derive(Debug, Clone, Copy)]
pub struct Parameters {
    pub smp: bool,
    pub symbolinfo: bool,
    pub low_memory: bool,
}

impl Parameters {
    pub fn parse(cmdline: &str) -> Self {
        let mut me = Self::default();

        if !cmdline.is_ascii() {
            warn!("Kernel command line must use ASCII characters only.");
            return me;
        }

        for arg in cmdline.split(' ') {
            match arg {
                "--nosmp" => me.smp = false,
                "--symbolinfo" => me.symbolinfo = true,
                "--lomem" => me.low_memory = true,

                // ignore
                "" => {}

                other => warn!("Unknown command line argument: {:?}", other),
            }
        }

        me
    }
}

impl Default for Parameters {
    fn default() -> Self {
        Self { smp: true, symbolinfo: false, low_memory: false }
    }
}

pub static PARAMETERS: spin::Lazy<Parameters> = spin::Lazy::new(|| match crate::boot::kernel_file() {
    Some(kernel_file) => Parameters::parse(kernel_file.cmdline()),
    None => Parameters::default(),
});
