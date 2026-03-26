//! Ethernet frame parsing and building utilities.
//!
//! Provides low-level Ethernet header parsing, ARP reply generation,
//! and MAC/CIDR address helpers for the hitz-net networking stack.

/// Ethernet header length in bytes (dst MAC + src MAC + ethertype).
pub const ETH_HEADER_LEN: usize = 14;

/// `EtherType` for IPv4 packets.
pub const ETHERTYPE_IPV4: u16 = 0x0800;

/// `EtherType` for IPv6 packets.
pub const ETHERTYPE_IPV6: u16 = 0x86DD;

/// `EtherType` for ARP packets.
pub const ETHERTYPE_ARP: u16 = 0x0806;

/// ARP operation: request.
pub const ARP_OP_REQUEST: u16 = 1;

/// ARP operation: reply.
pub const ARP_OP_REPLY: u16 = 2;

/// Parsed Ethernet header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EthHeader {
    /// Destination MAC address.
    pub dst_mac: [u8; 6],
    /// Source MAC address.
    pub src_mac: [u8; 6],
    /// `EtherType` field (e.g., 0x0800 for IPv4).
    pub ethertype: u16,
}

/// Parse an Ethernet header from a raw frame.
///
/// Returns `None` if the frame is shorter than [`ETH_HEADER_LEN`] (14 bytes).
#[must_use]
pub fn parse_eth_header(frame: &[u8]) -> Option<EthHeader> {
    if frame.len() < ETH_HEADER_LEN {
        return None;
    }

    let mut dst_mac = [0u8; 6];
    let mut src_mac = [0u8; 6];
    dst_mac.copy_from_slice(&frame[0..6]);
    src_mac.copy_from_slice(&frame[6..12]);
    let ethertype = u16::from_be_bytes([frame[12], frame[13]]);

    Some(EthHeader {
        dst_mac,
        src_mac,
        ethertype,
    })
}

/// Build an Ethernet frame from its components.
///
/// Returns a `Vec<u8>` containing the 14-byte Ethernet header followed
/// by the payload.
#[must_use]
pub fn build_eth_frame(
    src_mac: &[u8; 6],
    dst_mac: &[u8; 6],
    ethertype: u16,
    payload: &[u8],
) -> Vec<u8> {
    let mut frame = Vec::with_capacity(ETH_HEADER_LEN + payload.len());
    frame.extend_from_slice(dst_mac);
    frame.extend_from_slice(src_mac);
    frame.extend_from_slice(&ethertype.to_be_bytes());
    frame.extend_from_slice(payload);
    frame
}

/// Strip the Ethernet header and return the payload.
///
/// Returns `None` if the frame is shorter than [`ETH_HEADER_LEN`].
#[must_use]
pub fn strip_eth_header(frame: &[u8]) -> Option<&[u8]> {
    if frame.len() < ETH_HEADER_LEN {
        return None;
    }
    Some(&frame[ETH_HEADER_LEN..])
}

/// Detect the `EtherType` from an IP packet's version nibble.
///
/// Examines the high nibble of the first byte:
/// - 4 -> [`ETHERTYPE_IPV4`]
/// - 6 -> [`ETHERTYPE_IPV6`]
/// - anything else -> [`ETHERTYPE_IPV4`] (fallback)
#[must_use]
pub fn ethertype_from_ip(packet: &[u8]) -> u16 {
    if packet.is_empty() {
        return ETHERTYPE_IPV4;
    }
    match packet[0] >> 4 {
        6 => ETHERTYPE_IPV6,
        _ => ETHERTYPE_IPV4,
    }
}

