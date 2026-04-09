use std::collections::{HashMap, HashSet};
use std::ffi::{CString, c_char, c_int, c_void};
use std::io;
use std::mem::size_of;

use crate::args::InterfaceAssignment;
use crate::model::{CanRecord, Timeline};

const AF_CAN: c_int = 29;
const SOCK_RAW: c_int = 3;
const CAN_RAW: c_int = 1;
const SOL_CAN_RAW: c_int = 101;
const CAN_RAW_LOOPBACK: c_int = 3;
const CAN_RAW_FD_FRAMES: c_int = 5;

const CAN_EFF_FLAG: u32 = 0x8000_0000;
const CAN_RTR_FLAG: u32 = 0x4000_0000;

pub trait FrameSink: Send {
    fn send(&mut self, frame: &CanRecord) -> io::Result<()>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum RouteTarget {
    Socket(String),
    Stdout,
    Drop,
}

pub struct RouteConfig {
    iface_override: Option<String>,
    assignments: HashMap<String, RouteTarget>,
    has_assignments: bool,
}

impl RouteConfig {
    pub fn new(
        iface_override: Option<String>,
        assignments: &[InterfaceAssignment],
    ) -> Result<Self, String> {
        let mut route_map = HashMap::new();
        for assignment in assignments {
            let target = if assignment.output == "stdout" {
                RouteTarget::Stdout
            } else {
                RouteTarget::Socket(assignment.output.clone())
            };
            route_map.insert(assignment.input.clone(), target);
        }

        if iface_override.is_some() && !assignments.is_empty() {
            return Err(String::from(
                "interface assignments cannot be combined with --iface",
            ));
        }

        Ok(Self {
            iface_override,
            assignments: route_map,
            has_assignments: !assignments.is_empty(),
        })
    }

    fn resolve(&self, frame: &CanRecord) -> RouteTarget {
        if let Some(iface) = &self.iface_override {
            return RouteTarget::Socket(iface.clone());
        }

        if self.has_assignments {
            return self
                .assignments
                .get(&frame.iface)
                .cloned()
                .unwrap_or(RouteTarget::Drop);
        }

        RouteTarget::Socket(frame.iface.clone())
    }

    fn socket_ifaces_for_timeline(&self, timeline: &Timeline) -> HashSet<String> {
        let mut ifaces = HashSet::new();
        for frame in &timeline.frames {
            if let RouteTarget::Socket(iface) = self.resolve(frame) {
                ifaces.insert(iface);
            }
        }
        ifaces
    }
}

pub struct DryRunSink {
    routes: RouteConfig,
    verbose: bool,
}

impl DryRunSink {
    pub fn new(routes: RouteConfig, verbose: bool) -> Self {
        Self { routes, verbose }
    }
}

impl FrameSink for DryRunSink {
    fn send(&mut self, frame: &CanRecord) -> io::Result<()> {
        match self.routes.resolve(frame) {
            RouteTarget::Drop => Ok(()),
            RouteTarget::Stdout => {
                println!("{}", frame.raw_line);
                Ok(())
            }
            RouteTarget::Socket(output_iface) => {
                let separator = if frame.is_fd { "##" } else { "#" };
                let flags = if frame.is_fd {
                    format!("{:X}", frame.fd_flags)
                } else {
                    String::new()
                };
                println!(
                    "[dry-run line {:>5} +{:>8} ms] {} <= {} {}{}{}",
                    frame.line_number,
                    frame.timestamp.as_millis(),
                    output_iface,
                    frame.iface,
                    frame.id_string(),
                    separator,
                    format!("{flags}{}", frame.payload_string())
                );
                if self.verbose {
                    println!("          raw: {}", frame.raw_line);
                }
                Ok(())
            }
        }
    }
}

pub struct SocketCanSink {
    routes: RouteConfig,
    sockets: HashMap<String, RawSocket>,
    verbose: bool,
}

impl SocketCanSink {
    pub fn new(
        routes: RouteConfig,
        timeline: &Timeline,
        disable_loopback: bool,
        verbose: bool,
    ) -> Result<Self, String> {
        let mut sockets = HashMap::new();
        let ifaces = routes.socket_ifaces_for_timeline(timeline);
        if ifaces.is_empty() {
            return Err(String::from(
                "no output interfaces were selected for replay",
            ));
        }

        for iface in ifaces {
            let socket = RawSocket::open(&iface, disable_loopback)?;
            sockets.insert(iface, socket);
        }

        Ok(Self {
            routes,
            sockets,
            verbose,
        })
    }
}

impl FrameSink for SocketCanSink {
    fn send(&mut self, frame: &CanRecord) -> io::Result<()> {
        match self.routes.resolve(frame) {
            RouteTarget::Drop => Ok(()),
            RouteTarget::Stdout => {
                println!("{}", frame.raw_line);
                Ok(())
            }
            RouteTarget::Socket(iface) => {
                let socket = self.sockets.get_mut(&iface).ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::NotFound,
                        format!("no socket configured for interface {iface}"),
                    )
                })?;
                socket.send(frame)?;
                if self.verbose {
                    println!("[tx {} <= {}] {}", iface, frame.iface, frame.raw_line);
                }
                Ok(())
            }
        }
    }
}

