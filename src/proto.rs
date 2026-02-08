use crate::utils::Cursor;

const MAGIC: u32 = 0xDEADBEEF;
// magic:4|hash:4|opts:2|topic_len:2|data_len:4|topic|data|

#[derive(Debug, Clone, Copy)]
pub struct SdmqHeader {
    magic: u32,
    hash: u32,
    options: u16,
    topic_len: u16,
    data_len: u32,
}

impl SdmqHeader {
    pub fn parse(buf: &[u8]) -> Self {
        let mut cursor = Cursor::new(buf);
        SdmqHeader {
            magic: cursor.read_u32(),
            hash: cursor.read_u32(),
            options: cursor.read_u16(),
            topic_len: cursor.read_u16(),
            data_len: cursor.read_u32(),
        }
    }

    pub fn write_to(self, buf: &mut [u8]) {
        let mut cursor = Cursor::new(buf);

        cursor.write_n(self.magic.to_be_bytes());
        cursor.write_n(self.hash.to_be_bytes());
        cursor.write_n(self.options.to_be_bytes());
        cursor.write_n(self.topic_len.to_be_bytes());
        cursor.write_n(self.data_len.to_be_bytes());
    }
}

#[derive(Clone, Copy)]
#[repr(u16)]
pub enum SdmqMsgType {
    Push = 1,
    Sub = 2,
}

impl SdmqMsgType {
    pub fn to_network(self) -> [u8; 2] {
        let s = self as u16;
        s.to_be_bytes()
    }
}

pub mod read {
    use crate::{
        proto::{SdmqHeader, SdmqMsgType},
        topic::Topic,
    };

    pub struct SdmqProto<'a> {
        pub(super) header: SdmqHeader,
        pub(super) topic: Topic<'a>,
        pub(super) data: &'a [u8],
    }

    impl<'a> SdmqProto<'a> {
        pub fn parse(buf: &'a [u8]) -> Self {
            let header = SdmqHeader::parse(buf);

            let total_len = header.topic_len as usize + header.data_len as usize;
            let end = total_len + size_of::<SdmqHeader>();
            let clamped = end.min(buf.len());

            let (topic, data) =
                buf[size_of::<SdmqHeader>()..clamped].split_at(header.topic_len as usize);

            let topic = Topic::parse(topic);

            Self {
                header,
                topic,
                data,
            }
        }

        pub fn msg_type(&self) -> SdmqMsgType {
            match self.header.options & 0x0000_1111 {
                1 => SdmqMsgType::Push,
                2 => SdmqMsgType::Sub,
                other => panic!("Received unknown msg type: {}", other),
            }
        }

        pub fn topic(&self) -> Topic<'_> {
            self.topic
        }

        pub fn data(&self) -> &[u8] {
            self.data
        }

        #[cfg(feature = "std")]
        pub fn data_to_vec(&self) -> Vec<u8> {
            self.data.to_vec()
        }
    }
}

pub mod write {
    use crate::proto::{MAGIC, SdmqHeader, SdmqMsgType};

    pub type SdmqPacket = SdmqPacketBuf<1024>;

    pub struct SdmqPacketBuf<const SIZE: usize> {
        buffer: [u8; SIZE],
        pos: usize,
    }

    impl<const SIZE: usize> Default for SdmqPacketBuf<SIZE> {
        fn default() -> Self {
            Self::new()
        }
    }

    impl<const MAX_SIZE: usize> SdmqPacketBuf<MAX_SIZE> {
        pub fn new() -> Self {
            let buffer = [0u8; MAX_SIZE];

            Self { buffer, pos: 0 }
        }

        #[cfg(any(feature = "serde_json_core", feature = "serde_json"))]
        pub fn write_json<S, D>(&mut self, topic: S, data: D)
        where
            S: AsRef<str>,
            D: serde::Serialize,
        {
            // Write topic
            let topic = topic.as_ref();

            let cursor = SdMessageBuilder::new(&mut self.buffer);

            let pos = cursor.topic(topic).write_json(data).done(SdmqMsgType::Push);

            self.pos = pos;
        }