/// Build an ARP reply for a gateway ARP request.
///
/// Validates the ARP request frame and returns an Ethernet-framed ARP
/// reply if:
/// - The frame has a valid Ethernet + ARP structure (>= 42 bytes)
/// - `hw_type` == 1 (Ethernet)
/// - `proto_type` == 0x0800 (IPv4)
/// - ARP operation is [`ARP_OP_REQUEST`]
/// - Target IP matches `gateway_ip`
///
/// Returns `None` if any validation fails.
#[must_use]
pub fn build_arp_reply(
    request_frame: &[u8],
    gateway_mac: &[u8; 6],
    gateway_ip: &[u8; 4],
) -> Option<Vec<u8>> {
    // Minimum ARP over Ethernet frame: 14 (eth) + 28 (ARP for IPv4/Ethernet) = 42 bytes.
    if request_frame.len() < 42 {
        return None;
    }

    let eth = parse_eth_header(request_frame)?;
    if eth.ethertype != ETHERTYPE_ARP {
        return None;
    }

    let arp = &request_frame[ETH_HEADER_LEN..];

    // ARP header fields:
    // hw_type (2) + proto_type (2) + hw_len (1) + proto_len (1) + op (2) = 8 bytes header
    // sender_hw (6) + sender_ip (4) + target_hw (6) + target_ip (4) = 20 bytes addresses
    let hw_type = u16::from_be_bytes([arp[0], arp[1]]);
    let proto_type = u16::from_be_bytes([arp[2], arp[3]]);
    let hw_len = arp[4];
    let proto_len = arp[5];
    let op = u16::from_be_bytes([arp[6], arp[7]]);

    // Validate ARP for Ethernet + IPv4.
    if hw_type != 1 || proto_type != 0x0800 || hw_len != 6 || proto_len != 4 {
        return None;
    }

    if op != ARP_OP_REQUEST {
        return None;
    }

    // Extract addresses from request.
    let mut sender_mac = [0u8; 6];
    let mut sender_ip = [0u8; 4];
    let mut target_ip_buf = [0u8; 4];
    sender_mac.copy_from_slice(&arp[8..14]);
    sender_ip.copy_from_slice(&arp[14..18]);
    // target_hw at arp[18..24] (ignored in request, usually 00:00:00:00:00:00)
    target_ip_buf.copy_from_slice(&arp[24..28]);

    // Only reply if the target IP matches our gateway IP.
    if target_ip_buf != *gateway_ip {
        return None;
    }

    // Build the ARP reply payload (28 bytes).
    let mut arp_reply = [0u8; 28];
    // hw_type = 1 (Ethernet)
    arp_reply[0..2].copy_from_slice(&1u16.to_be_bytes());
    // proto_type = 0x0800 (IPv4)
    arp_reply[2..4].copy_from_slice(&0x0800u16.to_be_bytes());
    // hw_len = 6
    arp_reply[4] = 6;
    // proto_len = 4
    arp_reply[5] = 4;
    // op = reply (2)
    arp_reply[6..8].copy_from_slice(&ARP_OP_REPLY.to_be_bytes());
    // sender_hw = gateway MAC
    arp_reply[8..14].copy_from_slice(gateway_mac);
    // sender_ip = gateway IP
    arp_reply[14..18].copy_from_slice(gateway_ip);
    // target_hw = original sender MAC
    arp_reply[18..24].copy_from_slice(&sender_mac);
    // target_ip = original sender IP
    arp_reply[24..28].copy_from_slice(&sender_ip);

    Some(build_eth_frame(
        gateway_mac,
        &sender_mac,
        ETHERTYPE_ARP,
        &arp_reply,
    ))
}

/// Generate a random locally-administered unicast MAC address.
///
/// The result has:
/// - Bit 0 of byte 0 = 0 (unicast)
/// - Bit 1 of byte 0 = 1 (locally administered)
#[must_use]
pub fn random_mac() -> [u8; 6] {
    // Use a simple PRNG seeded from the current time to avoid
    // pulling in a full `rand` dependency.
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0x1234_5678_9ABC_DEF0u64, |d| {
            #[allow(clippy::cast_possible_truncation)]
            let val = d.as_nanos() as u64;
            val
        });

    // xorshift64
    let mut state = seed;
    let mut bytes = [0u8; 6];
    for byte in &mut bytes {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        #[allow(clippy::cast_possible_truncation)]
        {
            *byte = state as u8;
        }
    }

    // Enforce locally-administered unicast:
    // bit 0 = 0 (unicast), bit 1 = 1 (locally administered)
    bytes[0] = (bytes[0] & 0xFC) | 0x02;

    bytes
}

