use super::*;

#[test]
fn header_wire_layout_and_flag_updates_are_exact() {
    let header =
        WsIoPacketCompressionFrameHeader::new(WsIoPacketCompressionAlgorithm::Zstd, false, 0x0102_0304).unwrap();

    let mut bytes = header.encode_payload(&[9, 8]).unwrap().to_vec();
    assert_eq!(bytes, [0x22, 1, 2, 3, 4, 9, 8]);
    WsIoPacketCompressionFrameHeader::set_compressed_flag(&mut bytes, true);
    assert_eq!(bytes, [0x23, 1, 2, 3, 4, 9, 8]);
    let decoded = WsIoPacketCompressionFrameHeader::decode(&bytes).unwrap();
    assert!(decoded.compressed);
    assert_eq!(decoded.original_length, 0x0102_0304);
    assert_eq!(decoded.algorithm, WsIoPacketCompressionAlgorithm::Zstd);
    WsIoPacketCompressionFrameHeader::set_compressed_flag(&mut bytes, false);
    assert_eq!(bytes[0], 0x22);
}

#[test]
fn invalid_headers_and_capacity_overflow_are_rejected() {
    for length in 0..5 {
        assert!(WsIoPacketCompressionFrameHeader::decode(&[0x22; 5][..length]).is_err());
    }

    for descriptor in [0x02, 0x42, 0xe2, 0x20, 0x24, 0x3e] {
        assert!(WsIoPacketCompressionFrameHeader::decode(&[descriptor, 0, 0, 0, 0]).is_err());
    }

    let header = WsIoPacketCompressionFrameHeader::new(WsIoPacketCompressionAlgorithm::Zstd, false, 0).unwrap();
    assert!(header.allocate(usize::MAX).is_err());
    #[cfg(target_pointer_width = "64")]
    assert!(
        WsIoPacketCompressionFrameHeader::new(WsIoPacketCompressionAlgorithm::Zstd, false, u32::MAX as usize + 1)
            .is_err()
    );
}