struct RawSocket {
    fd: c_int,
}

impl RawSocket {
    fn open(iface: &str, disable_loopback: bool) -> Result<Self, String> {
        let iface_cstr = CString::new(iface)
            .map_err(|_| format!("interface name contains NUL byte: {iface}"))?;

        let fd = unsafe { socket(AF_CAN, SOCK_RAW, CAN_RAW) };
        if fd < 0 {
            return Err(format!(
                "failed to create SocketCAN raw socket for {iface}: {}",
                io::Error::last_os_error()
            ));
        }

        if disable_loopback {
            let disabled: c_int = 0;
            let setsockopt_result = unsafe {
                setsockopt(
                    fd,
                    SOL_CAN_RAW,
                    CAN_RAW_LOOPBACK,
                    &disabled as *const _ as *const c_void,
                    size_of::<c_int>() as u32,
                )
            };
            if setsockopt_result < 0 {
                let error = io::Error::last_os_error();
                unsafe {
                    close(fd);
                }
                return Err(format!("failed to disable loopback on {iface}: {error}"));
            }
        }

        let enabled: c_int = 1;
        let setsockopt_result = unsafe {
            setsockopt(
                fd,
                SOL_CAN_RAW,
                CAN_RAW_FD_FRAMES,
                &enabled as *const _ as *const c_void,
                size_of::<c_int>() as u32,
            )
        };
        if setsockopt_result < 0 {
            let error = io::Error::last_os_error();
            unsafe {
                close(fd);
            }
            return Err(format!(
                "failed to enable CAN FD support on {iface}: {error}"
            ));
        }

        let if_index = unsafe { if_nametoindex(iface_cstr.as_ptr()) };
        if if_index == 0 {
            let error = io::Error::last_os_error();
            unsafe {
                close(fd);
            }
            return Err(format!(
                "failed to resolve interface index for {iface}: {error}"
            ));
        }

        let addr = SockAddrCan {
            can_family: AF_CAN as u16,
            can_ifindex: if_index as i32,
            can_addr: [0; 8],
        };
        let bind_result = unsafe {
            bind(
                fd,
                &addr as *const SockAddrCan as *const SockAddr,
                size_of::<SockAddrCan>() as u32,
            )
        };
        if bind_result < 0 {
            let error = io::Error::last_os_error();
            unsafe {
                close(fd);
            }
            return Err(format!(
                "failed to bind SocketCAN socket to {iface}: {error}"
            ));
        }

        Ok(Self { fd })
    }

    fn send(&mut self, frame: &CanRecord) -> io::Result<()> {
        if frame.is_fd {
            let mut raw = CanFdFrame {
                can_id: encode_can_id(frame),
                len: frame.wire_len() as u8,
                flags: frame.fd_flags,
                __res0: 0,
                __res1: 0,
                data: [0; 64],
            };
            raw.data[..frame.data.len()].copy_from_slice(&frame.data);
            write_exact(
                self.fd,
                &raw as *const _ as *const c_void,
                size_of::<CanFdFrame>(),
            )
        } else {
            let mut raw = CanFrame {
                can_id: encode_can_id(frame),
                can_dlc: frame.wire_len() as u8,
                __pad: 0,
                __res0: 0,
                __res1: 0,
                data: [0; 8],
            };
            raw.data[..frame.data.len()].copy_from_slice(&frame.data);
            write_exact(
                self.fd,
                &raw as *const _ as *const c_void,
                size_of::<CanFrame>(),
            )
        }
    }
}

