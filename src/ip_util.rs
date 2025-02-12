use std::{
    net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6},
    os::fd::AsFd,
};

use nix::sys::socket::sockopt::{Ip6tOriginalDst, OriginalDst};

pub fn get_original_destination(fd: &impl AsFd, is_v6: bool) -> anyhow::Result<SocketAddr> {
    if is_v6 {
        let in_addr = nix::sys::socket::getsockopt(fd, Ip6tOriginalDst)?;
        let addr = in_addr.sin6_addr.s6_addr;

        Ok(SocketAddr::V6(SocketAddrV6::new(
            Ipv6Addr::new(
                (addr[0] as u16) << 8 | (addr[1] as u16),
                (addr[2] as u16) << 8 | (addr[3] as u16),
                (addr[4] as u16) << 8 | (addr[5] as u16),
                (addr[6] as u16) << 8 | (addr[7] as u16),
                (addr[8] as u16) << 8 | (addr[9] as u16),
                (addr[10] as u16) << 8 | (addr[11] as u16),
                (addr[12] as u16) << 8 | (addr[13] as u16),
                (addr[14] as u16) << 8 | (addr[15] as u16),
            ),
            u16::from_be(in_addr.sin6_port),
            in_addr.sin6_flowinfo,
            in_addr.sin6_scope_id,
        )))
    } else {
        let in_addr = nix::sys::socket::getsockopt(fd, OriginalDst)?;
        let addr = u32::from_be(in_addr.sin_addr.s_addr);

        Ok(SocketAddr::V4(SocketAddrV4::new(
            Ipv4Addr::new(
                (addr >> 24) as u8,
                (addr >> 16) as u8,
                (addr >> 8) as u8,
                addr as u8,
            ),
            u16::from_be(in_addr.sin_port),
        )))
    }
}