        pub fn write_raw<S>(&mut self, topic: &S, data: &[u8])
        where
            S: AsRef<str>,
        {
            // Write topic
            let topic = topic.as_ref();

            let cursor = SdMessageBuilder::new(&mut self.buffer);

            let pos = cursor.topic(topic).write_raw(data).done(SdmqMsgType::Push);

            self.pos = pos;
        }

        pub fn as_slice(&self) -> &[u8] {
            &self.buffer[..self.pos]
        }
    }

    impl<const MAX_SIZE: usize> AsRef<[u8]> for SdmqPacketBuf<MAX_SIZE> {
        fn as_ref(&self) -> &[u8] {
            self.as_slice()
        }
    }

    pub struct Empty;
    pub struct Topic(usize);
    pub struct Data(usize);
    #[cfg(feature = "std")]
    struct Intermediate(usize);

    pub struct SdMessageBuilder<'a, T, D> {
        inner: &'a mut [u8],
        topic: T,
        data: D,
    }

    impl<'a> SdMessageBuilder<'a, Empty, Empty> {
        pub fn new(buf: &'a mut [u8]) -> Self {
            Self {
                inner: buf,
                topic: Empty,
                data: Empty,
            }
        }

        pub fn topic(self, data: &str) -> SdMessageBuilder<'a, Topic, Empty> {
            let offset = size_of::<SdmqHeader>();

            let bytes = data.as_bytes();

            let topic_len = bytes.len();
            let buf = &mut self.inner[offset..];
            buf[..topic_len].copy_from_slice(bytes);

            SdMessageBuilder {
                inner: self.inner,
                topic: Topic(topic_len),
                data: self.data,
            }
        }
    }

    impl<'a> SdMessageBuilder<'a, Topic, Empty> {
        #[cfg(all(feature = "serde_json_core", not(feature = "serde_json")))]
        pub fn write_json(self, data: impl serde::Serialize) -> SdMessageBuilder<'a, Topic, Data> {
            let offset = size_of::<SdmqHeader>() + self.topic.0;
            let written = serde_json_core::to_slice(&data, &mut self.inner[offset..]).unwrap();

            SdMessageBuilder {
                inner: self.inner,
                topic: self.topic,
                data: Data(written),
            }
        }

        #[cfg(feature = "serde_json")]
        pub fn write_json(self, data: impl serde::Serialize) -> SdMessageBuilder<'a, Topic, Data> {
            let mut s = SdMessageBuilder {
                inner: self.inner,
                topic: self.topic,
                data: Intermediate(0),
            };
            serde_json::to_writer(&mut s, &data).unwrap();

            SdMessageBuilder {
                inner: s.inner,
                topic: s.topic,
                data: Data(s.data.0),
            }
        }

        pub fn write_raw(self, data: &[u8]) -> SdMessageBuilder<'a, Topic, Data> {
            let offset = size_of::<SdmqHeader>() + self.topic.0;

            let buf = &mut self.inner[offset..];

            buf[..data.len()].copy_from_slice(data);

            SdMessageBuilder {
                inner: self.inner,
                topic: self.topic,
                data: Data(data.len()),
            }
        }
    }

    impl<'a> SdMessageBuilder<'a, Topic, Data> {
        pub fn done(self, msg_type: SdmqMsgType) -> usize {
            let pos = self.data.0 + self.topic.0 + size_of::<SdmqHeader>();

            // CRC_hash topic + payload
            let hash = self.crc32_ieee();

            let header = SdmqHeader {
                magic: MAGIC,
                hash,
                options: msg_type as u16,
                topic_len: self.topic.0 as u16,
                data_len: self.data.0 as u32,
            };

            header.write_to(self.inner);

            pos
        }

        /// Bitwise CRC-32 (IEEE) with init=0xFFFF_FFFF and xorout=0xFFFF_FFFF.
        /// This matches the common "PKZip/Ethernet" CRC-32.
        fn crc32_ieee(&self) -> u32 {
            let pos = self.data.0 + self.topic.0 + size_of::<SdmqHeader>();
            let mut crc: u32 = 0xFFFF_FFFF;
            let data = &self.inner[size_of::<SdmqHeader>()..pos];

            for &b in data {
                crc ^= b as u32;
                for _ in 0..8 {
                    let mask = 0u32.wrapping_sub(crc & 1);
                    crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
                }
            }

            !crc
        }
    }

    #[cfg(feature = "std")]
    impl<'a> std::io::Write for SdMessageBuilder<'a, Topic, Intermediate> {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            let data_start = self.topic.0 + size_of::<SdmqHeader>();
            let pos = data_start + self.data.0;

            let self_buf = &mut self.inner[pos..];

            self_buf[..buf.len()].copy_from_slice(buf);

            self.data = Intermediate(self.data.0 + buf.len());
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::read::*;

    #[test]
    fn test() {
        let mut buf = vec![];

        buf.extend_from_slice(&[0, 1, 2, 3]);
        buf.extend_from_slice(&[5, 5, 5, 5]);

        buf.extend_from_slice(&[0, 1]);
        buf.extend_from_slice(&[0, 9]);
        buf.extend_from_slice(&[0, 0, 0, 8]);

        buf.extend_from_slice(b"topic_123");
        buf.extend_from_slice(b"data_123");
        let parsed = SdmqProto::parse(&buf);

        assert_eq!(parsed.topic().main(), "topic_123");
        assert_eq!(parsed.data.len(), 8);
    }

    #[test]
    fn python() {
        let mut buf = vec![];

        buf.extend_from_slice(&[
            83, 77, 84, 1, 131, 242, 185, 180, 0, 0, 0, 10, 0, 0, 4, 93, 119, 105, 102, 105, 95,
            117, 110, 105, 116, 115, 123, 34, 116, 115, 34, 58, 34, 50, 48, 50, 54, 45, 48, 49, 45,
            50, 53, 84, 49, 50, 58, 49, 52, 58, 50, 52, 46, 48, 49, 56, 54, 52, 55, 90, 34, 44, 34,
            115, 111, 117, 114, 99, 101, 34, 58, 34, 114, 97, 115, 112, 98, 101, 114, 114, 121,
            112, 105, 45, 115, 101, 108, 101, 110, 105, 117, 109, 34, 44, 34, 99, 111, 110, 110,
            101, 99, 116, 101, 100, 95, 100, 101, 118, 105, 99, 101, 115, 34, 58, 91, 123, 34, 110,
            97, 109, 101, 34, 58, 34, 99, 117, 50, 45, 50, 55, 69, 86, 78, 80, 88, 80, 34, 44, 34,
            115, 105, 103, 110, 97, 108, 34, 58, 34, 45, 54, 51, 32, 100, 66, 109, 34, 44, 34, 109,
            97, 99, 34, 58, 34, 53, 56, 58, 98, 53, 58, 54, 56, 58, 55, 48, 58, 55, 48, 58, 53,
            102, 34, 44, 34, 105, 112, 34, 58, 34, 49, 57, 50, 46, 49, 54, 56, 46, 49, 46, 55, 49,
            34, 44, 34, 108, 101, 97, 115, 101, 34, 58, 34, 50, 50, 32, 104, 111, 117, 114, 115,
            32, 51, 51, 32, 109, 105, 110, 117, 116, 101, 115, 32, 50, 54, 32, 115, 101, 99, 111,
            110, 100, 115, 34, 125, 44, 123, 34, 110, 97, 109, 101, 34, 58, 34, 83, 111, 110, 111,
            115, 90, 80, 34, 44, 34, 115, 105, 103, 110, 97, 108, 34, 58, 34, 45, 54, 52, 32, 100,
            66, 109, 34, 44, 34, 109, 97, 99, 34, 58, 34, 51, 52, 58, 55, 101, 58, 53, 99, 58, 102,
            100, 58, 49, 102, 58, 48, 56, 34, 44, 34, 105, 112, 34, 58, 34, 49, 57, 50, 46, 49, 54,
            56, 46, 49, 46, 55, 54, 34, 44, 34, 108, 101, 97, 115, 101, 34, 58, 34, 49, 50, 32,
            104, 111, 117, 114, 115, 32, 51, 32, 109, 105, 110, 117, 116, 101, 115, 32, 50, 51, 32,
            115, 101, 99, 111, 110, 100, 115, 34, 125, 44, 123, 34, 110, 97, 109, 101, 34, 58, 34,
            66, 48, 45, 52, 65, 45, 51, 57, 45, 48, 67, 45, 54, 55, 45, 55, 55, 34, 44, 34, 115,
            105, 103, 110, 97, 108, 34, 58, 34, 45, 52, 51, 32, 100, 66, 109, 34, 44, 34, 109, 97,
            99, 34, 58, 34, 98, 48, 58, 52, 97, 58, 51, 57, 58, 48, 99, 58, 54, 55, 58, 55, 55, 34,
            44, 34, 105, 112, 34, 58, 34, 49, 57, 50, 46, 49, 54, 56, 46, 49, 46, 56, 49, 34, 44,
            34, 108, 101, 97, 115, 101, 34, 58, 34, 49, 52, 32, 104, 111, 117, 114, 115, 32, 51,
            49, 32, 109, 105, 110, 117, 116, 101, 115, 32, 49, 32, 115, 101, 99, 111, 110, 100,
            115, 34, 125, 44, 123, 34, 110, 97, 109, 101, 34, 58, 34, 77, 97, 116, 105, 108, 100,
            101, 32, 105, 80, 104, 111, 110, 101, 34, 44, 34, 115, 105, 103, 110, 97, 108, 34, 58,
            34, 45, 54, 51, 32, 100, 66, 109, 34, 44, 34, 109, 97, 99, 34, 58, 34, 53, 50, 58, 57,
            57, 58, 101, 55, 58, 51, 48, 58, 57, 56, 58, 52, 55, 34, 44, 34, 105, 112, 34, 58, 34,
            49, 57, 50, 46, 49, 54, 56, 46, 49, 46, 55, 52, 34, 44, 34, 108, 101, 97, 115, 101, 34,
            58, 34, 50, 49, 32, 104, 111, 117, 114, 115, 32, 53, 56, 32, 109, 105, 110, 117, 116,
            101, 115, 32, 53, 54, 32, 115, 101, 99, 111, 110, 100, 115, 34, 125, 44, 123, 34, 110,
            97, 109, 101, 34, 58, 34, 74, 111, 110, 97, 115, 32, 105, 80, 104, 111, 110, 101, 34,
            44, 34, 115, 105, 103, 110, 97, 108, 34, 58, 34, 45, 54, 49, 32, 100, 66, 109, 34, 44,
            34, 109, 97, 99, 34, 58, 34, 55, 101, 58, 97, 51, 58, 98, 49, 58, 55, 56, 58, 100, 55,
            58, 55, 50, 34, 44, 34, 105, 112, 34, 58, 34, 49, 57, 50, 46, 49, 54, 56, 46, 49, 46,
            54, 54, 34, 44, 34, 108, 101, 97, 115, 101, 34, 58, 34, 50, 51, 32, 104, 111, 117, 114,
            115, 32, 53, 32, 109, 105, 110, 117, 116, 101, 115, 32, 53, 55, 32, 115, 101, 99, 111,
            110, 100, 115, 34, 125, 44, 123, 34, 110, 97, 109, 101, 34, 58, 34, 76, 71, 119, 101,
            98, 79, 83, 84, 86, 34, 44, 34, 115, 105, 103, 110, 97, 108, 34, 58, 34, 45, 53, 53,
            32, 100, 66, 109, 34, 44, 34, 109, 97, 99, 34, 58, 34, 55, 99, 58, 49, 99, 58, 52, 101,
            58, 101, 51, 58, 56, 50, 58, 52, 48, 34, 44, 34, 105, 112, 34, 58, 34, 49, 57, 50, 46,
            49, 54, 56, 46, 49, 46, 54, 57, 34, 44, 34, 108, 101, 97, 115, 101, 34, 58, 34, 52, 32,
            104, 111, 117, 114, 115, 32, 51, 57, 32, 109, 105, 110, 117, 116, 101, 115, 32, 50, 54,
            32, 115, 101, 99, 111, 110, 100, 115, 34, 125, 44, 123, 34, 110, 97, 109, 101, 34, 58,
            34, 77, 97, 99, 34, 44, 34, 115, 105, 103, 110, 97, 108, 34, 58, 34, 45, 53, 56, 32,
            100, 66, 109, 34, 44, 34, 109, 97, 99, 34, 58, 34, 52, 101, 58, 99, 98, 58, 48, 51, 58,
            48, 98, 58, 54, 57, 58, 53, 48, 34, 44, 34, 105, 112, 34, 58, 34, 49, 57, 50, 46, 49,
            54, 56, 46, 49, 46, 54, 56, 34, 44, 34, 108, 101, 97, 115, 101, 34, 58, 34, 49, 56, 32,
            104, 111, 117, 114, 115, 32, 51, 50, 32, 109, 105, 110, 117, 116, 101, 115, 32, 52, 56,
            32, 115, 101, 99, 111, 110, 100, 115, 34, 125, 44, 123, 34, 110, 97, 109, 101, 34, 58,
            34, 104, 111, 109, 101, 97, 115, 115, 105, 115, 116, 97, 110, 116, 34, 44, 34, 115,
            105, 103, 110, 97, 108, 34, 58, 34, 45, 52, 56, 32, 100, 66, 109, 34, 44, 34, 109, 97,
            99, 34, 58, 34, 98, 56, 58, 50, 55, 58, 101, 98, 58, 48, 49, 58, 51, 101, 58, 54, 97,
            34, 44, 34, 105, 112, 34, 58, 34, 49, 57, 50, 46, 49, 54, 56, 46, 49, 46, 54, 52, 34,
            44, 34, 108, 101, 97, 115, 101, 34, 58, 34, 50, 49, 32, 104, 111, 117, 114, 115, 32,
            50, 51, 32, 109, 105, 110, 117, 116, 101, 115, 32, 53, 53, 32, 115, 101, 99, 111, 110,
            100, 115, 34, 125, 93, 125,
        ]);

        let parsed = SdmqProto::parse(&buf);

        dbg!(parsed.header);

        assert_eq!(parsed.data.len(), parsed.header.data_len as usize)
    }

    #[test]
    fn esp() {
        let data = [
            222, 173, 190, 239, 222, 173, 190, 239, 0, 1, 0, 4, 0, 0, 0, 62, 116, 101, 109, 112,
            123, 34, 116, 101, 109, 112, 101, 114, 97, 116, 117, 114, 101, 34, 58, 50, 51, 46, 51,
            44, 34, 112, 114, 101, 115, 115, 117, 114, 101, 34, 58, 49, 48, 48, 53, 52, 53, 46, 51,
            51, 44, 34, 104, 117, 109, 105, 100, 105, 116, 121, 34, 58, 53, 50, 46, 55, 56, 51, 50,
            48, 51, 125,
        ];
        let parsed = SdmqProto::parse(&data);

        dbg!(parsed.header);
        let wat = String::from_utf8_lossy(parsed.data);
        dbg!(wat);
    }
}