impl Drop for RawSocket {
    fn drop(&mut self) {
        unsafe {
            close(self.fd);
        }
    }
}

fn encode_can_id(frame: &CanRecord) -> u32 {
    let mut can_id = frame.can_id;
    if frame.is_extended {
        can_id |= CAN_EFF_FLAG;
    }
    if frame.is_remote {
        can_id |= CAN_RTR_FLAG;
    }
    can_id
}

fn write_exact(fd: c_int, buffer: *const c_void, len: usize) -> io::Result<()> {
    let written = unsafe { write(fd, buffer, len) };
    if written < 0 {
        return Err(io::Error::last_os_error());
    }
    if written as usize != len {
        return Err(io::Error::new(
            io::ErrorKind::WriteZero,
            format!("short write to CAN socket: expected {len}, wrote {written}"),
        ));
    }
    Ok(())
}

#[repr(C)]
struct SockAddr {
    sa_family: u16,
    sa_data: [u8; 14],
}

#[repr(C)]
struct SockAddrCan {
    can_family: u16,
    can_ifindex: i32,
    can_addr: [u8; 8],
}

#[repr(C, align(8))]
struct CanFrame {
    can_id: u32,
    can_dlc: u8,
    __pad: u8,
    __res0: u8,
    __res1: u8,
    data: [u8; 8],
}

#[repr(C, align(8))]
struct CanFdFrame {
    can_id: u32,
    len: u8,
    flags: u8,
    __res0: u8,
    __res1: u8,
    data: [u8; 64],
}

unsafe extern "C" {
    fn socket(domain: c_int, socket_type: c_int, protocol: c_int) -> c_int;
    fn setsockopt(
        socket: c_int,
        level: c_int,
        option_name: c_int,
        option_value: *const c_void,
        option_len: u32,
    ) -> c_int;
    fn bind(socket: c_int, address: *const SockAddr, address_len: u32) -> c_int;
    fn write(fd: c_int, buffer: *const c_void, count: usize) -> isize;
    fn close(fd: c_int) -> c_int;
    fn if_nametoindex(ifname: *const c_char) -> u32;
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::args::InterfaceAssignment;
    use crate::model::{CanRecord, Timeline};

    use super::{RouteConfig, RouteTarget};

    fn frame(iface: &str) -> CanRecord {
        CanRecord {
            timestamp: Duration::ZERO,
            iface: iface.to_string(),
            can_id: 0x123,
            data: vec![0x01],
            is_extended: false,
            is_remote: false,
            is_fd: false,
            fd_flags: 0,
            line_number: 1,
            raw_line: format!("(0.0) {iface} 123#01"),
        }
    }

    #[test]
    fn resolves_assignments_and_drops_unmatched_interfaces() {
        let routes = RouteConfig::new(
            None,
            &[
                InterfaceAssignment {
                    output: "vcan2".into(),
                    input: "can0".into(),
                },
                InterfaceAssignment {
                    output: "stdout".into(),
                    input: "can1".into(),
                },
            ],
        )
        .expect("route config");

        assert_eq!(
            routes.resolve(&frame("can0")),
            RouteTarget::Socket("vcan2".into())
        );
        assert_eq!(routes.resolve(&frame("can1")), RouteTarget::Stdout);
        assert_eq!(routes.resolve(&frame("can9")), RouteTarget::Drop);
    }

    #[test]
    fn override_iface_wins_over_preserved_log_iface() {
        let routes = RouteConfig::new(Some("vcan5".into()), &[]).expect("route config");
        assert_eq!(
            routes.resolve(&frame("can0")),
            RouteTarget::Socket("vcan5".into())
        );
    }

    #[test]
    fn socket_ifaces_only_include_real_socket_targets() {
        let timeline = Timeline::from_frames(vec![frame("can0"), frame("can1"), frame("can9")]);
        let routes = RouteConfig::new(
            None,
            &[
                InterfaceAssignment {
                    output: "vcan2".into(),
                    input: "can0".into(),
                },
                InterfaceAssignment {
                    output: "stdout".into(),
                    input: "can1".into(),
                },
            ],
        )
        .expect("route config");

        let ifaces = routes.socket_ifaces_for_timeline(&timeline);
        assert_eq!(ifaces.len(), 1);
        assert!(ifaces.contains("vcan2"));
    }
}