/// Parse a MAC address string in "AA:BB:CC:DD:EE:FF" format.
///
/// ⚡ Bolt: Iterates over split parts directly instead of collecting into an intermediate `Vec<&str>`, avoiding heap allocations on the hot path.
///
/// # Errors
///
/// Returns an error string if the format is invalid.
pub fn parse_mac(s: &str) -> Result<[u8; 6], String> {
    let parts = s.split(':');

    // Quick length check before allocating or parsing.
    let count = parts.clone().count();
    if count != 6 {
        return Err(format!("expected 6 colon-separated octets, got {count}"));
    }

    let mut mac = [0u8; 6];
    for (i, part) in parts.enumerate() {
        mac[i] =
            u8::from_str_radix(part, 16).map_err(|e| format!("invalid hex octet '{part}': {e}"))?;
    }

    Ok(mac)
}

/// Parse a CIDR notation string like "192.168.100.1/24".
///
/// ⚡ Bolt: Consumes iterators directly instead of collecting into `Vec<&str>`, removing unnecessary heap allocations.
///
/// Returns the IPv4 address and prefix length.
///
/// # Errors
///
/// Returns an error string if the format is invalid.
pub fn parse_cidr(s: &str) -> Result<([u8; 4], u8), String> {
    let mut parts = s.splitn(2, '/');
    let ip_str = parts
        .next()
        .ok_or_else(|| "expected format: A.B.C.D/prefix".to_string())?;
    let prefix_str = parts
        .next()
        .ok_or_else(|| "expected format: A.B.C.D/prefix".to_string())?;

    let octets = ip_str.split('.');
    let count = octets.clone().count();
    if count != 4 {
        return Err(format!("expected 4 octets, got {count}"));
    }

    let mut ip = [0u8; 4];
    for (i, octet) in octets.enumerate() {
        ip[i] = octet
            .parse::<u8>()
            .map_err(|e| format!("invalid octet '{octet}': {e}"))?;
    }

    let prefix: u8 = prefix_str
        .parse()
        .map_err(|e| format!("invalid prefix length '{prefix_str}': {e}"))?;

    if prefix > 32 {
        return Err(format!("prefix length {prefix} exceeds 32"));
    }

    Ok((ip, prefix))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn parse_eth_header_basic() {
        let mut frame = vec![0u8; 60];
        // dst MAC: 11:22:33:44:55:66
        frame[0..6].copy_from_slice(&[0x11, 0x22, 0x33, 0x44, 0x55, 0x66]);
        // src MAC: AA:BB:CC:DD:EE:FF
        frame[6..12].copy_from_slice(&[0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]);
        // EtherType: IPv4
        frame[12..14].copy_from_slice(&ETHERTYPE_IPV4.to_be_bytes());

        let hdr = parse_eth_header(&frame).expect("should parse");
        assert_eq!(hdr.dst_mac, [0x11, 0x22, 0x33, 0x44, 0x55, 0x66]);
        assert_eq!(hdr.src_mac, [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]);
        assert_eq!(hdr.ethertype, ETHERTYPE_IPV4);
    }

    #[test]
    fn parse_eth_header_too_short() {
        let frame = [0u8; 13]; // one byte short
        assert!(parse_eth_header(&frame).is_none());
    }

    #[test]
    fn build_and_strip_roundtrip() {
        let src = [0x02, 0x00, 0x00, 0x00, 0x00, 0x01];
        let dst = [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
        let payload = b"hello network";

        let frame = build_eth_frame(&src, &dst, ETHERTYPE_IPV4, payload);
        assert_eq!(frame.len(), ETH_HEADER_LEN + payload.len());

        // Parse the header back.
        let hdr = parse_eth_header(&frame).expect("should parse built frame");
        assert_eq!(hdr.dst_mac, dst);
        assert_eq!(hdr.src_mac, src);
        assert_eq!(hdr.ethertype, ETHERTYPE_IPV4);

        // Strip header and check payload.
        let stripped = strip_eth_header(&frame).expect("should strip");
        assert_eq!(stripped, payload);
    }

    #[test]
    fn build_arp_reply_basic() {
        let guest_mac = [0x02, 0x00, 0x00, 0x00, 0x00, 0x01];
        let gateway_mac = [0x02, 0x00, 0x00, 0x00, 0x00, 0xFE];
        let guest_ip = [192, 168, 100, 2];
        let gateway_ip = [192, 168, 100, 1];

        // Build an ARP request frame from guest asking for gateway MAC.
        let mut arp_payload = [0u8; 28];
        // hw_type = 1 (Ethernet)
        arp_payload[0..2].copy_from_slice(&1u16.to_be_bytes());
        // proto_type = 0x0800 (IPv4)
        arp_payload[2..4].copy_from_slice(&0x0800u16.to_be_bytes());
        // hw_len = 6
        arp_payload[4] = 6;
        // proto_len = 4
        arp_payload[5] = 4;
        // op = request (1)
        arp_payload[6..8].copy_from_slice(&ARP_OP_REQUEST.to_be_bytes());
        // sender_hw = guest MAC
        arp_payload[8..14].copy_from_slice(&guest_mac);
        // sender_ip = guest IP
        arp_payload[14..18].copy_from_slice(&guest_ip);
        // target_hw = 00:00:00:00:00:00 (unknown)
        // target_ip = gateway IP
        arp_payload[24..28].copy_from_slice(&gateway_ip);

        let broadcast = [0xFF; 6];
        let request_frame = build_eth_frame(&guest_mac, &broadcast, ETHERTYPE_ARP, &arp_payload);

        let reply = build_arp_reply(&request_frame, &gateway_mac, &gateway_ip)
            .expect("should build ARP reply");

        // Parse the reply Ethernet header.
        let reply_hdr = parse_eth_header(&reply).expect("reply should parse");
        assert_eq!(reply_hdr.dst_mac, guest_mac);
        assert_eq!(reply_hdr.src_mac, gateway_mac);
        assert_eq!(reply_hdr.ethertype, ETHERTYPE_ARP);

        // Parse the reply ARP payload.
        let reply_arp = &reply[ETH_HEADER_LEN..];
        let reply_op = u16::from_be_bytes([reply_arp[6], reply_arp[7]]);
        assert_eq!(reply_op, ARP_OP_REPLY);

        // sender_hw should be gateway MAC
        assert_eq!(&reply_arp[8..14], &gateway_mac);
        // sender_ip should be gateway IP
        assert_eq!(&reply_arp[14..18], &gateway_ip);
        // target_hw should be guest MAC
        assert_eq!(&reply_arp[18..24], &guest_mac);
        // target_ip should be guest IP
        assert_eq!(&reply_arp[24..28], &guest_ip);
    }

    #[test]
    fn parse_mac_valid() {
        let mac = parse_mac("AA:BB:CC:DD:EE:FF").expect("should parse");
        assert_eq!(mac, [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]);
    }

    #[test]
    fn parse_mac_invalid() {
        assert!(parse_mac("AA:BB:CC:DD:EE").is_err()); // too few
        assert!(parse_mac("AA:BB:CC:DD:EE:FF:00").is_err()); // too many
        assert!(parse_mac("GG:BB:CC:DD:EE:FF").is_err()); // invalid hex
    }

    #[test]
    fn parse_cidr_valid() {
        let (ip, prefix) = parse_cidr("192.168.100.1/24").expect("should parse");
        assert_eq!(ip, [192, 168, 100, 1]);
        assert_eq!(prefix, 24);
    }

    #[test]
    fn random_mac_is_locally_administered() {
        let mac = random_mac();
        // Bit 0 of byte 0 should be 0 (unicast).
        assert_eq!(mac[0] & 0x01, 0, "should be unicast");
        // Bit 1 of byte 0 should be 1 (locally administered).
        assert_eq!(mac[0] & 0x02, 0x02, "should be locally administered");
    }

    #[test]
    fn ethertype_detection() {
        // IPv4 packet (version nibble = 4).
        let ipv4_packet = [0x45, 0x00, 0x00, 0x14];
        assert_eq!(ethertype_from_ip(&ipv4_packet), ETHERTYPE_IPV4);

        // IPv6 packet (version nibble = 6).
        let ipv6_packet = [0x60, 0x00, 0x00, 0x00];
        assert_eq!(ethertype_from_ip(&ipv6_packet), ETHERTYPE_IPV6);

        // Empty packet falls back to IPv4.
        assert_eq!(ethertype_from_ip(&[]), ETHERTYPE_IPV4);

        // Unknown version falls back to IPv4.
        let unknown = [0x30]; // version nibble = 3
        assert_eq!(ethertype_from_ip(&unknown), ETHERTYPE_IPV4);
    }
}
