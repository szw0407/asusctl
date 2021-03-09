pub mod error;

pub mod gfx;

pub mod system;

const NVIDIA_DRIVERS: [&str; 4] = ["nvidia_drm", "nvidia_uvm", "nvidia_modeset", "nvidia"];

const DISPLAY_MANAGER: &str = "display-manager.service";

const MODPROBE_PATH: &str = "/etc/modprobe.d/asusd.conf";

static MODPROBE_BASE: &[u8] = br#"# Automatically generated by asusd
#blacklist i2c_nvidia_gpu
#alias i2c_nvidia_gpu off
blacklist nouveau
alias nouveau off
options nvidia NVreg_DynamicPowerManagement=0x02
options nvidia-drm modeset=1
"#;

const XORG_FILE: &str = "90-nvidia-primary.conf";
const XORG_PATH: &str = "/etc/X11/xorg.conf.d/";

static PRIMARY_GPU_BEGIN: &[u8] = br#"# Automatically generated by asusd
Section "OutputClass"
    Identifier "nvidia"
    MatchDriver "nvidia-drm"
    Driver "nvidia"
    Option "AllowEmptyInitialConfiguration"
    Option "AllowExternalGpus""#;

static PRIMARY_GPU_NVIDIA: &[u8] = br#"
    Option "PrimaryGPU" "true""#;

static PRIMARY_GPU_END: &[u8] = br#"
EndSection"#;